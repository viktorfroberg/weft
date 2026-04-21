import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Plus } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { importScheme } from "@/lib/themes/import";
import { loadPresets } from "@/lib/themes/presets";
import { usePrefs } from "@/stores/prefs";
import type { ColorScheme } from "@/lib/themes/schemes";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Paste-in + preset-browse dialog for adding a `ColorScheme` to
 * `prefs.userSchemes`. Two tabs:
 *   - Paste: textarea, auto-detects base16/base24 YAML or iTerm2 XML
 *     plist; shows a palette swatch preview as the user types.
 *   - Presets: 15-scheme curated grid, one click to add.
 */
export function AddSchemeDialog({ open, onOpenChange }: Props) {
  const [tab, setTab] = useState<"paste" | "presets">("paste");
  const [text, setText] = useState("");
  const addUserScheme = usePrefs((s) => s.addUserScheme);

  const parsed: { scheme?: ColorScheme; error?: string } = useMemo(() => {
    if (!text.trim()) return {};
    try {
      return { scheme: importScheme(text) };
    } catch (e) {
      return { error: String(e) };
    }
  }, [text]);

  const presets = useMemo(() => loadPresets(), []);

  const add = (s: ColorScheme) => {
    try {
      addUserScheme(s);
    } catch (e) {
      toast.error("Couldn't add scheme", { description: String(e) });
      return;
    }
    toast.success(`Added “${s.name}”`, {
      description: `${s.appearance === "dark" ? "Dark" : "Light"} scheme — available in the ${s.appearance} scheme picker.`,
    });
    setText("");
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Add color scheme</DialogTitle>
          <DialogDescription>
            Paste a base16 / base24 YAML or iTerm2 `.itermcolors` XML, or
            pick from the curated presets. All derivations (chrome,
            terminal palette, Monaco theme) happen on the fly — no
            per-scheme hand-tuning needed.
          </DialogDescription>
        </DialogHeader>

        <div className="bg-muted flex shrink-0 rounded-md p-0.5">
          <TabBtn active={tab === "paste"} onClick={() => setTab("paste")}>
            Paste
          </TabBtn>
          <TabBtn
            active={tab === "presets"}
            onClick={() => setTab("presets")}
          >
            Presets
          </TabBtn>
        </div>

        {tab === "paste" && (
          <div className="space-y-3">
            <Textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder={`scheme: "Nord"\nauthor: "Arctic Ice Studio"\nbase00: "2e3440"\nbase01: "3b4252"\n…`}
              className="min-h-[180px] resize-y font-mono text-xs"
              autoFocus
            />
            {parsed.scheme && (
              <div>
                <p className="text-muted-foreground mb-1.5 text-xs">
                  Preview — <span className="text-foreground">{parsed.scheme.name}</span>{" "}
                  ({parsed.scheme.appearance})
                </p>
                <SwatchRow scheme={parsed.scheme} />
              </div>
            )}
            {parsed.error && (
              <p className="text-destructive text-xs">{parsed.error}</p>
            )}
          </div>
        )}

        {tab === "presets" && (
          <div className="grid max-h-[400px] grid-cols-2 gap-2 overflow-y-auto pr-1">
            {presets.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => add(p)}
                className="border-border hover:bg-accent flex items-center gap-2 rounded-md border p-2 text-left transition-colors"
              >
                <SwatchRow scheme={p} compact />
                <span className="flex-1 truncate text-xs font-medium">
                  {p.name}
                </span>
                <Plus size={12} className="text-muted-foreground shrink-0" />
              </button>
            ))}
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          {tab === "paste" && (
            <Button
              onClick={() => parsed.scheme && add(parsed.scheme)}
              disabled={!parsed.scheme}
            >
              Add scheme
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex-1 rounded px-3 py-1 text-xs transition-colors ${
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

/** Six-swatch palette strip — bg, fg, red, green, blue, yellow. Used as
 * the visual "what this scheme looks like" summary everywhere. */
export function SwatchRow({
  scheme,
  compact = false,
}: {
  scheme: ColorScheme;
  compact?: boolean;
}) {
  const size = compact ? 12 : 18;
  const cells = [
    scheme.terminal.background,
    scheme.terminal.foreground,
    scheme.terminal.red,
    scheme.terminal.green,
    scheme.terminal.blue,
    scheme.terminal.yellow,
  ];
  return (
    <div className="flex gap-0.5">
      {cells.map((c, i) => (
        <div
          key={i}
          className="border-border rounded-sm border"
          style={{ width: size, height: size, background: c }}
        />
      ))}
    </div>
  );
}
