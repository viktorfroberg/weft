import { convertFileSrc } from "@tauri-apps/api/core";
import type { CustomFontRow } from "./commands";

/**
 * Custom-font runtime registration.
 *
 * Why we copy + register fonts ourselves instead of just referencing
 * macOS-installed families by name: WKWebView (Tauri's macOS webview)
 * restricts CSS `font-family` resolution to Apple-shipped fonts. Fonts
 * the user installs via Font Book or `~/Library/Fonts/` are NOT
 * reachable via plain `font-family: 'Foo'` — a privacy/fingerprinting
 * restriction baked into WebKit. The only path that works is
 * `@font-face` against a URL the webview can fetch.
 *
 * Pipeline:
 *   1. Backend (`services/fonts.rs::install`) copies the user's chosen
 *      file into `~/Library/Application Support/weft/fonts/<id>.<ext>`.
 *   2. `injectFontFaces` writes one `@font-face` block per row into a
 *      single `<style id="weft-custom-fonts">` tag. The `font-family`
 *      is namespaced as `weft-custom-<id>` (NOT the user's display
 *      name) so two custom fonts named "MyMono" can't collide.
 *   3. `awaitCustomFont` is called from `Terminal.tsx` before xterm
 *      measures cell width — without it, xterm grabs the fallback
 *      metrics and the font renders with huge gaps.
 */

const STYLE_TAG_ID = "weft-custom-fonts";

/** CSS family name xterm sees. Used by `customFontToEntry`. */
export function cssFamilyForId(id: string): string {
  return `weft-custom-${id}`;
}

function formatFromExt(filename: string): string | null {
  const ext = filename.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "ttf":
    case "ttc":
      return "truetype";
    case "otf":
      return "opentype";
    case "woff":
      return "woff";
    case "woff2":
      return "woff2";
    default:
      return null;
  }
}

/** Inject `@font-face` rules for every row. Idempotent — runs on every
 *  refetch so removed fonts disappear and added fonts appear.
 *  Each row produces one `font-style: normal` block; rows with a
 *  paired italic file produce a second `font-style: italic` block
 *  under the same family so xterm's italic ANSI text picks it up. */
export function injectFontFaces(rows: CustomFontRow[], dataDir: string): void {
  const blocks: string[] = [];
  for (const row of rows) {
    const family = cssFamilyForId(row.id);
    const regularExt =
      row.file_basename.split(".").pop()?.toLowerCase() ?? "ttf";
    const regularPath = `${dataDir}/fonts/${row.id}.${regularExt}`;
    const regularUrl = convertFileSrc(regularPath);
    const regularFmt = formatFromExt(row.file_basename);
    const regularFormatHint = regularFmt ? ` format('${regularFmt}')` : "";
    blocks.push(`@font-face {
  font-family: '${family}';
  font-style: normal;
  src: url('${regularUrl}')${regularFormatHint};
  font-display: block;
}`);

    if (row.italic_file_basename) {
      const italicExt =
        row.italic_file_basename.split(".").pop()?.toLowerCase() ?? "ttf";
      const italicPath = `${dataDir}/fonts/${row.id}.italic.${italicExt}`;
      const italicUrl = convertFileSrc(italicPath);
      const italicFmt = formatFromExt(row.italic_file_basename);
      const italicFormatHint = italicFmt ? ` format('${italicFmt}')` : "";
      blocks.push(`@font-face {
  font-family: '${family}';
  font-style: italic;
  src: url('${italicUrl}')${italicFormatHint};
  font-display: block;
}`);
    }
  }

  let tag = document.getElementById(STYLE_TAG_ID) as HTMLStyleElement | null;
  if (!tag) {
    tag = document.createElement("style");
    tag.id = STYLE_TAG_ID;
    document.head.appendChild(tag);
  }
  tag.textContent = blocks.join("\n\n");
}

/** Wait for a custom font's bytes to load, so xterm measures the right
 *  cell width on first use. Resolves with `true` on success, `false` if
 *  the load fails (file missing, asset-protocol scope misconfig, etc.).
 *  Safe to call for bundled fonts too — they're already in
 *  `document.fonts`, so the await is effectively a noop. */
export async function awaitCustomFont(
  family: string,
  sizePx = 16,
): Promise<boolean> {
  try {
    const loaded = await document.fonts.load(`${sizePx}px '${family}'`);
    return loaded.length > 0;
  } catch {
    return false;
  }
}

// Re-export the CustomFontRow type alias if any consumer wants it.
export type { CustomFontRow };
