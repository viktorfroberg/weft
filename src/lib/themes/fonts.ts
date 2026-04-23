/**
 * Terminal font registry.
 *
 * Each entry names the font-family CSS value to pass into xterm's
 * `fontFamily` option, plus whether it ships with OpenType ligatures
 * (enables/disables the Ligatures toggle). Variable fonts (all four
 * bundled) also accept a numeric weight via xterm's `fontWeight` option.
 *
 * The font files themselves are imported side-effect-only from
 * `src/main.tsx` so `@fontsource-variable/*` register the @font-face
 * declarations before first paint.
 *
 * Custom fonts (user-registered system-installed families) live in the
 * prefs store (`customFonts`) and are merged in by `mergeFonts()` at
 * read time — they don't get a row in `FONT_FAMILIES`.
 */
export type FontEntry = {
  id: string;
  name: string;
  css: string;
  ligatures: boolean;
  variable: boolean;
  /** `bundled` = ships with weft (rows in `FONT_FAMILIES`); `custom` =
   *  user-registered system font derived from a `CustomFont` row. */
  kind: "bundled" | "custom";
};

const FALLBACK = ', SF Mono, Menlo, Monaco, "Courier New", monospace';

export const FONT_FAMILIES: FontEntry[] = [
  {
    id: "jetbrains-mono",
    name: "JetBrains Mono",
    css: "'JetBrains Mono Variable'" + FALLBACK,
    ligatures: true,
    variable: true,
    kind: "bundled",
  },
  {
    id: "fira-code",
    name: "Fira Code",
    css: "'Fira Code Variable'" + FALLBACK,
    ligatures: true,
    variable: true,
    kind: "bundled",
  },
  {
    id: "geist-mono",
    name: "Geist Mono",
    css: "'Geist Mono Variable'" + FALLBACK,
    ligatures: true,
    variable: true,
    kind: "bundled",
  },
  {
    id: "source-code-pro",
    name: "Source Code Pro",
    css: "'Source Code Pro Variable'" + FALLBACK,
    ligatures: false,
    variable: true,
    kind: "bundled",
  },
  {
    id: "system",
    name: "System Monospace",
    css: 'SF Mono, Menlo, Monaco, "Courier New", monospace',
    ligatures: false,
    variable: false,
    kind: "bundled",
  },
];

import type { CustomFontRow } from "@/lib/commands";
import { cssFamilyForId } from "@/lib/custom-fonts";

/** Convert a backend `CustomFontRow` into a `FontEntry` so it can flow
 *  through every existing surface (dropdown, `findFont` fallback,
 *  Terminal.tsx live-pref effect). The CSS `font-family` is the
 *  namespaced `weft-custom-<id>` injected by `injectFontFaces`, NOT a
 *  user string — that way two custom fonts with the same display name
 *  never collide, and non-ASCII display names are safe everywhere. */
export function customFontToEntry(c: CustomFontRow): FontEntry {
  const family = cssFamilyForId(c.id);
  return {
    id: `custom:${c.id}`,
    name: c.display_name,
    css: `'${family}'` + FALLBACK,
    ligatures: c.ligatures,
    variable: c.variable,
    kind: "custom",
  };
}

export function mergeFonts(custom: CustomFontRow[]): FontEntry[] {
  return [...FONT_FAMILIES, ...custom.map(customFontToEntry)];
}

/** Resolve an id to a `FontEntry`. Custom-id misses (deleted row,
 *  startup before custom-fonts hydrate) fall through to JetBrains Mono
 *  so the terminal always has *something* to render with. The Settings
 *  UI runs a separate mount guard that rewrites the pref so the
 *  dropdown doesn't end up with a blank `<option>`. */
export function findFont(id: string, custom: CustomFontRow[] = []): FontEntry {
  const merged = custom.length > 0 ? mergeFonts(custom) : FONT_FAMILIES;
  return merged.find((f) => f.id === id) ?? FONT_FAMILIES[0];
}
