import { parse as parseYaml } from "yaml";
import { deriveScheme } from "../derive";
import type { Base24 } from "../derive";
import type { ColorScheme } from "../schemes";

/**
 * Parse a tinted-theming Base16 or Base24 YAML scheme. Shape:
 *
 *   scheme: "Tokyo Night"
 *   author: "enkia"
 *   variant: "dark"
 *   base00: "1a1b26"
 *   base01: "24283b"
 *   ...
 *   base0F: "c0caf5"
 *
 * Hex values may be written with or without a leading `#`. Variant is
 * optional; we auto-detect from `base00`'s luminosity if missing (< 0.5
 * lightness → dark, else light).
 *
 * Base16 schemes (no base10–base17) already carry base00–base0F, which
 * is everything Base24's accent section needs. Base24 adds 10–17 for
 * extra palette richness; we ignore those slots — our derivation only
 * reads 00–0F.
 */
export function parseBase24Yaml(yaml: string): ColorScheme {
  const parsed = parseYaml(yaml) as Record<string, string>;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("not a YAML object");
  }
  // Lowercase every key for lookup. tinted-theming's canonical YAMLs
  // publish keys as `base0a`/`base0b` (lowercase hex digits), while the
  // original Base16 spec wrote them as `base0A`/`base0B` (uppercase).
  // Both are valid — normalize so either import cleanly.
  const raw: Record<string, string> = {};
  for (const [k, v] of Object.entries(parsed)) {
    raw[k.toLowerCase()] = v;
  }
  const name =
    typeof raw.scheme === "string"
      ? raw.scheme
      : typeof (raw as { name?: string }).name === "string"
        ? (raw as { name: string }).name
        : "Imported scheme";
  const base = readBase(raw);
  const appearance = detectAppearance(base, raw.variant);
  const id = slugify(name);
  return deriveScheme(id, name, appearance, base);
}

/** Detect base24 YAML by the presence of scheme + base00 keys. Used by
 * the unified importer to pick the right parser. */
export function looksLikeBase24Yaml(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed.startsWith("scheme:") && !trimmed.includes("\nscheme:")) {
    // Base16 builder schemes sometimes start with comments; allow any
    // line-start "base00:" as a signal too.
    if (!/\bbase00\s*:/.test(trimmed)) return false;
  }
  return /\bbase0[0-9a-fA-F]\s*:/.test(trimmed);
}

function readBase(raw: Record<string, string>): Base24 {
  // Caller has already lowercased every key, so lookups here use the
  // lowercase form. Output shape keeps the canonical mixed-case Base16
  // naming our `Base24` type expects.
  const get = (key: string) => {
    const v = raw[key];
    if (typeof v !== "string") {
      throw new Error(`missing slot: ${key}`);
    }
    return normHex(v);
  };
  return {
    base00: get("base00"),
    base01: get("base01"),
    base02: get("base02"),
    base03: get("base03"),
    base04: get("base04"),
    base05: get("base05"),
    base06: get("base06"),
    base07: get("base07"),
    base08: get("base08"),
    base09: get("base09"),
    base0A: get("base0a"),
    base0B: get("base0b"),
    base0C: get("base0c"),
    base0D: get("base0d"),
    base0E: get("base0e"),
    base0F: get("base0f"),
  };
}

function normHex(h: string): string {
  const s = h.trim();
  if (s.startsWith("#")) return s.toLowerCase();
  return "#" + s.toLowerCase();
}

function detectAppearance(base: Base24, variant?: string): "dark" | "light" {
  if (variant === "dark" || variant === "light") return variant;
  // Relative luminance of base00.
  const hex = base.base00.replace("#", "");
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  const l = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  return l < 0.5 ? "dark" : "light";
}

function slugify(name: string): string {
  return (
    "user-" +
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "")
  );
}
