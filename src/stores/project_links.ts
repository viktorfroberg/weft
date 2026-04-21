import { useQuery } from "@tanstack/react-query";
import {
  projectLinksDetectPreset,
  projectLinksHealth,
  projectLinksList,
  projectLinksPresetsList,
  type HealthResponse,
  type PresetDescriptor,
  type ProjectLinkRow,
} from "@/lib/commands";
import { qk } from "@/query";

/** Per-project warm-worktree link list. Invalidated by the `db_event`
 * bridge on `project_link` events. */
export function useProjectLinks(projectId: string | null) {
  return useQuery<ProjectLinkRow[]>({
    queryKey: projectId
      ? qk.projectLinks(projectId)
      : ([...qk.projectLinksAll(), "disabled"] as const),
    queryFn: () => projectLinksList(projectId as string),
    enabled: !!projectId,
  });
}

/** Static preset catalog. Loaded once per session (infinite cache —
 * it's a constant on the Rust side). */
export function useProjectLinkPresets() {
  return useQuery<PresetDescriptor[]>({
    queryKey: qk.projectLinkPresets(),
    queryFn: () => projectLinksPresetsList(),
  });
}

/** Pre-flight detection — call on AddProjectDialog when the user
 * picks a repo path, before the project row exists. */
export function detectPresetForPath(path: string): Promise<string | null> {
  return projectLinksDetectPreset(path);
}

/** Per-project warm-link health. Stat-checks every link across every
 * active worktree for a project; returns per-row status + aggregate
 * summary. Invalidated alongside project_link writes by the db_event
 * bridge. */
export function useProjectLinkHealth(projectId: string | null) {
  return useQuery<HealthResponse>({
    queryKey: projectId
      ? [...qk.projectLinks(projectId), "health"]
      : (["projectLinks", "health", "disabled"] as const),
    queryFn: () => projectLinksHealth(projectId as string),
    enabled: !!projectId,
  });
}
