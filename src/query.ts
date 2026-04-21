import { QueryClient } from "@tanstack/react-query";

/**
 * App-wide QueryClient. weft reads from a local SQLite via Tauri; there
 * is no flaky network between us and the source of truth, so most of
 * the defaults tuned for REST-over-WAN don't apply. Override:
 *
 * - `staleTime: Infinity` — queries never become stale on their own.
 *   Freshness comes from Rust's `db_event` channel, not wall-clock
 *   heuristics. See `src/lib/db-event-bridge.ts`.
 * - `refetchOnWindowFocus: false` — we already listen to focus/
 *   visibility events where it matters (ticket titles); don't refetch
 *   everything every time the user alt-tabs to a browser.
 * - `retry: 1` — Tauri commands either work or fail fast; retry loops
 *   just compound failures into worse UX.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: Infinity,
      refetchOnWindowFocus: false,
      retry: 1,
    },
    mutations: {
      retry: 0,
    },
  },
});

// ---------------------------------------------------------------------------
// Query-key factory. Centralized so the `db_event` bridge knows exactly
// which prefixes to invalidate, and so a typo in one component doesn't
// silently split a cache into two entries.
// ---------------------------------------------------------------------------

export const qk = {
  projects: () => ["projects"] as const,
  workspaces: () => ["workspaces"] as const,
  workspaceRepos: (workspaceId: string) =>
    ["workspaceRepos", workspaceId] as const,
  workspaceReposAll: () => ["workspaceRepos"] as const,
  tasks: (workspaceId: string) => ["tasks", workspaceId] as const,
  tasksAll: () => ["tasks"] as const,
  taskWorktrees: (taskId: string) => ["taskWorktrees", taskId] as const,
  taskWorktreesAll: () => ["taskWorktrees"] as const,
  taskTickets: (taskId: string) => ["taskTickets", taskId] as const,
  taskTicketsAll: () => ["taskTickets"] as const,
  taskTicketsByProvider: (provider: string) =>
    ["taskTicketsByProvider", provider] as const,
  taskTicketsByProviderAll: () => ["taskTicketsByProvider"] as const,
  changes: (taskId: string) => ["changes", taskId] as const,
  changesAll: () => ["changes"] as const,
  integrations: () => ["integrations"] as const,
  ticketBacklog: (providerId: string) =>
    ["ticketBacklog", providerId] as const,
  ticketBacklogAll: () => ["ticketBacklog"] as const,
  ticket: (providerId: string, externalId: string) =>
    ["ticket", providerId, externalId] as const,
  ticketAll: () => ["ticket"] as const,
  presetDefault: () => ["presetDefault"] as const,
  agentPresets: () => ["agentPresets"] as const,
  projectLinks: (projectId: string) => ["projectLinks", projectId] as const,
  projectLinksAll: () => ["projectLinks"] as const,
  projectLinkPresets: () => ["projectLinkPresets"] as const,
  linearSettings: () => ["linearSettings"] as const,
} as const;
