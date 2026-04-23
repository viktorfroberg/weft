import { create } from "zustand";
import {
  tabCreate,
  tabDelete,
  tabList,
  terminalShutdownGraceful,
  type TabKind,
  type TabState,
  type TerminalTabRow,
} from "@/lib/commands";
import { onDbEvent } from "@/lib/events";
import { usePtyExits } from "@/stores/pty_exits";

export type TerminalTabKind = TabKind;

export interface TerminalTab {
  /** Stable id, used as React key + map key. Matches the SQLite row. */
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
  /** Persistent lifecycle state. `live` = PTY running (or about to
   *  start). `dormant` = PTY exited; `Terminal.tsx` renders the
   *  scrollback replay + resume banner. */
  state: TabState;
  /** Optional cwd captured for shell-tab respawn after dormant. */
  cwd?: string;
}

interface TerminalTabsState {
  /** Tabs per task id, keyed so closing a task view doesn't clobber others. */
  byTaskId: Record<string, TerminalTab[]>;
  /** Active tab id per task. Unknown tasks → null (default = first). */
  activeByTaskId: Record<string, string>;
  /** Task id whose "new tab" picker is currently requested open. */
  newTabOpenForTask: string | null;
  /** Task ids we've already hydrated from SQLite this session. Prevents
   *  repeated `tabList` round-trips as components mount/unmount. */
  hydratedTaskIds: Record<string, true>;

  /** Load persisted tabs for a task and seed the store. Idempotent. */
  hydrate: (taskId: string) => Promise<void>;

  /** Ensure the task has at least a default shell tab. Creates the row
   *  in SQLite on first call. Idempotent. Awaits the round-trip. */
  ensureDefaults: (taskId: string) => Promise<void>;

  /** Optimistically add a tab and activate it. Writes to SQLite in the
   *  background; the subsequent db_event refetch confirms the row. */
  addTab: (tab: Omit<TerminalTab, "state"> & { state?: TabState; cwd?: string }) => void;

  /** Soft-close path: graceful PTY shutdown → row flips to dormant via
   *  the Rust waiter. Returns once the graceful shutdown resolves.
   *  Leaves the tab in the strip. */
  softCloseTab: (taskId: string, tabId: string) => Promise<void>;

  /** Hard-close path: graceful PTY shutdown, THEN delete the row. The
   *  tab disappears from the strip. */
  closeTab: (taskId: string, tabId: string) => Promise<void>;

  /** Switch active tab. */
  setActive: (taskId: string, tabId: string) => void;

  /** Called by `Terminal.tsx` once Rust returns a session id. */
  setTabSessionId: (taskId: string, tabId: string, sessionId: string) => void;

  /** Open/close the new-tab picker. */
  requestNewTab: (taskId: string | null) => void;

  /** Apply a fresh tabList to the store for the given task. Called from
   *  the db_event subscriber when an Entity::TerminalTab write lands. */
  refetch: (taskId: string) => Promise<void>;
}

/** Module-level dedupe map. Keyed by task id; value is the in-flight
 *  fetch promise so concurrent callers all await the same fetch. */
const inflightHydrates = new Map<string, Promise<void>>();

function rowToTab(row: TerminalTabRow, prev?: TerminalTab): TerminalTab {
  return {
    id: row.id,
    kind: row.kind,
    label: row.label,
    presetId: row.preset_id ?? undefined,
    taskId: row.task_id,
    state: row.state,
    cwd: row.cwd ?? undefined,
    // Preserve transient sessionId across refetches — SQLite doesn't
    // track it.
    sessionId: prev?.sessionId,
  };
}

export const useTerminalTabs = create<TerminalTabsState>((set, get) => ({
  byTaskId: {},
  activeByTaskId: {},
  newTabOpenForTask: null,
  hydratedTaskIds: {},

  hydrate: async (taskId) => {
    if (get().hydratedTaskIds[taskId]) return;
    // Dedupe concurrent hydrate calls. Without this, a second caller
    // (TerminalTabStrip's useEffect re-firing under React StrictMode,
    // or another component triggering hydrate) sees no
    // `hydratedTaskIds` flag yet, kicks off ITS OWN tabList fetch,
    // and proceeds to `ensureDefaults` before the first fetch
    // resolves. Both then see `byTaskId[taskId] === undefined` and
    // each create a fresh default shell tab → on app reopen, the
    // user gets an extra shell next to their persisted tabs.
    const inflight = inflightHydrates.get(taskId);
    if (inflight) return inflight;
    const promise = (async () => {
      try {
        const rows = await tabList(taskId);
        set((s) => {
          const prev = s.byTaskId[taskId] ?? [];
          const prevById = new Map(prev.map((t) => [t.id, t]));
          const tabs = rows.map((r) => rowToTab(r, prevById.get(r.id)));
          const active = s.activeByTaskId[taskId];
          const activeStillExists = tabs.some((t) => t.id === active);
          return {
            byTaskId: { ...s.byTaskId, [taskId]: tabs },
            activeByTaskId: {
              ...s.activeByTaskId,
              [taskId]: activeStillExists ? active : tabs[0]?.id ?? "",
            },
            // Set the flag ONLY after the fetch + state write
            // succeed. Concurrent callers awaiting the same promise
            // proceed with the populated state.
            hydratedTaskIds: { ...s.hydratedTaskIds, [taskId]: true },
          };
        });
      } catch (err) {
        console.warn("tabList hydrate failed", err);
      } finally {
        inflightHydrates.delete(taskId);
      }
    })();
    inflightHydrates.set(taskId, promise);
    return promise;
  },

  refetch: async (taskId) => {
    try {
      const rows = await tabList(taskId);
      set((s) => {
        const prev = s.byTaskId[taskId] ?? [];
        const prevById = new Map(prev.map((t) => [t.id, t]));
        const tabs = rows.map((r) => rowToTab(r, prevById.get(r.id)));
        // Active handling: if current active is gone, fall back to the
        // first tab. Otherwise leave it.
        const active = s.activeByTaskId[taskId];
        const activeStillExists = tabs.some((t) => t.id === active);
        return {
          byTaskId: { ...s.byTaskId, [taskId]: tabs },
          activeByTaskId: {
            ...s.activeByTaskId,
            [taskId]: activeStillExists ? active : tabs[0]?.id ?? "",
          },
        };
      });
    } catch (err) {
      console.warn("tabList refetch failed", err);
    }
  },

  ensureDefaults: async (taskId) => {
    await get().hydrate(taskId);
    const existing = get().byTaskId[taskId];
    if (existing && existing.length > 0) return;
    try {
      const row = await tabCreate({
        task_id: taskId,
        kind: "shell",
        label: "Shell",
      });
      set((s) => {
        const list = s.byTaskId[taskId] ?? [];
        if (list.some((t) => t.id === row.id)) return {};
        return {
          byTaskId: { ...s.byTaskId, [taskId]: [...list, rowToTab(row)] },
          activeByTaskId: { ...s.activeByTaskId, [taskId]: row.id },
        };
      });
    } catch (err) {
      console.warn("ensureDefaults: tabCreate failed", err);
    }
  },

  addTab: (tab) => {
    const state: TabState = tab.state ?? "live";
    const optimistic: TerminalTab = {
      ...tab,
      state,
      sessionId: tab.sessionId,
      cwd: tab.cwd,
    };
    set((s) => {
      const existing = s.byTaskId[tab.taskId] ?? [];
      if (existing.some((t) => t.id === tab.id)) return {};
      return {
        byTaskId: { ...s.byTaskId, [tab.taskId]: [...existing, optimistic] },
        activeByTaskId: { ...s.activeByTaskId, [tab.taskId]: tab.id },
      };
    });
    // Write-through. tabCreate assigns its own id server-side, but we
    // honor the client-supplied id for round-trip consistency so the
    // optimistic row and the DB row have the same primary key.
    tabCreate({
      task_id: tab.taskId,
      kind: tab.kind,
      label: tab.label,
      preset_id: tab.presetId ?? null,
      cwd: tab.cwd ?? null,
    })
      .then((row) => {
        // Replace the optimistic row's id with the server-assigned one.
        // We do this by patching the entry whose label/kind match and
        // whose id is ephemeral (not yet confirmed).
        set((s) => {
          const list = s.byTaskId[tab.taskId] ?? [];
          const idx = list.findIndex((t) => t.id === tab.id);
          if (idx === -1) return {};
          const next = list.slice();
          const prev = next[idx]!;
          next[idx] = {
            ...prev,
            id: row.id,
            state: row.state,
          };
          const activeId = s.activeByTaskId[tab.taskId];
          return {
            byTaskId: { ...s.byTaskId, [tab.taskId]: next },
            activeByTaskId: {
              ...s.activeByTaskId,
              [tab.taskId]: activeId === tab.id ? row.id : activeId,
            },
          };
        });
      })
      .catch((err) => {
        console.warn("tabCreate failed, rolling back optimistic tab", err);
        set((s) => {
          const list = s.byTaskId[tab.taskId] ?? [];
          return {
            byTaskId: {
              ...s.byTaskId,
              [tab.taskId]: list.filter((t) => t.id !== tab.id),
            },
          };
        });
      });
  },

  softCloseTab: async (taskId, tabId) => {
    const tab = (get().byTaskId[taskId] ?? []).find((t) => t.id === tabId);
    if (!tab) return;
    if (!tab.sessionId) return;
    // Tell pty_exits this exit is user-initiated so the toast doesn't
    // surface as a "crash". SIGHUP exits with code 1; without this
    // marker every soft-close would yell at the user.
    usePtyExits.getState().markExpectedExit(tab.sessionId);
    try {
      await terminalShutdownGraceful(tab.sessionId);
    } catch (err) {
      console.warn("shutdown_graceful failed", err);
    }
  },

  closeTab: async (taskId, tabId) => {
    const tab = (get().byTaskId[taskId] ?? []).find((t) => t.id === tabId);
    if (!tab) return;
    // Optimistically remove so the UI is responsive; if the backend
    // call fails we refetch + re-show.
    set((s) => {
      const list = s.byTaskId[taskId] ?? [];
      const nextList = list.filter((t) => t.id !== tabId);
      const active = s.activeByTaskId[taskId];
      let nextActive = active;
      if (active === tabId) {
        const origIdx = list.findIndex((t) => t.id === tabId);
        nextActive = nextList[Math.max(0, origIdx - 1)]?.id ?? "";
      }
      return {
        byTaskId: { ...s.byTaskId, [taskId]: nextList },
        activeByTaskId: { ...s.activeByTaskId, [taskId]: nextActive },
      };
    });
    try {
      if (tab.sessionId && tab.state === "live") {
        usePtyExits.getState().markExpectedExit(tab.sessionId);
        await terminalShutdownGraceful(tab.sessionId);
      }
      await tabDelete(tab.id);
    } catch (err) {
      console.warn("closeTab failed, refetching", err);
      await get().refetch(taskId);
    }
  },

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
            t.id === tabId ? { ...t, sessionId, state: "live" as const } : t,
          ),
        },
      };
    }),

  requestNewTab: (taskId) => set({ newTabOpenForTask: taskId }),
}));

// -- db_event subscription --------------------------------------------------
// Mirror the Rust side: every Entity::TerminalTab write triggers a refetch
// of the tabs for every task the store has hydrated. Runs once per module
// load (module state), matching how other stores in this app subscribe.

let subscribed = false;
export function ensureTerminalTabSubscription() {
  if (subscribed) return;
  subscribed = true;
  onDbEvent((ev) => {
    if (ev.entity !== "terminal_tab") return;
    const store = useTerminalTabs.getState();
    // Refetch every task we've hydrated. Cheap — tabList is one query.
    for (const taskId of Object.keys(store.hydratedTaskIds)) {
      store.refetch(taskId);
    }
  }).catch((err) => {
    subscribed = false;
    console.warn("terminal_tab event subscription failed", err);
  });
}
