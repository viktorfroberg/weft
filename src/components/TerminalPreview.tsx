import { useEffect, useRef, useState } from "react";
import { Terminal as Xterm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { LigaturesAddon } from "@xterm/addon-ligatures";
import "@xterm/xterm/css/xterm.css";
import { useActiveScheme } from "@/lib/theme";
import { usePrefs } from "@/stores/prefs";
import { findFont } from "@/lib/themes/fonts";
import { onTestBell, playBell } from "@/lib/themes/bell";

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
  const [bellFlash, setBellFlash] = useState(false);

  const font = findFont(fontFamilyId);

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
      drawBoldTextInBrightColors: boldIsBright,
      letterSpacing: 0,
      minimumContrastRatio: 4.5,
      allowTransparency: false,
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

    // Two-frame layout settle (same reason as TerminalView).
    let disposed = false;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (disposed) return;
        try {
          fit.fit();
        } catch {
          /* ignore */
        }
        xterm.write(FIXTURE);
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
    x.options.cursorBlink = cursorBlink;
    x.options.drawBoldTextInBrightColors = boldIsBright;
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
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

  return (
    <div
      className={`border-border/60 overflow-hidden rounded-lg border transition-[box-shadow] duration-150 ${
        bellFlash ? "ring-primary ring-2 ring-inset" : ""
      }`}
      style={{ background: scheme.terminal.background, height: 220 }}
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
