import { useMemo } from "react";
import { usePrefs } from "@/stores/prefs";
import { useAllTasksFlat } from "@/stores/tasks";
import { useNavigateRoute } from "@/lib/active-route";
import { TaskStatusDot } from "@/components/ui/task-status-dot";
import { TaskComposeCard } from "@/components/TaskComposeCard";
import { HomeBacklogStrip } from "@/components/HomeBacklogStrip";
import logoMark from "@/assets/logo-mark.png";

/**
 * v1.0.7.x Home — committed to the launcher pattern. Not a dashboard.
 * The workflow is start-a-task → jump into the task view → stay there,
 * so Home's job is to get you out of Home fast.
 *
 * Layout (top→bottom, centered column ~14vh from top):
 *   1. Logo mark + greeting headline — count-derived "N waiting"/"N in
 *      flight" with a "Good {part-of-day}" fallback.
 *   2. TaskComposeCard — the hero.
 *   3. Shortcut strip — always-visible keycap hints for the top 4 actions.
 *   4. Recent-chip strip — up to 3 chips when recents exist (sidebar +
 *      ⌘⇧O own the long form).
 *   5. Linear backlog strip — up to 5 ticket rows when a provider is
 *      connected; clicking a row pre-fills the compose card with that
 *      ticket attached. Adds a launcher path without becoming a dashboard.
 *
 * Dropped vs. earlier iterations: `weft` h1 + subtitle (window chrome
 * names the app), the "Active" section (sidebar groups tasks by status),
 * dashed empty-state (compose card's repo picker now surfaces first-run
 * CTA inline).
 */
export function Home() {
  const tasks = useAllTasksFlat();
  const recentIds = usePrefs((s) => s.recentTaskIds);
  const userName = usePrefs((s) => s.userName);
  const navigate = useNavigateRoute();
  const firstName = pickFirstName(userName);

  const waitingCount = tasks.filter((t) => t.status === "waiting").length;
  const activeCount = tasks.filter(
    (t) => t.status === "working" || t.status === "waiting",
  ).length;

  const taskById = useMemo(() => {
    const m = new Map<string, (typeof tasks)[number]>();
    for (const t of tasks) m.set(t.id, t);
    return m;
  }, [tasks]);
  const recentTasks = recentIds
    .map((id) => taskById.get(id))
    .filter((t): t is (typeof tasks)[number] => !!t)
    .slice(0, 3);

  const greeting = buildGreeting(waitingCount, activeCount, firstName);

  return (
    // Outer wrapper owns the scroll: Shell's content slot is
    // `overflow-hidden`, so without scrolling here the backlog grid gets
    // clipped on shorter windows. `pb-12` keeps the last row off the
    // bottom edge.
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex min-h-full w-full max-w-2xl flex-col items-stretch space-y-6 px-6 pt-[12vh] pb-12">
        <div className="flex flex-col items-center gap-4">
          <img
            src={logoMark}
            alt=""
            className="h-24 w-24 select-none"
            draggable={false}
          />
          <h1 className="text-foreground text-center text-3xl font-semibold tracking-tight">
            {greeting}
          </h1>
        </div>

        <TaskComposeCard
          variant="inline"
          onCreated={(task) => navigate({ kind: "task", id: task.id })}
        />

        <div className="text-muted-foreground flex flex-wrap items-center justify-center gap-x-4 gap-y-2 text-[11px]">
          <ShortcutHint keys="⌘K" label="jump" />
          <ShortcutHint keys="⌘⇧N" label="new" />
          <ShortcutHint keys="⌘⇧O" label="recent" />
          <ShortcutHint keys="⌘P" label="add repo" />
        </div>

        {recentTasks.length > 0 && (
          <div className="flex flex-wrap justify-center gap-1.5 pt-2">
            {recentTasks.map((t) => (
              <button
                key={t.id}
                type="button"
                onClick={() => navigate({ kind: "task", id: t.id })}
                title={t.branch_name}
                className="border-border bg-card hover:bg-accent hover:border-foreground/20 flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors"
              >
                <TaskStatusDot status={t.status} size="xs" />
                <span className="max-w-[180px] truncate">{t.name}</span>
              </button>
            ))}
          </div>
        )}

        <HomeBacklogStrip />
      </div>
    </div>
  );
}

function ShortcutHint({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <kbd className="bg-muted text-muted-foreground rounded border px-1.5 py-0.5 font-mono text-[10px]">
        {keys}
      </kbd>
      <span>{label}</span>
    </span>
  );
}

function buildGreeting(
  waitingCount: number,
  activeCount: number,
  firstName: string | null,
): string {
  if (waitingCount > 0) {
    return waitingCount === 1
      ? "1 task waiting on you"
      : `${waitingCount} tasks waiting on you`;
  }
  if (activeCount > 0) {
    return activeCount === 1 ? "1 in flight" : `${activeCount} in flight`;
  }
  const hour = new Date().getHours();
  const part =
    hour < 5 ? "evening" : hour < 12 ? "morning" : hour < 18 ? "afternoon" : "evening";
  return firstName ? `Good ${part}, ${firstName}` : `Good ${part}`;
}

/** "Viktor Froberg" → "Viktor". Falls back to null on empty/whitespace
 *  so the greeting cleanly drops the comma instead of saying "Good morning, ". */
function pickFirstName(full: string | null | undefined): string | null {
  const trimmed = full?.trim();
  if (!trimmed) return null;
  const first = trimmed.split(/\s+/)[0];
  return first && first.length > 0 ? first : null;
}
