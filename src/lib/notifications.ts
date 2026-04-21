import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { getCurrentWindow } from "@tauri-apps/api/window";

let permissionChecked = false;
let permissionGranted = false;

export async function ensurePermission(): Promise<boolean> {
  if (permissionChecked) return permissionGranted;
  permissionGranted = await isPermissionGranted();
  if (!permissionGranted) {
    const res = await requestPermission();
    permissionGranted = res === "granted";
  }
  permissionChecked = true;
  return permissionGranted;
}

export async function notifyTaskWaiting(taskName: string, taskId: string) {
  const ok = await ensurePermission();
  if (!ok) return;
  try {
    sendNotification({
      title: "weft · attention required",
      body: `${taskName} is waiting for input`,
      // Use the task id as an implicit correlation key.
      // macOS will replace prior notifications with the same identifier.
      // (Tauri exposes this via the extra field `sound` / `threadId` in
      // some versions; keeping minimal for compatibility.)
    });
    void taskId;
  } catch (e) {
    console.warn("notification failed", e);
  }
}

/**
 * Update the macOS dock badge with a count. 0 clears it. Tauri v2 exposes
 * `setBadgeCount` on the current Window; wrapping here so callers don't
 * have to wire `getCurrentWindow()` each time.
 */
export async function setDockBadge(count: number) {
  try {
    const win = getCurrentWindow();
    await win.setBadgeCount(count > 0 ? count : undefined);
  } catch (e) {
    console.warn("setBadgeCount failed", e);
  }
}
