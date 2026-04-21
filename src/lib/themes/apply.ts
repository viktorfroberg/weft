import type { EffectiveTheme } from "@/lib/theme";
import type { ColorScheme } from "./schemes";
import { findScheme } from "./schemes";

/**
 * Single source of truth for "make the app look like this appearance +
 * scheme right now." Called from:
 *   - `applyInitialTheme()` pre-mount (synchronous, no flash).
 *   - A top-level effect that reacts to prefs + OS media query changes.
 *   - The Settings UI live-preview handlers (same fn, so preview and
 *     apply never diverge).
 *
 * The fn is intentionally synchronous and monolithic: toggling `.dark`
 * and writing CSS vars in separate effects can paint one frame with the
 * wrong combination (e.g. OS flips dark→light but chrome is still
 * Tokyo-Night). One call, one frame, always consistent.
 *
 * Monaco's `setTheme` is called lazily — only if monaco-editor is
 * already loaded (we lazy-load DiffViewer, so on first paint monaco is
 * absent). DiffViewer's own mount effect re-applies on load.
 */
export function applyTheme(theme: EffectiveTheme, scheme: ColorScheme) {
  const root = document.documentElement;

  // 1) Class + data-attr so selectors can target (.dark .foo, [data-scheme] .bar).
  root.classList.toggle("dark", theme === "dark");
  root.dataset.theme = theme;
  root.dataset.scheme = scheme.id;

  // 2) Chrome CSS vars — the same slots shadcn/Nova defines in
  // `src/index.css` under :root / .dark. Written at the element level,
  // so they override the stylesheet values without rebuilding Tailwind.
  const c = scheme.chrome;
  const s = root.style;
  s.setProperty("--background", c.background);
  s.setProperty("--foreground", c.foreground);
  s.setProperty("--card", c.surface);
  s.setProperty("--card-foreground", c.foreground);
  s.setProperty("--popover", c.surface);
  s.setProperty("--popover-foreground", c.foreground);
  s.setProperty("--primary", c.accent);
  s.setProperty("--primary-foreground", c.accentForeground);
  s.setProperty("--secondary", c.muted);
  s.setProperty("--secondary-foreground", c.foreground);
  s.setProperty("--muted", c.muted);
  s.setProperty("--muted-foreground", c.mutedForeground);
  // `--accent` is the hover/active-pill surface — must be a visibly
  // distinct step above `muted`, otherwise every `bg-accent` hover is
  // invisible against resting `bg-muted`.
  s.setProperty("--accent", c.hoverSurface);
  s.setProperty("--accent-foreground", c.foreground);
  s.setProperty("--destructive", c.destructive);
  s.setProperty("--border", c.border);
  s.setProperty("--input", c.border);
  s.setProperty("--ring", c.accent);
  s.setProperty("--sidebar", c.sidebar);
  s.setProperty("--sidebar-foreground", c.foreground);
  s.setProperty("--sidebar-primary", c.accent);
  s.setProperty("--sidebar-primary-foreground", c.accentForeground);
  s.setProperty("--sidebar-accent", c.hoverSurface);
  s.setProperty("--sidebar-accent-foreground", c.foreground);
  s.setProperty("--sidebar-border", c.border);
  s.setProperty("--sidebar-ring", c.accent);

  // 3) Monaco — only if it's already loaded. DiffViewer re-applies
  // on its own mount, so lazy-loading is covered.
  const monaco = (globalThis as unknown as { monaco?: MonacoGlobal }).monaco;
  if (monaco?.editor?.defineTheme) {
    const themeName = `weft-${scheme.id}`;
    monaco.editor.defineTheme(themeName, scheme.monaco);
    monaco.editor.setTheme(themeName);
  }
}

/** Minimal shape of the monaco global we poke — avoids importing the
 * entire monaco-editor package just for a type. */
type MonacoGlobal = {
  editor: {
    defineTheme: (name: string, data: ColorScheme["monaco"]) => void;
    setTheme: (name: string) => void;
  };
};

/**
 * Resolve the active scheme for an effective theme + user prefs. Central
 * so everywhere agrees on which scheme wins when `theme === "dark"` vs
 * `"light"`.
 */
export function resolveActiveScheme(
  theme: EffectiveTheme,
  prefs: { schemeLight: string; schemeDark: string; userSchemes: ColorScheme[] },
): ColorScheme {
  const id = theme === "dark" ? prefs.schemeDark : prefs.schemeLight;
  return findScheme(id, theme, prefs.userSchemes);
}
