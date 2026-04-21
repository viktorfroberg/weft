import { useEffect, useRef, useState } from "react";
import { Terminal as Xterm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SearchAddon } from "@xterm/addon-search";
import { LigaturesAddon } from "@xterm/addon-ligatures";
import { Channel } from "@tauri-apps/api/core";
import "@xterm/xterm/css/xterm.css";
import { Search, X as XIcon } from "lucide-react";
import {
  terminalKill,
  terminalResize,
  terminalWrite,
} from "@/lib/commands";
import { useActiveScheme } from "@/lib/theme";
import { usePrefs } from "@/stores/prefs";
import { findFont } from "@/lib/themes/fonts";
import { playBell } from "@/lib/themes/bell";
import { traceEvent, useLifecycleTrace, useRateCounter } from "@/lib/dev-trace";

export interface TerminalSpawnArgs {
  channel: Channel<Uint8Array>;
  rows: number;
  cols: number;
}

/** Caller provides the actual spawn fn (shell vs agent vs future kinds).
 * Must return the new PTY session id. */
export type TerminalSpawn = (args: TerminalSpawnArgs) => Promise<string>;

interface Props {
  /** Identity for keying remounts — changing this recreates the PTY. */
  sessionKey: string;
  /** How to get a PTY. Returns the session id so we can later write/kill. */
  spawn: TerminalSpawn;
  /** Whether this terminal is currently visible. Hidden tabs stay mounted
   * (to preserve scrollback + alive PTY) but skip `fit()` since they have
   * zero dimensions. */
  visible?: boolean;
  /** Fires once, after the PTY session id is returned from Rust. Lets
   *  the parent correlate `pty_exit` events (keyed by session id) with
   *  this tab (keyed by sessionKey). */
  onSpawned?: (sessionId: string) => void;
}

/**
 * xterm.js wrapper. Spawns a PTY via the caller's `spawn` fn, streams
 * output via a Tauri v2 `Channel<Uint8Array>` (NOT `emit` — see plan.md),
 * sends stdin via `terminal_write`, propagates `ResizeObserver` →
 * `terminal_resize`, and kills the PTY on unmount.
 *
 * Appearance (scheme, font, size, weight, line-height, cursor, bell,
 * padding, bold-is-bright) is all prefs-driven and live-updatable via
 * `xterm.options.*` on change. Ligatures are the single exception —
 * toggling requires a terminal restart because the addon reads OT font
 * metadata at mount.
 */
export function TerminalView({
  sessionKey,
  spawn,
  visible = true,
  onSpawned,
}: Props) {
  useLifecycleTrace(`Terminal(${sessionKey})`);
  const countChunk = useRateCounter(`Terminal(${sessionKey}).onmessage`);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const xtermRef = useRef<Xterm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [bellFlash, setBellFlash] = useState(false);

  // Appearance prefs — read into locals so effects can depend on them
  // individually. Scheme is the big one; font/cursor/bell are live via
  // `xterm.options`.
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

  // A ref to the current bellStyle so the xterm `onBell` handler (wired
  // once at mount) always sees the latest value without needing a
  // re-mount.
  const bellStyleRef = useRef(bellStyle);
  useEffect(() => {
    bellStyleRef.current = bellStyle;
  }, [bellStyle]);

  const font = findFont(fontFamilyId);

  // ⌘F while the terminal (or its host) has keyboard focus opens the
  // inline search bar. Scoped to this Terminal's host so hitting ⌘F
  // over the diff panel doesn't intercept it.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        if (host.contains(document.activeElement)) {
          e.preventDefault();
          setSearchOpen(true);
          setTimeout(() => searchInputRef.current?.select(), 0);
        }
      }
    };
    host.addEventListener("keydown", onKey, true);
    return () => host.removeEventListener("keydown", onKey, true);
  }, []);

  // Hot-swap scheme palette — cheaper than recreating the Terminal.
  useEffect(() => {
    if (!xtermRef.current) return;
    xtermRef.current.options.theme = scheme.terminal;
  }, [scheme]);

  // Hot-swap font / cursor options. xterm recomputes cell dimensions on
  // next render; we re-fit so rows/cols stay accurate.
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

  // When a hidden tab becomes visible, its dimensions change from 0 to
  // real — fit again so xterm knows its size.
  useEffect(() => {
    if (visible && fitRef.current) {
      try {
        fitRef.current.fit();
      } catch {
        // ignore — might still be mid-mount
      }
    }
  }, [visible]);

  // Terminal init — effect keyed on sessionKey + ligatures (only). Other
  // prefs update live via the effects above; ligatures is the lone
  // exception because LigaturesAddon reads OT metadata at addon-load
  // time and has no runtime toggle.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    let sessionId: string | null = null;
    let xterm: Xterm | null = null;
    let fitAddon: FitAddon | null = null;
    let resizeObserver: ResizeObserver | null = null;
    let intersectionObserver: IntersectionObserver | null = null;
    // PTY-write batching. Shell startup (e.g. a heavy `.zshrc`) can deliver
    // 100+ channel messages back-to-back; calling `xterm.write` synchronously
    // on each one can starve the main thread for long enough to look like a
    // hang. Coalesce pending chunks into one write per animation frame so
    // DOM / React work can interleave.
    let pendingChunks: Uint8Array[] = [];
    let rafId: number | null = null;
    const flushPending = () => {
      rafId = null;
      if (!xterm || disposed || pendingChunks.length === 0) return;
      if (pendingChunks.length === 1) {
        xterm.write(pendingChunks[0]);
      } else {
        let total = 0;
        for (const c of pendingChunks) total += c.length;
        const merged = new Uint8Array(total);
        let offset = 0;
        for (const c of pendingChunks) {
          merged.set(c, offset);
          offset += c.length;
        }
        xterm.write(merged);
      }
      pendingChunks = [];
    };

    const init = async () => {
      xterm = new Xterm({
        theme: scheme.terminal,
        fontFamily: font.css,
        fontSize,
        fontWeight,
        fontWeightBold: Math.min(fontWeight + 100, 700),
        lineHeight,
        cursorStyle,
        cursorBlink,
        drawBoldTextInBrightColors: boldIsBright,
        // Baked universal defaults — not user-facing.
        letterSpacing: 0,
        minimumContrastRatio: 4.5,
        allowTransparency: false,
        scrollback: 5000,
        // Required by LigaturesAddon (uses proposed xterm APIs).
        allowProposedApi: true,
      });
      xtermRef.current = xterm;
      fitAddon = new FitAddon();
      fitRef.current = fitAddon;
      const searchAddon = new SearchAddon();
      searchRef.current = searchAddon;
      xterm.loadAddon(searchAddon);
      xterm.loadAddon(fitAddon);
      xterm.loadAddon(new WebLinksAddon());
      // Bell: xterm emits onBell whenever \a is written. Dispatch to
      // visual flash and/or audio via the user's current bellStyle.
      xterm.onBell(() => {
        const mode = bellStyleRef.current;
        if (mode === "visual" || mode === "both") {
          setBellFlash(true);
          setTimeout(() => setBellFlash(false), 150);
        }
        if (mode === "audible" || mode === "both") {
          playBell();
        }
      });
      xterm.open(host);
      // LigaturesAddon needs the renderer, which only exists after
      // `open()`. Loading earlier throws "Cannot activate before open".
      // Disabling or switching fonts still requires a terminal restart
      // (OT parse is cached in the addon).
      if (ligatures && font.ligatures) {
        xterm.loadAddon(new LigaturesAddon());
      }
      // Wait two frames for flex layout + react-resizable-panels to
      // settle. One frame is enough on a static layout; two covers the
      // case where Panel computes its pixel size after the first paint.
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() =>
          requestAnimationFrame(() => resolve()),
        ),
      );
      if (disposed) return;
      try {
        fitAddon.fit();
      } catch {
        // ignore — resize observer will re-fit once layout is real
      }

      const channel = new Channel<Uint8Array>();
      channel.onmessage = (chunk) => {
        if (!xterm || disposed) return;
        countChunk();
        const bytes =
          chunk instanceof Uint8Array ? chunk : new Uint8Array(chunk);
        pendingChunks.push(bytes);
        if (rafId === null) rafId = requestAnimationFrame(flushPending);
      };

      traceEvent(`Terminal(${sessionKey}).spawn.start`, {
        rows: xterm.rows,
        cols: xterm.cols,
      });
      const spawnStarted = performance.now();
      try {
        sessionId = await spawn({
          channel,
          rows: xterm.rows,
          cols: xterm.cols,
        });
        traceEvent(`Terminal(${sessionKey}).spawn.done`, {
          sessionId,
          ms: Math.round(performance.now() - spawnStarted),
          disposed,
        });
        if (sessionId && !disposed) onSpawned?.(sessionId);
        // Some TUIs (Claude Code, vim, btop) paint their layout to the
        // first dimensions they see and only re-render on an explicit
        // SIGWINCH. If the shell picks up its size before the Panel
        // finalized, the TUI stays cramped. Re-fit 100ms after spawn
        // to force a resize event — harmless if dimensions didn't
        // change, corrective if they did.
        setTimeout(() => {
          if (disposed || !fitAddon) return;
          try {
            fitAddon.fit();
          } catch {
            /* ignore */
          }
        }, 100);
      } catch (e) {
        if (xterm) {
          xterm.write(
            `\r\n\x1b[31mspawn failed: ${String(e)}\x1b[0m\r\n`,
          );
        }
        return;
      }

      if (disposed && sessionId) {
        traceEvent(`Terminal(${sessionKey}).disposed-cleanup`, {
          sessionId,
        });
        terminalKill(sessionId).catch(() => {});
        return;
      }
      traceEvent(`Terminal(${sessionKey}).wired`, { sessionId });

      const encoder = new TextEncoder();
      xterm.onData((data) => {
        if (!sessionId) return;
        terminalWrite(sessionId, encoder.encode(data)).catch((e) =>
          console.warn("terminal_write failed", e),
        );
      });

      xterm.onResize(({ cols, rows }) => {
        if (!sessionId) return;
        terminalResize(sessionId, rows, cols).catch((e) =>
          console.warn("terminal_resize failed", e),
        );
      });

      resizeObserver = new ResizeObserver(() => {
        try {
          fitAddon?.fit();
        } catch {
          // ignore; hidden tabs have 0-size host
        }
      });
      resizeObserver.observe(host);

      // IntersectionObserver catches the display:none → visible
      // transition that TaskPanelPool does when the user navigates
      // back to a task. ResizeObserver alone misses this on WebKit:
      // the host's size goes from 0×0 to real, but no "resize" entry
      // fires reliably when display flips. Without this fit, xterm
      // stays stuck at the 0×0 dimensions it last measured (when the
      // panel was hidden) and rendering wraps at ~3 chars per line
      // until the user drags the window. Re-fit on any visibility
      // increase is safe — fit() is idempotent when dimensions match.
      intersectionObserver = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (entry.isIntersecting) {
              try {
                fitAddon?.fit();
              } catch {
                // ignore — transient zero-size window during layout
              }
            }
          }
        },
        { threshold: 0 },
      );
      intersectionObserver.observe(host);
    };

    init();

    return () => {
      disposed = true;
      if (rafId !== null) cancelAnimationFrame(rafId);
      pendingChunks = [];
      resizeObserver?.disconnect();
      intersectionObserver?.disconnect();
      if (sessionId) terminalKill(sessionId).catch(() => {});
      xterm?.dispose();
      xtermRef.current = null;
      fitRef.current = null;
    };
    // sessionKey + ligatures intentionally recreate the terminal.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionKey, ligatures]);

  // Re-fit whenever padding changes. Debouncing happens in the Settings
  // slider, not here — any downstream callers who batch setters already
  // benefit from React's batching.
  useEffect(() => {
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
  }, [padX, padY]);

  const runSearch = (q: string, back = false) => {
    const s = searchRef.current;
    if (!s) return;
    if (!q) return;
    if (back) s.findPrevious(q);
    else s.findNext(q);
  };

  return (
    <div
      className={`relative h-full w-full overflow-hidden transition-[box-shadow] duration-150 ${
        bellFlash ? "ring-primary ring-2 ring-inset" : ""
      }`}
      style={{ background: scheme.terminal.background }}
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
        tabIndex={0}
      />
      {searchOpen && (
        <div
          className="bg-background border-border absolute z-10 flex items-center gap-1 rounded-md border px-2 py-1 shadow-lg animate-in fade-in slide-in-from-top-1 duration-150"
          style={{
            top: `calc(${padY}px + 0.5rem)`,
            right: `calc(${padX}px + 0.5rem)`,
          }}
        >
          <Search size={12} className="text-muted-foreground" />
          <input
            ref={searchInputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              runSearch(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                runSearch(searchQuery, e.shiftKey);
              } else if (e.key === "Escape") {
                e.preventDefault();
                setSearchOpen(false);
                setSearchQuery("");
                hostRef.current?.focus();
              }
            }}
            placeholder="Search scrollback"
            className="w-40 bg-transparent text-xs outline-none"
            autoFocus
          />
          <button
            type="button"
            onClick={() => {
              setSearchOpen(false);
              setSearchQuery("");
            }}
            className="text-muted-foreground hover:text-foreground"
            title="Close search (Esc)"
          >
            <XIcon size={12} />
          </button>
        </div>
      )}
    </div>
  );
}
