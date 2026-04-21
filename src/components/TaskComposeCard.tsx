import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  ArrowUp,
  Check,
  ChevronDown,
  GitBranch,
  Loader2,
  Plus,
  Sparkles,
  Ticket as TicketIcon,
  X,
} from "lucide-react";
import {
  presetsList,
  taskCreate,
  workspaceAddRepo,
  workspaceCreate,
  type AgentPreset,
  type Project,
  type Task,
  type Workspace,
} from "@/lib/commands";
import { useProjects } from "@/stores/projects";
import { useWorkspaces, useWorkspaceRepos } from "@/stores/workspaces";
import { useIntegrations } from "@/stores/integrations";
import { useUi } from "@/stores/ui";
import { usePrefs } from "@/stores/prefs";
import { useQuery } from "@tanstack/react-query";
import { qk } from "@/query";
import { ProjectBadge } from "@/components/ProjectBadge";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { TicketPicker, ticketKey, toLink } from "@/components/TicketPicker";
import type { Ticket } from "@/lib/commands";
import { launchDefaultAgent } from "@/lib/launch-agent";

interface Props {
  variant: "inline" | "floating";
  initialRepoGroupId?: string | null;
  onCreated?: (task: Task) => void;
  onCancel?: () => void;
}

const EMPTY_REPOS: never[] = [];

/** Turn a full compose-card message into a short label suitable for
 *  the sidebar / breadcrumb. Takes the first non-empty line (keeps
 *  the "intent" signal that usually opens a prompt), truncates at
 *  roughly 60 chars on a word boundary, strips trailing punctuation.
 *  Falls back to the whole trimmed input if it's already short.
 *
 *  Follow-up: replace with an LLM-written title via `claude -p` once
 *  we have a background-rename command. Keeping this local keeps task
 *  creation instant; the label can be overwritten by the upgrade path
 *  later. */
function deriveShortName(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";
  const firstLine = trimmed.split(/\r?\n/).find((l) => l.trim())?.trim() ?? trimmed;
  const LIMIT = 60;
  if (firstLine.length <= LIMIT) {
    return firstLine.replace(/[.,;:!?\-–—\s]+$/u, "");
  }
  const slice = firstLine.slice(0, LIMIT);
  const lastSpace = slice.lastIndexOf(" ");
  const cut = lastSpace > LIMIT / 2 ? slice.slice(0, lastSpace) : slice;
  return cut.replace(/[.,;:!?\-–—\s]+$/u, "") + "…";
}

/**
 * v1.0.7 task-create card — Superset-pattern. Prompt + repo pills +
 * base-branch + agent preset + ticket picker + send. Used inline on
 * Home and as a floating overlay via `⌘⇧N`.
 *
 * Submission semantics:
 *   1. If `repoGroupName` is non-empty AND no existing workspace with
 *      that name matches → create a new workspace first + attach every
 *      selected project to it (with base-branch overrides). Use the
 *      resulting id as `workspace_id` on the task.
 *   2. Otherwise → `workspace_id = selectedRepoGroupId ?? null`.
 *   3. Always submit explicit `projectIds` + `baseBranches` so the
 *      backend uses them directly (doesn't fall back to workspace_repos).
 */
export function TaskComposeCard({
  variant,
  initialRepoGroupId = null,
  onCreated,
  onCancel,
}: Props) {
  const { data: projects = [] } = useProjects();
  const { data: workspaces = [] } = useWorkspaces();
  const { data: providers = [] } = useIntegrations();
  const { data: agentPresets = [] } = useQuery<AgentPreset[]>({
    queryKey: qk.agentPresets(),
    queryFn: () => presetsList(),
  });
  const connectedProvider = providers.find((p) => p.connected);

  // Selected repo group (if any). When set, the repo pills are
  // pre-seeded from its `workspace_repos` on first mount.
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(
    initialRepoGroupId,
  );
  const { data: groupRepos = EMPTY_REPOS } = useWorkspaceRepos(
    selectedGroupId ?? "",
  );

  const [prompt, setPrompt] = useState("");
  const [repoGroupName, setRepoGroupName] = useState("");
  const [selectedProjectIds, setSelectedProjectIds] = useState<string[]>([]);
  const [baseBranches, setBaseBranches] = useState<Record<string, string>>({});
  const [tickets, setTickets] = useState<Ticket[]>([]);
  const [agentPresetId, setAgentPresetId] = useState<string | null>(null);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [repoMenuOpen, setRepoMenuOpen] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const promptRef = useRef<HTMLTextAreaElement | null>(null);
  const agentMenuRef = useRef<HTMLDivElement | null>(null);
  const repoMenuRef = useRef<HTMLDivElement | null>(null);
  const repoTriggerRef = useRef<HTMLButtonElement | null>(null);
  const ticketPickerRef = useRef<HTMLDivElement | null>(null);
  // Ticket trigger sits in a different DOM subtree from the picker popover
  // (right side of the submit row), so we can't wrap both in one ref like
  // the agent/repo menus. Track the trigger separately and exempt it from
  // the outside-click close so clicking-to-close doesn't immediately reopen.
  const ticketTriggerRef = useRef<HTMLButtonElement | null>(null);

  // Close open popovers on outside click or Esc. Each menu owns a ref
  // wrapping its trigger + panel so clicks on the trigger don't re-open
  // immediately.
  useEffect(() => {
    if (!agentMenuOpen && !repoMenuOpen && !pickerOpen) return;
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (agentMenuOpen && !agentMenuRef.current?.contains(target)) {
        setAgentMenuOpen(false);
      }
      if (repoMenuOpen && !repoMenuRef.current?.contains(target)) {
        setRepoMenuOpen(false);
      }
      if (
        pickerOpen &&
        !ticketPickerRef.current?.contains(target) &&
        !ticketTriggerRef.current?.contains(target)
      ) {
        setPickerOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setAgentMenuOpen(false);
        setRepoMenuOpen(false);
        setPickerOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [agentMenuOpen, repoMenuOpen, pickerOpen]);

  // Hydrate from the picked group (first time it loads, or when group
  // changes).
  useEffect(() => {
    if (!selectedGroupId) return;
    if (groupRepos.length === 0) return;
    setSelectedProjectIds(groupRepos.map((r) => r.project_id));
    const overrides: Record<string, string> = {};
    for (const r of groupRepos) {
      if (r.base_branch) overrides[r.project_id] = r.base_branch;
    }
    setBaseBranches(overrides);
    const g = workspaces.find((w) => w.id === selectedGroupId);
    if (g) setRepoGroupName(g.name);
  }, [selectedGroupId, groupRepos, workspaces]);

  // Default agent preset: whichever has `is_default = true`.
  useEffect(() => {
    if (agentPresetId === null && agentPresets.length > 0) {
      const d = agentPresets.find((p) => p.is_default) ?? agentPresets[0];
      setAgentPresetId(d?.id ?? null);
    }
  }, [agentPresets, agentPresetId]);

  // Prune ticket chips when a provider disconnects mid-compose.
  useEffect(() => {
    const connectedIds = new Set(
      providers.filter((p) => p.connected).map((p) => p.id),
    );
    setTickets((prev) => prev.filter((t) => connectedIds.has(t.provider)));
  }, [providers]);

  // Autofocus the prompt on mount (both variants).
  useEffect(() => {
    promptRef.current?.focus();
  }, []);

  // Consume `ui.composePrefillTicket` — set by Home's backlog strip so a
  // click on a ticket card attaches the ticket here. We deliberately do
  // NOT seed the prompt from the ticket title: the user's prompt is the
  // intent ("fix bug X", "investigate Y"), not a copy of the ticket name.
  // Visibility comes from the ticket-chip strip below the textarea.
  const autoRename = usePrefs((s) => s.autoRenameTasks);
  const composePrefillTicket = useUi((s) => s.composePrefillTicket);
  const clearComposePrefillTicket = useUi((s) => s.setComposePrefillTicket);
  useEffect(() => {
    if (!composePrefillTicket) return;
    const key = ticketKey(composePrefillTicket);
    setTickets((prev) =>
      prev.some((p) => ticketKey(p) === key)
        ? prev
        : [...prev, composePrefillTicket],
    );
    clearComposePrefillTicket(null);
    promptRef.current?.focus();
  }, [composePrefillTicket, clearComposePrefillTicket]);

  // Inline variant only: keep the ui store mirror of attached ticket keys
  // up to date, and consume detach signals from Home's backlog strip
  // (so clicking an already-attached card removes it). The floating
  // overlay is its own scratch space — never written to the mirror —
  // because the Home strip pairs visually with the inline card on screen.
  const composeDetachTicketKey = useUi((s) => s.composeDetachTicketKey);
  const clearComposeDetachTicketKey = useUi((s) => s.setComposeDetachTicketKey);
  const setComposeAttachedTicketKeys = useUi(
    (s) => s.setComposeAttachedTicketKeys,
  );
  useEffect(() => {
    if (variant !== "inline") return;
    setComposeAttachedTicketKeys(new Set(tickets.map(ticketKey)));
  }, [variant, tickets, setComposeAttachedTicketKeys]);
  useEffect(() => {
    if (variant !== "inline") return;
    if (!composeDetachTicketKey) return;
    const k = composeDetachTicketKey;
    setTickets((prev) => prev.filter((p) => ticketKey(p) !== k));
    clearComposeDetachTicketKey(null);
  }, [variant, composeDetachTicketKey, clearComposeDetachTicketKey]);
  // Clear the mirror when the inline card unmounts (route away from Home)
  // so a subsequent floating overlay doesn't paint stale "attached" state
  // on the next Home visit.
  useEffect(() => {
    if (variant !== "inline") return;
    return () => setComposeAttachedTicketKeys(new Set());
  }, [variant, setComposeAttachedTicketKeys]);

  const selectedProjects: Project[] = useMemo(
    () =>
      selectedProjectIds
        .map((id) => projects.find((p) => p.id === id))
        .filter((p): p is Project => !!p),
    [selectedProjectIds, projects],
  );

  // "Base branch" dropdown currently applies to the task as a whole
  // (single value). Per-repo overrides come from the repo group.
  const [taskBaseBranch, setTaskBaseBranch] = useState("main");

  // Submit mutation.
  const createTask = useMutation({
    mutationFn: async () => {
      // Step 1: optionally create a new repo group if the user named
      // one and no existing group matches.
      let workspaceId: string | null = selectedGroupId;
      const nameTrim = repoGroupName.trim();
      if (nameTrim) {
        const existing = workspaces.find(
          (w) => w.name.toLowerCase() === nameTrim.toLowerCase(),
        );
        if (existing) {
          workspaceId = existing.id;
        } else {
          const ws: Workspace = await workspaceCreate({ name: nameTrim });
          workspaceId = ws.id;
          // Attach every currently-selected project with its base-branch.
          for (const pid of selectedProjectIds) {
            await workspaceAddRepo({
              workspace_id: ws.id,
              project_id: pid,
              base_branch: baseBranches[pid] ?? null,
            });
          }
        }
      }

      // Step 2: merge per-repo overrides with the task-level base branch.
      const mergedBranches: Record<string, string> = { ...baseBranches };
      for (const pid of selectedProjectIds) {
        if (!(pid in mergedBranches) && taskBaseBranch && taskBaseBranch !== "main") {
          mergedBranches[pid] = taskBaseBranch;
        }
      }

      const promptTrim = prompt.trim();
      return taskCreate(
        {
          workspace_id: workspaceId,
          // Display name = short label (first sentence / first ~60 chars);
          // full prompt goes to initialPrompt for the agent. A multi-line
          // compose message as the sidebar + breadcrumb label reads as
          // wall-of-text and blows out truncation everywhere.
          name: deriveShortName(promptTrim) || "untitled",
          agent_preset: agentPresetId,
        },
        {
          tickets: tickets.length > 0 ? tickets.map(toLink) : undefined,
          projectIds: selectedProjectIds,
          baseBranches: mergedBranches,
          // Persist the prompt so the spawned agent receives it as its
          // first user message. Agents auto-launch on task creation.
          initialPrompt: promptTrim || null,
          // Lets Rust fire a background `claude -p` rename so the
          // sidebar shows a short LLM title instead of the heuristic
          // first-line-truncated name. Gated by the user's pref.
          autoRename,
        },
      );
    },
    onSuccess: (resp) => {
      toast.success("Task created", { description: resp.task.branch_name });
      setPrompt("");
      setRepoGroupName("");
      setSelectedProjectIds([]);
      setBaseBranches({});
      setSelectedGroupId(null);
      setTickets([]);
      // Auto-launch the default agent on every task creation. The
      // TerminalTabStrip's onSpawned handler then injects the task's
      // `initial_prompt` as Claude's first user message. If no preset
      // is configured, launchDefaultAgent resolves to null and we
      // leave the task with the Shell tab only — user can still
      // press Launch manually.
      void launchDefaultAgent(resp.task.id).catch(() => {});
      onCreated?.(resp.task);
    },
    onError: (e) => toast.error("Couldn't create task", { description: String(e) }),
  });

  const promptMissing = prompt.trim().length === 0;
  const reposMissing = selectedProjectIds.length === 0;
  const canSubmit = !promptMissing && !reposMissing && !createTask.isPending;

  const submitBlockerTitle = createTask.isPending
    ? "Creating task…"
    : promptMissing && reposMissing
      ? "Type a prompt and pick a repo"
      : promptMissing
        ? "Type a prompt to create a task"
        : reposMissing
          ? "Pick a repo to create a task"
          : "Send (Enter)";

  const onSubmit = () => {
    if (canSubmit) {
      createTask.mutate();
      return;
    }
    // Submission blocked. Tell the user what's missing and pull their
    // eye to the right field — silent gating reads as "app broken".
    if (promptMissing) {
      promptRef.current?.focus();
      return;
    }
    if (reposMissing) {
      toast("Pick a repo to create a task");
      // Open the repo picker if it's closed, and put focus on its
      // trigger so a keyboard user can tab straight into the list.
      setRepoMenuOpen(true);
      requestAnimationFrame(() => repoTriggerRef.current?.focus());
    }
  };

  const toggleProject = (id: string) => {
    setSelectedProjectIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  };
  const removeProject = (id: string) => {
    setSelectedProjectIds((prev) => prev.filter((x) => x !== id));
    setBaseBranches((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  };

  const activeAgent = agentPresets.find((p) => p.id === agentPresetId);

  const toggleTicket = (t: Ticket) => {
    const key = ticketKey(t);
    setTickets((prev) => {
      const idx = prev.findIndex((p) => ticketKey(p) === key);
      if (idx >= 0) return prev.filter((_, i) => i !== idx);
      return [...prev, t];
    });
  };


  const selectedTicketKeys = useMemo(
    () => new Set(tickets.map(ticketKey)),
    [tickets],
  );

  const showSaveGroup = selectedProjects.length >= 2;

  return (
    <div
      className={`border-border bg-card relative rounded-xl border shadow-sm ${
        variant === "floating" ? "w-[680px] max-w-[92vw]" : "w-full"
      }`}
    >
      {/* Prompt */}
      <div className="px-4 pt-4 pb-2">
        <textarea
          ref={promptRef}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => {
            // Enter submits, Shift+Enter (and ⌘/Ctrl+Enter) inserts a
            // newline. `isComposing` guard prevents firing while an IME
            // candidate window is open (common with CJK input).
            if (
              e.key === "Enter" &&
              !e.shiftKey &&
              !e.metaKey &&
              !e.ctrlKey &&
              !e.nativeEvent.isComposing
            ) {
              e.preventDefault();
              onSubmit();
            }
            if (e.key === "Escape" && variant === "floating") {
              e.preventDefault();
              onCancel?.();
            }
          }}
          rows={2}
          placeholder="What do you want to do? (Enter to send, Shift+Enter for newline)"
          className="text-foreground placeholder:text-muted-foreground w-full resize-y bg-transparent text-sm outline-none"
          disabled={createTask.isPending}
        />
      </div>

      {/* Agent + icon row */}
      <div className="flex items-center justify-between gap-2 px-4 py-2">
        <div ref={agentMenuRef} className="relative">
          <button
            type="button"
            onClick={() => setAgentMenuOpen((v) => !v)}
            className="bg-muted hover:bg-accent flex h-7 items-center gap-1.5 rounded px-2 text-xs transition-colors"
          >
            <Sparkles size={12} className="text-muted-foreground" />
            <span>{activeAgent?.name ?? "No agent"}</span>
            <ChevronDown size={10} className="text-muted-foreground" />
          </button>
          {agentMenuOpen && (
            <div className="bg-popover border-border absolute left-0 top-full z-20 mt-1 min-w-[180px] rounded-md border py-1 shadow-lg">
              <AgentOption
                active={agentPresetId === null}
                label="No agent"
                onClick={() => {
                  setAgentPresetId(null);
                  setAgentMenuOpen(false);
                }}
              />
              {agentPresets.map((p) => (
                <AgentOption
                  key={p.id}
                  active={agentPresetId === p.id}
                  label={p.name}
                  onClick={() => {
                    setAgentPresetId(p.id);
                    setAgentMenuOpen(false);
                  }}
                />
              ))}
            </div>
          )}
        </div>

        <div className="flex items-center gap-1">
          {/* Picker trigger. The picker (and its checkmarks) is the
              source of truth for *which* tickets are attached, so this
              button only needs to communicate count + "click to add". */}
          <button
            ref={ticketTriggerRef}
            type="button"
            onClick={() => {
              if (connectedProvider) setPickerOpen((v) => !v);
              else {
                toast.message("Connect a provider first", {
                  description: "Settings → Integrations",
                });
              }
            }}
            title={
              connectedProvider
                ? tickets.length > 0
                  ? `${tickets.length} ${connectedProvider.display_name} ticket${tickets.length === 1 ? "" : "s"} attached — click to add another`
                  : `Link ${connectedProvider.display_name} ticket`
                : "Connect a ticket provider in Settings"
            }
            disabled={!connectedProvider}
            className={`text-muted-foreground hover:bg-accent hover:text-foreground flex h-7 items-center gap-1 rounded px-2 text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
              pickerOpen || tickets.length > 0
                ? "bg-muted text-foreground"
                : ""
            }`}
          >
            <TicketIcon size={12} />
            {tickets.length > 0 && (
              <span className="font-mono text-[11px]">{tickets.length}</span>
            )}
            <Plus size={10} className="-ml-0.5" />
          </button>
          <Button
            size="sm"
            onClick={onSubmit}
            disabled={!canSubmit}
            className="h-7 w-7 p-0"
            title={submitBlockerTitle}
          >
            {createTask.isPending ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <ArrowUp size={14} />
            )}
          </Button>
        </div>

        {pickerOpen && connectedProvider && (
          <div
            ref={ticketPickerRef}
            className="bg-popover border-border absolute right-4 top-full z-10 mt-1 rounded-md border shadow-lg"
          >
            <TicketPicker
              providerId={connectedProvider.id}
              selectedKeys={selectedTicketKeys}
              onToggle={toggleTicket}
              onClose={() => setPickerOpen(false)}
              autoFocus
            />
          </div>
        )}
      </div>

      {/* Repo pills + base branch. The outer `relative` container anchors
          the repo-picker popover to its left edge regardless of how many
          pills are in front of the `+ add` trigger. The inner `min-w-0`
          scroller prevents pill overflow from wrapping and pushing the
          base-branch / send-hint onto their own line. */}
      <div
        ref={repoMenuRef}
        className="border-border relative flex items-center gap-1.5 border-t px-4 py-2"
      >
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
        {selectedProjects.map((p) => (
          <span
            key={p.id}
            className="bg-muted inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-xs"
          >
            <ProjectBadge name={p.name} color={p.color} size="sm" />
            <span>{p.name}</span>
            <button
              type="button"
              onClick={() => removeProject(p.id)}
              className="text-muted-foreground hover:text-destructive"
              title="Remove"
            >
              <X size={10} />
            </button>
          </span>
        ))}
        <button
          ref={repoTriggerRef}
          type="button"
          onClick={() => setRepoMenuOpen((v) => !v)}
          // Prompt-typed + no repo selected surfaces a subtle amber
          // ring so the user sees *where* the missing piece is without
          // us disabling their text. Drops the moment they pick one.
          className={`flex h-6 shrink-0 items-center gap-1 rounded px-1.5 text-xs transition-colors ${
            !promptMissing && reposMissing
              ? "text-amber-600 ring-1 ring-amber-500/40 hover:bg-amber-500/10 dark:text-amber-400"
              : "text-muted-foreground hover:bg-accent hover:text-foreground"
          }`}
          title={
            !promptMissing && reposMissing
              ? "Required: pick at least one repo"
              : undefined
          }
        >
          <Plus size={10} />
          {selectedProjects.length === 0 ? "pick repos…" : "add"}
        </button>
        </div>
        {repoMenuOpen && (
          <div className="bg-popover border-border absolute left-4 top-full z-20 mt-1 min-w-[220px] max-h-[280px] overflow-y-auto rounded-md border py-1 shadow-lg">
            {projects.length === 0 ? (
              <button
                type="button"
                onClick={() => {
                  setRepoMenuOpen(false);
                  useUi.getState().setAddProjectOpen(true);
                }}
                className="hover:bg-accent flex w-full items-center gap-2 px-2 py-1 text-left text-xs"
              >
                <Plus size={12} className="text-primary" />
                <span className="text-foreground">Add a repo</span>
                <span className="text-muted-foreground ml-auto text-[10px]">
                  ⌘P
                </span>
              </button>
            ) : (
              projects.map((p) => {
                const checked = selectedProjectIds.includes(p.id);
                return (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => toggleProject(p.id)}
                    className="hover:bg-accent flex w-full items-center gap-2 px-2 py-1 text-left text-xs"
                  >
                    <span className="w-3">
                      {checked && <Check size={12} className="text-primary" />}
                    </span>
                    <ProjectBadge name={p.name} color={p.color} size="sm" />
                    <span className="flex-1 truncate">{p.name}</span>
                  </button>
                );
              })
            )}
            {workspaces.length > 0 && (
              <div className="border-border mt-1 border-t pt-1">
                <div className="text-muted-foreground px-2 py-0.5 text-[10px] uppercase">
                  Apply repo group
                </div>
                {workspaces.map((w) => (
                  <button
                    key={w.id}
                    type="button"
                    onClick={() => {
                      setSelectedGroupId(w.id);
                      setRepoMenuOpen(false);
                    }}
                    className="hover:bg-accent flex w-full items-center gap-2 px-2 py-1 text-left text-xs"
                  >
                    <span className="w-3" />
                    <span className="truncate">{w.name}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        <div className="bg-muted ml-2 inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-xs">
          <GitBranch size={10} className="text-muted-foreground" />
          <span className="text-muted-foreground text-[10px]">base</span>
          <input
            value={taskBaseBranch}
            onChange={(e) => setTaskBaseBranch(e.target.value)}
            className="w-20 bg-transparent font-mono text-[11px] outline-none"
            placeholder="main"
          />
        </div>
      </div>

      {/* Save-as-group toggle: only surfaces once 2+ repos picked, since
          single-repo tasks are almost never worth saving as a preset. */}
      {showSaveGroup && (
        <div className="border-border flex items-center gap-2 border-t px-4 py-2">
          <label className="text-muted-foreground flex items-center gap-1.5 text-[11px]">
            <span>Save this combo as</span>
          </label>
          <Input
            value={repoGroupName}
            onChange={(e) => setRepoGroupName(e.target.value)}
            placeholder="repo group name (optional)"
            className="text-foreground placeholder:text-muted-foreground h-6 flex-1 text-xs"
          />
        </div>
      )}
    </div>
  );
}

function AgentOption({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`hover:bg-accent flex w-full items-center gap-2 px-2 py-1 text-left text-xs ${
        active ? "text-foreground" : "text-muted-foreground"
      }`}
    >
      <span className="w-3">{active && <Check size={12} />}</span>
      {label}
    </button>
  );
}
