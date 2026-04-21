import { useCallback } from "react";
import { useNavigate, useRouterState } from "@tanstack/react-router";

/**
 * Compatibility layer over TanStack Router that exposes the old
 * `Route` discriminated-union shape the rest of the app was written
 * against. Lets pass 2 migrate call sites mechanically without
 * rewriting every nav boundary to the TanStack Router primitives at
 * once.
 *
 * New code should prefer `useNavigate()` + typed `<Link to="/tasks/$taskId">`
 * directly; this helper exists to keep the diff contained.
 */

/** v1.0.7: the `workspace` route kind is gone — workspaces are now
 * repo groups living under Settings, not primary nav. Kept as a type
 * alias temporarily for defensive nav calls; any `{ kind: "workspace" }`
 * now redirects to `/`. */
export type Route =
  | { kind: "home" }
  | { kind: "workspace"; id: string }
  | { kind: "task"; id: string }
  | { kind: "project"; id: string }
  | { kind: "settings" };

/** Derive the active route from the current pathname. */
export function useActiveRoute(): Route {
  return useRouterState({
    select: (s) => pathnameToRoute(s.location.pathname),
  });
}

function pathnameToRoute(pathname: string): Route {
  if (pathname.startsWith("/tasks/")) {
    const id = pathname.slice("/tasks/".length);
    if (id) return { kind: "task", id };
  }
  if (pathname.startsWith("/projects/")) {
    const id = pathname.slice("/projects/".length);
    if (id) return { kind: "project", id };
  }
  if (pathname === "/settings") return { kind: "settings" };
  return { kind: "home" };
}

/**
 * Drop-in replacement for the old `navigate(route)` signature. Returns
 * a stable function that dispatches to the TanStack Router navigator.
 */
export function useNavigateRoute(): (route: Route) => void {
  const navigate = useNavigate();
  return useCallback(
    (route: Route) => {
      switch (route.kind) {
        case "home":
          void navigate({ to: "/" });
          return;
        case "workspace":
          // v1.0.7: workspaces are no longer a navigation target.
          // Any lingering call site redirects to home.
          void navigate({ to: "/" });
          return;
        case "task":
          void navigate({
            to: "/tasks/$taskId",
            params: { taskId: route.id },
          });
          return;
        case "project":
          void navigate({
            to: "/projects/$projectId",
            params: { projectId: route.id },
          });
          return;
        case "settings":
          void navigate({ to: "/settings" });
          return;
      }
    },
    [navigate],
  );
}

/** Stable key for keyed-remount animations: changes iff route kind or id changes. */
export function routeKey(route: Route): string {
  switch (route.kind) {
    case "home":
    case "settings":
      return route.kind;
    case "workspace":
    case "task":
    case "project":
      return `${route.kind}:${route.id}`;
  }
}
