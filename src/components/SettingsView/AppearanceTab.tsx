import { useState } from "react";
import {
  Bell,
  Columns2,
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
import { useEffectiveTheme } from "@/lib/theme";
import { BUNDLED_SCHEMES, type ColorScheme } from "@/lib/themes/schemes";
import { FONT_FAMILIES } from "@/lib/themes/fonts";
import { fireTestBell } from "@/lib/themes/bell";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { TerminalPreview } from "@/components/TerminalPreview";
import { Card } from "./Card";
import { AddSchemeDialog, SwatchRow } from "./AddSchemeDialog";

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

  const effective = useEffectiveTheme();
  const [addOpen, setAddOpen] = useState(false);

  const allSchemes: ColorScheme[] = [...BUNDLED_SCHEMES, ...userSchemes];
  const darkSchemes = allSchemes.filter((s) => s.appearance === "dark");
  const lightSchemes = allSchemes.filter((s) => s.appearance === "light");

  const activeFont =
    FONT_FAMILIES.find((f) => f.id === terminalFontFamily) ??
    FONT_FAMILIES[0];

  return (
    <div className="space-y-4">
      {/* Preview sits above the controls so scheme/font/size changes
          are visually obvious. Not inside a Card — it's the focus. */}
      <TerminalPreview />

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
        <Row label="Dark">
          <SchemeSegmented
            value={schemeDark}
            options={darkSchemes}
            onChange={setSchemeDark}
            onRemove={(id) => {
              if (schemeDark === id) {
                setSchemeDark("tokyo-night");
              }
              removeUserScheme(id);
            }}
          />
        </Row>
        <Row label="Light">
          <SchemeSegmented
            value={schemeLight}
            options={lightSchemes}
            onChange={setSchemeLight}
            onRemove={(id) => {
              if (schemeLight === id) {
                setSchemeLight("catppuccin-latte");
              }
              removeUserScheme(id);
            }}
          />
        </Row>
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
            {FONT_FAMILIES.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
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

function SchemeSegmented({
  value,
  options,
  onChange,
  onRemove,
}: {
  value: string;
  options: ColorScheme[];
  onChange: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="bg-muted flex flex-wrap gap-0.5 rounded-md p-0.5">
      {options.map((s) => {
        const active = s.id === value;
        // Bundled schemes (tokyo-night, one-dark, catppuccin-latte,
        // github-light) live in BUNDLED_SCHEMES and can't be removed.
        // Everything added via paste-in / presets gets a "user-" or
        // "preset-" id prefix.
        const isUser = s.id.startsWith("user-") || s.id.startsWith("preset-");
        return (
          <div
            key={s.id}
            className={`group flex items-center gap-1.5 rounded px-2 py-1 text-xs transition-all ${
              active
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            <button
              type="button"
              onClick={() => onChange(s.id)}
              className="flex items-center gap-1.5"
            >
              <SwatchRow scheme={s} compact />
              {s.name}
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
                <X size={10} />
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
