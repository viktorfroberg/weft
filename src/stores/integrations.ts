import { useQuery, useQueryClient } from "@tanstack/react-query";
import { integrationList, type ProviderInfo } from "@/lib/commands";
import { qk } from "@/query";

/** Registered provider integrations + their connection state. */
export function useIntegrations() {
  return useQuery<ProviderInfo[]>({
    queryKey: qk.integrations(),
    queryFn: integrationList,
  });
}

/** Imperative refetch trigger — call after connect/disconnect actions
 *  that the db_event bus doesn't cover (integrations aren't DB rows).
 *  Returns a stable handle so callers can pass it to child components. */
export function useRefetchIntegrations() {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: qk.integrations() });
}

export type { ProviderInfo };
