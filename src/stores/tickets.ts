import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  taskTicketsList,
  ticketGet,
  ticketListBacklog,
  type Ticket,
  type TicketLink,
} from "@/lib/commands";
import { qk } from "@/query";

/** Ticket links persisted on a task. */
export function useTaskTickets(taskId: string) {
  return useQuery<TicketLink[]>({
    queryKey: qk.taskTickets(taskId),
    queryFn: () => taskTicketsList(taskId),
    enabled: !!taskId,
  });
}

/** Imperative refetch of task ticket links. Call after link/unlink
 *  mutations that the db_event bus doesn't cover with enough specificity. */
export function useRefetchTaskTickets() {
  const qc = useQueryClient();
  return (taskId: string) =>
    qc.invalidateQueries({ queryKey: qk.taskTickets(taskId) });
}

/** Provider backlog — live-fetched, 30s TTL handled Rust-side. */
export function useTicketBacklog(providerId: string, enabled = true) {
  return useQuery<Ticket[]>({
    queryKey: qk.ticketBacklog(providerId),
    queryFn: () => ticketListBacklog(providerId),
    enabled: enabled && !!providerId,
  });
}

/**
 * Live fetch for a single ticket — backs the TaskView chip rendering.
 * Returns `data === null` when the ticket is unavailable (deleted,
 * unauthorized, offline) so the chip can degrade to ID-only without
 * a thrown error.
 */
export function useTicketLive(
  providerId: string,
  externalId: string,
  enabled = true,
) {
  return useQuery<Ticket | null>({
    queryKey: qk.ticket(providerId, externalId),
    queryFn: async () => {
      try {
        return await ticketGet(providerId, externalId);
      } catch {
        return null;
      }
    },
    enabled: enabled && !!providerId && !!externalId,
    // Live titles change upstream — give them a short stale window
    // (separate from the global `Infinity` default) so the 60s poll +
    // focus refetch in TaskView actually trigger a request.
    staleTime: 30_000,
    refetchOnWindowFocus: true,
  });
}

export type { Ticket, TicketLink };
