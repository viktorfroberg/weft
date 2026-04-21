import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowUpRight, Check, Ticket as TicketIcon } from "lucide-react";
import { useTicketBacklog } from "@/stores/tickets";
import { useIntegrations } from "@/stores/integrations";
import { useAllTasksFlat } from "@/stores/tasks";
import { useUi } from "@/stores/ui";
import { useNavigateRoute } from "@/lib/active-route";
import { taskTicketsByProvider, type TaskTicketRow } from "@/lib/commands";
import { qk } from "@/query";

const EMPTY_TICKETS: never[] = [];
const EMPTY_LINKS: never[] = [];
const MAX_CARDS = 9;

/**
 * Home-only Linear-backlog launcher. Up to MAX_CARDS open tickets scoped
 * by Settings → Integrations → Backlog scope (default: In Progress + Todo),
 * sorted Urgent → Low (priority 0 sinks to the bottom). Each card resolves
 * one of two click-paths:
 *
 *   - has a weft task already linked  →  navigate straight to it (no new task)
 *   - no link yet                     →  pre-fill the inline compose card
 *
 * Renders nothing when no provider is connected or the (scoped) backlog
 * is empty — the launcher minimalism holds in those cases.
 */
export function HomeBacklogStrip() {
  const { data: providers = [] } = useIntegrations();
  const connected = providers.find((p) => p.connected);
  const setComposePrefillTicket = useUi((s) => s.setComposePrefillTicket);
  const setComposeDetachTicketKey = useUi((s) => s.setComposeDetachTicketKey);
  const attachedKeys = useUi((s) => s.composeAttachedTicketKeys);
  const setPendingSettingsTab = useUi((s) => s.setPendingSettingsTab);
  const navigate = useNavigateRoute();

  const { data: tickets = EMPTY_TICKETS, isFetching } = useTicketBacklog(
    connected?.id ?? "",
    !!connected,
  );

  // Existing ticket→task links for this provider. Most-recently-linked
  // first (DESC linked_at), so the lookup below picks the freshest task
  // when a single ticket somehow has multiple links.
  const { data: links = EMPTY_LINKS } = useQuery<TaskTicketRow[]>({
    queryKey: qk.taskTicketsByProvider(connected?.id ?? ""),
    queryFn: () => taskTicketsByProvider(connected!.id),
    enabled: !!connected,
  });

  // Cross-reference with the live task list so a stale link (either from
  // a task-in-flight-delete or from orphaned rows that predate the FK
  // cascade) never produces a backlog card that navigates to a missing
  // task. Clicking such a card used to land on "Task not found".
  const tasks = useAllTasksFlat();
  const existingTaskIds = useMemo(
    () => new Set(tasks.map((t) => t.id)),
    [tasks],
  );

  const linkedTaskByExternalId = useMemo(() => {
    const m = new Map<string, string>();
    for (const l of links) {
      if (m.has(l.external_id)) continue;
      if (!existingTaskIds.has(l.task_id)) continue;
      m.set(l.external_id, l.task_id);
    }
    return m;
  }, [links, existingTaskIds]);

  if (!connected) return null;
  if (isFetching && tickets.length === 0) return null;
  if (tickets.length === 0) return null;

  const cards = tickets.slice(0, MAX_CARDS);

  return (
    <div className="space-y-2 pt-2">
      <div className="flex items-center justify-between">
        <div className="text-muted-foreground flex items-center gap-1.5 text-[10px] uppercase tracking-wide">
          <TicketIcon size={10} />
          <span>Your {connected.display_name} backlog</span>
        </div>
        <button
          type="button"
          onClick={() => {
            setPendingSettingsTab("integrations");
            navigate({ kind: "settings" });
          }}
          className="text-muted-foreground hover:text-foreground text-[10px] uppercase tracking-wide"
          title="Configure backlog scope (Settings → Integrations)"
        >
          Scope
        </button>
      </div>
      <div className="grid grid-cols-3 gap-2">
        {cards.map((t) => {
          const tKey = `${t.provider}:${t.external_id}`;
          const existingTaskId = linkedTaskByExternalId.get(t.external_id);
          const isAttached = attachedKeys.has(tKey);
          // Click resolution priority:
          //   1. ticket already has a weft task → navigate to it
          //   2. ticket already attached to compose card → detach (toggle off)
          //   3. otherwise → attach
          const onClick = existingTaskId
            ? () => navigate({ kind: "task", id: existingTaskId })
            : isAttached
              ? () => setComposeDetachTicketKey(tKey)
              : () => setComposePrefillTicket(t);
          const cycleLabel = formatCycle(t.cycle_name, t.cycle_number);
          const tip = existingTaskId
            ? "Open existing weft task linked to this ticket"
            : isAttached
              ? "Attached to compose card — click to detach"
              : t.assignee
                ? `Assigned to ${t.assignee} — start a task from this ticket`
                : "Start a task from this ticket";
          return (
            <button
              key={tKey}
              type="button"
              onClick={onClick}
              title={tip}
              className={`group flex h-full flex-col gap-2 rounded-lg border p-3 text-left transition-colors ${
                isAttached
                  ? "border-foreground/40 bg-accent ring-foreground/20 ring-1"
                  : "border-border bg-card hover:bg-accent hover:border-foreground/20"
              }`}
            >
              {/* Header row: priority dot + ID. ID is `whitespace-nowrap`
                  so badges below never push it onto two lines. State-
                  indicator badges live in their own row beneath. */}
              <div className="flex items-center gap-2">
                <PriorityDot priority={t.priority} />
                <span className="text-muted-foreground font-mono text-[11px] whitespace-nowrap">
                  {t.external_id}
                </span>
                {t.assignee && !existingTaskId && !isAttached && (
                  <span
                    className="bg-muted text-foreground ml-auto flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[9px] font-semibold"
                    title={`Assigned to ${t.assignee}`}
                  >
                    {assigneeInitial(t.assignee)}
                  </span>
                )}
              </div>
              <p className="text-foreground line-clamp-2 text-sm leading-snug">
                {t.title}
              </p>
              <div className="text-muted-foreground mt-auto flex flex-wrap items-center gap-1.5 text-[10px]">
                {isAttached && (
                  <span className="bg-foreground text-background inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 font-medium">
                    <Check size={10} />
                    Attached
                  </span>
                )}
                {existingTaskId && !isAttached && (
                  <span className="bg-accent text-foreground inline-flex items-center gap-0.5 rounded px-1.5 py-0.5">
                    <ArrowUpRight size={10} />
                    Open
                  </span>
                )}
                {t.status && (
                  <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5">
                    {t.status}
                  </span>
                )}
                {cycleLabel && <span className="ml-auto">{cycleLabel}</span>}
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function assigneeInitial(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "?";
  return trimmed[0]!.toUpperCase();
}

function formatCycle(
  name: string | null,
  number: number | null,
): string | null {
  // Linear assigns each cycle a sequential number; teams optionally name
  // them. Prefer the name (if present, since teams pick it for a reason);
  // otherwise fall back to "Cycle N". Don't concatenate both — Linear's
  // own UI never shows the pair together either.
  if (name && name.trim()) return name.trim();
  if (number != null) return `Cycle ${number}`;
  return null;
}

const PRIORITY_META: Record<
  number,
  { label: string; color: string }
> = {
  1: { label: "Urgent", color: "bg-red-500" },
  2: { label: "High", color: "bg-orange-500" },
  3: { label: "Medium", color: "bg-yellow-500" },
  4: { label: "Low", color: "bg-blue-500" },
};

function PriorityDot({ priority }: { priority: number | null }) {
  if (priority == null || priority === 0) {
    return (
      <span
        className="bg-muted-foreground/30 inline-block h-2 w-2 shrink-0 rounded-full"
        title="No priority"
      />
    );
  }
  const meta = PRIORITY_META[priority];
  if (!meta) return null;
  return (
    <span
      className={`${meta.color} inline-block h-2 w-2 shrink-0 rounded-full`}
      title={`Priority: ${meta.label}`}
    />
  );
}
