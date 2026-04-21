import { useQueries, useQuery } from "@tanstack/react-query";
import {
  workspaceReposList,
  workspacesList,
  type Workspace,
  type WorkspaceRepo,
} from "@/lib/commands";
import { qk } from "@/query";

export function useWorkspaces() {
  return useQuery<Workspace[]>({
    queryKey: qk.workspaces(),
    queryFn: workspacesList,
  });
}

/** Repos attached to a single workspace. */
export function useWorkspaceRepos(workspaceId: string) {
  return useQuery<WorkspaceRepo[]>({
    queryKey: qk.workspaceRepos(workspaceId),
    queryFn: () => workspaceReposList(workspaceId),
    enabled: !!workspaceId,
  });
}

/**
 * Repos across every known workspace, returned as `Record<wsId, repos>`
 * to match the old `byWorkspaceId` shape. Used by views that list
 * repos alongside every workspace (WorkspaceView repo chip preview).
 */
export function useAllWorkspaceRepos(): Record<string, WorkspaceRepo[]> {
  const { data: workspaces = [] } = useWorkspaces();
  return useQueries({
    queries: workspaces.map((ws) => ({
      queryKey: qk.workspaceRepos(ws.id),
      queryFn: () => workspaceReposList(ws.id),
    })),
    combine: (results) => {
      const byId: Record<string, WorkspaceRepo[]> = {};
      workspaces.forEach((ws, i) => {
        byId[ws.id] = results[i]?.data ?? [];
      });
      return byId;
    },
  });
}

export type { Workspace, WorkspaceRepo };
