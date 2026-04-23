import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { fontList, type CustomFontRow, appInfo } from "@/lib/commands";
import { injectFontFaces } from "@/lib/custom-fonts";

interface CustomFontsState {
  rows: CustomFontRow[];
  /** weft's data dir (resolved once at boot from `app_info()`). Needed
   *  to build absolute paths for `@font-face src: url(...)`. Empty until
   *  the first hydrate. */
  dataDir: string;
  refetch: () => Promise<void>;
}

const EMPTY_ROWS: CustomFontRow[] = [];

export const useCustomFonts = create<CustomFontsState>((set, get) => ({
  rows: EMPTY_ROWS,
  dataDir: "",
  refetch: async () => {
    try {
      // Resolve dataDir lazily on first call. `app_info()` is cheap +
      // idempotent, but we cache so subsequent refetches skip it.
      let { dataDir } = get();
      if (!dataDir) {
        const info = await appInfo();
        dataDir = info.data_dir;
        set({ dataDir });
      }
      const rows = await fontList();
      injectFontFaces(rows, dataDir);
      set({ rows });
    } catch (err) {
      console.warn("custom_fonts refetch failed", err);
    }
  },
}));

/** Subscribe once at module load to the backend's
 *  `weft://custom-fonts-changed` event so any install / rename / delete
 *  re-renders the UI without manual refresh. */
let subscribed = false;
export function ensureCustomFontsSubscription() {
  if (subscribed) return;
  subscribed = true;
  listen("weft://custom-fonts-changed", () => {
    useCustomFonts.getState().refetch();
  }).catch((err) => {
    subscribed = false;
    console.warn("custom-fonts subscription failed", err);
  });
}
