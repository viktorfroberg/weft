import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  Bell,
  Columns2,
  Eye,
  Monitor,
  Moon,
  MousePointer2,
  Palette,
  Plus,
  Squircle,
  Sun,
  Type,
  Underline,
  X,
} from "lucide-react";
import { usePrefs, type ThemePref } from "@/stores/prefs";
import { useCustomFonts } from "@/stores/custom_fonts";
import {
  fontPairItalicPick,
  fontRemove,
  fontRename,
  fontSetLigatures,
  fontSetVariable,
  fontUnpairItalic,
  type CustomFontRow,
} from "@/lib/commands";
import { useEffectiveTheme } from "@/lib/theme";
import { BUNDLED_SCHEMES, type ColorScheme } from "@/lib/themes/schemes";
import { FONT_FAMILIES, mergeFonts } from "@/lib/themes/fonts";
import { fireTestBell } from "@/lib/themes/bell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { TerminalPreview } from "@/components/TerminalPreview";
import { Card } from "./Card";
import { AddSchemeDialog, SwatchRow } from "./AddSchemeDialog";
import { AddSystemFontDialog } from "./AddSystemFontDialog";
import { useConfirm } from "@/components/ConfirmDialog";

const THEME_OPTIONS: Array<{
  value: ThemePref;
  label: string;
  Icon: typeof Sun;
}> = [
  { value: "light", label: "Light", Icon: Sun },
  { value: "dark", label: "Dark", Icon: Moon },
  { value: "system", label: "System", Icon: Monitor },
];

export function AppearanceTab() {
  // Fine-grained selectors — each subscription is narrow so a slider
  // drag re-renders only the components whose value changed, not the
  // whole tab. (Whole-object subscription would re-render every Card
  // on every pref tick.)
  const theme = usePrefs((s) => s.theme);
  const setTheme = usePrefs((s) => s.setTheme);
  const schemeDark = usePrefs((s) => s.schemeDark);
  const schemeLight = usePrefs((s) => s.schemeLight);
  const setSchemeDark = usePrefs((s) => s.setSchemeDark);
  const setSchemeLight = usePrefs((s) => s.setSchemeLight);
  const userSchemes = usePrefs((s) => s.userSchemes);
  const removeUserScheme = usePrefs((s) => s.removeUserScheme);
  const setAppearance = usePrefs((s) => s.setAppearance);
  const terminalFontFamily = usePrefs((s) => s.terminalFontFamily);
  const terminalFontWeight = usePrefs((s) => s.terminalFontWeight);
  const terminalFontSize = usePrefs((s) => s.terminalFontSize);
  const terminalLineHeight = usePrefs((s) => s.terminalLineHeight);
  const terminalLigatures = usePrefs((s) => s.terminalLigatures);
  const terminalPadX = usePrefs((s) => s.terminalPadX);
  const terminalPadY = usePrefs((s) => s.terminalPadY);
  const boldIsBright = usePrefs((s) => s.boldIsBright);
  const cursorStyle = usePrefs((s) => s.cursorStyle);
  const cursorBlink = usePrefs((s) => s.cursorBlink);
  const bellStyle = usePrefs((s) => s.bellStyle);
  const customFonts = useCustomFonts((s) => s.rows);

  const effective = useEffectiveTheme();
  const [addOpen, setAddOpen] = useState(false);
  const [addFontOpen, setAddFontOpen] = useState(false);
  const confirm = useConfirm();

  const allSchemes: ColorScheme[] = [...BUNDLED_SCHEMES, ...userSchemes];
  const darkSchemes = allSchemes.filter((s) => s.appearance === "dark");
  const lightSchemes = allSchemes.filter((s) => s.appearance === "light");

  const allFonts = mergeFonts(customFonts);
  const activeFont =
    allFonts.find((f) => f.id === terminalFontFamily) ?? FONT_FAMILIES[0];

  // Stale-pref guard: if `terminalFontFamily` points at a custom id
  // that no longer exists (e.g. corrupt localStorage, cross-device
  // sync brought in an id we don't have here), reset to the default
  // bundled font. Without this the `<select value={...}>` shows blank
  // and React warns "value doesn't match an option".
  useEffect(() => {
    if (allFonts.some((f) => f.id === terminalFontFamily)) return;
    setAppearance({ terminalFontFamily: "jetbrains-mono" });
    // Run only on mount + when the row set changes (custom font
    // added/removed). Intentionally not a dep on terminalFontFamily
    // itself — the setter would loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allFonts.length]);

  const renameCustomFont = (id: string, newName: string) => {
    fontRename(id, newName).catch((err) =>
      console.warn("fontRename failed", err),
    );
  };
  const toggleLigatures = (id: string, on: boolean) => {
    fontSetLigatures(id, on).catch((err) =>
      console.warn("fontSetLigatures failed", err),
    );
  };
  const toggleVariable = (id: string, on: boolean) => {
    fontSetVariable(id, on).catch((err) =>
      console.warn("fontSetVariable failed", err),
    );
  };
  const pairItalic = async (c: CustomFontRow) => {
    try {
      const updated = await fontPairItalicPick(c.id);
      if (updated) {
        toast.success(`Paired italic for ${updated.display_name}`);
      }
    } catch (err) {
      toast.error("Couldn't pair italic", { description: String(err) });
    }
  };
  const unpairItalic = (c: CustomFontRow) => {
    fontUnpairItalic(c.id).catch((err) =>
      console.warn("fontUnpairItalic failed", err),
    );
  };

  const deleteCustomFont = async (c: CustomFontRow) => {
    const ok = await confirm({
      title: `Remove ${c.display_name}?`,
      description:
        "It'll disappear from the family dropdown and the imported file is deleted from weft's data dir. The original on your system is untouched.",
      confirmText: "Remove",
      destructive: true,
    });
    if (!ok) return;
    // Rewrite the active pref BEFORE removal so the dropdown never
    // holds a stale id between the two state updates.
    if (terminalFontFamily === `custom:${c.id}`) {
      setAppearance({ terminalFontFamily: "jetbrains-mono" });
    }
    fontRemove(c.id).catch((err) => console.warn("fontRemove failed", err));
  };

  return (
    <div className="space-y-4">
      <Card
        title="Preview"
        description="Live sample. Reflects your current scheme, font, weight, ligatures, cursor, and padding choices."
        Icon={Eye}
      >
        <TerminalPreview />
      </Card>

      <Card
        title="Theme"
        description="Weft follows your macOS appearance by default. Cross-fades between light and dark so the flip isn't jarring."
        Icon={Palette}
      >
        <Row label="Mode">
          <Segmented
            value={theme}
            onChange={(v) => setTheme(v as ThemePref)}
            options={THEME_OPTIONS.map((o) => ({
              value: o.value,
              label: o.label,
              Icon: o.Icon,
            }))}
          />
        </Row>
        {theme === "system" && (
          <p className="text-muted-foreground/80 mt-2 text-xs">
            Following system — currently{" "}
            <span className="text-foreground">{effective}</span>.
          </p>
        )}
      </Card>

      <Card
        title="Color scheme"
        description="Swap bundled schemes or paste your own. Chrome, terminal palette, and Monaco diff all flow from one coherent Base24 palette."
        Icon={Palette}
      >
        <SchemeGrid
          label="Dark"
          value={schemeDark}
          options={darkSchemes}
          onChange={setSchemeDark}
          onRemove={(id) => {
            if (schemeDark === id) setSchemeDark("tokyo-night");
            removeUserScheme(id);
          }}
        />
        <SchemeGrid
          label="Light"
          value={schemeLight}
          options={lightSchemes}
          onChange={setSchemeLight}
          onRemove={(id) => {
            if (schemeLight === id) setSchemeLight("catppuccin-latte");
            removeUserScheme(id);
          }}
        />
        <div className="mt-3 flex justify-end">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setAddOpen(true)}
            className="h-7 gap-1 text-xs"
          >
            <Plus size={12} />
            Add scheme…
          </Button>
        </div>
        <AddSchemeDialog open={addOpen} onOpenChange={setAddOpen} />
      </Card>

      <Card
        title="Font"
        description="Terminal font, weight, size, line height, and ligatures. Monaco diff uses the same family."
        Icon={Type}
      >
        <Row label="Family">
          <select
            value={terminalFontFamily}
            onChange={(e) =>
              setAppearance({ terminalFontFamily: e.target.value })
            }
            className="bg-background border-border h-7 rounded border px-2 text-xs"
          >
            {allFonts.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
                {f.kind === "custom" ? "  ·  custom" : ""}
              </option>
            ))}
          </select>
        </Row>
        <Row label="Weight">
          <Segmented
            value={String(terminalFontWeight)}
            onChange={(v) =>
              setAppearance({
                terminalFontWeight: Number(v) as 400 | 500 | 600,
              })
            }
            options={[
              { value: "400", label: "400" },
              { value: "500", label: "500" },
              { value: "600", label: "600" },
            ]}
            disabled={!activeFont.variable}
          />
        </Row>
        <Row label={`Size — ${terminalFontSize}px`}>
          <input
            type="range"
            min={10}
            max={20}
            step={1}
            value={terminalFontSize}
            onChange={(e) =>
              setAppearance({ terminalFontSize: Number(e.target.value) })
            }
            className="accent-primary w-40"
          />
        </Row>
        <Row label={`Line height — ${terminalLineHeight.toFixed(2)}`}>
          <input
            type="range"
            min={1.0}
            max={1.5}
            step={0.05}
            value={terminalLineHeight}
            onChange={(e) =>
              setAppearance({
                terminalLineHeight: Number(e.target.value),
              })
            }
            className="accent-primary w-40"
          />
        </Row>
        <Row
          label="Ligatures"
          hint={
            activeFont.ligatures
              ? "Toggling rebuilds the terminal (scrollback preserved on the preview; cleared on live tabs)."
              : `${activeFont.name} has no ligatures.`
          }
        >
          <Switch
            checked={terminalLigatures && activeFont.ligatures}
            onCheckedChange={(v) =>
              setAppearance({ terminalLigatures: v })
            }
            disabled={!activeFont.ligatures}
          />
        </Row>
      </Card>

      <Card
        title="Custom fonts"
        description="Add a font file from your disk. Required for any third-party font (Maple Mono, Berkeley Mono, MonoLisa, etc.) — macOS's webview can't see your Font Book installs by name."
        Icon={Type}
      >
        <div className="space-y-2">
          {customFonts.length === 0 ? (
            <p className="text-muted-foreground text-xs">
              None added yet.
            </p>
          ) : (
            <ul className="space-y-2">
              {customFonts.map((c) => (
                <li
                  key={c.id}
                  className="border-border space-y-2 rounded-md border p-2"
                >
                  <div className="flex items-center gap-2">
                    <Input
                      defaultValue={c.display_name}
                      onBlur={(e) => {
                        const v = e.target.value.trim();
                        if (v && v !== c.display_name) {
                          renameCustomFont(c.id, v);
                        } else if (!v) {
                          // Reset the input to the existing name if
                          // they cleared it.
                          e.target.value = c.display_name;
                        }
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          (e.target as HTMLInputElement).blur();
                        }
                      }}
                      className="h-7 flex-1 text-xs"
                      aria-label="Display name"
                    />
                    <span className="text-muted-foreground font-mono text-[10px]">
                      {c.file_basename}
                    </span>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => deleteCustomFont(c)}
                      className="h-7 w-7 p-0"
                      aria-label="Remove custom font"
                    >
                      <X size={12} />
                    </Button>
                  </div>
                  <div className="flex items-center justify-between gap-3 text-xs">
                    <label className="flex items-center gap-1.5">
                      <Switch
                        checked={c.ligatures}
                        onCheckedChange={(v) => toggleLigatures(c.id, v)}
                      />
                      <span>Ligatures</span>
                    </label>
                    <label
                      className="flex items-center gap-1.5"
                      title="Only enable if your font has multiple weight files installed under one family. Single-weight fonts can show subtle cell-size jitter from synthetic-bold fallback."
                    >
                      <Switch
                        checked={c.variable}
                        onCheckedChange={(v) => toggleVariable(c.id, v)}
                      />
                      <span>Variable / weight slider</span>
                    </label>
                  </div>
                  <div className="flex items-center justify-between gap-2 text-xs">
                    {c.italic_file_basename ? (
                      <>
                        <span className="text-muted-foreground truncate">
                          Italic: <span className="font-mono text-[10px]">{c.italic_file_basename}</span>
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => unpairItalic(c)}
                          className="h-6 text-xs"
                        >
                          Remove italic
                        </Button>
                      </>
                    ) : (
                      <>
                        <span className="text-muted-foreground/70">
                          No italic file paired — italic ANSI text falls back
                          to the regular face.
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => pairItalic(c)}
                          className="h-6 text-xs"
                        >
                          Pair italic file…
                        </Button>
                      </>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          )}
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setAddFontOpen(true)}
            className="h-7 gap-1 text-xs"
          >
            <Plus size={12} />
            Add custom font…
          </Button>
        </div>
        <AddSystemFontDialog open={addFontOpen} onOpenChange={setAddFontOpen} />
      </Card>

      <Card
        title="Terminal"
        description="Padding around the terminal canvas + bold-text behavior."
        Icon={Squircle}
      >
        <Row label={`Padding horizontal — ${terminalPadX}px`}>
          <input
            type="range"
            min={0}
            max={24}
            step={1}
            value={terminalPadX}
            onChange={(e) =>
              setAppearance({ terminalPadX: Number(e.target.value) })
            }
            className="accent-primary w-40"
          />
        </Row>
        <Row label={`Padding vertical — ${terminalPadY}px`}>
          <input
            type="range"
            min={0}
            max={24}
            step={1}
            value={terminalPadY}
            onChange={(e) =>
              setAppearance({ terminalPadY: Number(e.target.value) })
            }
            className="accent-primary w-40"
          />
        </Row>
        <Row
          label="Bold uses bright colors"
          hint="Off: bold renders as font-weight 600. On: substitutes to bright ANSI."
        >
          <Switch
            checked={boldIsBright}
            onCheckedChange={(v) => setAppearance({ boldIsBright: v })}
          />
        </Row>
      </Card>

      <Card title="Cursor" description="Shape and blink." Icon={MousePointer2}>
        <Row label="Style">
          <Segmented
            value={cursorStyle}
            onChange={(v) =>
              setAppearance({
                cursorStyle: v as "block" | "bar" | "underline",
              })
            }
            options={[
              { value: "block", label: "Block", Icon: Squircle },
              { value: "bar", label: "Bar", Icon: Columns2 },
              { value: "underline", label: "Underline", Icon: Underline },
            ]}
          />
        </Row>
        <Row label="Blink">
          <Switch
            checked={cursorBlink}
            onCheckedChange={(v) => setAppearance({ cursorBlink: v })}
          />
        </Row>
      </Card>

      <Card title="Bell" description="Off, visual flash, audible beep, or both." Icon={Bell}>
        <Row label="Style">
          <Segmented
            value={bellStyle}
            onChange={(v) =>
              setAppearance({
                bellStyle: v as "off" | "visual" | "audible" | "both",
              })
            }
            options={[
              { value: "off", label: "Off" },
              { value: "visual", label: "Visual" },
              { value: "audible", label: "Audible" },
              { value: "both", label: "Both" },
            ]}
          />
        </Row>
        <div className="mt-2 flex justify-end">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => fireTestBell()}
            className="h-7 text-xs"
            disabled={bellStyle === "off"}
          >
            Test bell
          </Button>
        </div>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="flex items-center justify-between gap-3 py-1.5 text-sm">
      <div className="min-w-0 flex-1">
        <span className="text-muted-foreground text-sm">{label}</span>
        {hint && (
          <p className="text-muted-foreground/70 text-xs">{hint}</p>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </label>
  );
}

function Segmented<T extends string>({
  value,
  onChange,
  options,
  disabled = false,
}: {
  value: T;
  onChange: (v: T) => void;
  options: Array<{ value: T; label: string; Icon?: typeof Sun }>;
  disabled?: boolean;
}) {
  return (
    <div
      className={`bg-muted flex rounded-md p-0.5 ${disabled ? "opacity-50" : ""}`}
    >
      {options.map(({ value: v, label, Icon }) => {
        const active = v === value;
        return (
          <button
            key={v}
            type="button"
            disabled={disabled}
            onClick={() => onChange(v)}
            className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-xs transition-all ${
              active
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            } ${disabled ? "cursor-not-allowed" : ""}`}
          >
            {Icon && <Icon size={12} />}
            {label}
          </button>
        );
      })}
    </div>
  );
}

/** Uniform-cell grid picker for color schemes. Each cell is the same
 *  width regardless of scheme name length — the previous wrap-strip
 *  layout had variable-width pills which packed unevenly and could
 *  overflow at long names. CSS grid with `auto-fill, minmax` makes
 *  the cells reflow into N columns based on container width while
 *  keeping each cell predictable.
 *
 *  Each cell shows: swatch row + truncated name. Active cell gets a
 *  ring + filled background. User-added schemes show a × on hover. */
function SchemeGrid({
  label,
  value,
  options,
  onChange,
  onRemove,
}: {
  label: string;
  value: string;
  options: ColorScheme[];
  onChange: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="py-3 first:pt-1">
      <div className="text-muted-foreground mb-2 text-sm">{label}</div>
      <div
        className="grid gap-2"
        style={{ gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))" }}
      >
        {options.map((s) => {
          const active = s.id === value;
          const isUser =
            s.id.startsWith("user-") || s.id.startsWith("preset-");
          return (
            <div
              key={s.id}
              className={`group border-border relative flex items-center gap-2 rounded-md border px-2.5 py-2 text-xs transition-all ${
                active
                  ? "ring-primary bg-accent text-foreground ring-2"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
            >
              <button
                type="button"
                onClick={() => onChange(s.id)}
                className="flex min-w-0 flex-1 items-center gap-2"
              >
                <SwatchRow scheme={s} compact />
                <span className="truncate">{s.name}</span>
              </button>
              {isUser && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemove(s.id);
                  }}
                  className="text-muted-foreground hover:text-destructive opacity-0 transition-opacity group-hover:opacity-100"
                  title={`Remove ${s.name}`}
                >
                  <X size={11} />
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
