import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  createHashHistory,
} from "@tanstack/react-router";
import { Shell } from "@/components/Shell";
import { Home } from "@/components/Home";
import { ProjectView } from "@/components/ProjectView";
import { SettingsView } from "@/components/SettingsView";

/**
 * Route tree — code-based because the app has 4 routes and a full
 * file-based setup with codegen is ceremony. Hash history because weft
 * is a Tauri WebView, not a browser; there's no "paste this URL" use
 * case and hash-mode avoids any history-mode weirdness.
 *
 * Layout: `__root__` renders `<Shell />` which owns the toolbar +
 * sidebar + global overlays and contains `<Outlet />` where the per-
 * route view renders. This means the Shell no longer remounts on every
 * nav — a structural win over the old `route.kind === "X" && <X/>` fan
 * in App.tsx.
 */

const rootRoute = createRootRoute({
  component: Shell,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Home,
});

// Task route deliberately renders nothing. The actual `<TaskView />` lives
// in `<TaskPanelPool />` mounted inside Shell, which keeps every visited
// task's terminal + PTY alive across navigations. Outlet would unmount on
// route change and kill the PTY (Terminal.tsx cleanup → terminalKill).
const taskRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "tasks/$taskId",
  component: () => null,
});

const projectRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "projects/$projectId",
  component: function ProjectRouteView() {
    const { projectId } = projectRoute.useParams();
    return <ProjectView projectId={projectId} />;
  },
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "settings",
  component: SettingsView,
});

const routeTree = rootRoute.addChildren([
  homeRoute,
  taskRoute,
  projectRoute,
  settingsRoute,
]);

export const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: false,
});

// Type augmentation — gives `<Link to="/tasks/$taskId">` etc. full
// autocompletion + param checking without an external codegen step.
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

// Re-export the Outlet so the Shell component can render children.
export { Outlet };
