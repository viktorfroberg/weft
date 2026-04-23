import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
// Terminal font @font-face registrations — must come before xterm mounts.
// Not all four fonts are loaded at runtime; the CSS just registers their
// URLs so the browser fetches the one the user selects.
import "@fontsource-variable/jetbrains-mono";
import "@fontsource-variable/fira-code";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/source-code-pro";

// ─── Prefs backup seed ────────────────────────────────────────────────
// WKWebView's localStorage (where Zustand's persist middleware writes
// `weft-prefs`) is bundle-id-scoped under `~/Library/WebKit/<bundle>/`
// and gets re-provisioned on identifier changes, code-signing identity
// swaps, Gatekeeper-quarantine first-launch paths, etc. — prefs vanish.
// We mirror prefs to a disk JSON under `~/Library/Application Support/
// weft/prefs.json` (bundle-id-independent, same tree as weft.db, which
// survives the same reinstalls). On cold boot, if localStorage is
// empty, seed it from the disk backup BEFORE Zustand creates its store.
//
// Timing is load-bearing: Zustand calls `getStorage().getItem(...)`
// during `create(persist(...))` — at the module-evaluation time of
// `src/stores/prefs.ts`. ES modules are depth-first synchronous, so any
// STATIC import chain from here that reaches `stores/prefs.ts`
// evaluates before our IIFE runs. That means `App` (via `Shell` →
// everything) and `applyInitialTheme` (via `lib/theme.ts:2`
// `import { usePrefs } from "@/stores/prefs"`) MUST be dynamic below.
// Future refactors: don't statically import anything here that reaches
// the prefs store. The `window.__weftPrefsSeedRan` flag guards this at
// runtime — see `src/stores/prefs.ts`.
async function hydratePrefsFromBackup() {
  try {
    if (window.localStorage.getItem("weft-prefs")) return;
    const { invoke } = await import("@tauri-apps/api/core");
    const backup = await invoke<string | null>("prefs_read_backup");
    if (backup) window.localStorage.setItem("weft-prefs", backup);
  } catch (err) {
    console.warn("prefs backup hydrate failed", err);
  }
}

(async () => {
  await hydratePrefsFromBackup();
  (window as unknown as { __weftPrefsSeedRan?: boolean }).__weftPrefsSeedRan =
    true;

  // Dynamic imports so prefs.ts evaluation is deferred until AFTER
  // the seed step — see comment above.
  const { default: App } = await import("./App");
  const { applyInitialTheme } = await import("./lib/theme");

  // Apply saved theme BEFORE React mounts so there's no light-theme
  // flash on cold load of a dark-pref user.
  applyInitialTheme();

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
})();
