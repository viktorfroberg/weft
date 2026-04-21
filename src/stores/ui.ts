import { create } from "zustand";
import type { TaskStatus, Ticket } from "@/lib/commands";

const EMPTY_PROJECTS = new Set<string>();
const ALL_STATUSES: Set<TaskStatus> = new Set([
  "working",
  "waiting",
  "idle",
  "error",
  "done",
]);

/** UI-only state: sidebar visibility, dialog open flags, etc. */
interface UiState {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  setSidebarCollapsed: (v: boolean) => void;

  createWorkspaceOpen: boolean;
  setCreateWorkspaceOpen: (v: boolean) => void;

  addProjectOpen: boolean;
  setAddProjectOpen: (v: boolean) => void;

  createTaskOpen: boolean;
  setCreateTaskOpen: (v: boolean) => void;

  shortcutsOpen: boolean;
  setShortcutsOpen: (v: boolean) => void;

  /** Custom About dialog (⌘I-less; opened by the native `weft → About weft`
   *  menu item since we override Tauri's default about panel). */
  aboutOpen: boolean;
  setAboutOpen: (v: boolean) => void;

  /** ⌘K global fuzzy-search palette. */
  commandPaletteOpen: boolean;
  setCommandPaletteOpen: (v: boolean) => void;

  /** ⌘⇧O recent-tasks switcher. Narrow palette over `prefs.recentTaskIds`. */
  recentTasksOpen: boolean;
  setRecentTasksOpen: (v: boolean) => void;

  /** Whether the right-side ChangesPanel (review surface) is visible
   *  in the task view. Hidden = terminal takes the whole workspace;
   *  shown = the panel reappears at its remembered width. Per-user,
   *  not per-task. Defaults to shown. */
  changesPanelVisible: boolean;
  setChangesPanelVisible: (v: boolean) => void;

  /** When set, ChangesPanel will scroll the repo with this project_id
   *  into view on its next render and then reset the flag. Powers the
   *  ⌘1–⌘9 worktree-focus shortcut. `null` = no focus request pending. */
  focusRepoId: string | null;
  setFocusRepoId: (v: string | null) => void;

  // v1.0.7 --------------------------------------------------------------

  /** Floating task-compose overlay (⌘⇧N from anywhere). */
  composeOpen: boolean;
  setComposeOpen: (v: boolean) => void;

  /** Transient: a ticket the next-mounted/already-mounted TaskComposeCard
   *  should pre-attach (and seed the prompt with, if empty). Set by Home's
   *  backlog strip; consumer reads it once and clears it back to null. */
  composePrefillTicket: Ticket | null;
  setComposePrefillTicket: (t: Ticket | null) => void;

  /** Transient: a `provider:external_id` key the inline TaskComposeCard
   *  should detach. Set by Home's backlog strip when the user clicks an
   *  already-attached card (toggle-off). Consumer reads + clears. */
  composeDetachTicketKey: string | null;
  setComposeDetachTicketKey: (k: string | null) => void;

  /** Live mirror of the *inline* TaskComposeCard's attached ticket keys.
   *  Written by the card on every tickets-state change; read by Home's
   *  backlog strip to render which cards are currently attached. The
   *  floating compose overlay does NOT mirror — Home's strip pairs with
   *  the inline card only. */
  composeAttachedTicketKeys: Set<string>;
  setComposeAttachedTicketKeys: (s: Set<string>) => void;

  /** Transient: the tab id SettingsView should switch to on its next
   *  render (and then immediately clear). Set by deep-link callers like
   *  Home's "Scope" affordance; SettingsView reads it once. Kept loose
   *  (string) so this file doesn't have to import the Settings tab union. */
  pendingSettingsTab: string | null;
  setPendingSettingsTab: (v: string | null) => void;

  /** Sidebar task filters. Session-only — resets on restart.
   * `projects` is an empty set = "All projects"; otherwise only tasks
   * touching at least one of the listed project_ids are shown.
   * `statuses` is the inclusive status set; default = all. */
  taskFilterProjects: Set<string>;
  taskFilterStatuses: Set<TaskStatus>;
  setTaskFilterProjects: (v: Set<string>) => void;
  setTaskFilterStatuses: (v: Set<TaskStatus>) => void;
  toggleTaskFilterProject: (projectId: string) => void;
  toggleTaskFilterStatus: (status: TaskStatus) => void;
  clearTaskFilters: () => void;
}

export const useUi = create<UiState>((set) => ({
  sidebarCollapsed: false,
  toggleSidebar: () =>
    set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  setSidebarCollapsed: (v) => set({ sidebarCollapsed: v }),

  createWorkspaceOpen: false,
  setCreateWorkspaceOpen: (v) => set({ createWorkspaceOpen: v }),

  addProjectOpen: false,
  setAddProjectOpen: (v) => set({ addProjectOpen: v }),

  createTaskOpen: false,
  setCreateTaskOpen: (v) => set({ createTaskOpen: v }),

  shortcutsOpen: false,
  setShortcutsOpen: (v) => set({ shortcutsOpen: v }),

  aboutOpen: false,
  setAboutOpen: (v) => set({ aboutOpen: v }),

  commandPaletteOpen: false,
  setCommandPaletteOpen: (v) => set({ commandPaletteOpen: v }),

  recentTasksOpen: false,
  setRecentTasksOpen: (v) => set({ recentTasksOpen: v }),

  changesPanelVisible: true,
  setChangesPanelVisible: (changesPanelVisible) => set({ changesPanelVisible }),

  focusRepoId: null,
  setFocusRepoId: (v) => set({ focusRepoId: v }),

  composeOpen: false,
  setComposeOpen: (composeOpen) => set({ composeOpen }),

  composePrefillTicket: null,
  setComposePrefillTicket: (composePrefillTicket) =>
    set({ composePrefillTicket }),

  composeDetachTicketKey: null,
  setComposeDetachTicketKey: (composeDetachTicketKey) =>
    set({ composeDetachTicketKey }),

  composeAttachedTicketKeys: new Set<string>(),
  setComposeAttachedTicketKeys: (composeAttachedTicketKeys) =>
    set({ composeAttachedTicketKeys }),

  pendingSettingsTab: null,
  setPendingSettingsTab: (pendingSettingsTab) =>
    set({ pendingSettingsTab }),

  taskFilterProjects: EMPTY_PROJECTS,
  taskFilterStatuses: ALL_STATUSES,
  setTaskFilterProjects: (taskFilterProjects) => set({ taskFilterProjects }),
  setTaskFilterStatuses: (taskFilterStatuses) => set({ taskFilterStatuses }),
  toggleTaskFilterProject: (projectId) =>
    set((s) => {
      const next = new Set(s.taskFilterProjects);
      if (next.has(projectId)) next.delete(projectId);
      else next.add(projectId);
      return { taskFilterProjects: next };
    }),
  toggleTaskFilterStatus: (status) =>
    set((s) => {
      const next = new Set(s.taskFilterStatuses);
      if (next.has(status)) next.delete(status);
      else next.add(status);
      return { taskFilterStatuses: next };
    }),
  clearTaskFilters: () =>
    set({
      taskFilterProjects: EMPTY_PROJECTS,
      taskFilterStatuses: ALL_STATUSES,
    }),
}));
