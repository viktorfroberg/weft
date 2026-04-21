import { useEffect, useState } from "react";
import { usePrefs, type ThemePref } from "@/stores/prefs";
import { applyTheme, resolveActiveScheme } from "@/lib/themes/apply";
import {
  BUNDLED_SCHEMES,
  DEFAULT_DARK_ID,
  DEFAULT_LIGHT_ID,
  findScheme,
  type ColorScheme,
} from "@/lib/themes/schemes";

export type EffectiveTheme = "light" | "dark";

const MEDIA = "(prefers-color-scheme: dark)";

function systemPrefersDark(): boolean {
  if (typeof window === "undefined" || !window.matchMedia) return true;
  return window.matchMedia(MEDIA).matches;
}

/** Pure: resolve a pref + current-system signal to a concrete theme. */
export function resolveTheme(
  pref: ThemePref,
  systemDark: boolean,
): EffectiveTheme {
  if (pref === "system") return systemDark ? "dark" : "light";
  return pref;
}

/** Hook: always returns the currently-effective theme, reacting to both
 * the user's saved pref and live OS appearance changes. */
export function useEffectiveTheme(): EffectiveTheme {
  const pref = usePrefs((s) => s.theme);
  const [systemDark, setSystemDark] = useState(systemPrefersDark());

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mql = window.matchMedia(MEDIA);
    const handler = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, []);

  return resolveTheme(pref, systemDark);
}

/** Hook: the `ColorScheme` currently in effect. Driven by `theme` +
 * `schemeLight` / `schemeDark` prefs. Terminal, Monaco, and chrome all
 * read this — anything that rereads on scheme change should depend on
 * the returned object identity. */
export function useActiveScheme(): ColorScheme {
  const theme = useEffectiveTheme();
  const schemeLight = usePrefs((s) => s.schemeLight);
  const schemeDark = usePrefs((s) => s.schemeDark);
  const userSchemes = usePrefs((s) => s.userSchemes);
  return resolveActiveScheme(theme, { schemeLight, schemeDark, userSchemes });
}

/** Hook: call at the top of the app tree. Re-applies class + CSS vars +
 * Monaco theme whenever the resolved scheme changes, atomically. */
export function useThemeApplier() {
  const theme = useEffectiveTheme();
  const scheme = useActiveScheme();
  useEffect(() => {
    applyTheme(theme, scheme);
  }, [theme, scheme]);
}

/**
 * Pre-mount shim: reads saved prefs from localStorage (synchronously, so
 * there's no theme/scheme flash) and applies the class + scheme CSS
 * vars before React runs. Call once in `main.tsx` before
 * `createRoot(...).render(...)`.
 */
export function applyInitialTheme() {
  let pref: ThemePref = "system";
  let schemeDarkId: string = DEFAULT_DARK_ID;
  let schemeLightId: string = DEFAULT_LIGHT_ID;
  let userSchemes: ColorScheme[] = [];
  try {
    const raw = localStorage.getItem("weft-prefs");
    if (raw) {
      const parsed = JSON.parse(raw);
      const t = parsed?.state?.theme;
      if (t === "light" || t === "dark" || t === "system") pref = t;
      if (typeof parsed?.state?.schemeDark === "string")
        schemeDarkId = parsed.state.schemeDark;
      if (typeof parsed?.state?.schemeLight === "string")
        schemeLightId = parsed.state.schemeLight;
      if (Array.isArray(parsed?.state?.userSchemes))
        userSchemes = parsed.state.userSchemes as ColorScheme[];
    }
  } catch {
    // ignore — fall through to defaults
  }
  const theme = resolveTheme(pref, systemPrefersDark());
  const id = theme === "dark" ? schemeDarkId : schemeLightId;
  // Guard against stale ids pointing at deleted user schemes and
  // against user schemes whose shape drifted from an older deriveScheme
  // (extra paranoia — apply can throw if `scheme.chrome.background` is
  // undefined). Fall back to the bundled default on any failure.
  const scheme =
    findScheme(id, theme, userSchemes) ??
    BUNDLED_SCHEMES.find(
      (s) => s.id === (theme === "dark" ? DEFAULT_DARK_ID : DEFAULT_LIGHT_ID),
    )!;
  try {
    applyTheme(theme, scheme);
  } catch (e) {
    console.warn("applyInitialTheme failed, using bundled default", e);
    const safe = BUNDLED_SCHEMES[0];
    applyTheme(theme, safe);
  }
}
