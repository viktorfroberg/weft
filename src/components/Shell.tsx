import { useEffect, useMemo, useRef } from "react";
import { Outlet } from "@tanstack/react-router";
import { Sidebar } from "@/components/Sidebar";
import { Toolbar } from "@/components/Toolbar";
import { ShortcutsOverlay } from "@/components/ShortcutsOverlay";
import { AboutDialog } from "@/components/AboutDialog";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { Onboarding } from "@/components/Onboarding";
import { CommandPalette } from "@/components/CommandPalette";
import { RecentTasksPalette } from "@/components/RecentTasksPalette";
import { TaskComposeOverlay } from "@/components/TaskComposeOverlay";
import { TaskPanelPool } from "@/components/TaskPanelPool";
import { usePrefs } from "@/stores/prefs";
import { useThemeApplier } from "@/lib/theme";
import { onMenuEvent } from "@/lib/menu";
import { useProjects } from "@/stores/projects";
import { useAllTasks } from "@/stores/tasks";
import { useUi } from "@/stores/ui";
import { useQueryClient } from "@tanstack/react-query";
import { qk } from "@/query";
import type { Task, TaskWorktree } from "@/lib/commands";
import { useShortcuts } from "@/lib/shortcuts";
import { notifyTaskWaiting, setDockBadge } from "@/lib/notifications";
import { launchDefaultAgent } from "@/lib/launch-agent";
import { useTerminalTabs } from "@/stores/terminal_tabs";
import { routeKey, useActiveRoute, useNavigateRoute } from "@/lib/active-route";
import { useRenderCount } from "@/lib/dev-trace";

/**
 * Layout shell — the route tree's `__root__` component. Owns the
 * chrome (toolbar + sidebar + overlays) and renders `<Outlet />`
 * where the active route's view goes. Moved out of `App.tsx` so
 * the provider shell stays clean.
 *
 * Structural win vs the old App.tsx: the Outlet doesn't unmount the
 * Toolbar/Sidebar on route change — only the content pane swaps. That
 * matters for input focus (search fields, open palettes) and for
 * avoiding gratuitous remounts of Sidebar's subscription tree.
 */
export function Shell() {
  useRenderCount("Shell", 50);
  const route = useActiveRoute();
  const navigate = useNavigateRoute();

  const sidebarCollapsed = useUi((s) => s.sidebarCollapsed);
  const toggleSidebar = useUi((s) => s.toggleSidebar);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const shortcutsOpen = useUi((s) => s.shortcutsOpen);
  const setShortcutsOpen = useUi((s) => s.setShortcutsOpen);
  const setAboutOpen = useUi((s) => s.setAboutOpen);
  const setChangesPanelVisible = useUi((s) => s.setChangesPanelVisible);
  const changesPanelVisible = useUi((s) => s.changesPanelVisible);
  const setCommandPaletteOpen = useUi((s) => s.setCommandPaletteOpen);
  const setComposeOpen = useUi((s) => s.setComposeOpen);
  const setRecentTasksOpen = useUi((s) => s.setRecentTasksOpen);
  const setFocusRepoId = useUi((s) => s.setFocusRepoId);
  const pushRecentTaskId = usePrefs((s) => s.pushRecentTaskId);
  const queryClient = useQueryClient();

  const { data: projects = [] } = useProjects();
  const allTasks = useAllTasks();
  const hasCompletedOnboarding = usePrefs((s) => s.hasCompletedOnboarding);
  const showOnboarding = !hasCompletedOnboarding && projects.length === 0;

  // Theme + scheme: re-runs `applyTheme(theme, scheme)` atomically
  // whenever either changes — class toggle + chrome CSS vars + Monaco
  // `defineTheme` all land in the same frame.
  useThemeApplier();

  // Track prior task statuses to detect transitions → notify on enter `waiting`.
  const priorStatusRef = useRef<Map<string, string>>(new Map());
  const waitingCount = useMemo(() => {
    let count = 0;
    const priors = priorStatusRef.current;
    const next = new Map<string, string>();
    for (const list of Object.values(allTasks)) {
      for (const t of list) {
        next.set(t.id, t.status);
        if (t.status === "waiting") count++;
        const prior = priors.get(t.id);
        if (prior && prior !== "waiting" && t.status === "waiting") {
          void notifyTaskWaiting(t.name, t.id);
        }
      }
    }
    priorStatusRef.current = next;
    return count;
  }, [allTasks]);

  useEffect(() => {
    void setDockBadge(waitingCount);
  }, [waitingCount]);

  // Native menu → same handlers the keyboard shortcuts use.
  useEffect(() => {
    const unlisten = onMenuEvent((id) => {
      switch (id) {
        case "new_task":
          setComposeOpen(true);
          break;
        case "add_project":
          setAddProjectOpen(true);
          break;
        case "toggle_sidebar":
          toggleSidebar();
          break;
        case "toggle_mode":
          if (route.kind === "task") {
            setChangesPanelVisible(!changesPanelVisible);
          }
          break;
        case "shortcuts":
          setShortcutsOpen(true);
          break;
        case "about":
          setAboutOpen(true);
          break;
        case "back":
          if (route.kind !== "home") navigate({ kind: "home" });
          break;
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [
    route,
    navigate,
    setComposeOpen,
    setAddProjectOpen,
    toggleSidebar,
    setShortcutsOpen,
    setAboutOpen,
    setChangesPanelVisible,
    changesPanelVisible,
  ]);

  // Keyboard shortcuts — global app actions.
  useShortcuts(
    useMemo(
      () => [
        {
          key: "b",
          meta: true,
          description: "Toggle sidebar",
          handler: () => toggleSidebar(),
        },
        {
          key: "p",
          meta: true,
          description: "Add a repo",
          handler: () => setAddProjectOpen(true),
        },
        {
          key: "/",
          meta: true,
          description: "Show keyboard shortcuts",
          handler: () => setShortcutsOpen(true),
        },
        {
          key: "\\",
          meta: true,
          description: "Toggle changes panel (in task view)",
          handler: () => {
            if (route.kind === "task") {
              setChangesPanelVisible(!changesPanelVisible);
            }
          },
        },
        {
          key: "l",
          meta: true,
          description: "Launch default agent (in task view)",
          handler: () => {
            if (route.kind === "task") {
              void launchDefaultAgent(route.id);
            }
          },
        },
        {
          key: "t",
          meta: true,
          description: "New terminal tab (in task view) — picker",
          handler: () => {
            if (route.kind === "task") {
              useTerminalTabs.getState().requestNewTab(route.id);
            }
          },
        },
        {
          key: "k",
          meta: true,
          description: "Command palette (jump anywhere)",
          handler: () => setCommandPaletteOpen(true),
        },
        {
          key: "n",
          meta: true,
          shift: true,
          description: "New task (compose)",
          handler: () => setComposeOpen(true),
        },
        {
          key: "o",
          meta: true,
          shift: true,
          description: "Recent tasks",
          handler: () => setRecentTasksOpen(true),
        },
        ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({
          key: String(n),
          meta: true,
          description:
            route.kind === "task"
              ? `Focus worktree ${n} in diff panel`
              : `Jump to task ${n}`,
          handler: () => {
            if (route.kind === "task") {
              const worktrees =
                queryClient.getQueryData<TaskWorktree[]>(
                  qk.taskWorktrees(route.id),
                ) ?? [];
              const wt = worktrees[n - 1];
              if (wt) setFocusRepoId(wt.project_id);
              return;
            }
            // v1.0.7: ⌘1-9 outside a task jumps to the Nth task in the
            // flat, status-ranked list. Read from TanStack cache (the
            // sidebar already subscribes to it, so it's warm).
            const flat =
              queryClient.getQueryData<Task[]>(
                [...qk.tasksAll(), "flat"] as const,
              ) ?? [];
            const task = flat[n - 1];
            if (task) navigate({ kind: "task", id: task.id });
          },
        })),
        {
          key: "Escape",
          description: "Back to home",
          handler: () => {
            if (route.kind !== "home") navigate({ kind: "home" });
          },
        },
      ],
      [
        toggleSidebar,
        setAddProjectOpen,
        setShortcutsOpen,
        setChangesPanelVisible,
        changesPanelVisible,
        setCommandPaletteOpen,
        setComposeOpen,
        setRecentTasksOpen,
        navigate,
        route,
        queryClient,
        setFocusRepoId,
      ],
    ),
  );

  // MRU tracking: every time the user lands on a task route, push it
  // to recentTaskIds (prefs-persisted). Powers the ⌘⇧O switcher.
  useEffect(() => {
    if (route.kind === "task") pushRecentTaskId(route.id);
  }, [route, pushRecentTaskId]);

  return (
    <div className="bg-background text-foreground flex h-screen flex-col">
      <Toolbar />

      <div
        className={`grid flex-1 overflow-hidden ${sidebarCollapsed ? "grid-cols-[0_1fr]" : "grid-cols-[240px_1fr]"}`}
      >
        <div className={sidebarCollapsed ? "hidden" : "contents"}>
          <Sidebar />
        </div>
        {/* ErrorBoundary wraps the outlet so a route-component crash
            doesn't nuke the shell. `resetKey` clears the error on nav.
            Two children inside the boundary:
              1. TaskPanelPool — keeps every visited task mounted (PTY +
                 xterm.js scrollback survive home/settings round-trips).
                 Renders nothing when no task has been visited yet.
              2. Outlet — renders Home/Settings/Project routes. Hidden
                 (via `display: none`) when on a task route since the
                 pool is showing that task instead. Outlet stays in the
                 tree so its components don't lose state during a quick
                 task→home→task hop either.
            The outer `key={routeKey(route)}` was retired — it forced a
            full remount on every nav and was the original PTY-killer. */}
        <ErrorBoundary scope="main view" resetKey={routeKey(route)}>
          <div className="relative flex flex-1 flex-col overflow-hidden">
            <TaskPanelPool
              currentTaskId={route.kind === "task" ? route.id : null}
            />
            <div
              className={
                route.kind === "task"
                  ? "hidden"
                  : "animate-in fade-in flex flex-1 flex-col overflow-hidden duration-100"
              }
              key={route.kind === "task" ? "task" : routeKey(route)}
            >
              <Outlet />
            </div>
          </div>
        </ErrorBoundary>
      </div>
      <ShortcutsOverlay open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
      <AboutDialog />
      <CommandPalette />
      <RecentTasksPalette />
      <TaskComposeOverlay />
      {showOnboarding && <Onboarding />}
    </div>
  );
}
