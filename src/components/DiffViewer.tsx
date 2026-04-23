import { useEffect, useState } from "react";
import { DiffEditor } from "@monaco-editor/react";
import type { Monaco } from "@monaco-editor/react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { worktreeFileSides } from "@/lib/commands";
import type { FileChange } from "@/lib/commands";
import { useActiveScheme } from "@/lib/theme";
import { usePrefs } from "@/stores/prefs";
import { useCustomFonts } from "@/stores/custom_fonts";
import { findFont } from "@/lib/themes/fonts";

interface Props {
  worktreePath: string;
  baseBranch: string;
  change: FileChange;
  onClose: () => void;
}

const KIND_LABEL: Record<FileChange["kind"], string> = {
  added: "added",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
  copied: "copied",
  untracked: "untracked",
  conflicted: "conflicted",
  type_changed: "type changed",
  other: "changed",
};

function languageFromPath(path: string): string {
  const lower = path.toLowerCase();
  // Monaco picks these up by file extension; a simple mapping keeps us
  // working even where the Monaco auto-detection doesn't fire (webpack
  // minimized bundles etc).
  if (lower.endsWith(".ts") || lower.endsWith(".tsx")) return "typescript";
  if (lower.endsWith(".js") || lower.endsWith(".jsx") || lower.endsWith(".mjs"))
    return "javascript";
  if (lower.endsWith(".rs")) return "rust";
  if (lower.endsWith(".go")) return "go";
  if (lower.endsWith(".py")) return "python";
  if (lower.endsWith(".rb")) return "ruby";
  if (lower.endsWith(".php")) return "php";
  if (lower.endsWith(".json")) return "json";
  if (lower.endsWith(".yaml") || lower.endsWith(".yml")) return "yaml";
  if (lower.endsWith(".toml")) return "toml";
  if (lower.endsWith(".md") || lower.endsWith(".mdx")) return "markdown";
  if (lower.endsWith(".css")) return "css";
  if (lower.endsWith(".scss") || lower.endsWith(".sass")) return "scss";
  if (lower.endsWith(".html")) return "html";
  if (lower.endsWith(".sql")) return "sql";
  if (lower.endsWith(".sh") || lower.endsWith(".bash")) return "shell";
  return "plaintext";
}

export function DiffViewer({
  worktreePath,
  baseBranch,
  change,
  onClose,
}: Props) {
  const [base, setBase] = useState<string | null>("");
  const [current, setCurrent] = useState<string | null>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const scheme = useActiveScheme();
  const fontFamilyId = usePrefs((s) => s.terminalFontFamily);
  const customFonts = useCustomFonts((s) => s.rows);
  const font = findFont(fontFamilyId, customFonts);
  const themeName = `weft-${scheme.id}`;

  // Register + activate the scheme's Monaco theme on mount AND on scheme
  // change. `defineTheme` is idempotent, so re-calling on every scheme
  // swap is cheap. Done in a `beforeMount` callback so the editor
  // instance is theme'd on first paint (not post-mount flash).
  const beforeMount = (monaco: Monaco) => {
    monaco.editor.defineTheme(themeName, scheme.monaco);
    // Also expose the monaco global so `applyTheme()` in
    // `src/lib/themes/apply.ts` can call setTheme on future swaps even
    // when DiffViewer isn't mounted yet (then re-applies here on mount).
    (globalThis as unknown as { monaco?: typeof monaco }).monaco = monaco;
  };

  // Re-define on scheme change so mid-edit user-scheme tweaks land.
  useEffect(() => {
    const g = globalThis as unknown as {
      monaco?: { editor: { defineTheme: (n: string, d: typeof scheme.monaco) => void; setTheme: (n: string) => void } };
    };
    g.monaco?.editor.defineTheme(themeName, scheme.monaco);
    g.monaco?.editor.setTheme(themeName);
  }, [scheme, themeName]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    worktreeFileSides(worktreePath, baseBranch, change.path)
      .then((res) => {
        if (cancelled) return;
        setBase(res.base);
        setCurrent(res.current);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [worktreePath, baseBranch, change.path]);

  const language = languageFromPath(change.path);
  const isBinary = base === null && current === null && !loading && !error;

  return (
    <div className="flex h-full flex-col">
      <header className="border-border flex items-center gap-2 border-b px-3 py-2">
        <Button size="sm" variant="ghost" onClick={onClose}>
          ← Back
        </Button>
        <Badge variant="secondary" className="font-mono text-[10px]">
          {KIND_LABEL[change.kind]}
        </Badge>
        <span className="truncate font-mono text-xs">{change.path}</span>
      </header>

      <div className="flex-1 overflow-hidden">
        {loading && (
          <div className="text-muted-foreground p-6 text-sm">Loading diff…</div>
        )}
        {error && (
          <div className="text-destructive p-6 text-sm">{error}</div>
        )}
        {!loading && !error && isBinary && (
          <div className="text-muted-foreground p-6 text-sm">
            Binary file — no diff preview.
          </div>
        )}
        {!loading && !error && !isBinary && (
          <DiffEditor
            beforeMount={beforeMount}
            theme={themeName}
            original={base ?? ""}
            modified={current ?? ""}
            language={language}
            height="100%"
            options={{
              readOnly: true,
              renderSideBySide: true,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              fontSize: 12,
              fontFamily: font.css,
              renderOverviewRuler: false,
              diffWordWrap: "inherit",
              wordWrap: "off",
            }}
          />
        )}
      </div>
    </div>
  );
}
