import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ColorScheme } from "@/lib/themes/schemes";
import {
  DEFAULT_DARK_ID,
  DEFAULT_LIGHT_ID,
} from "@/lib/themes/schemes";

export type ThemePref = "light" | "dark" | "system";
export type CursorStyle = "block" | "bar" | "underline";
export type BellStyle = "off" | "visual" | "audible" | "both";
export type FontWeight = 400 | 500 | 600;

/**
 * Font family ids — one-of:
 *   "jetbrains-mono" | "fira-code" | "geist-mono" | "source-code-pro" | "system"
 *   "custom:<uuid>" for user-added system fonts
 * Kept as `string` so user-installed fonts can slot in without a
 * type migration; `FONT_FAMILIES` in `src/lib/themes/fonts.ts` plus
 * `customFonts` here are the authoritative source.
 */
export type FontFamilyId = string;

// `CustomFont` lives in the backend now (`services/fonts.rs::CustomFont`)
// because file-imported fonts need a copied file on disk that can outlive
// localStorage. The frontend store mirror is in `src/stores/custom_fonts.ts`.
// The pre-v6 system-name shape stored here was removed; the v5→v6 migrate
// drops the orphan field.

interface PrefsState {
  theme: ThemePref;
  setTheme: (t: ThemePref) => void;

  // v1.0.5 appearance ---------------------------------------------------
  schemeLight: string;
  schemeDark: string;
  setSchemeLight: (id: string) => void;
  setSchemeDark: (id: string) => void;
  userSchemes: ColorScheme[];
  addUserScheme: (s: ColorScheme) => void;
  removeUserScheme: (id: string) => void;

  terminalFontFamily: FontFamilyId;
  terminalFontWeight: FontWeight;
  terminalFontSize: number;
  terminalLineHeight: number;
  terminalLigatures: boolean;
  terminalPadX: number;
  terminalPadY: number;
  boldIsBright: boolean;
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
  bellStyle: BellStyle;
  setAppearance: (patch: Partial<AppearanceFields>) => void;

  /** When true, creating a task with ≥1 linked tickets auto-spawns the
   *  default agent preset after worktrees settle. */
  autoLaunchAgentOnTickets: boolean;
  setAutoLaunchAgentOnTickets: (v: boolean) => void;

  /** When true, task_create fires a background `claude -p --model haiku`
   *  rename so the sidebar / breadcrumb label is a short LLM-written
   *  title instead of the raw compose-card prompt. User can rename from
   *  the task header (pencil icon) at any time — an explicit rename
   *  locks the row (`tasks.name_locked_at`) and any late-arriving LLM
   *  rename skips it. */
  autoRenameTasks: boolean;
  setAutoRenameTasks: (v: boolean) => void;

  /** Onboarding overlay has been dismissed / completed. */
  hasCompletedOnboarding: boolean;
  completeOnboarding: () => void;

  /** Display name used in Home's "Good morning, {name}" greeting.
   *  weft-level pref (no provider tied) — set in Settings → Workflow.
   *  Empty string treated the same as null: greeting drops the name. */
  userName: string;
  setUserName: (n: string) => void;

  /** MRU list of task ids the user has opened (newest first, capped at 20). */
  recentTaskIds: string[];
  pushRecentTaskId: (id: string) => void;
}

type AppearanceFields = Pick<
  PrefsState,
  | "terminalFontFamily"
  | "terminalFontWeight"
  | "terminalFontSize"
  | "terminalLineHeight"
  | "terminalLigatures"
  | "terminalPadX"
  | "terminalPadY"
  | "boldIsBright"
  | "cursorStyle"
  | "cursorBlink"
  | "bellStyle"
>;

const MAX_RECENT_TASKS = 20;

/**
 * Persistent user preferences. Backed by localStorage under
 * `weft-prefs`. Version history:
 *   3 → 4  v1.0.5 appearance fields landed.
 *   4 → 5  added `customFonts` (system-installed font references).
 *   5 → 6  removed `customFonts` — moved to backend-managed file
 *          imports (system fonts couldn't actually resolve in
 *          WKWebView for third-party Font Book installs). Migrate
 *          strips the orphan field; everything else is preserved.
 *
 * From v5 onwards we always ship an explicit `migrate` fn — without
 * one, Zustand's persist middleware drops the entire persisted state
 * on a version mismatch.
 */
export const usePrefs = create<PrefsState>()(
  persist(
    (set) => ({
      theme: "system",
      setTheme: (theme) => set({ theme }),

      schemeLight: DEFAULT_LIGHT_ID,
      schemeDark: DEFAULT_DARK_ID,
      setSchemeLight: (schemeLight) => set({ schemeLight }),
      setSchemeDark: (schemeDark) => set({ schemeDark }),
      userSchemes: [],
      addUserScheme: (s) => {
        // Shape guard — a malformed scheme (partial derivation) would
        // crash `applyTheme` the next time the user selects it. Keep
        // the paste-in path robust against parser bugs or mangled
        // localStorage writes from future schema drift.
        if (
          !s ||
          typeof s.id !== "string" ||
          !s.terminal?.background ||
          !s.chrome?.background ||
          !s.monaco?.colors
        ) {
          throw new Error("scheme is missing required fields");
        }
        set((st) => ({
          userSchemes: [...st.userSchemes.filter((x) => x.id !== s.id), s],
        }));
      },
      removeUserScheme: (id) =>
        set((st) => ({ userSchemes: st.userSchemes.filter((x) => x.id !== id) })),

      terminalFontFamily: "jetbrains-mono",
      terminalFontWeight: 400,
      terminalFontSize: 13,
      terminalLineHeight: 1.15,
      terminalLigatures: true,
      terminalPadX: 6,
      terminalPadY: 4,
      boldIsBright: false,
      cursorStyle: "block",
      cursorBlink: true,
      bellStyle: "off",
      setAppearance: (patch) => set((s) => ({ ...s, ...patch })),

      // Default ON: when the user explicitly attaches one or more
      // tickets they're committing to the work — auto-spawning the
      // agent matches their expectation. Empty-session tasks still
      // require pressing Launch.
      autoLaunchAgentOnTickets: true,
      setAutoLaunchAgentOnTickets: (autoLaunchAgentOnTickets) =>
        set({ autoLaunchAgentOnTickets }),

      autoRenameTasks: true,
      setAutoRenameTasks: (autoRenameTasks) => set({ autoRenameTasks }),

      hasCompletedOnboarding: false,
      completeOnboarding: () => set({ hasCompletedOnboarding: true }),

      userName: "",
      setUserName: (userName) => set({ userName }),

      recentTaskIds: [],
      pushRecentTaskId: (id) =>
        set((s) => ({
          recentTaskIds: [id, ...s.recentTaskIds.filter((x) => x !== id)].slice(
            0,
            MAX_RECENT_TASKS,
          ),
        })),
    }),
    {
      name: "weft-prefs",
      version: 6,
      migrate: (persisted, fromVersion) => {
        // Defensive: preserve every existing field; only mutate what
        // each version-bump explicitly demands.
        const prev = (persisted as Record<string, unknown>) ?? {};
        if (fromVersion < 6) {
          // Drop the v5 `customFonts` field if present — it pointed at
          // system family names that often couldn't resolve in
          // WKWebView. File-imported fonts are now backend-managed.
          const { customFonts: _drop, ...rest } = prev as {
            customFonts?: unknown;
          } & Record<string, unknown>;
          return rest as unknown as PrefsState;
        }
        return prev as unknown as PrefsState;
      },
    },
  ),
);
