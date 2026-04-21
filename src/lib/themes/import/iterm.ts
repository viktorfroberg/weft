import { parse as parsePlist } from "plist";
import { deriveScheme, mix } from "../derive";
import type { Base24 } from "../derive";
import type { ColorScheme } from "../schemes";

/**
 * iTerm2 `.itermcolors` XML plist. Shape:
 *
 *   <plist version="1.0">
 *     <dict>
 *       <key>Background Color</key>
 *       <dict>
 *         <key>Red Component</key><real>0.1</real>
 *         <key>Green Component</key><real>0.1</real>
 *         <key>Blue Component</key><real>0.1</real>
 *       </dict>
 *       <key>Ansi 0 Color</key> … <key>Ansi 15 Color</key>
 *       <key>Foreground Color</key>
 *       <key>Cursor Color</key>
 *       <key>Selection Color</key>
 *       …
 *     </dict>
 *   </plist>
 *
 * iTerm palettes don't publish base03/04/06 explicitly — we synthesize
 * them by blending background and foreground at perceptual steps.
 */
export function parseItermPlist(xml: string, name = "Imported iTerm"): ColorScheme {
  const parsed = parsePlist(xml) as Record<string, ItermColor | undefined>;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("not a valid plist");
  }
  const get = (key: string): string | undefined => {
    const entry = parsed[key];
    if (!entry) return undefined;
    return toHex(entry);
  };

  const bg = get("Background Color") ?? get("Ansi 0 Color");
  const fg = get("Foreground Color") ?? get("Ansi 7 Color");
  if (!bg || !fg) {
    throw new Error("missing Background or Foreground color");
  }

  const ansi = (n: number) => get(`Ansi ${n} Color`);
  const red = ansi(1) ?? "#ff5555";
  const green = ansi(2) ?? "#50fa7b";
  const yellow = ansi(3) ?? "#f1fa8c";
  const blue = ansi(4) ?? "#bd93f9";
  const magenta = ansi(5) ?? "#ff79c6";
  const cyan = ansi(6) ?? "#8be9fd";
  const brightBlack = ansi(8) ?? mix(bg, fg, 0.35);
  const brightWhite = ansi(15) ?? fg;

  // Synthesize missing Base24 slots.
  const base01 = ansi(0) && bg !== ansi(0) ? ansi(0)! : mix(bg, fg, 0.08);
  const base02 = get("Selection Color") ?? mix(bg, fg, 0.2);
  const base03 = brightBlack;
  const base04 = mix(base03, fg, 0.35);
  const base06 = mix(fg, brightWhite, 0.5);
  // Orange (base09) — iTerm has no dedicated orange. Blend red/yellow.
  const base09 = mix(red, yellow, 0.35);
  // Brown / secondary accent (base0F) — blend red/magenta.
  const base0F = mix(red, magenta, 0.5);

  const base: Base24 = {
    base00: bg,
    base01,
    base02,
    base03,
    base04,
    base05: fg,
    base06,
    base07: brightWhite,
    base08: red,
    base09,
    base0A: yellow,
    base0B: green,
    base0C: cyan,
    base0D: blue,
    base0E: magenta,
    base0F,
  };

  const appearance = isDarkBackground(bg) ? "dark" : "light";
  const id =
    "user-" +
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");
  return deriveScheme(id, name, appearance, base);
}

/** Detect iTerm plist by the XML preamble. */
export function looksLikeItermPlist(text: string): boolean {
  const t = text.trim();
  return t.startsWith("<?xml") || t.includes("<!DOCTYPE plist");
}

type ItermColor = {
  "Red Component"?: number;
  "Green Component"?: number;
  "Blue Component"?: number;
  "Alpha Component"?: number;
  "Color Space"?: string;
};

function toHex(c: ItermColor): string {
  const r = Math.round((c["Red Component"] ?? 0) * 255);
  const g = Math.round((c["Green Component"] ?? 0) * 255);
  const b = Math.round((c["Blue Component"] ?? 0) * 255);
  return (
    "#" +
    r.toString(16).padStart(2, "0") +
    g.toString(16).padStart(2, "0") +
    b.toString(16).padStart(2, "0")
  );
}

function isDarkBackground(hex: string): boolean {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  const l = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  return l < 0.5;
}
