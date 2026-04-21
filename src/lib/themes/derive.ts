import type {
  ChromeTokens,
  ColorScheme,
  MonacoTheme,
  XtermTheme,
} from "./schemes";

/**
 * Base24 slot map. `base00` is the darkest background (light schemes
 * invert the light-to-dark gradient), `base07` the brightest fg,
 * `base08–0F` are accent colors roughly mapping to ANSI red, orange,
 * yellow, green, cyan, blue, magenta, brown.
 *
 * Reference: <https://github.com/tinted-theming/base24>.
 */
export type Base24 = {
  base00: string; // default bg
  base01: string; // lighter bg (surface)
  base02: string; // selection bg
  base03: string; // comments / subtle text
  base04: string; // dark foreground
  base05: string; // default foreground
  base06: string; // lighter foreground
  base07: string; // brightest foreground
  base08: string; // red
  base09: string; // orange
  base0A: string; // yellow
  base0B: string; // green
  base0C: string; // cyan
  base0D: string; // blue
  base0E: string; // magenta
  base0F: string; // brown / accent
};

/** Blend two hex colors in straight RGB space. Not perceptually ideal
 * (oklch would be better) but produces predictable mid-tones for
 * sidebar-between-bg-and-surface derivations, and avoids the muddy
 * oklch-of-hex issues that motivated the "no color-mix" decision. */
export function mix(a: string, b: string, t: number): string {
  const ra = parseInt(a.slice(1, 3), 16);
  const ga = parseInt(a.slice(3, 5), 16);
  const ba = parseInt(a.slice(5, 7), 16);
  const rb = parseInt(b.slice(1, 3), 16);
  const gb = parseInt(b.slice(3, 5), 16);
  const bb = parseInt(b.slice(5, 7), 16);
  const r = Math.round(ra + (rb - ra) * t);
  const g = Math.round(ga + (gb - ga) * t);
  const bl = Math.round(ba + (bb - ba) * t);
  return (
    "#" +
    r.toString(16).padStart(2, "0") +
    g.toString(16).padStart(2, "0") +
    bl.toString(16).padStart(2, "0")
  );
}

function deriveXtermTheme(b: Base24): XtermTheme {
  return {
    background: b.base00,
    foreground: b.base05,
    cursor: b.base05,
    cursorAccent: b.base00,
    selectionBackground: b.base02,
    selectionForeground: b.base05,
    // 40% of selection — visible but quieter when the terminal isn't focused.
    selectionInactiveBackground: mix(b.base00, b.base02, 0.5),
    // ANSI 16: base16 slots 08-0F map to red/orange/yellow/green/cyan/blue/magenta.
    // Bright variants are perceptual lifts — either the authors' canonical
    // bright (not always published) or a uniform +12% toward base07. Since
    // Base24 doesn't prescribe brights, we brighten base08-0E toward base07
    // by a fixed amount; authors who want exact brights can ship via import.
    black: b.base00,
    red: b.base08,
    green: b.base0B,
    yellow: b.base0A,
    blue: b.base0D,
    magenta: b.base0E,
    cyan: b.base0C,
    white: b.base05,
    brightBlack: b.base03,
    brightRed: mix(b.base08, b.base07, 0.15),
    brightGreen: mix(b.base0B, b.base07, 0.15),
    brightYellow: mix(b.base0A, b.base07, 0.15),
    brightBlue: mix(b.base0D, b.base07, 0.15),
    brightMagenta: mix(b.base0E, b.base07, 0.15),
    brightCyan: mix(b.base0C, b.base07, 0.15),
    brightWhite: b.base07,
  };
}

function deriveChromeTokens(b: Base24, appearance: "dark" | "light"): ChromeTokens {
  // Linear-inspired: cards ALWAYS lift above the page bg.
  //
  // Base24 meets this naturally for dark schemes (base00 = darkest page,
  // base01 = slightly lighter surface → cards lift), but inverts for light
  // schemes (base00 = pure white, base01 = slightly grey "surface" — so
  // cards would render greyer than the page, the opposite of "lifted").
  //
  // Swap the mapping in light mode so cards stay at base00 (white) and
  // the page picks up base01 (slight off-white). This matches Linear's
  // two-tone: soft off-white canvas, pure-white elevated cards.
  const isLight = appearance === "light";
  const page = isLight ? b.base01 : b.base00;
  const card = isLight ? b.base00 : b.base01;
  // Sidebar sits between page and card in both modes — a subtle chrome
  // tier distinct from both.
  const sidebar = mix(page, card, 0.4);
  // Resting chip / muted fill: base02 (selection tier) — guaranteed to
  // sit a visible step away from both page and card in either mode.
  const muted = b.base02;
  // Hover / active-pill fill — one step beyond muted toward the text
  // contrast direction so `bg-accent` hovers pop on any scheme.
  const hoverSurface = mix(b.base02, b.base03, 0.5);
  // One text color. Captions, subtitles, and inactive labels all render at
  // body-text contrast; hierarchy comes from size and weight, not color.
  // (Previously `base04`, which landed too faint in many light Base24
  // schemes.) The `text-muted-foreground` token stays wired up across the
  // app — it just resolves to the same value as `text-foreground` now.
  const mutedForeground = b.base05;
  return {
    background: page,
    foreground: b.base05,
    surface: card,
    sidebar,
    muted,
    mutedForeground,
    hoverSurface,
    border: b.base02,
    accent: b.base0D,
    accentForeground: b.base00,
    destructive: b.base08,
  };
}

function deriveMonacoTheme(b: Base24, appearance: "dark" | "light"): MonacoTheme {
  return {
    base: appearance === "dark" ? "vs-dark" : "vs",
    inherit: true,
    // Monaco tokens use dot paths; the token registry has broad fallbacks,
    // so covering the core eight gives coherent syntax across most langs.
    // `fontStyle: "italic"` on comment is a near-universal convention.
    rules: [
      { token: "comment", foreground: b.base03.slice(1), fontStyle: "italic" },
      { token: "string", foreground: b.base0B.slice(1) },
      { token: "number", foreground: b.base09.slice(1) },
      { token: "keyword", foreground: b.base0E.slice(1) },
      { token: "type", foreground: b.base0A.slice(1) },
      { token: "function", foreground: b.base0D.slice(1) },
      { token: "variable", foreground: b.base08.slice(1) },
      { token: "constant", foreground: b.base09.slice(1) },
      { token: "operator", foreground: b.base0C.slice(1) },
    ],
    colors: {
      "editor.background": b.base00,
      "editor.foreground": b.base05,
      "editorLineNumber.foreground": b.base03,
      "editorLineNumber.activeForeground": b.base05,
      "editorCursor.foreground": b.base05,
      "editor.selectionBackground": b.base02,
      "editor.inactiveSelectionBackground": mix(b.base00, b.base02, 0.5),
      "editor.lineHighlightBackground": b.base01,
      "editorIndentGuide.background": b.base01,
      "editorIndentGuide.activeBackground": b.base02,
      // Diff editor — tinted insertion/deletion at low alpha so the
      // text remains legible on the gutter-less side-by-side layout.
      "diffEditor.insertedTextBackground": b.base0B + "33",
      "diffEditor.removedTextBackground": b.base08 + "33",
      "diffEditor.insertedLineBackground": b.base0B + "1a",
      "diffEditor.removedLineBackground": b.base08 + "1a",
      "scrollbarSlider.background": b.base02 + "66",
      "scrollbarSlider.hoverBackground": b.base02 + "99",
      "scrollbarSlider.activeBackground": b.base03 + "cc",
    },
  };
}

/**
 * Build a full ColorScheme from a Base24 palette. One fn, applied
 * identically to bundled and imported schemes, so imports look native.
 */
export function deriveScheme(
  id: string,
  name: string,
  appearance: "dark" | "light",
  base: Base24,
): ColorScheme {
  return {
    id,
    name,
    appearance,
    base,
    terminal: deriveXtermTheme(base),
    chrome: deriveChromeTokens(base, appearance),
    monaco: deriveMonacoTheme(base, appearance),
  };
}

/**
 * Base16 → Base24 upconversion. Imported base16 schemes don't publish
 * slots 08–0F separately (they only have 00–07 structural + 08–0F
 * accent, which matches Base24's accent section). The tinted-theming
 * community's canonical algorithm is a no-op on the accent slots — the
 * "missing" slots in base16 are actually the structural 08+ colors which
 * also exist in Base16. So a base16 scheme IS a valid Base24 scheme
 * after a shape check.
 *
 * This helper is kept for explicit intent in the importer.
 */
export function base16ToBase24(b16: Base24): Base24 {
  return b16;
}
