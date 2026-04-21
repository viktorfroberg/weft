import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Search, X } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useTicketBacklog } from "@/stores/tickets";
import { useIntegrations } from "@/stores/integrations";
import type { Ticket, TicketLink } from "@/lib/commands";

// Zustand selector-stability sentinels.
const EMPTY_TICKETS: never[] = [];
/** Fallback for the `excludeKeys` prop. Must be an actual Set — the picker's
 *  filter uses `.has()`, which a stray empty array would silently fail. */
const EMPTY_EXCLUDED: Set<string> = new Set();

interface Props {
  /** Provider to browse. v1.0.2 ships only Linear; picker UI is
   *  provider-agnostic so it can host others later. */
  providerId: string;
  /** Already-selected ticket keys (`provider:external_id`) — rendered
   *  with a checkmark and de-dup'd from results. */
  selectedKeys: Set<string>;
  /** Emit when the user toggles a ticket. Caller maintains the list. */
  onToggle: (ticket: Ticket) => void;
  /** Optional click-away handler. */
  onClose?: () => void;
  /** Exclude these tickets from the result set (e.g. already linked to
   *  the current task — no point re-linking). */
  excludeKeys?: Set<string>;
  autoFocus?: boolean;
}

/** Key used to match backlog tickets against selected/excluded sets. */
export function ticketKey(t: { provider: string; external_id: string }): string {
  return `${t.provider}:${t.external_id}`;
}

/** Convert a `Ticket` into the minimal `TicketLink` we persist. */
export function toLink(t: Ticket): TicketLink {
  return { provider: t.provider, external_id: t.external_id, url: t.url };
}

export function TicketPicker({
  providerId,
  selectedKeys,
  onToggle,
  onClose,
  excludeKeys,
  autoFocus,
}: Props) {
  const { data: providers = [] } = useIntegrations();
  const provider = providers.find((p) => p.id === providerId);
  const connected = provider?.connected ?? false;

  const { data: tickets = EMPTY_TICKETS, isFetching: inFlight } =
    useTicketBacklog(providerId, connected);

  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (autoFocus) inputRef.current?.focus();
  }, [autoFocus]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const excluded = excludeKeys ?? EMPTY_EXCLUDED;
    return tickets.filter((t) => {
      if (excluded.has(ticketKey(t))) return false;
      if (!q) return true;
      return (
        t.external_id.toLowerCase().includes(q) ||
        t.title.toLowerCase().includes(q) ||
        (t.status ?? "").toLowerCase().includes(q)
      );
    });
  }, [tickets, query, excludeKeys]);

  if (!connected) {
    return (
      <div className="text-muted-foreground p-3 text-xs">
        {provider?.display_name ?? providerId} not connected. Add a token
        in Settings → Integrations.
      </div>
    );
  }

  return (
    <div className="flex max-h-[420px] w-[380px] flex-col overflow-hidden">
      <div className="border-border relative flex items-center gap-2 border-b px-2 py-2">
        <Search size={14} className="text-muted-foreground shrink-0" />
        <Input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search tickets"
          className="h-7 border-0 bg-transparent px-0 text-sm shadow-none focus-visible:ring-0"
        />
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground"
            title="Close"
          >
            <X size={14} />
          </button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto">
        {inFlight && tickets.length === 0 && (
          <div className="text-muted-foreground p-3 text-xs">loading…</div>
        )}
        {!inFlight && filtered.length === 0 && (
          <div className="text-muted-foreground/80 p-3 text-xs">
            {query ? "No matches." : "No open tickets assigned to you."}
          </div>
        )}
        <ul>
          {filtered.map((t) => {
            const key = ticketKey(t);
            const selected = selectedKeys.has(key);
            return (
              <li key={key}>
                <button
                  type="button"
                  onClick={() => onToggle(t)}
                  className={`hover:bg-accent flex w-full items-start gap-2 px-3 py-2 text-left ${
                    selected ? "bg-accent" : ""
                  }`}
                >
                  <span className="mt-[3px] w-4 shrink-0">
                    {selected && <Check size={12} className="text-emerald-500" />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-muted-foreground shrink-0 font-mono text-[11px]">
                        {t.external_id}
                      </span>
                      {t.status && (
                        <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-[10px]">
                          {t.status}
                        </span>
                      )}
                    </div>
                    <p className="truncate text-sm">{t.title}</p>
                  </div>
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
