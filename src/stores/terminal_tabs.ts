import { create } from "zustand";

export type TerminalTabKind = "shell" | "agent";

export interface TerminalTab {
  /** Stable id, used as React key + map key. */
  id: string;
  kind: TerminalTabKind;
  /** Display label (e.g. "Shell", "Claude Code"). */
  label: string;
  /** Preset id if kind === "agent". Null for the default shell. */
  presetId?: string;
  /** Task this tab belongs to. */
  taskId: string;
  /** PTY session id assigned by Rust after `spawn()` resolves. Set by
   *  `Terminal.tsx` once the IPC call returns so the tab strip can
   *  correlate exit events (`pty_exit`) with the right tab. */
  sessionId?: string;
}

interface TerminalTabsState {
  /** Tabs per task id, keyed so closing a task view doesn't clobber others. */
  byTaskId: Record<string, TerminalTab[]>;
  /** Active tab id per task. Unknown tasks → null (default = first). */
  activeByTaskId: Record<string, string>;
  /** Task id whose "new tab" picker is currently requested open. Set by
   *  the ⌘T shortcut in `Shell.tsx` and the `+` button in
   *  `TerminalTabStrip`; the strip renders its picker dialog when this
   *  matches its own task id. `null` = closed. */
  newTabOpenForTask: string | null;

  /** Seed the default shell tab if the task has none yet. Idempotent. */
  ensureDefaults: (taskId: string) => void;

  /** Add a tab and activate it. */
  addTab: (tab: TerminalTab) => void;

  /** Close a tab. If it was active, activate the previous one. */
  closeTab: (taskId: string, tabId: string) => void;

  /** Switch active tab. */
  setActive: (taskId: string, tabId: string) => void;

  /** Called by `Terminal.tsx` once Rust returns a session id. Lets
   *  tab-strip UI correlate exits (`pty_exit` events) with tabs. */
  setTabSessionId: (taskId: string, tabId: string, sessionId: string) => void;

  /** Open/close the new-tab picker. Gated by task id so background task
   *  views don't pop their own dialogs. */
  requestNewTab: (taskId: string | null) => void;
}

const SHELL_TAB_ID = (taskId: string) => `${taskId}:shell`;

export const useTerminalTabs = create<TerminalTabsState>((set, get) => ({
  byTaskId: {},
  activeByTaskId: {},
  newTabOpenForTask: null,

  ensureDefaults: (taskId) => {
    const existing = get().byTaskId[taskId];
    if (existing && existing.length > 0) return;
    const shell: TerminalTab = {
      id: SHELL_TAB_ID(taskId),
      kind: "shell",
      label: "Shell",
      taskId,
    };
    set((s) => ({
      byTaskId: { ...s.byTaskId, [taskId]: [shell] },
      activeByTaskId: { ...s.activeByTaskId, [taskId]: shell.id },
    }));
  },

  addTab: (tab) =>
    set((s) => {
      const existing = s.byTaskId[tab.taskId] ?? [];
      return {
        byTaskId: {
          ...s.byTaskId,
          [tab.taskId]: [...existing, tab],
        },
        activeByTaskId: { ...s.activeByTaskId, [tab.taskId]: tab.id },
      };
    }),

  closeTab: (taskId, tabId) =>
    set((s) => {
      const list = s.byTaskId[taskId] ?? [];
      const idx = list.findIndex((t) => t.id === tabId);
      if (idx === -1) return {};
      const nextList = list.filter((t) => t.id !== tabId);

      // Pick a new active if we just closed the active tab.
      let activeId = s.activeByTaskId[taskId];
      if (activeId === tabId) {
        const nextActive = nextList[Math.max(0, idx - 1)];
        activeId = nextActive?.id ?? "";
      }

      return {
        byTaskId: { ...s.byTaskId, [taskId]: nextList },
        activeByTaskId: { ...s.activeByTaskId, [taskId]: activeId },
      };
    }),

  setActive: (taskId, tabId) =>
    set((s) => ({
      activeByTaskId: { ...s.activeByTaskId, [taskId]: tabId },
    })),

  setTabSessionId: (taskId, tabId, sessionId) =>
    set((s) => {
      const list = s.byTaskId[taskId] ?? [];
      return {
        byTaskId: {
          ...s.byTaskId,
          [taskId]: list.map((t) =>
            t.id === tabId ? { ...t, sessionId } : t,
          ),
        },
      };
    }),

  requestNewTab: (taskId) => set({ newTabOpenForTask: taskId }),
}));
