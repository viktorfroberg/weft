import { useState } from "react";
import { Check, Eye, EyeOff, Loader2, Plug, X } from "lucide-react";
import { toast } from "sonner";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  integrationClear,
  integrationSetToken,
  integrationTest,
  linearSettingsGet,
  linearSettingsSet,
  type AuthStatus,
  type LinearBacklogScope,
  type LinearSettings,
  type ProviderInfo,
} from "@/lib/commands";
import { useIntegrations, useRefetchIntegrations } from "@/stores/integrations";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { qk } from "@/query";
import { Card } from "./Card";

export function IntegrationsTab() {
  const { data: providers = [] } = useIntegrations();

  return (
    <Card
      title="Ticket providers"
      description="Tokens are stored in macOS Keychain, never on disk. Disconnect any time — connected state is tracked in a separate non-secret file."
      Icon={Plug}
    >
      {providers.length === 0 ? (
        <div className="text-muted-foreground flex items-center gap-2 py-2 text-xs">
          <Loader2 size={12} className="animate-spin" />
          loading providers…
        </div>
      ) : (
        <ul className="space-y-2">
          {providers.map((p) => (
            <ProviderRow key={p.id} provider={p} />
          ))}
        </ul>
      )}
      <p className="text-muted-foreground/80 mt-3 text-xs">
        Linear: create a personal API key at{" "}
        <a
          className="text-foreground underline"
          onClick={(e) => {
            e.preventDefault();
            openPath("https://linear.app/settings/api").catch(() => {});
          }}
          href="#"
        >
          linear.app/settings/api
        </a>
        . Keys start with <code className="font-mono">lin_api_</code>.
      </p>
    </Card>
  );
}

function ProviderRow({ provider }: { provider: ProviderInfo }) {
  const refetch = useRefetchIntegrations();
  const [editing, setEditing] = useState(!provider.connected);
  const [token, setToken] = useState("");
  const [showToken, setShowToken] = useState(false);
  const [status, setStatus] = useState<AuthStatus | null>(null);

  // Three related actions, all using TanStack Query's `useMutation` so
  // busy/error/retry are handled uniformly instead of hand-rolling a
  // try/catch/setBusy triplet per call. `busy` derived from the sum of
  // `isPending` flags so any in-flight action disables all buttons.
  const save = useMutation({
    mutationFn: () => integrationSetToken(provider.id, token.trim()),
    onSuccess: async (s) => {
      setStatus(s);
      if (s.ok) {
        setToken("");
        setShowToken(false);
        setEditing(false);
        await refetch();
        toast.success(`${provider.display_name} connected`, {
          description: s.viewer ? `as ${s.viewer}` : undefined,
        });
      }
    },
    onError: (e) => {
      setStatus({ ok: false, viewer: null, error: String(e) });
    },
  });

  const test = useMutation({
    mutationFn: () => integrationTest(provider.id),
    onSuccess: (s) => {
      setStatus(s);
      if (s.ok) {
        toast.success("Connection OK", {
          description: s.viewer ?? undefined,
        });
      } else {
        toast.error("Test failed", { description: s.error ?? undefined });
      }
    },
  });

  const disconnect = useMutation({
    mutationFn: () => integrationClear(provider.id),
    onSuccess: async () => {
      setStatus(null);
      setEditing(true);
      await refetch();
      toast.success(`${provider.display_name} disconnected`);
    },
  });

  const busy = save.isPending || test.isPending || disconnect.isPending;

  const onSave = () => {
    if (!token.trim()) return;
    save.mutate();
  };
  const onTest = () => test.mutate();
  const onDisconnect = () => disconnect.mutate();

  return (
    <li className="border-border bg-card rounded-md border p-3">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-sm">
          <span className="font-medium">{provider.display_name}</span>
          {provider.connected ? (
            <span className="animate-in fade-in inline-flex items-center gap-1 text-xs text-emerald-500 duration-200">
              <Check size={12} /> connected
            </span>
          ) : (
            <span className="text-muted-foreground text-xs">not connected</span>
          )}
        </div>
        <div className="flex gap-1">
          {provider.connected && !editing && (
            <>
              <Button
                size="sm"
                variant="ghost"
                onClick={onTest}
                disabled={busy}
                className="h-7 text-xs"
              >
                {busy ? <Loader2 size={12} className="animate-spin" /> : "Test"}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={onDisconnect}
                disabled={busy}
                className="text-muted-foreground hover:text-destructive h-7 text-xs"
              >
                Disconnect
              </Button>
            </>
          )}
        </div>
      </div>
      {editing && (
        <div className="animate-in fade-in slide-in-from-top-1 mt-2 flex items-center gap-2 duration-200">
          <div className="relative flex-1">
            <Input
              type={showToken ? "text" : "password"}
              placeholder={
                provider.id === "linear" ? "lin_api_…" : "Personal API token"
              }
              value={token}
              onChange={(e) => setToken(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  onSave();
                }
              }}
              className="h-8 pr-7 font-mono text-xs"
              disabled={busy}
              autoFocus
            />
            {token && (
              <button
                type="button"
                onClick={() => setShowToken((v) => !v)}
                className="text-muted-foreground hover:text-foreground absolute right-2 top-1/2 -translate-y-1/2"
                title={showToken ? "Hide" : "Show"}
                tabIndex={-1}
              >
                {showToken ? <EyeOff size={12} /> : <Eye size={12} />}
              </button>
            )}
          </div>
          <Button
            size="sm"
            onClick={onSave}
            disabled={busy || !token.trim()}
            className="h-8 gap-1 text-xs"
          >
            {busy ? (
              <>
                <Loader2 size={12} className="animate-spin" />
                Testing…
              </>
            ) : (
              "Save & test"
            )}
          </Button>
          {provider.connected && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setEditing(false);
                setToken("");
                setShowToken(false);
                setStatus(null);
              }}
              disabled={busy}
              className="h-8"
              title="Cancel"
            >
              <X size={14} />
            </Button>
          )}
        </div>
      )}
      {status && !status.ok && (
        <p className="text-destructive animate-in fade-in mt-2 text-xs duration-200">
          {status.error ?? "unknown error"}
        </p>
      )}
      {status?.ok && status.viewer && !editing && (
        <p className="text-muted-foreground animate-in fade-in mt-2 text-xs duration-200">
          Authenticated as <span className="text-foreground">{status.viewer}</span>
        </p>
      )}
      {provider.id === "linear" && provider.connected && !editing && (
        <LinearScopeRow />
      )}
    </li>
  );
}

const SCOPE_OPTIONS: Array<{
  value: LinearBacklogScope;
  label: string;
  hint: string;
}> = [
  {
    value: "in_progress",
    label: "In progress",
    hint: "Only tickets actively started.",
  },
  {
    value: "actionable",
    label: "In progress + Todo",
    hint: "What you should be working on now. Default.",
  },
  {
    value: "all_open",
    label: "All open",
    hint: "Every assigned ticket that's not done — including the long Backlog tail.",
  },
];

function LinearScopeRow() {
  const qc = useQueryClient();
  const { data: settings } = useQuery<LinearSettings>({
    queryKey: qk.linearSettings(),
    queryFn: linearSettingsGet,
  });

  const update = useMutation({
    mutationFn: (next: LinearBacklogScope) =>
      linearSettingsSet({
        backlog_scope: next,
        viewer_name: settings?.viewer_name ?? null,
      }),
    onSuccess: async (_, next) => {
      qc.setQueryData<LinearSettings>(qk.linearSettings(), (prev) => ({
        backlog_scope: next,
        viewer_name: prev?.viewer_name ?? null,
      }));
      // Backlog cache (Rust-side) was already cleared by the setter; force
      // the frontend cache to refetch too so Home + TicketPicker react now.
      await qc.invalidateQueries({ queryKey: qk.ticketBacklogAll() });
    },
    onError: (e) => toast.error("Couldn't save scope", { description: String(e) }),
  });

  const current = settings?.backlog_scope ?? "actionable";

  return (
    <div className="border-border/60 mt-3 border-t pt-3">
      <div className="text-muted-foreground mb-1.5 text-[11px] uppercase tracking-wide">
        Backlog scope
      </div>
      <div className="flex flex-wrap gap-1.5">
        {SCOPE_OPTIONS.map((opt) => {
          const active = current === opt.value;
          return (
            <button
              key={opt.value}
              type="button"
              onClick={() => !active && update.mutate(opt.value)}
              title={opt.hint}
              disabled={update.isPending}
              className={`rounded-md border px-2.5 py-1 text-xs transition-colors ${
                active
                  ? "border-foreground/30 bg-accent text-foreground"
                  : "border-border bg-card text-muted-foreground hover:bg-accent hover:text-foreground"
              }`}
            >
              {opt.label}
            </button>
          );
        })}
      </div>
      <p className="text-muted-foreground/80 mt-2 text-[11px] leading-relaxed">
        {SCOPE_OPTIONS.find((o) => o.value === current)?.hint}
      </p>
    </div>
  );
}
