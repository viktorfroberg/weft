import { useEffect } from "react";
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
  taskConsumeInitialPrompt,
  terminalSpawn,
  type TerminalSpawnInput,
} from "@/lib/commands";
import {
  useTerminalTabs,
  type TerminalTab,
} from "@/stores/terminal_tabs";
import { TerminalView, type TerminalSpawn } from "./Terminal";
import { useLifecycleTrace, useRenderCount } from "@/lib/dev-trace";
import { usePtyExits } from "@/stores/pty_exits";
import { NewTabPicker } from "./NewTabPicker";

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
  if (tab.kind !== "agent") return null;
  if (!tab.sessionId) {
    // Spawn hasn't returned yet — neutral "starting" look.
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

  // Kill the agent's PTY and immediately respawn one with the same
  // preset. The fresh `agent_launch` reads current task_worktrees so
  // any repos added since the original launch land in the new
  // session — at the cost of the conversation. `agent_launch`
  // always decides the prompt based on `initial_prompt_consumed_at`
  // (see Rust `task_context::compose_first_turn`), so reload reliably
  // falls through to the bootstrap template once the first turn has
  // been delivered.
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

  // Seed a default "Shell" tab the first time this task view mounts.
  useEffect(() => {
    ensureDefaults(taskId);
  }, [taskId, ensureDefaults]);

  const makeSpawn = (tab: TerminalTab): TerminalSpawn => {
    if (tab.kind === "shell") {
      return async ({ channel, rows, cols }) => {
        return terminalSpawn(
          {
            command: shellSpawn.command,
            args: shellSpawn.args,
            cwd: shellSpawn.cwd,
            env: shellSpawn.env,
            rows,
            cols,
            task_id: taskId,
          },
          channel,
        );
      };
    }
    // Agent tab. Prompt composition lives in Rust (v1.1): we always
    // pass `initial_prompt: null` and let `agent_launch` decide
    // between (a) first-turn compose from DB (task has unconsumed
    // prompt) or (b) the preset's bootstrap template (second agent,
    // reload, or any launch after the first was consumed). The
    // frontend is deliberately kept dumb here so the two paths can
    // never diverge.
    return async ({ channel, rows, cols }) => {
      const sessionId = await agentLaunch(
        {
          task_id: taskId,
          preset_id: tab.presetId ?? null,
          rows,
          cols,
          initial_prompt: null,
        },
        channel,
      );
      // Idempotent on the Rust side (WHERE IS NULL). Fire-and-forget:
      // if this was a second agent (already-consumed task) the command
      // no-ops.
      taskConsumeInitialPrompt(taskId).catch((e) =>
        console.warn("taskConsumeInitialPrompt failed", e),
      );
      return sessionId;
    };
  };

  if (tabs.length === 0) {
    return (
      <div className="text-muted-foreground flex h-full items-center justify-center text-sm">
        No terminals
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-border flex shrink-0 items-center gap-0.5 overflow-x-auto border-b px-2 py-1">
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          return (
            <div
              key={tab.id}
              className={`flex items-center gap-1.5 rounded px-2 py-1 text-xs transition-colors ${
                active
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
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
              {/* Shell tab is "pinned" — closing it would leave the task
                  with no way to drop into its worktree. Agent tabs get
                  Reload (kill + respawn so new repos/tickets land) and
                  Close. Reload is a no-op once the PTY has exited — the
                  user can just click the tab to relaunch. */}
              {tab.kind === "agent" && (
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
              {tab.kind !== "shell" && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(taskId, tab.id);
                  }}
                  className="text-muted-foreground hover:text-foreground ml-0.5"
                  title="Close tab"
                >
                  <X size={10} />
                </button>
              )}
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
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          return (
            <div
              key={tab.id}
              className="absolute inset-0"
              style={{ display: active ? "block" : "none" }}
            >
              <TerminalView
                sessionKey={tab.id}
                spawn={makeSpawn(tab)}
                visible={active}
                onSpawned={(sessionId) =>
                  setTabSessionId(taskId, tab.id, sessionId)
                }
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

// `composeInitialMessage` relocated to `src/lib/launch-agent.ts` as part
// of the CLI-arg delivery rewrite. The old PTY-stdin inject is gone —
// the prompt now rides in `agent_launch`'s `initial_prompt` parameter
// and fills the preset's `{prompt}` template token. See plan:
// "Initial prompt delivery via Claude CLI positional arg".
