import { useEffect, useRef, useState } from "react";
import { Terminal as Xterm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { LigaturesAddon } from "@xterm/addon-ligatures";
import "@xterm/xterm/css/xterm.css";
import { useActiveScheme } from "@/lib/theme";
import { usePrefs } from "@/stores/prefs";
import { useCustomFonts } from "@/stores/custom_fonts";
import { findFont } from "@/lib/themes/fonts";
import { onTestBell, playBell } from "@/lib/themes/bell";

// 12 fixture lines today; +4 buffer covers wraps when the user picks a
// large font + a narrow window (the long `// arrow…` line wraps to 2
// or 3 visual rows easily). Rows we ask xterm to display = preview
// rows; container height tracks (rows × cell-height + padding).
const FIXTURE_LINES = 12;
const PREVIEW_ROWS = FIXTURE_LINES + 4;

const FIXTURE = [
  // Prompt + command
  "\x1b[32m→\x1b[0m \x1b[36madmin git:(\x1b[31mfeature/abc-123\x1b[36m)\x1b[0m",
  "\x1b[90m$ \x1b[0mls -la && git status",
  // `ls` output with the standard BSD colors
  "\x1b[34mdrwxr-xr-x\x1b[0m   \x1b[32mviktor  staff\x1b[0m   \x1b[1msrc/\x1b[0m",
  "\x1b[34m-rw-r--r--\x1b[0m   \x1b[32mviktor  staff\x1b[0m   README.md",
  // `git status` fragment
  "\x1b[1mOn branch\x1b[0m feature/abc-123",
  "  \x1b[32mmodified:\x1b[0m   src/components/Terminal.tsx",
  "  \x1b[31mdeleted:\x1b[0m    src/lib/old-theme.ts",
  "  \x1b[33mnew file:\x1b[0m   src/lib/themes/schemes.ts",
  // Ligature fodder + ANSI 16 check strip
  "",
  "\x1b[90m// arrow -> check != equality => map:\x1b[0m const f = (x) => x >= 0 ? x * 2 : 0;",
  "",
  "\x1b[40m \x1b[41m \x1b[42m \x1b[43m \x1b[44m \x1b[45m \x1b[46m \x1b[47m \x1b[0m \x1b[100m \x1b[101m \x1b[102m \x1b[103m \x1b[104m \x1b[105m \x1b[106m \x1b[107m \x1b[0m",
].join("\r\n");

/**
 * Non-PTY xterm instance used in Settings → Appearance as a live preview.
 * Writes a static ANSI fixture so the user can see their scheme + font
 * + cursor + ligature choices applied to realistic output without
 * spinning up a shell.
 *
 * Separate from `TerminalView` on purpose: no spawn, no kill, no
 * ResizeObserver driving SIGWINCH, no PTY leak risk on unmount.
 */
export function TerminalPreview() {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const xtermRef = useRef<Xterm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);

  const scheme = useActiveScheme();
  const fontFamilyId = usePrefs((s) => s.terminalFontFamily);
  const fontWeight = usePrefs((s) => s.terminalFontWeight);
  const fontSize = usePrefs((s) => s.terminalFontSize);
  const lineHeight = usePrefs((s) => s.terminalLineHeight);
  const ligatures = usePrefs((s) => s.terminalLigatures);
  const padX = usePrefs((s) => s.terminalPadX);
  const padY = usePrefs((s) => s.terminalPadY);
  const boldIsBright = usePrefs((s) => s.boldIsBright);
  const cursorStyle = usePrefs((s) => s.cursorStyle);
  const cursorBlink = usePrefs((s) => s.cursorBlink);
  const bellStyle = usePrefs((s) => s.bellStyle);
  const customFonts = useCustomFonts((s) => s.rows);
  const [bellFlash, setBellFlash] = useState(false);

  const font = findFont(fontFamilyId, customFonts);

  // Subscribe to test-bell events from the Settings "Test bell" button.
  // Dispatch visual + audible per the user's active bellStyle.
  useEffect(() => {
    return onTestBell(() => {
      if (bellStyle === "visual" || bellStyle === "both") {
        setBellFlash(true);
        setTimeout(() => setBellFlash(false), 150);
      }
      if (bellStyle === "audible" || bellStyle === "both") {
        playBell();
      }
    });
  }, [bellStyle]);

  // Mount effect — keyed on ligatures (same reason as TerminalView: the
  // addon reads OT font data at load time and has no runtime toggle).
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const xterm = new Xterm({
      theme: scheme.terminal,
      fontFamily: font.css,
      fontSize,
      fontWeight,
      fontWeightBold: Math.min(fontWeight + 100, 700),
      lineHeight,
      cursorStyle,
      cursorBlink,
      // Match the active cursor style when the preview isn't focused
      // so the user can actually see the cursor without clicking
      // into the preview. xterm's default `outline` renders an empty
      // hollow box on unfocused terminals — fine for a real terminal
      // tab but useless for a static preview where the user is here
      // SPECIFICALLY to see how the cursor looks.
      cursorInactiveStyle: cursorStyle,
      drawBoldTextInBrightColors: boldIsBright,
      letterSpacing: 0,
      minimumContrastRatio: 4.5,
      allowTransparency: false,
      // Pin rows to FIXTURE_LINES + a wrap buffer so the swatch line
      // at the bottom never falls off into scrollback. Without an
      // explicit `rows`, xterm's default (24) + fit()'s container-
      // driven shrink can leave us with rows < fixture lines → user
      // has to scroll inside the preview.
      rows: PREVIEW_ROWS,
      // Smaller scrollback — preview never grows.
      scrollback: 100,
      // Disable input — preview is read-only.
      disableStdin: true,
      // Required by LigaturesAddon (uses proposed xterm APIs).
      allowProposedApi: true,
    });
    xtermRef.current = xterm;
    const fit = new FitAddon();
    fitRef.current = fit;
    xterm.loadAddon(fit);
    xterm.open(host);
    // LigaturesAddon requires `open()` first — it reads renderer state
    // at load time and throws otherwise.
    if (ligatures && font.ligatures) {
      xterm.loadAddon(new LigaturesAddon());
    }

    // Two-frame layout settle (same reason as TerminalView), then
    // wait for the font to actually be ready before measuring cells.
    // Skipping the document.fonts.load step makes xterm cache the
    // fallback's (wider) metrics on first mount with a custom font.
    let disposed = false;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (disposed) return;
        document.fonts
          .load(`${fontSize}px ${font.css}`)
          .catch(() => {})
          .then(() => {
            if (disposed) return;
            try {
              fit.fit();
              // fit() picked cols from container width; it ALSO
              // overrode rows from container height which we want to
              // be PREVIEW_ROWS. Restore so the fixture (and any
              // wrapped lines) always have room without scrolling.
              xterm.resize(xterm.cols, PREVIEW_ROWS);
            } catch {
              /* ignore */
            }
            xterm.write(FIXTURE);
          });
      });
    });

    return () => {
      disposed = true;
      xterm.dispose();
      xtermRef.current = null;
      fitRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ligatures, font.id]);

  // Hot-swap everything else.
  useEffect(() => {
    const x = xtermRef.current;
    if (!x) return;
    x.options.theme = scheme.terminal;
  }, [scheme]);

  useEffect(() => {
    const x = xtermRef.current;
    if (!x) return;
    x.options.fontFamily = font.css;
    x.options.fontWeight = fontWeight;
    x.options.fontWeightBold = Math.min(fontWeight + 100, 700);
    x.options.fontSize = fontSize;
    x.options.lineHeight = lineHeight;
    x.options.cursorStyle = cursorStyle;
    x.options.cursorInactiveStyle = cursorStyle;
    x.options.cursorBlink = cursorBlink;
    x.options.drawBoldTextInBrightColors = boldIsBright;
    // See Terminal.tsx for the rationale — refit AFTER the font has
    // actually loaded, otherwise xterm measures with the fallback's
    // wider cell metrics and the new glyphs render with huge gaps.
    let cancelled = false;
    document.fonts
      .load(`${fontSize}px ${font.css}`)
      .catch(() => {})
      .then(() => {
        if (cancelled) return;
        try {
          fitRef.current?.fit();
          // Same restore as in init — fit() will have shrunk rows back
          // to whatever the container height supports; pin to
          // PREVIEW_ROWS so the fixture keeps fitting.
          xtermRef.current?.resize(xtermRef.current.cols, PREVIEW_ROWS);
        } catch {
          /* ignore */
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    font,
    fontWeight,
    fontSize,
    lineHeight,
    cursorStyle,
    cursorBlink,
    boldIsBright,
  ]);

  useEffect(() => {
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
  }, [padX, padY]);

  // Cursor blink animation is xterm's own — only fires when the
  // preview has focus. We tried driving a synthetic blink via a
  // setInterval toggling `cursorInactiveStyle`, but combined with
  // xterm's native blink on focus it produced a doubled / desynced
  // animation when the user clicked into the preview. Click-to-blink
  // is good enough; the static `cursorInactiveStyle` (set in the
  // hot-swap effect above) keeps the cursor visible at all times.

  // Height = PREVIEW_ROWS × cell height + padding + small fudge for
  // xterm's per-cell descender allowance (real cell-height is
  // `ceil(fontSize × lineHeight)` plus ~1-2px the renderer adds for
  // descenders). Without `+ PREVIEW_ROWS * 2` margin the swatch line
  // at the bottom gets clipped at large font sizes.
  const cellHeight = Math.ceil(fontSize * lineHeight) + 2;
  const previewHeight = PREVIEW_ROWS * cellHeight + padY * 2 + 8;

  return (
    <div
      className={`border-border/60 overflow-hidden rounded-lg border transition-[box-shadow] duration-150 ${
        bellFlash ? "ring-primary ring-2 ring-inset" : ""
      }`}
      style={{
        background: scheme.terminal.background,
        height: previewHeight,
      }}
    >
      <div
        ref={hostRef}
        className="h-full w-full"
        style={{
          paddingLeft: `${padX}px`,
          paddingRight: `${padX}px`,
          paddingTop: `${padY}px`,
          paddingBottom: `${padY}px`,
        }}
      />
    </div>
  );
}
