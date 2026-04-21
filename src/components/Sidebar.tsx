import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, Filter, Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useProjects } from "@/stores/projects";
import { useAllTasksFlat, useTaskProjectIds } from "@/stores/tasks";
import { useIntegrations } from "@/stores/integrations";
import {
  useActiveRoute,
  useNavigateRoute,
  type Route,
} from "@/lib/active-route";
import { useUi } from "@/stores/ui";
import {
  taskTicketsByProvider,
  type Task,
  type TaskStatus,
  type TaskTicketRow,
} from "@/lib/commands";
import { qk } from "@/query";
import { ProjectBadge } from "./ProjectBadge";
import { TaskStatusDot } from "./ui/task-status-dot";
import { AddProjectDialog } from "./AddProjectDialog";

const EMPTY_LINKS: never[] = [];

/**
 * v1.0.7 sidebar. Tasks are primary navigation; workspaces live under
 * Settings → Repo groups. Flat list grouped by status; top filter bar
 * for project + status.
 */
export function Sidebar() {
  const { data: projects = [] } = useProjects();
  const tasks = useAllTasksFlat();
  const route = useActiveRoute();
  const navigate = useNavigateRoute();

  const addProjectOpen = useUi((s) => s.addProjectOpen);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const setComposeOpen = useUi((s) => s.setComposeOpen);
  const filterProjects = useUi((s) => s.taskFilterProjects);
  const filterStatuses = useUi((s) => s.taskFilterStatuses);
  const toggleProject = useUi((s) => s.toggleTaskFilterProject);
  const toggleStatus = useUi((s) => s.toggleTaskFilterStatus);
  const clearFilters = useUi((s) => s.clearTaskFilters);

  // Collapsed by default — the repo list is also available as an inner
  // sub-nav on the project-detail page, so the main sidebar doesn't
  // need to surface it up-front.
  const [projectsExpanded, setProjectsExpanded] = useState(false);
  const [doneExpanded, setDoneExpanded] = useState(false);
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);

  const hasFilters =
    filterProjects.size > 0 || filterStatuses.size < 5;

  return (
    <aside className="bg-sidebar text-sidebar-foreground border-border flex flex-col border-r">
      <div className="flex-1 overflow-y-auto px-2 pt-3 pb-2">
        <section>
          <div className="text-muted-foreground flex items-center justify-between px-2 pt-1 pb-1.5 text-[11px] font-medium uppercase tracking-wider">
            <span>Tasks</span>
            <div className="flex items-center gap-0.5">
              <button
                type="button"
                onClick={() => setFilterMenuOpen((v) => !v)}
                className={`hover:bg-accent text-muted-foreground hover:text-foreground flex h-5 w-5 items-center justify-center rounded transition-colors ${
                  hasFilters ? "text-foreground" : ""
                }`}
                title="Filter"
                aria-label="Filter"
              >
                <Filter size={11} />
              </button>
              <button
                type="button"
                onClick={() => setComposeOpen(true)}
                className="hover:bg-accent text-muted-foreground hover:text-foreground flex h-5 w-5 items-center justify-center rounded transition-colors"
                title="New task  ⌘⇧N"
                aria-label="New task"
              >
                <Plus size={12} />
              </button>
            </div>
          </div>

          {filterMenuOpen && (
            <FilterPanel
              projects={projects}
              activeProjectIds={filterProjects}
              activeStatuses={filterStatuses}
              onToggleProject={toggleProject}
              onToggleStatus={toggleStatus}
              onClear={clearFilters}
            />
          )}

          <FlatTaskList
            tasks={tasks}
            filterProjects={filterProjects}
            filterStatuses={filterStatuses}
            navigate={navigate}
            activeRoute={route}
            doneExpanded={doneExpanded}
            onToggleDone={() => setDoneExpanded((v) => !v)}
            ticketsByTaskId={useTaskTicketsLookup()}
          />
        </section>
      </div>

      <section className="border-border border-t">
        <button
          type="button"
          onClick={() => setProjectsExpanded((v) => !v)}
          className="hover:bg-sidebar-accent text-muted-foreground flex w-full items-center justify-between px-4 py-1.5 text-[11px] font-medium uppercase tracking-wider transition-colors"
        >
          <span className="flex items-center gap-1.5">
            {projectsExpanded ? (
              <ChevronDown size={10} />
            ) : (
              <ChevronRight size={10} />
            )}
            Repos ({projects.length})
          </span>
          <span
            onClick={(e) => {
              e.stopPropagation();
              setAddProjectOpen(true);
            }}
            role="button"
            tabIndex={0}
            title="Add a repo  ⌘P"
            aria-label="Add a repo"
            className="hover:bg-accent hover:text-foreground flex h-5 w-5 cursor-pointer items-center justify-center rounded transition-colors"
          >
            <Plus size={12} />
          </span>
        </button>
        {projectsExpanded && (
          <ul className="max-h-40 space-y-0.5 overflow-y-auto px-2 pb-2">
            {projects.length === 0 ? (
              <li>
                <button
                  type="button"
                  onClick={() => setAddProjectOpen(true)}
                  className="hover:bg-sidebar-accent text-muted-foreground hover:text-foreground flex w-full items-center gap-2 rounded px-1.5 py-1 text-left text-xs transition-colors"
                >
                  <Plus size={12} />
                  <span>Add a repo</span>
                </button>
              </li>
            ) : (
              projects.map((p) => {
                const isActive =
                  route.kind === "project" && route.id === p.id;
                return (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => navigate({ kind: "project", id: p.id })}
                      className={`flex w-full items-center gap-2 rounded px-1.5 py-1 text-left text-sm transition-colors ${
                        isActive
                          ? "bg-sidebar-accent text-sidebar-accent-foreground"
                          : "hover:bg-sidebar-accent"
                      }`}
                      title={p.main_repo_path}
                    >
                      <ProjectBadge name={p.name} color={p.color} size="sm" />
                      <span className="truncate">{p.name}</span>
                    </button>
                  </li>
                );
              })
            )}
          </ul>
        )}
      </section>

      <footer className="border-border flex gap-1 border-t p-2">
        <Button
          variant="ghost"
          size="sm"
          className="flex-1 justify-start text-xs"
          onClick={() => navigate({ kind: "settings" })}
        >
          Settings
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="text-muted-foreground hover:text-foreground px-2 text-xs"
          title="Keyboard shortcuts  ⌘/"
          onClick={() => useUi.getState().setShortcutsOpen(true)}
        >
          ⌘/
        </Button>
      </footer>

      <AddProjectDialog
        open={addProjectOpen}
        onOpenChange={setAddProjectOpen}
      />
    </aside>
  );
}

// ---------------------------------------------------------------------------
// Tickets-per-task lookup. Reuses the same `task_tickets_by_provider`
// command Home's backlog strip uses, so the cache is shared. Builds a
// `Map<task_id, external_ids[]>` once per render so each TaskRow is O(1).
// ---------------------------------------------------------------------------

function useTaskTicketsLookup(): Map<string, string[]> {
  const { data: providers = [] } = useIntegrations();
  const provider = providers.find((p) => p.connected);
  const { data: links = EMPTY_LINKS } = useQuery<TaskTicketRow[]>({
    queryKey: qk.taskTicketsByProvider(provider?.id ?? ""),
    queryFn: () => taskTicketsByProvider(provider!.id),
    enabled: !!provider,
  });
  return useMemo(() => {
    const m = new Map<string, string[]>();
    for (const l of links) {
      const arr = m.get(l.task_id);
      if (arr) arr.push(l.external_id);
      else m.set(l.task_id, [l.external_id]);
    }
    return m;
  }, [links]);
}

// ---------------------------------------------------------------------------
// Filter panel
// ---------------------------------------------------------------------------

const STATUS_OPTIONS: { status: TaskStatus; label: string }[] = [
  { status: "working", label: "Working" },
  { status: "waiting", label: "Waiting" },
  { status: "idle", label: "Idle" },
  { status: "error", label: "Error" },
  { status: "done", label: "Done" },
];

function FilterPanel({
  projects,
  activeProjectIds,
  activeStatuses,
  onToggleProject,
  onToggleStatus,
  onClear,
}: {
  projects: ReturnType<typeof useProjects>["data"] extends (infer U)[] | undefined
    ? U[]
    : never;
  activeProjectIds: Set<string>;
  activeStatuses: Set<TaskStatus>;
  onToggleProject: (id: string) => void;
  onToggleStatus: (s: TaskStatus) => void;
  onClear: () => void;
}) {
  return (
    <div className="border-border mx-1 my-1 space-y-2 rounded-md border p-2 text-xs">
      <div>
        <div className="text-muted-foreground mb-1 text-[10px] uppercase tracking-wider">
          Status
        </div>
        <div className="flex flex-wrap gap-1">
          {STATUS_OPTIONS.map(({ status, label }) => {
            const active = activeStatuses.has(status);
            return (
              <button
                key={status}
                type="button"
                onClick={() => onToggleStatus(status)}
                className={`rounded px-1.5 py-0.5 text-[10px] transition-colors ${
                  active
                    ? "bg-accent text-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
              >
                {label}
              </button>
            );
          })}
        </div>
      </div>
      <div>
        <div className="text-muted-foreground mb-1 text-[10px] uppercase tracking-wider">
          Repos
        </div>
        <div className="flex flex-wrap gap-1">
          {projects && projects.length === 0 ? (
            <span className="text-muted-foreground/60 text-[10px]">
              none registered
            </span>
          ) : (
            projects?.map((p) => {
              const active = activeProjectIds.has(p.id);
              return (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => onToggleProject(p.id)}
                  className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] transition-colors ${
                    active
                      ? "bg-accent text-foreground"
                      : "text-muted-foreground hover:bg-muted hover:text-foreground"
                  }`}
                >
                  <ProjectBadge name={p.name} color={p.color} size="sm" />
                  {p.name}
                </button>
              );
            })
          )}
        </div>
      </div>
      <button
        type="button"
        onClick={onClear}
        className="text-muted-foreground hover:text-foreground flex items-center gap-1 text-[10px]"
      >
        <X size={10} />
        Clear filters
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Flat task list
// ---------------------------------------------------------------------------

const GROUP_ORDER: { status: TaskStatus; label: string }[] = [
  { status: "working", label: "Working" },
  { status: "waiting", label: "Waiting" },
  { status: "error", label: "Error" },
  { status: "idle", label: "Idle" },
];

function FlatTaskList({
  tasks,
  filterProjects,
  filterStatuses,
  navigate,
  activeRoute,
  doneExpanded,
  onToggleDone,
  ticketsByTaskId,
}: {
  tasks: Task[];
  filterProjects: Set<string>;
  filterStatuses: Set<TaskStatus>;
  navigate: (r: Route) => void;
  activeRoute: Route;
  doneExpanded: boolean;
  onToggleDone: () => void;
  ticketsByTaskId: Map<string, string[]>;
}) {
  // Status filter applied first; project filter needs per-task project
  // ids which are fetched by `TaskRow` (cached), so we apply it there.
  const statusFiltered = useMemo(
    () => tasks.filter((t) => filterStatuses.has(t.status)),
    [tasks, filterStatuses],
  );

  const byStatus: Record<string, Task[]> = useMemo(() => {
    const out: Record<string, Task[]> = {};
    for (const t of statusFiltered) {
      if (!out[t.status]) out[t.status] = [];
      out[t.status].push(t);
    }
    return out;
  }, [statusFiltered]);

  const done = byStatus.done ?? [];
  const hasAny = GROUP_ORDER.some((g) => (byStatus[g.status] ?? []).length > 0) ||
    done.length > 0;

  if (!hasAny) {
    return (
      <div className="text-muted-foreground/60 px-2 py-1 text-xs italic">
        {tasks.length === 0
          ? "no tasks yet"
          : "no tasks match the current filter"}
      </div>
    );
  }

  return (
    <ul className="space-y-1">
      {GROUP_ORDER.map(({ status, label }) => {
        const group = byStatus[status] ?? [];
        if (group.length === 0) return null;
        return (
          <li key={status}>
            <div className="text-muted-foreground/70 px-2 pt-1.5 pb-0.5 text-[10px] uppercase tracking-wider">
              {label}
            </div>
            <ul className="space-y-0.5">
              {group.map((t) => (
                <TaskRow
                  key={t.id}
                  task={t}
                  filterProjects={filterProjects}
                  navigate={navigate}
                  activeRoute={activeRoute}
                  ticketIds={ticketsByTaskId.get(t.id)}
                />
              ))}
            </ul>
          </li>
        );
      })}
      {done.length > 0 && (
        <li>
          <button
            type="button"
            onClick={onToggleDone}
            className="text-muted-foreground/70 hover:text-foreground flex w-full items-center gap-1 px-2 pt-2 pb-0.5 text-[10px] uppercase tracking-wider"
          >
            {doneExpanded ? (
              <ChevronDown size={10} />
            ) : (
              <ChevronRight size={10} />
            )}
            Done ({done.length})
          </button>
          {doneExpanded && (
            <ul className="space-y-0.5">
              {done.map((t) => (
                <TaskRow
                  key={t.id}
                  task={t}
                  filterProjects={filterProjects}
                  navigate={navigate}
                  activeRoute={activeRoute}
                  ticketIds={ticketsByTaskId.get(t.id)}
                  dimmed
                />
              ))}
            </ul>
          )}
        </li>
      )}
    </ul>
  );
}

function TaskRow({
  task,
  filterProjects,
  navigate,
  activeRoute,
  ticketIds,
  dimmed = false,
}: {
  task: Task;
  filterProjects: Set<string>;
  navigate: (r: Route) => void;
  activeRoute: Route;
  ticketIds?: string[];
  dimmed?: boolean;
}) {
  const { data: taskProjectIds = [] } = useTaskProjectIds(task.id);
  const { data: projects = [] } = useProjects();
  const isActive = activeRoute.kind === "task" && activeRoute.id === task.id;

  // Hide tasks that don't touch any of the filtered projects.
  if (filterProjects.size > 0) {
    const anyMatch = taskProjectIds.some((id) => filterProjects.has(id));
    if (!anyMatch) return null;
  }

  const projectById = new Map(projects.map((p) => [p.id, p] as const));
  const tickets = ticketIds ?? [];
  // Branch fallback covers pre-v1.0.2 rows where `branch_name` may be
  // absent; harmless on every modern row.
  const branch = task.branch_name ?? `weft/${task.id.slice(0, 8)}`;

  return (
    <li>
      <button
        type="button"
        onClick={() => navigate({ kind: "task", id: task.id })}
        title={`${task.name}\n${branch}${tickets.length ? `\n${tickets.join(", ")}` : ""}`}
        className={`flex w-full flex-col gap-0.5 rounded px-2 py-1.5 text-left text-xs transition-colors ${
          isActive
            ? "bg-sidebar-accent text-sidebar-accent-foreground"
            : "hover:bg-sidebar-accent"
        } ${dimmed ? "opacity-55" : ""}`}
      >
        <div className="flex items-center gap-2">
          <TaskStatusDot status={task.status} size="xs" pulse />
          <span className="flex-1 truncate">{task.name}</span>
          <span className="flex shrink-0 items-center gap-0.5">
            {taskProjectIds.slice(0, 4).map((pid) => {
              const p = projectById.get(pid);
              return (
                <span
                  key={pid}
                  className="inline-block h-1.5 w-1.5 rounded-full"
                  style={{
                    background: p?.color ?? "var(--muted-foreground)",
                  }}
                  title={p?.name ?? pid}
                />
              );
            })}
            {taskProjectIds.length > 4 && (
              <span className="text-muted-foreground/80 font-mono text-[9px]">
                +{taskProjectIds.length - 4}
              </span>
            )}
          </span>
        </div>
        {/* Sub-line: branch + ticket IDs. Indented to clear the status
            dot above so the branch starts at the same x as the task name.
            Uses min-w-0 + truncate so a long branch never blows out the
            sidebar width. */}
        <div className="text-muted-foreground/80 flex min-w-0 items-center gap-1.5 pl-3.5 text-[10px]">
          <span className="truncate font-mono">{branch}</span>
          {tickets.length > 0 && (
            <span className="bg-muted/60 text-muted-foreground shrink-0 rounded px-1 py-px font-mono text-[9px]">
              {tickets.length === 1 ? tickets[0] : `${tickets[0]} +${tickets.length - 1}`}
            </span>
          )}
        </div>
      </button>
    </li>
  );
}
