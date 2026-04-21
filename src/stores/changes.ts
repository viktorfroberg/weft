import { useQuery, useQueryClient } from "@tanstack/react-query";
import { taskChangesByRepo, type RepoChanges } from "@/lib/commands";
import { qk } from "@/query";

/** Per-repo changes for a task. `isLoading` is true during the first
 *  fetch only; subsequent invalidations (e.g. after a commit) use
 *  `isFetching`. Callers previously checked a `loading` flag — most
 *  of them should now read `isFetching` instead. */
export function useChanges(taskId: string) {
  return useQuery<RepoChanges[]>({
    queryKey: qk.changes(taskId),
    queryFn: () => taskChangesByRepo(taskId),
    enabled: !!taskId,
  });
}

/** Imperative refetch for post-mutation (commit/discard/refresh button). */
export function useRefetchChanges() {
  const qc = useQueryClient();
  return (taskId: string) =>
    qc.invalidateQueries({ queryKey: qk.changes(taskId) });
}

export type { RepoChanges };
