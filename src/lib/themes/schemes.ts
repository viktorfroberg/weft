import { deriveScheme, type Base24 } from "./derive";

/**
 * A color scheme for weft — one coherent palette used across the xterm
 * terminal, the Tailwind chrome, and Monaco's diff editor.
 *
 * Shape is Base24: 16 perceptual slots (base00 darkest bg → base07
 * brightest fg, base08–0F accent colors roughly matching ANSI red …
 * bright-yellow). Everything else (xterm ITheme, chrome CSS vars, Monaco
 * IStandaloneThemeData) is derived from `base` via one deterministic fn
 * in `./derive.ts` — so imported schemes render coherently with no
 * per-scheme hand-tuning.
 */
export type ColorScheme = {
  id: string;
  name: string;
  appearance: "dark" | "light";
  base: Base24;
  terminal: XtermTheme;
  chrome: ChromeTokens;
  monaco: MonacoTheme;
};

/** Subset of `@xterm/xterm`'s ITheme we actually set. Plain strings so
 * ColorScheme is JSON-serializable (important for user-scheme import). */
export type XtermTheme = {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  selectionForeground: string;
  selectionInactiveBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
};

/** Tailwind-backing CSS var targets. These write onto the existing
 * `:root` / `.dark` slots in `src/index.css` so every shadcn component
 * retints with no component-side changes. */
export type ChromeTokens = {
  background: string;
  foreground: string;
  surface: string;
  sidebar: string;
  muted: string;
  mutedForeground: string;
  /** Hover / active-pill fill. One step brighter than `muted` so
   * `bg-accent` is visibly distinct from `bg-muted` at rest. Prior to
   * this field, `--accent` was aliased to `muted`, making hovers
   * invisible on every Base24 scheme. */
  hoverSurface: string;
  border: string;
  accent: string;
  accentForeground: string;
  destructive: string;
};

/** Structural match for `monaco.editor.IStandaloneThemeData` — kept as
 * a plain type to avoid eagerly loading `monaco-editor` just to read
 * scheme files. DiffViewer casts when calling `defineTheme`. */
export type MonacoTheme = {
  base: "vs" | "vs-dark";
  inherit: boolean;
  rules: Array<{
    token: string;
    foreground?: string;
    background?: string;
    fontStyle?: string;
  }>;
  colors: Record<string, string>;
};

// ---------------------------------------------------------------------------
// Bundled schemes — Base24 canonical values from the scheme authors.
// ---------------------------------------------------------------------------

const TOKYO_NIGHT_BASE: Base24 = {
  base00: "#1a1b26",
  base01: "#1f2335",
  base02: "#2e3251",
  base03: "#565f89",
  base04: "#787c99",
  base05: "#c0caf5",
  base06: "#cbccd1",
  base07: "#d5d6db",
  base08: "#f7768e",
  base09: "#ff9e64",
  base0A: "#e0af68",
  base0B: "#9ece6a",
  base0C: "#7dcfff",
  base0D: "#7aa2f7",
  base0E: "#bb9af7",
  base0F: "#c0caf5",
};

const ONE_DARK_BASE: Base24 = {
  base00: "#282c34",
  base01: "#353b45",
  base02: "#3e4451",
  base03: "#545862",
  base04: "#565c64",
  base05: "#abb2bf",
  base06: "#b6bdca",
  base07: "#c8ccd4",
  base08: "#e06c75",
  base09: "#d19a66",
  base0A: "#e5c07b",
  base0B: "#98c379",
  base0C: "#56b6c2",
  base0D: "#61afef",
  base0E: "#c678dd",
  base0F: "#be5046",
};

const CATPPUCCIN_LATTE_BASE: Base24 = {
  base00: "#eff1f5",
  base01: "#e6e9ef",
  base02: "#ccd0da",
  base03: "#bcc0cc",
  base04: "#acb0be",
  base05: "#4c4f69",
  base06: "#5c5f77",
  base07: "#6c6f85",
  base08: "#d20f39",
  base09: "#fe640b",
  base0A: "#df8e1d",
  base0B: "#40a02b",
  base0C: "#179299",
  base0D: "#1e66f5",
  base0E: "#8839ef",
  base0F: "#dd7878",
};

const GITHUB_LIGHT_BASE: Base24 = {
  base00: "#ffffff",
  base01: "#f6f8fa",
  base02: "#d0d7de",
  base03: "#8c959f",
  base04: "#6e7781",
  base05: "#1f2328",
  base06: "#24292f",
  base07: "#323941",
  base08: "#cf222e",
  base09: "#d1242f",
  base0A: "#9a6700",
  base0B: "#1a7f37",
  base0C: "#0a3069",
  base0D: "#0969da",
  base0E: "#8250df",
  base0F: "#a40e26",
};

export const BUNDLED_SCHEMES: ColorScheme[] = [
  deriveScheme("tokyo-night", "Tokyo Night", "dark", TOKYO_NIGHT_BASE),
  deriveScheme("one-dark", "One Dark", "dark", ONE_DARK_BASE),
  deriveScheme("catppuccin-latte", "Catppuccin Latte", "light", CATPPUCCIN_LATTE_BASE),
  deriveScheme("github-light", "GitHub Light", "light", GITHUB_LIGHT_BASE),
];

export const DEFAULT_DARK_ID = "tokyo-night";
export const DEFAULT_LIGHT_ID = "catppuccin-latte";

/** Look up a scheme by id across bundled + user schemes. Returns the
 * default for the appearance if not found (defensive against prefs
 * pointing at a deleted user scheme). */
export function findScheme(
  id: string,
  appearance: "dark" | "light",
  userSchemes: ColorScheme[] = [],
): ColorScheme {
  const all = [...BUNDLED_SCHEMES, ...userSchemes];
  const hit = all.find((s) => s.id === id);
  if (hit) return hit;
  const fallbackId = appearance === "dark" ? DEFAULT_DARK_ID : DEFAULT_LIGHT_ID;
  return all.find((s) => s.id === fallbackId) ?? BUNDLED_SCHEMES[0];
}
