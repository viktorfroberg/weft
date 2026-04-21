import { useEffect } from "react";

export interface ShortcutBinding {
  /** Key to match, case-insensitive (e.g. "n", "b", "1"). */
  key: string;
  meta?: boolean; // ⌘ on macOS
  shift?: boolean;
  alt?: boolean;
  ctrl?: boolean;
  /** Description for help / future settings UI. */
  description?: string;
  handler: (e: KeyboardEvent) => void;
}

/**
 * Register a list of keyboard shortcuts for the lifetime of the calling
 * component. Ignores events while typing in form inputs except when the
 * binding explicitly uses a modifier (⌘/ctrl/alt).
 */
export function useShortcuts(bindings: ShortcutBinding[]) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName ?? "";
      const typingInInput =
        tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable;

      for (const b of bindings) {
        if (e.key.toLowerCase() !== b.key.toLowerCase()) continue;
        if ((b.meta ?? false) !== e.metaKey) continue;
        if ((b.shift ?? false) !== e.shiftKey) continue;
        if ((b.alt ?? false) !== e.altKey) continue;
        if ((b.ctrl ?? false) !== e.ctrlKey) continue;

        const hasModifier =
          (b.meta ?? false) || (b.ctrl ?? false) || (b.alt ?? false);
        if (typingInInput && !hasModifier) continue;

        e.preventDefault();
        e.stopPropagation();
        b.handler(e);
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [bindings]);
}
