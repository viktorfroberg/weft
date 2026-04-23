import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Check,
  Loader2,
  Plus,
  RefreshCw,
  Terminal as TerminalIcon,
  Sparkles,
  X,
  XCircle,
} from "lucide-react";
import { toast } from "sonner";
import {
  agentLaunch,
  agentLaunchResume,
  presetsList,
  tabScrollbackRead,
  taskAgentSessionGet,
  taskConsumeInitialPrompt,
  terminalAliveSessionsWorthWarning,
  terminalSpawn,
  type AgentPreset,
  type TerminalSpawnInput,
} from "@/lib/commands";
import {
  ensureTerminalTabSubscription,
  useTerminalTabs,
  type TerminalTab,
} from "@/stores/terminal_tabs";
import { TerminalView, type TerminalSpawn } from "./Terminal";
import { useLifecycleTrace, useRenderCount } from "@/lib/dev-trace";
import { usePtyExits } from "@/stores/pty_exits";
import { NewTabPicker } from "./NewTabPicker";
import { useConfirm } from "./ConfirmDialog";

const EMPTY_TABS: never[] = [];

interface Props {
  taskId: string;
  /** Default spawn for the shell tab. Must not include rows/cols — the
   * Terminal component fills those from its actual dimensions. */
  shellSpawn: Omit<TerminalSpawnInput, "rows" | "cols">;
}

function tabIcon(tab: TerminalTab) {
  if (tab.kind === "agent") return <Sparkles size={12} />;
  return <TerminalIcon size={12} />;
}

/** Status glyph to the right of the tab label. Shows:
 *   - `Loader2` (spinning) while the agent's PTY is alive
 *   - `Check` (green) after a clean exit (code 0)
 *   - `XCircle` (destructive) after a non-zero exit or signal
 *   - nothing for shell tabs (they're always alive) */
function tabStatusBadge(
  tab: TerminalTab,
  exit: ReturnType<typeof usePtyExits.getState>["bySessionId"][string] | undefined,
) {
  if (tab.state === "dormant") {
    return <span className="text-muted-foreground">○</span>;
  }
  if (tab.kind !== "agent") return null;
  if (!tab.sessionId) {
    return <Loader2 size={10} className="text-muted-foreground animate-spin" />;
  }
  if (!exit) {
    return <Loader2 size={10} className="text-emerald-400 animate-spin" />;
  }
  if (exit.success) {
    return <Check size={10} className="text-emerald-500" />;
  }
  return <XCircle size={10} className="text-destructive" />;
}

export function TerminalTabStrip({ taskId, shellSpawn }: Props) {
  useLifecycleTrace(`TerminalTabStrip(${taskId})`);
  useRenderCount(`TerminalTabStrip(${taskId})`, 10);
  const tabs = useTerminalTabs((s) => s.byTaskId[taskId] ?? EMPTY_TABS);
  const activeId = useTerminalTabs((s) => s.activeByTaskId[taskId]);
  const ensureDefaults = useTerminalTabs((s) => s.ensureDefaults);
  const setActive = useTerminalTabs((s) => s.setActive);
  const closeTab = useTerminalTabs((s) => s.closeTab);
  const addTab = useTerminalTabs((s) => s.addTab);
  const setTabSessionId = useTerminalTabs((s) => s.setTabSessionId);
  const requestNewTab = useTerminalTabs((s) => s.requestNewTab);
  const exitsBySessionId = usePtyExits((s) => s.bySessionId);
  const confirm = useConfirm();

  // Dormant-replay state. Keyed by tab id so switching between two
  // dormant tabs doesn't clobber the other's loaded bytes. `null` =
  // "loaded, empty"; `undefined` = "not loaded yet"; `Uint8Array` =
  // "loaded with content".
  const [dormantBytesByTab, setDormantBytesByTab] = useState<
    Record<string, Uint8Array | null | undefined>
  >({});

  // Bump to force TerminalView remount when a dormant tab resumes. The
  // sessionKey prop becomes `${tab.id}:${resumeNonce}` so the xterm
  // instance is recreated and the live-spawn path runs fresh.
  const [resumeNonceByTab, setResumeNonceByTab] = useState<
    Record<string, number>
  >({});

  const loadInFlightRef = useRef<Record<string, boolean>>({});

  // Load scrollback for dormant tabs on demand. Fires when a tab
  // transitions into dormant state and we haven't loaded its bytes
  // yet. Skips tabs whose `resumeNonceByTab` has been bumped — the
  // user just pressed Enter to resume, we cleared `dormantBytesByTab`
  // to force the remount, and we DO NOT want this loader to
  // immediately re-fetch and put the dormant overlay back. The
  // backend's `mark_live` flips state to "live" momentarily later, so
  // this guard only needs to hold for the brief window between
  // `resumeTab()` and the spawn-driven state flip.
  useEffect(() => {
    for (const tab of tabs) {
      if (tab.state !== "dormant") continue;
      if ((resumeNonceByTab[tab.id] ?? 0) > 0) continue;
      if (dormantBytesByTab[tab.id] !== undefined) continue;
      if (loadInFlightRef.current[tab.id]) continue;
      loadInFlightRef.current[tab.id] = true;
      tabScrollbackRead(tab.id)
        .then((bytes) => {
          setDormantBytesByTab((prev) => ({
            ...prev,
            [tab.id]: bytes.length > 0 ? bytes : null,
          }));
        })
        .catch((e) => {
          console.warn("tabScrollbackRead failed", tab.id, e);
          setDormantBytesByTab((prev) => ({ ...prev, [tab.id]: null }));
        })
        .finally(() => {
          delete loadInFlightRef.current[tab.id];
        });
    }
  }, [tabs, dormantBytesByTab, resumeNonceByTab]);

  // Clear loaded bytes AND reset the resume-nonce guard when a tab
  // flips back to live, so the next dormant cycle works cleanly.
  useEffect(() => {
    setDormantBytesByTab((prev) => {
      const next = { ...prev };
      const ids = new Set(tabs.map((t) => t.id));
      let changed = false;
      for (const id of Object.keys(next)) {
        const tab = tabs.find((t) => t.id === id);
        if (!tab || tab.state === "live") {
          delete next[id];
          changed = true;
        }
      }
      for (const id of Object.keys(prev)) {
        if (!ids.has(id)) changed = true;
      }
      return changed ? next : prev;
    });
    // Reset resume-nonce guard for live tabs. Without this, a user
    // who closes the tab again later would never re-load its
    // dormant transcript because the gate is still tripped from the
    // previous resume.
    setResumeNonceByTab((prev) => {
      const next = { ...prev };
      let changed = false;
      for (const id of Object.keys(next)) {
        const tab = tabs.find((t) => t.id === id);
        if (!tab) {
          delete next[id];
          changed = true;
        } else if (tab.state === "live" && next[id] > 0) {
          delete next[id];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [tabs]);

  // Reload an agent tab: hard-close + add a fresh tab with the same
  // preset. Differs from soft-close: this is an explicit user action to
  // discard the current conversation and start over.
  const reloadAgentTab = (tab: TerminalTab) => {
    if (tab.kind !== "agent") return;
    closeTab(taskId, tab.id);
    addTab({
      id: `${taskId}:agent-${Date.now()}`,
      kind: "agent",
      label: tab.label,
      presetId: tab.presetId,
      taskId,
    });
    toast.success(`Reloaded ${tab.label}`, {
      description: "New session sees current repos + tickets.",
    });
  };

  // Confirm + hard-close. Asks only when there's something worth
  // saving:
  //   - dormant tabs: confirm before deleting the saved transcript
  //   - live agent tabs: always (an in-flight conversation is work)
  //   - live shell tabs: only if a foreground job is running. An
  //     idle shell with just `$` showing is not worth a dialog.
  //     Backend `terminal_alive_sessions_worth_warning` does the
  //     `ps -o ppid=` check for us — reuse it instead of duplicating
  //     the heuristic in JS.
  const confirmAndClose = async (tab: TerminalTab) => {
    const isLive = tab.state === "live" && !!tab.sessionId;

    if (!isLive) {
      const ok = await confirm({
        title: `Delete ${tab.label}?`,
        description: "This tab's saved transcript will be deleted.",
        confirmText: "Delete",
        destructive: false,
      });
      if (!ok) return;
      await closeTab(taskId, tab.id);
      return;
    }

    // Live tab. Agents always warn; shells only warn if they have a
    // running child.
    let needsConfirm = tab.kind === "agent";
    if (tab.kind === "shell") {
      try {
        const alive = await terminalAliveSessionsWorthWarning();
        needsConfirm = alive.some((s) => s.session_id === tab.sessionId);
      } catch (err) {
        // If the check fails, fall back to confirming — better a
        // spurious dialog than silently killing a long-running job.
        console.warn("alive-check failed, defaulting to confirm", err);
        needsConfirm = true;
      }
    }

    if (!needsConfirm) {
      await closeTab(taskId, tab.id);
      return;
    }

    const ok = await confirm({
      title: `Close ${tab.label}?`,
      description:
        tab.kind === "agent"
          ? "The agent session will be gracefully stopped and this tab will be removed."
          : "The running process will be gracefully stopped and this tab will be removed.",
      confirmText: "Close",
      destructive: true,
    });
    if (!ok) return;
    await closeTab(taskId, tab.id);
  };

  // Resume a dormant tab. Bumps the per-tab nonce, which flips
  // sessionKey → TerminalView remounts with dormantBytes = null → runs
  // the spawn path below. No network call here; the spawn fn is the
  // same live-launch path used for fresh tabs.
  const resumeTab = useCallback((tab: TerminalTab) => {
    setDormantBytesByTab((prev) => ({ ...prev, [tab.id]: undefined }));
    setResumeNonceByTab((prev) => ({
      ...prev,
      [tab.id]: (prev[tab.id] ?? 0) + 1,
    }));
  }, []);

  // Subscribe to terminal_tab db_events once per app lifetime.
  useEffect(() => {
    ensureTerminalTabSubscription();
  }, []);

  // Seed a default "Shell" tab the first time this task view mounts.
  useEffect(() => {
    ensureDefaults(taskId);
  }, [taskId, ensureDefaults]);

  const makeSpawn = (tab: TerminalTab, isResume: boolean): TerminalSpawn => {
    if (tab.kind === "shell") {
      return async ({ channel, rows, cols }) => {
        return terminalSpawn(
          {
            command: shellSpawn.command,
            args: shellSpawn.args,
            cwd: tab.cwd ?? shellSpawn.cwd,
            env: shellSpawn.env,
            rows,
            cols,
            task_id: taskId,
            tab_id: tab.id,
          },
          channel,
        );
      };
    }
    // Agent tab.
    return async ({ channel, rows, cols }) => {
      // Resume path: if this mount comes from a dormant→live transition
      // (resumeNonce bumped) AND we have a captured external session id
      // AND the preset supports resume, splice `--resume <id>`. Otherwise
      // fall back to plain agent_launch which delivers bootstrap-only.
      if (isResume) {
        try {
          const [sess, presets] = await Promise.all([
            taskAgentSessionGet(taskId, "claude_code"),
            presetsList(),
          ]);
          const preset =
            presets.find((p: AgentPreset) => p.id === tab.presetId) ??
            presets.find((p: AgentPreset) => p.is_default) ??
            null;
          if (sess && preset?.supports_resume) {
            const sessionId = await agentLaunchResume(
              {
                task_id: taskId,
                preset_id: tab.presetId ?? null,
                rows,
                cols,
                external_session_id: sess.external_session_id,
                tab_id: tab.id,
              },
              channel,
            );
            return sessionId;
          }
        } catch (err) {
          console.warn(
            "agent resume lookup failed, falling back to plain launch",
            err,
          );
        }
      }
      const sessionId = await agentLaunch(
        {
          task_id: taskId,
          preset_id: tab.presetId ?? null,
          rows,
          cols,
          initial_prompt: null,
          tab_id: tab.id,
        },
        channel,
      );
      taskConsumeInitialPrompt(taskId).catch((e) =>
        console.warn("taskConsumeInitialPrompt failed", e),
      );
      return sessionId;
    };
  };

  const tabList = useMemo(() => tabs, [tabs]);

  if (tabList.length === 0) {
    return (
      <div className="text-muted-foreground flex h-full items-center justify-center text-sm">
        No terminals
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-border flex shrink-0 items-center gap-0.5 overflow-x-auto border-b px-2 py-1">
        {tabList.map((tab) => {
          const active = tab.id === activeId;
          const dormant = tab.state === "dormant";
          return (
            <div
              key={tab.id}
              className={`flex items-center gap-1.5 rounded px-2 py-1 text-xs transition-colors ${
                active
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              } ${dormant ? "opacity-70" : ""}`}
            >
              <button
                type="button"
                onClick={() => setActive(taskId, tab.id)}
                className="flex items-center gap-1.5"
              >
                {tabIcon(tab)}
                <span>{tab.label}</span>
                {tabStatusBadge(
                  tab,
                  tab.sessionId ? exitsBySessionId[tab.sessionId] : undefined,
                )}
              </button>
              {tab.kind === "agent" && tab.state === "live" && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    reloadAgentTab(tab);
                  }}
                  disabled={!!tab.sessionId && !!exitsBySessionId[tab.sessionId]}
                  className="text-muted-foreground hover:text-foreground ml-0.5 disabled:opacity-40"
                  title="Reload agent (kill + respawn with current repos)"
                >
                  <RefreshCw size={10} />
                </button>
              )}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  confirmAndClose(tab);
                }}
                className="text-muted-foreground hover:text-foreground ml-0.5"
                title="Close tab"
              >
                <X size={10} />
              </button>
            </div>
          );
        })}
        <button
          type="button"
          onClick={() => requestNewTab(taskId)}
          className="text-muted-foreground hover:bg-muted hover:text-foreground ml-0.5 rounded px-1.5 py-1"
          title="New tab (⌘T)"
        >
          <Plus size={12} />
        </button>
      </div>

      <NewTabPicker taskId={taskId} />

      {/* All tabs mounted; only active is visible. Keeps each PTY alive +
          scrollback intact across tab switches. */}
      <div className="relative flex-1 overflow-hidden">
        {tabList.map((tab) => {
          const active = tab.id === activeId;
          const dormant = tab.state === "dormant";
          const nonce = resumeNonceByTab[tab.id] ?? 0;
          const dormantBytes = dormant ? dormantBytesByTab[tab.id] : undefined;
          // Stable across the dormant→live transition. Earlier this
          // included `:dormant:` when dormant — but the spawn that
          // fires after the user presses Enter happens DURING the
          // dormant phase (before the backend's `mark_live` round-
          // trips back). When state flipped to live a beat later,
          // the `:dormant:` prefix dropped → sessionKey changed →
          // TerminalView remounted → cleanup ran → `terminal_kill`
          // killed the just-spawned PTY. The dormant-vs-live
          // distinction is fully carried by the `dormantBytes` prop;
          // sessionKey only needs to bump on explicit user resume
          // (the nonce bump).
          const sessionKey = `${tab.id}:${nonce}`;
          return (
            <div
              key={tab.id}
              className="absolute inset-0"
              style={{ display: active ? "block" : "none" }}
            >
              <TerminalView
                sessionKey={sessionKey}
                spawn={makeSpawn(tab, !dormant && nonce > 0)}
                visible={active}
                onSpawned={(sessionId) =>
                  setTabSessionId(taskId, tab.id, sessionId)
                }
                dormantBytes={dormant ? dormantBytes ?? null : null}
                onResume={dormant ? () => resumeTab(tab) : undefined}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

// `composeInitialMessage` relocated to `src/lib/launch-agent.ts` as part
// of the CLI-arg delivery rewrite.
