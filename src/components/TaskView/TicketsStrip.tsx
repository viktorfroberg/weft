import { useMemo, useState } from "react";
import { Ticket as TicketIcon, X } from "lucide-react";
import { openPath } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import {
  taskLinkTicket,
  taskUnlinkTicket,
  terminalWrite,
  type Ticket,
  type TicketLink,
} from "@/lib/commands";
import { useIntegrations } from "@/stores/integrations";
import {
  useRefetchTaskTickets,
  useTaskTickets,
  useTicketLive,
} from "@/stores/tickets";
import { useTerminalTabs } from "@/stores/terminal_tabs";
import { usePtyExits } from "@/stores/pty_exits";
import { TicketPicker, ticketKey, toLink } from "../TicketPicker";

const EMPTY_TICKET_LINKS: TicketLink[] = [];

/** Tabs whose PTY is alive (sessionId set, no recorded exit). Used to
 *  decide whether weft can patch context live or whether the user must
 *  reload the agent to see the change. */
function liveAgentsForTask(taskId: string) {
  const tabs = useTerminalTabs.getState().byTaskId[taskId] ?? [];
  const exits = usePtyExits.getState().bySessionId;
  return tabs.filter(
    (t) => t.kind === "agent" && t.sessionId && !exits[t.sessionId],
  );
}

/**
 * Strip above TaskView's terminal tabs showing every ticket linked to
 * the task. Live-fetched chip titles via TanStack Query (see TicketChip).
 * "+ Link ticket" opens the provider-scoped picker. Renders nothing
 * when no tickets are linked AND no provider is connected.
 */
export function TicketsStrip({ taskId }: { taskId: string }) {
  const { data: links = EMPTY_TICKET_LINKS } = useTaskTickets(taskId);
  const { data: providers = [] } = useIntegrations();
  const refetchTickets = useRefetchTaskTickets();
  const connectedProvider = providers.find((p) => p.connected);

  const [pickerOpen, setPickerOpen] = useState(false);

  // Subscribe so the "Unlink" X disables/enables reactively when the
  // agent launches or exits. `getState()` inside onToggle (below) is fine
  // for one-shot reads; rendering state needs a proper subscription.
  const liveAgentCount = useTerminalTabs((s) => {
    const tabs = s.byTaskId[taskId] ?? [];
    const exits = usePtyExits.getState().bySessionId;
    return tabs.filter(
      (t) => t.kind === "agent" && t.sessionId && !exits[t.sessionId],
    ).length;
  });
  const agentIsLive = liveAgentCount > 0;

  const linkedKeys = useMemo(
    () => new Set(links.map((l) => `${l.provider}:${l.external_id}`)),
    [links],
  );

  const onToggle = async (t: Ticket) => {
    const key = ticketKey(t);
    if (linkedKeys.has(key)) {
      await taskUnlinkTicket(taskId, t.provider, t.external_id);
      toast.success(`Unlinked ${t.external_id}`);
    } else {
      await taskLinkTicket(taskId, toLink(t));
      // Backend has already refreshed `.weft/tickets.md` in every
      // worktree. If an agent is alive, append the new ticket to its
      // input as a short note so it picks up the context without a
      // reload — Claude Code queues input so this lands at the next
      // prompt boundary even if the agent is mid-response.
      const liveAgents = liveAgentsForTask(taskId);
      if (liveAgents.length > 0) {
        const oneLineTitle = t.title.replace(/[\r\n]+/g, " ").trim();
        const note = `Also working on ${t.external_id}: ${oneLineTitle} (${t.url}). I refreshed .weft/tickets.md — please factor this in.\n`;
        const bytes = new TextEncoder().encode(note);
        await Promise.all(
          liveAgents.map((tab) =>
            terminalWrite(tab.sessionId!, bytes).catch(() => {}),
          ),
        );
        toast.success(`Linked ${t.external_id}`, {
          description: `${oneLineTitle} · sent to ${liveAgents.length === 1 ? "the agent" : `${liveAgents.length} agents`}`,
        });
      } else {
        toast.success(`Linked ${t.external_id}`, { description: t.title });
      }
    }
    await refetchTickets(taskId);
  };

  const onUnlinkLink = async (l: TicketLink) => {
    await taskUnlinkTicket(taskId, l.provider, l.external_id);
    toast.success(`Unlinked ${l.external_id}`);
    await refetchTickets(taskId);
  };

  if (links.length === 0 && !connectedProvider) return null;

  return (
    <div className="border-border relative flex flex-wrap items-center gap-1.5 border-b px-6 py-2 text-xs">
      {links.length === 0 ? (
        <span className="text-muted-foreground">No tickets linked.</span>
      ) : (
        links.map((l) => (
          <TicketChip
            key={`${l.provider}:${l.external_id}`}
            link={l}
            onUnlink={onUnlinkLink}
            unlinkDisabled={agentIsLive}
          />
        ))
      )}
      {connectedProvider && (
        <button
          type="button"
          onClick={() => setPickerOpen((v) => !v)}
          className="text-muted-foreground hover:bg-accent hover:text-foreground ml-1 inline-flex shrink-0 items-center gap-1 rounded px-2 py-0.5 transition-colors"
          title={`Link ${connectedProvider.display_name} ticket`}
        >
          <TicketIcon size={12} />
          Link ticket
        </button>
      )}
      {pickerOpen && connectedProvider && (
        <div className="bg-popover border-border absolute left-6 top-full z-20 mt-1 rounded-md border shadow-lg">
          <TicketPicker
            providerId={connectedProvider.id}
            selectedKeys={linkedKeys}
            onToggle={onToggle}
            onClose={() => setPickerOpen(false)}
            autoFocus
          />
        </div>
      )}
    </div>
  );
}

/** Single ticket chip. Isolated so `useTicketLive` runs per link
 *  (hooks must be called at a stable top level; a loop inside the
 *  parent won't satisfy that). TanStack Query's cache dedupes
 *  identical (provider, external_id) pairs across chips. */
function TicketChip({
  link,
  onUnlink,
  unlinkDisabled,
}: {
  link: TicketLink;
  onUnlink: (l: TicketLink) => void;
  /** Hide the X when an agent is live — unlinking from weft doesn't
   *  clear Claude's conversation history, so the affordance misleads. */
  unlinkDisabled?: boolean;
}) {
  const { data: live, isFetched } = useTicketLive(
    link.provider,
    link.external_id,
  );
  const title = live?.title;
  const unavailable = isFetched && live === null;
  return (
    <span
      className={`group bg-muted/20 inline-flex shrink-0 items-center gap-1.5 rounded-md px-1.5 py-0.5 ${
        unavailable ? "text-muted-foreground/70" : ""
      }`}
      title={title ?? link.url}
    >
      <button
        type="button"
        onClick={() => openPath(link.url).catch(() => {})}
        className="inline-flex items-center gap-1.5 hover:underline"
      >
        <span className="text-muted-foreground font-mono text-[10px]">
          {link.external_id}
        </span>
        <span className="max-w-[220px] truncate">
          {title ?? (unavailable ? "(unavailable)" : "…")}
        </span>
      </button>
      {!unlinkDisabled && (
        <button
          type="button"
          onClick={() => onUnlink(link)}
          className="text-muted-foreground hover:text-destructive ml-0.5 opacity-60 transition-opacity group-hover:opacity-100"
          title="Unlink"
        >
          <X size={10} />
        </button>
      )}
    </span>
  );
}
