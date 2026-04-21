import { Channel, invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Entity shapes — mirrored from src-tauri/src/model/*.rs
// ---------------------------------------------------------------------------
export interface Project {
  id: string;
  name: string;
  main_repo_path: string;
  default_branch: string;
  color: string | null;
  last_opened_at: number;
  created_at: number;
}

export interface Workspace {
  id: string;
  name: string;
  sort_order: number;
  created_at: number;
  updated_at: number;
}

export interface WorkspaceRepo {
  workspace_id: string;
  project_id: string;
  base_branch: string | null;
  sort_order: number;
  added_at: number;
}

export type TaskStatus = "idle" | "working" | "waiting" | "error" | "done";

export interface Task {
  id: string;
  /** Optional "repo group" tag (v1.0.7). Points at a `workspaces` row
   * the task was born from. Null for ad-hoc tasks, or tasks whose
   * group was later deleted (ON DELETE SET NULL). */
  workspace_id: string | null;
  name: string;
  slug: string;
  branch_name: string;
  status: TaskStatus;
  agent_preset: string | null;
  created_at: number;
  completed_at: number | null;
  /** Prompt from Home's compose card — destined for the agent's first
   *  user message. Null when the user created a task with an empty
   *  compose box. */
  initial_prompt: string | null;
  /** Unix-ms when weft wrote `initial_prompt` into the agent's PTY.
   *  Used to guard against re-injecting on relaunch. Null = pending
   *  delivery. */
  initial_prompt_consumed_at: number | null;
}

// ---------------------------------------------------------------------------
// Command wrappers. Keep argument shapes aligned with Rust NewX structs.
// ---------------------------------------------------------------------------

export const projectsList = () => invoke<Project[]>("projects_list");

export const projectCreate = (input: {
  name: string;
  main_repo_path: string;
  default_branch: string;
  color?: string | null;
}) => invoke<Project>("project_create", { input });

export const projectDelete = (id: string) =>
  invoke<void>("project_delete", { id });

export const projectSetColor = (id: string, color: string | null) =>
  invoke<void>("project_set_color", { id, color });

export const projectRename = (id: string, name: string) =>
  invoke<void>("project_rename", { id, name });

export const workspacesList = () => invoke<Workspace[]>("workspaces_list");

export const workspaceCreate = (input: { name: string; sort_order?: number }) =>
  invoke<Workspace>("workspace_create", { input });

export const workspaceDelete = (id: string) =>
  invoke<void>("workspace_delete", { id });

export const workspaceReposList = (workspaceId: string) =>
  invoke<WorkspaceRepo[]>("workspace_repos_list", { workspaceId });

export const workspaceAddRepo = (input: {
  workspace_id: string;
  project_id: string;
  base_branch?: string | null;
  sort_order?: number;
}) => invoke<WorkspaceRepo>("workspace_add_repo", { input });

export const workspaceRemoveRepo = (workspaceId: string, projectId: string) =>
  invoke<void>("workspace_remove_repo", { workspaceId, projectId });

export const tasksList = (workspaceId: string) =>
  invoke<Task[]>("tasks_list", { workspaceId });

/** Flat list of every task, newest first. Sidebar + Home read this. */
export const tasksListAll = () => invoke<Task[]>("tasks_list_all");

/** Project ids a task currently touches — derived from `task_worktrees`.
 * Drives the sidebar repo color-dots. */
export const taskProjectIds = (taskId: string) =>
  invoke<string[]>("task_project_ids", { taskId });

export interface TaskWorktree {
  task_id: string;
  project_id: string;
  worktree_path: string;
  task_branch: string;
  base_branch: string;
  status: string; // creating | ready | failed | cleaned | missing
  created_at: number;
}

export const taskWorktreesList = (taskId: string) =>
  invoke<TaskWorktree[]>("task_worktrees_list", { taskId });

export interface WorktreeSummary {
  project_id: string;
  project_name: string;
  worktree_path: string;
  task_branch: string;
  base_branch: string;
}

export interface TaskCreateResponse {
  task: Task;
  worktrees: WorktreeSummary[];
}

export const taskCreate = (
  input: {
    /** v1.0.7: optional repo-group tag. Pass `null` for an ad-hoc
     * task. When non-null AND `projectIds` is empty, backend falls
     * back to the group's `workspace_repos` list. */
    workspace_id: string | null;
    name: string;
    agent_preset?: string | null;
  },
  opts?: {
    tickets?: TicketLink[];
    /** Apply the project's `project_links` (warm env). Default true. */
    warmLinks?: boolean;
    /** v1.0.7: explicit repo selection. Takes precedence over
     * `workspace_id`. */
    projectIds?: string[];
    /** v1.0.7: optional per-project base-branch overrides. */
    baseBranches?: Record<string, string>;
    /** Compose-card prompt. Persisted on the task and injected into
     * the agent's PTY as the first user message on auto-launch. */
    initialPrompt?: string | null;
    /** Whether Rust should fire a background `claude -p` rename after
     * the task row is committed. Default `true` unless the user
     * disabled it in Settings → Workflow. */
    autoRename?: boolean;
  },
) =>
  invoke<TaskCreateResponse>("task_create", {
    input,
    tickets: opts?.tickets ?? null,
    warmLinks: opts?.warmLinks ?? true,
    projectIds: opts?.projectIds ?? null,
    baseBranches: opts?.baseBranches ?? null,
    initialPrompt: opts?.initialPrompt ?? null,
    autoRename: opts?.autoRename ?? true,
  });

export const taskConsumeInitialPrompt = (taskId: string) =>
  invoke<void>("task_consume_initial_prompt", { taskId });

export const taskRename = (taskId: string, name: string) =>
  invoke<string>("task_rename", { taskId, name });

/** A task branch weft refused to delete during cleanup because its
 *  tip had commits that the branch's base_branch didn't contain.
 *  Surfaces to the UI so the user can decide whether to merge or
 *  force-delete it manually. */
export interface PreservedBranch {
  project_id: string;
  project_name: string;
  branch: string;
  base_branch: string;
  repo_path: string;
}

export interface TaskDeleteResponse {
  preserved_branches: PreservedBranch[];
}

export const taskDelete = (id: string) =>
  invoke<TaskDeleteResponse>("task_delete", { id });

export const taskAddRepo = (input: {
  task_id: string;
  project_id: string;
  base_branch?: string | null;
}) =>
  invoke<WorktreeSummary>("task_add_repo", {
    taskId: input.task_id,
    projectId: input.project_id,
    baseBranch: input.base_branch ?? null,
  });

export const taskRemoveRepo = (taskId: string, projectId: string) =>
  invoke<void>("task_remove_repo", { taskId, projectId });

export const taskOpenInEditor = (taskId: string, editor?: string) =>
  invoke<string>("task_open_in_editor", {
    taskId,
    editor: editor ?? null,
  });

// ---------------------------------------------------------------------------
// Terminal (Phase 5). PTY output streams via Tauri v2 Channel<Vec<u8>> — see
// src/components/Terminal.tsx for the wiring.
// ---------------------------------------------------------------------------

export interface TerminalSpawnInput {
  command: string;
  args?: string[];
  cwd: string;
  env?: Array<[string, string]>;
  rows: number;
  cols: number;
  task_id?: string;
}

export function terminalSpawn(
  input: TerminalSpawnInput,
  channel: Channel<Uint8Array>,
): Promise<string> {
  return invoke<string>("terminal_spawn", { input, channel });
}

export const terminalWrite = (id: string, data: Uint8Array) =>
  invoke<void>("terminal_write", { id, data: Array.from(data) });

export const terminalResize = (id: string, rows: number, cols: number) =>
  invoke<void>("terminal_resize", { id, rows, cols });

export const terminalKill = (id: string) =>
  invoke<void>("terminal_kill", { id });

// ---------------------------------------------------------------------------
// Phase 6: diff / changes
// ---------------------------------------------------------------------------

export type FileChangeKind =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "untracked"
  | "conflicted"
  | "type_changed"
  | "other";

export interface FileChange {
  path: string;
  kind: FileChangeKind;
  from_path: string | null;
}

export interface RepoChanges {
  project_id: string;
  worktree_path: string;
  base_branch: string;
  task_branch: string;
  changes: FileChange[];
  error: string | null;
}

export const taskChangesByRepo = (taskId: string) =>
  invoke<RepoChanges[]>("task_changes_by_repo", { taskId });

export interface FileSides {
  base: string | null;
  current: string | null;
  base_branch: string;
  path: string;
}

export const worktreeFileSides = (
  worktreePath: string,
  baseBranch: string,
  file: string,
) =>
  invoke<FileSides>("worktree_file_sides", {
    worktreePath,
    baseBranch,
    file,
  });

export interface CommitResult {
  project_id: string;
  ok: boolean;
  sha: string | null;
  error: string | null;
}

export const worktreeCommit = (
  projectId: string,
  worktreePath: string,
  message: string,
) =>
  invoke<CommitResult>("worktree_commit", {
    projectId,
    worktreePath,
    message,
  });

export const worktreeDiscard = (worktreePath: string) =>
  invoke<void>("worktree_discard", { worktreePath });

export const taskCommitAll = (taskId: string, message: string) =>
  invoke<CommitResult[]>("task_commit_all", { taskId, message });

export interface AppInfo {
  version: string;
  hook_port: number | null;
  hook_manifest_path: string;
  data_dir: string;
  worktrees_dir: string;
  db_path: string;
  default_shell: string;
}

export const appInfo = () => invoke<AppInfo>("app_info");

// ---------------------------------------------------------------------------
// v1.0.1 — agent presets + launch
// ---------------------------------------------------------------------------

export interface AgentPreset {
  id: string;
  name: string;
  command: string;
  args_json: string;
  env_json: string;
  is_default: boolean;
  sort_order: number;
  created_at: number;
}

export const presetsList = () => invoke<AgentPreset[]>("presets_list");

export const presetDefault = () =>
  invoke<AgentPreset | null>("preset_default");

export function agentLaunch(
  input: {
    task_id: string;
    preset_id?: string | null;
    rows: number;
    cols: number;
    /** Fills the preset's `{prompt}` template token. Pass `undefined` /
     *  `null` on relaunch so Claude doesn't auto-submit a stale first
     *  message. */
    initial_prompt?: string | null;
  },
  channel: Channel<Uint8Array>,
): Promise<string> {
  return invoke<string>("agent_launch", {
    taskId: input.task_id,
    presetId: input.preset_id ?? null,
    rows: input.rows,
    cols: input.cols,
    initialPrompt: input.initial_prompt ?? null,
    channel,
  });
}

// ---------------------------------------------------------------------------
// v1.0.2 — ticket integrations
// ---------------------------------------------------------------------------

/** One provider weft knows how to drive. `connected` = user has supplied
 *  a token that passed `integration_test` at least once. */
export interface ProviderInfo {
  id: string;
  display_name: string;
  connected: boolean;
}

/** Live ticket info, fetched on demand (not cached in SQLite).
 *  `priority` follows Linear's scale: 0 = No priority, 1 = Urgent,
 *  2 = High, 3 = Medium, 4 = Low. */
export interface Ticket {
  provider: string;
  external_id: string;
  title: string;
  url: string;
  status: string | null;
  assignee: string | null;
  priority: number | null;
  cycle_name: string | null;
  cycle_number: number | null;
}

/** Persisted task↔ticket link row (no title — titles are live). */
export interface TicketLink {
  provider: string;
  external_id: string;
  url: string;
}

export interface AuthStatus {
  ok: boolean;
  viewer: string | null;
  error: string | null;
}

export const integrationList = () =>
  invoke<ProviderInfo[]>("integration_list");

export const integrationSetToken = (providerId: string, token: string) =>
  invoke<AuthStatus>("integration_set_token", { providerId, token });

export const integrationClear = (providerId: string) =>
  invoke<void>("integration_clear", { providerId });

export const integrationTest = (providerId: string) =>
  invoke<AuthStatus>("integration_test", { providerId });

export type LinearBacklogScope = "in_progress" | "actionable" | "all_open";
export interface LinearSettings {
  backlog_scope: LinearBacklogScope;
  /** Cached viewer display name from the last successful auth check.
   *  Powers the Home greeting; null when never connected. */
  viewer_name: string | null;
}

export const linearSettingsGet = () =>
  invoke<LinearSettings>("linear_settings_get");

export const linearSettingsSet = (settings: LinearSettings) =>
  invoke<void>("linear_settings_set", { settings });

export const ticketListBacklog = (providerId: string) =>
  invoke<Ticket[]>("ticket_list_backlog", { providerId });

export const ticketGet = (providerId: string, externalId: string) =>
  invoke<Ticket | null>("ticket_get", { providerId, externalId });

export const taskTicketsList = (taskId: string) =>
  invoke<TicketLink[]>("task_tickets_list", { taskId });

export interface TaskTicketRow {
  task_id: string;
  provider: string;
  external_id: string;
  url: string;
  linked_at: number;
}

/** Every ticket↔task link for one provider — used by Home's backlog
 *  strip to navigate to an existing task instead of starting a new one. */
export const taskTicketsByProvider = (provider: string) =>
  invoke<TaskTicketRow[]>("task_tickets_by_provider", { provider });

export const taskLinkTicket = (taskId: string, link: TicketLink) =>
  invoke<void>("task_link_ticket", { taskId, link });

export const taskUnlinkTicket = (
  taskId: string,
  provider: string,
  externalId: string,
) =>
  invoke<void>("task_unlink_ticket", { taskId, provider, externalId });

export const taskContextGet = (taskId: string) =>
  invoke<string>("task_context_get", { taskId });

/** v1.1: `content` is interpreted as the NOTES block only. Rust
 *  re-renders the full `.weft/context.md` with the auto block
 *  composed from DB state (prompt, tickets, repos) and your notes
 *  spliced inside the `weft:notes` fence. The auto block was
 *  read-only to the user anyway — this keeps the "what I typed" ==
 *  "what was saved" invariant. */
export const taskContextSet = (taskId: string, content: string) =>
  invoke<void>("task_context_set", { taskId, content });

/** Re-hit each linked ticket's provider and update the cached
 *  title/status columns. Returns the count of rows refreshed.
 *  Wired from the "Refresh titles" button in ContextDialog. */
export const taskRefreshTicketTitles = (taskId: string) =>
  invoke<number>("task_refresh_ticket_titles", { taskId });

// ---------------------------------------------------------------------------
// Project warm-worktree links (v1.0.6)
// ---------------------------------------------------------------------------

export type LinkType = "symlink" | "clone";

export interface ProjectLinkRow {
  project_id: string;
  path: string;
  link_type: LinkType;
}

export interface ProjectLinkInput {
  path: string;
  link_type: LinkType;
}

export interface PresetDescriptor {
  id: string;
  name: string;
  paths: string[];
}

export const projectLinksList = (projectId: string) =>
  invoke<ProjectLinkRow[]>("project_links_list", { projectId });

export const projectLinksSet = (projectId: string, links: ProjectLinkInput[]) =>
  invoke<void>("project_links_set", { projectId, links });

export const projectLinksPresetApply = (projectId: string, presetId: string) =>
  invoke<void>("project_links_preset_apply", { projectId, presetId });

export const projectLinksPresetsList = () =>
  invoke<PresetDescriptor[]>("project_links_presets_list");

export const projectLinksDetectPreset = (path: string) =>
  invoke<string | null>("project_links_detect_preset", { path });

export interface ReapplyResponse {
  worktrees_touched: number;
  worktrees_failed: string[];
}

export const projectLinksReapply = (projectId: string) =>
  invoke<ReapplyResponse>("project_links_reapply", { projectId });

export interface WarmupResponse {
  command: string;
  success: boolean;
  stdout: string;
  stderr: string;
}

export const projectLinksWarmUpMain = (projectId: string) =>
  invoke<WarmupResponse>("project_links_warm_up_main", { projectId });

export type LinkStatus = "ok" | "missing" | "dangling" | "mismatched";

export interface LinkHealth {
  task_id: string;
  worktree_path: string;
  path: string;
  expected_type: LinkType;
  status: LinkStatus;
}

export interface HealthSummary {
  total: number;
  ok: number;
  missing: number;
  dangling: number;
  mismatched: number;
}

export interface HealthResponse {
  rows: LinkHealth[];
  summary: HealthSummary;
}

export const projectLinksHealth = (projectId: string) =>
  invoke<HealthResponse>("project_links_health", { projectId });
