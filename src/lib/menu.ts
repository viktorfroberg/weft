import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const MENU_EVENT = "menu";

export type MenuId =
  | "new_workspace"
  | "new_task"
  | "add_project"
  | "toggle_sidebar"
  | "toggle_mode"
  | "back"
  | "shortcuts"
  | "about";

/**
 * Subscribe to native menu item clicks. Matching keyboard-shortcut logic
 * lives in App.tsx — the menu is just another entry point.
 */
export function onMenuEvent(
  handler: (id: MenuId) => void,
): Promise<UnlistenFn> {
  return listen<string>(MENU_EVENT, (e) => handler(e.payload as MenuId));
}
