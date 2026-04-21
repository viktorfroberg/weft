import { deriveScheme } from "./derive";
import type { Base24 } from "./derive";
import type { ColorScheme } from "./schemes";

/**
 * Curated preset collection — 15 popular Base16 schemes ported to the
 * Base24 shape. Shown in the Settings → Appearance "Add scheme…"
 * dialog under the "Browse presets" tab, one click to add to
 * `userSchemes`.
 *
 * Values come from tinted-theming / base16-schemes canonical files.
 * Kept as data (not JSON) so the bundle tree-shakes cleanly if nothing
 * imports it on a given page, and so edits read like code.
 */
type Preset = {
  id: string;
  name: string;
  appearance: "dark" | "light";
  base: Base24;
};

const PRESETS: Preset[] = [
  {
    id: "preset-dracula",
    name: "Dracula",
    appearance: "dark",
    base: {
      base00: "#282936", base01: "#3a3c4e", base02: "#4d4f68", base03: "#626483",
      base04: "#62d6e8", base05: "#e9e9f4", base06: "#f1f2f8", base07: "#f7f7fb",
      base08: "#ea51b2", base09: "#b45bcf", base0A: "#00f769", base0B: "#ebff87",
      base0C: "#a1efe4", base0D: "#62d6e8", base0E: "#b45bcf", base0F: "#00f769",
    },
  },
  {
    id: "preset-nord",
    name: "Nord",
    appearance: "dark",
    base: {
      base00: "#2e3440", base01: "#3b4252", base02: "#434c5e", base03: "#4c566a",
      base04: "#d8dee9", base05: "#e5e9f0", base06: "#eceff4", base07: "#8fbcbb",
      base08: "#bf616a", base09: "#d08770", base0A: "#ebcb8b", base0B: "#a3be8c",
      base0C: "#88c0d0", base0D: "#81a1c1", base0E: "#b48ead", base0F: "#5e81ac",
    },
  },
  {
    id: "preset-gruvbox-dark",
    name: "Gruvbox Dark",
    appearance: "dark",
    base: {
      base00: "#282828", base01: "#3c3836", base02: "#504945", base03: "#665c54",
      base04: "#bdae93", base05: "#d5c4a1", base06: "#ebdbb2", base07: "#fbf1c7",
      base08: "#fb4934", base09: "#fe8019", base0A: "#fabd2f", base0B: "#b8bb26",
      base0C: "#8ec07c", base0D: "#83a598", base0E: "#d3869b", base0F: "#d65d0e",
    },
  },
  {
    id: "preset-gruvbox-light",
    name: "Gruvbox Light",
    appearance: "light",
    base: {
      base00: "#fbf1c7", base01: "#ebdbb2", base02: "#d5c4a1", base03: "#bdae93",
      base04: "#665c54", base05: "#504945", base06: "#3c3836", base07: "#282828",
      base08: "#9d0006", base09: "#af3a03", base0A: "#b57614", base0B: "#79740e",
      base0C: "#427b58", base0D: "#076678", base0E: "#8f3f71", base0F: "#d65d0e",
    },
  },
  {
    id: "preset-solarized-dark",
    name: "Solarized Dark",
    appearance: "dark",
    base: {
      base00: "#002b36", base01: "#073642", base02: "#586e75", base03: "#657b83",
      base04: "#839496", base05: "#93a1a1", base06: "#eee8d5", base07: "#fdf6e3",
      base08: "#dc322f", base09: "#cb4b16", base0A: "#b58900", base0B: "#859900",
      base0C: "#2aa198", base0D: "#268bd2", base0E: "#6c71c4", base0F: "#d33682",
    },
  },
  {
    id: "preset-solarized-light",
    name: "Solarized Light",
    appearance: "light",
    base: {
      base00: "#fdf6e3", base01: "#eee8d5", base02: "#93a1a1", base03: "#839496",
      base04: "#657b83", base05: "#586e75", base06: "#073642", base07: "#002b36",
      base08: "#dc322f", base09: "#cb4b16", base0A: "#b58900", base0B: "#859900",
      base0C: "#2aa198", base0D: "#268bd2", base0E: "#6c71c4", base0F: "#d33682",
    },
  },
  {
    id: "preset-monokai",
    name: "Monokai",
    appearance: "dark",
    base: {
      base00: "#272822", base01: "#383830", base02: "#49483e", base03: "#75715e",
      base04: "#a59f85", base05: "#f8f8f2", base06: "#f5f4f1", base07: "#f9f8f5",
      base08: "#f92672", base09: "#fd971f", base0A: "#f4bf75", base0B: "#a6e22e",
      base0C: "#a1efe4", base0D: "#66d9ef", base0E: "#ae81ff", base0F: "#cc6633",
    },
  },
  {
    id: "preset-material",
    name: "Material",
    appearance: "dark",
    base: {
      base00: "#263238", base01: "#2e3c43", base02: "#314549", base03: "#546e7a",
      base04: "#b2ccd6", base05: "#eeffff", base06: "#eeffff", base07: "#ffffff",
      base08: "#f07178", base09: "#f78c6c", base0A: "#ffcb6b", base0B: "#c3e88d",
      base0C: "#89ddff", base0D: "#82aaff", base0E: "#c792ea", base0F: "#ff5370",
    },
  },
  {
    id: "preset-ayu-dark",
    name: "Ayu Dark",
    appearance: "dark",
    base: {
      base00: "#0a0e14", base01: "#01060e", base02: "#131721", base03: "#4d5566",
      base04: "#686868", base05: "#b3b1ad", base06: "#e6e1cf", base07: "#f3f4f5",
      base08: "#ff3333", base09: "#ff7733", base0A: "#ffee99", base0B: "#c2d94c",
      base0C: "#95e6cb", base0D: "#59c2ff", base0E: "#d2a6ff", base0F: "#e6b673",
    },
  },
  {
    id: "preset-ayu-light",
    name: "Ayu Light",
    appearance: "light",
    base: {
      base00: "#fafafa", base01: "#f3f4f5", base02: "#f8f9fa", base03: "#abb0b6",
      base04: "#828c99", base05: "#5c6773", base06: "#242936", base07: "#1a1f29",
      base08: "#f07171", base09: "#fa8d3e", base0A: "#f2ae49", base0B: "#86b300",
      base0C: "#4cbf99", base0D: "#36a3d9", base0E: "#a37acc", base0F: "#e6ba7e",
    },
  },
  {
    id: "preset-rose-pine",
    name: "Rosé Pine",
    appearance: "dark",
    base: {
      base00: "#191724", base01: "#1f1d2e", base02: "#26233a", base03: "#6e6a86",
      base04: "#908caa", base05: "#e0def4", base06: "#e0def4", base07: "#524f67",
      base08: "#eb6f92", base09: "#f6c177", base0A: "#ebbcba", base0B: "#31748f",
      base0C: "#9ccfd8", base0D: "#c4a7e7", base0E: "#f6c177", base0F: "#524f67",
    },
  },
  {
    id: "preset-rose-pine-dawn",
    name: "Rosé Pine Dawn",
    appearance: "light",
    base: {
      base00: "#faf4ed", base01: "#fffaf3", base02: "#f2e9de", base03: "#9893a5",
      base04: "#797593", base05: "#575279", base06: "#575279", base07: "#cecacd",
      base08: "#b4637a", base09: "#ea9d34", base0A: "#d7827e", base0B: "#286983",
      base0C: "#56949f", base0D: "#907aa9", base0E: "#ea9d34", base0F: "#cecacd",
    },
  },
  {
    id: "preset-catppuccin-mocha",
    name: "Catppuccin Mocha",
    appearance: "dark",
    base: {
      base00: "#1e1e2e", base01: "#181825", base02: "#313244", base03: "#45475a",
      base04: "#585b70", base05: "#cdd6f4", base06: "#f5e0dc", base07: "#b4befe",
      base08: "#f38ba8", base09: "#fab387", base0A: "#f9e2af", base0B: "#a6e3a1",
      base0C: "#94e2d5", base0D: "#89b4fa", base0E: "#cba6f7", base0F: "#f2cdcd",
    },
  },
  {
    id: "preset-catppuccin-frappe",
    name: "Catppuccin Frappé",
    appearance: "dark",
    base: {
      base00: "#303446", base01: "#292c3c", base02: "#414559", base03: "#51576d",
      base04: "#626880", base05: "#c6d0f5", base06: "#f2d5cf", base07: "#babbf1",
      base08: "#e78284", base09: "#ef9f76", base0A: "#e5c890", base0B: "#a6d189",
      base0C: "#81c8be", base0D: "#8caaee", base0E: "#ca9ee6", base0F: "#eebebe",
    },
  },
  {
    id: "preset-night-owl",
    name: "Night Owl",
    appearance: "dark",
    base: {
      base00: "#011627", base01: "#0e293f", base02: "#234d5f", base03: "#5f7e97",
      base04: "#7fdbca", base05: "#d6deeb", base06: "#ffffff", base07: "#ffffff",
      base08: "#ef5350", base09: "#f78c6c", base0A: "#ffeb95", base0B: "#addb67",
      base0C: "#7fdbca", base0D: "#82aaff", base0E: "#c792ea", base0F: "#d3423e",
    },
  },
];

/** Materialize the preset list as full `ColorScheme` records. Done
 * lazily inside a getter so the derivation runs only when the Settings
 * dialog opens — not at app start. */
export function loadPresets(): ColorScheme[] {
  return PRESETS.map((p) => deriveScheme(p.id, p.name, p.appearance, p.base));
}
