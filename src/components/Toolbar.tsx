import { useMemo } from "react";
import { ArrowLeft } from "lucide-react";
import { useActiveRoute, useNavigateRoute, type Route } from "@/lib/active-route";
import { useAllTasks } from "@/stores/tasks";
import { useProjects } from "@/stores/projects";
import type { Project, Task } from "@/lib/commands";
import { useTerminalTabs } from "@/stores/terminal_tabs";
import { terminalKill } from "@/lib/commands";
import { TaskStatusDot } from "./ui/task-status-dot";
import { useConfirm } from "./ConfirmDialog";
import { toast } from "sonner";

/**
 * Full-width top toolbar. Acts as the window-drag region AND shows:
 *   - left: back arrow + route breadcrumb
 *   - right: contextual primary action for the current route
 *
 * v1.0.7: dropped "New workspace" (workspaces aren't primary nav) and
 * "New task" (replaced by the compose card / ⌘⇧N overlay). Launch-agent
 * stays as the task-view primary.
 *
 * Note on drag-region semantics: the outer container is the drag region;
 * every interactive child MUST opt out with `data-tauri-drag-region="false"`
 * or buttons will swallow mousedown events instead of firing clicks.
 */
export function Toolbar() {
  const route = useActiveRoute();
  const navigate = useNavigateRoute();
  const allTasks = useAllTasks();
  const { data: projects = [] } = useProjects();
  const terminalTabs = useTerminalTabs((s) => s.byTaskId);
  const closeTab = useTerminalTabs((s) => s.closeTab);
  const confirm = useConfirm();

  const crumbs = useMemo(
    () => buildCrumbs(route, allTasks, projects, navigate),
    [route, allTasks, projects, navigate],
  );
  const back = backFromRoute(route);

  const activeTask = useMemo(() => {
    if (route.kind !== "task") return null;
    for (const list of Object.values(allTasks)) {
      const found = (list ?? []).find((t) => t.id === route.id);
      if (found) return found;
    }
    return null;
  }, [route, allTasks]);

  const onKillAgents = async () => {
    if (!activeTask) return;
    const tabs = terminalTabs[activeTask.id] ?? [];
    const agentTabs = tabs.filter((t) => t.kind === "agent");
    if (agentTabs.length === 0) return;
    const ok = await confirm({
      title: `Kill ${agentTabs.length} agent session${agentTabs.length === 1 ? "" : "s"}?`,
      description:
        "SIGKILLs the running agent process(es). The shell tab stays open. You can re-launch with ⌘L.",
      confirmText: "Kill agent",
      destructive: true,
    });
    if (!ok) return;
    for (const tab of agentTabs) {
      await terminalKill(tab.id).catch(() => {});
      closeTab(activeTask.id, tab.id);
    }
    toast.success(
      `Killed ${agentTabs.length} agent session${agentTabs.length === 1 ? "" : "s"}`,
      { description: "Shell stays open. ⌘L re-launches." },
    );
  };

  return (
    <div
      data-tauri-drag-region
      className="border-border bg-background/80 supports-[backdrop-filter]:bg-background/60 flex h-10 shrink-0 items-center gap-2 border-b px-3 pl-[80px] backdrop-blur"
    >
      {back && (
        <button
          type="button"
          data-tauri-drag-region="false"
          onClick={() => navigate(back)}
          className="text-foreground hover:bg-accent flex h-6 w-6 shrink-0 items-center justify-center rounded transition-colors"
          title="Back"
          aria-label="Back"
        >
          <ArrowLeft size={12} />
        </button>
      )}

      <nav
        className="flex items-center gap-1.5 overflow-hidden text-sm"
        aria-label="Breadcrumb"
      >
        {crumbs.map((c, idx) => {
          const isLast = idx === crumbs.length - 1;
          const showStatus = isLast && activeTask && route.kind === "task";
          return (
            <span
              key={idx}
              className={`flex items-center gap-1.5 truncate ${
                isLast ? "text-foreground font-medium" : "text-foreground"
              }`}
            >
              {idx > 0 && (
                <span className="text-muted-foreground/50 select-none">/</span>
              )}
              {showStatus && activeTask && (
                <button
                  type="button"
                  data-tauri-drag-region="false"
                  onContextMenu={(e) => {
                    e.preventDefault();
                    void onKillAgents();
                  }}
                  title={`Status: ${activeTask.status} · right-click to kill agent`}
                  className="flex items-center"
                >
                  <TaskStatusDot status={activeTask.status} size="sm" pulse />
                </button>
              )}
              {c.onClick ? (
                <button
                  type="button"
                  data-tauri-drag-region="false"
                  onClick={c.onClick}
                  className="hover:text-foreground truncate"
                >
                  {c.label}
                </button>
              ) : (
                <span className="truncate">{c.label}</span>
              )}
            </span>
          );
        })}
      </nav>

      <div className="ml-auto flex items-center gap-2" />
      {/* Launch button removed: task creation auto-spawns the default
          agent tab, and ⌘T / the `+` button in the tab strip opens a
          picker for adding more shell/agent tabs. ⌘L (launch default
          agent as a new tab) remains as a keyboard-only shortcut for
          users who want a one-key way to spin up another agent. */}
    </div>
  );
}

interface Crumb {
  label: string;
  onClick?: () => void;
}

function buildCrumbs(
  route: Route,
  allTasks: Record<string, Task[]>,
  projects: Project[],
  nav: (r: Route) => void,
): Crumb[] {
  const crumbs: Crumb[] = [
    { label: "weft", onClick: () => nav({ kind: "home" }) },
  ];
  if (route.kind === "task") {
    // Task name used to live here. Moved to the TaskView header so
    // long names don't cram into the window drag-region + the rename
    // affordance has room to breathe. The status-dot chip in the
    // toolbar is enough contextual "you're in a task" signal; the
    // back arrow gets the user home.
    const task = Object.values(allTasks)
      .flatMap((v) => v ?? [])
      .find((t) => t && t.id === route.id);
    void task;
  } else if (route.kind === "project") {
    const project = projects.find((p) => p.id === route.id);
    if (project) crumbs.push({ label: project.name });
  } else if (route.kind === "settings") {
    crumbs.push({ label: "Settings" });
  }
  return crumbs;
}

function backFromRoute(route: Route): Route | null {
  if (route.kind === "home") return null;
  return { kind: "home" };
}
