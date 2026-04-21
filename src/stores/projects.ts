import { useQuery } from "@tanstack/react-query";
import { projectsList, type Project } from "@/lib/commands";
import { qk } from "@/query";

// Kept module name for callers' ergonomics. What was a Zustand store is
// now a TanStack Query hook — same semantic ("give me the project list"),
// but cache invalidation now comes from the db_event bridge instead of
// per-store refetch methods.

/** Returns the current project list + query metadata. Access the data
 *  directly: `const { data: projects = [] } = useProjects();` */
export function useProjects() {
  return useQuery<Project[]>({
    queryKey: qk.projects(),
    queryFn: projectsList,
  });
}

export type { Project };
