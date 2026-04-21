import type { ColorScheme } from "../schemes";
import { looksLikeBase24Yaml, parseBase24Yaml } from "./base24";
import { looksLikeItermPlist, parseItermPlist } from "./iterm";

/**
 * Auto-detect format + parse. Throws with a user-friendly message on
 * unknown / malformed input. Called from the Settings paste-in dialog.
 */
export function importScheme(text: string): ColorScheme {
  const trimmed = text.trim();
  if (!trimmed) {
    throw new Error("Nothing pasted");
  }
  if (looksLikeItermPlist(trimmed)) {
    return parseItermPlist(trimmed);
  }
  if (looksLikeBase24Yaml(trimmed)) {
    return parseBase24Yaml(trimmed);
  }
  throw new Error(
    "Couldn't recognize format. Paste either base16/base24 YAML (starts with `scheme:`) or an iTerm2 `.itermcolors` XML plist.",
  );
}
