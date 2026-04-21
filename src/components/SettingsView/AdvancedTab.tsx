import { useState } from "react";
import {
  Check,
  Copy,
  ExternalLink,
  Folder,
  Keyboard,
  Sparkles,
  Wrench,
} from "lucide-react";
import { toast } from "sonner";
import { openPath } from "@tauri-apps/plugin-opener";
import type { AppInfo } from "@/lib/commands";
import { Card } from "./Card";

const SHORTCUTS: [string, string][] = [
  ["⌘K", "Command palette (jump anywhere)"],
  ["⌘⇧N", "New task (compose)"],
  ["⌘⇧O", "Recent tasks"],
  ["⌘P", "Add a repo"],
  ["⌘L", "Launch default agent (in task view)"],
  ["⌘T", "New terminal tab picker (in task view)"],
  ["⌘1…⌘9", "Jump to task (or focus worktree in task view)"],
  ["⌘B", "Toggle sidebar"],
  ["⌘\\", "Toggle changes panel"],
  ["⌘/", "Show keyboard shortcuts"],
  ["⌘↵", "Commit all (in commit message)"],
  ["Esc", "Back / close"],
];

export function AdvancedTab({ info }: { info: AppInfo | null }) {
  return (
    <>
      <Card title="Paths" description="Where weft stores your stuff." Icon={Folder}>
        {info ? (
          <div className="space-y-1">
            <PathRow label="Worktrees" value={info.worktrees_dir} reveal />
            <PathRow label="Data dir" value={info.data_dir} reveal />
            <PathRow label="Database" value={info.db_path} />
            <PathRow label="Hook manifest" value={info.hook_manifest_path} />
          </div>
        ) : (
          <p className="text-muted-foreground text-xs">loading…</p>
        )}
      </Card>

      <Card
        title="Agent integration"
        description="Env vars + hook server exposed to agents spawned inside weft terminals."
        Icon={Sparkles}
      >
        {info && (
          <div className="space-y-1">
            <PathRow
              label="Hook port"
              value={info.hook_port ? String(info.hook_port) : "not running"}
            />
            <PathRow label="Shell" value={info.default_shell} />
          </div>
        )}
        <p className="text-muted-foreground/80 mt-3 text-xs leading-relaxed">
          Agents get <code className="font-mono">WEFT_TASK_ID</code>,{" "}
          <code className="font-mono">WEFT_TASK_SLUG</code>,{" "}
          <code className="font-mono">WEFT_TASK_BRANCH</code>, and{" "}
          <code className="font-mono">WEFT_HOOKS_URL</code> in env. POST JSON to{" "}
          <code className="font-mono">$WEFT_HOOKS_URL</code> with an{" "}
          <code className="font-mono">Authorization: Bearer &lt;token&gt;</code>{" "}
          header (token in the hook manifest) to report status.
        </p>
      </Card>

      <Card
        title="Keyboard shortcuts"
        description="User-configurable bindings land in a later release."
        Icon={Keyboard}
      >
        <ul className="space-y-0.5">
          {SHORTCUTS.map(([keys, desc]) => (
            <li
              key={keys}
              className="border-border flex items-center justify-between border-b py-1.5 last:border-b-0"
            >
              <span className="text-sm">{desc}</span>
              <kbd className="bg-muted text-muted-foreground rounded border px-1.5 py-0.5 font-mono text-[11px]">
                {keys}
              </kbd>
            </li>
          ))}
        </ul>
      </Card>

      <Card
        title="Coming later"
        description="On the roadmap but not shipped yet."
        Icon={Wrench}
      >
        <ul className="text-muted-foreground/80 space-y-1 text-sm">
          <li>– Editable keyboard shortcuts</li>
          <li>– Notification preferences (per-state, sound toggle)</li>
          <li>– Agent presets (Claude Code / Codex / Gemini profiles)</li>
          <li>– Custom worktree base directory</li>
          <li>
            – PR creation via <code className="font-mono">gh</code>
          </li>
        </ul>
      </Card>
    </>
  );
}

function PathRow({
  label,
  value,
  reveal,
}: {
  label: string;
  value: string;
  reveal?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      toast.success("Copied to clipboard");
      setTimeout(() => setCopied(false), 1200);
    } catch {
      toast.error("Couldn't copy");
    }
  };
  return (
    <div className="border-border group flex items-center justify-between gap-3 border-b py-2 last:border-b-0">
      <span className="text-muted-foreground shrink-0 text-xs uppercase tracking-wider">
        {label}
      </span>
      <div className="flex min-w-0 items-center gap-1">
        <code className="truncate font-mono text-xs" title={value}>
          {value}
        </code>
        <button
          type="button"
          onClick={onCopy}
          className="text-muted-foreground hover:text-foreground opacity-0 transition-opacity group-hover:opacity-100"
          title="Copy path"
        >
          {copied ? (
            <Check size={12} className="text-emerald-500" />
          ) : (
            <Copy size={12} />
          )}
        </button>
        {reveal && (
          <button
            type="button"
            onClick={() => openPath(value).catch(() => {})}
            className="text-muted-foreground hover:text-foreground opacity-0 transition-opacity group-hover:opacity-100"
            title="Reveal in Finder"
          >
            <ExternalLink size={12} />
          </button>
        )}
      </div>
    </div>
  );
}
