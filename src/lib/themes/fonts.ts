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
 */
export type FontEntry = {
  id: string;
  name: string;
  css: string;
  ligatures: boolean;
  variable: boolean;
};

const FALLBACK = ', SF Mono, Menlo, Monaco, "Courier New", monospace';

export const FONT_FAMILIES: FontEntry[] = [
  {
    id: "jetbrains-mono",
    name: "JetBrains Mono",
    css: "'JetBrains Mono Variable'" + FALLBACK,
    ligatures: true,
    variable: true,
  },
  {
    id: "fira-code",
    name: "Fira Code",
    css: "'Fira Code Variable'" + FALLBACK,
    ligatures: true,
    variable: true,
  },
  {
    id: "geist-mono",
    name: "Geist Mono",
    css: "'Geist Mono Variable'" + FALLBACK,
    ligatures: true,
    variable: true,
  },
  {
    id: "source-code-pro",
    name: "Source Code Pro",
    css: "'Source Code Pro Variable'" + FALLBACK,
    ligatures: false,
    variable: true,
  },
  {
    id: "system",
    name: "System Monospace",
    css: 'SF Mono, Menlo, Monaco, "Courier New", monospace',
    ligatures: false,
    variable: false,
  },
];

export function findFont(id: string): FontEntry {
  return FONT_FAMILIES.find((f) => f.id === id) ?? FONT_FAMILIES[0];
}
