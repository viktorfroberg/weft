import { useEffect } from "react";
import {
  ensureCustomFontsSubscription,
  useCustomFonts,
} from "@/stores/custom_fonts";

/** Mounted once at the App root. Hydrates the custom-fonts store from
 *  the backend manifest + injects `@font-face` rules so any custom
 *  fonts the user has installed are available before xterm mounts.
 *  Renders nothing. */
export function CustomFontsBootstrap() {
  useEffect(() => {
    ensureCustomFontsSubscription();
    useCustomFonts.getState().refetch();
  }, []);
  return null;
}
