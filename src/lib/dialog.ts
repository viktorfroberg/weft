import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";

/**
 * Let the user pick a directory. Returns the absolute path, or null if
 * they cancelled.
 */
export async function pickDirectory(): Promise<string | null> {
  const picked = await open({
    directory: true,
    multiple: false,
    title: "Select a git repository",
  });
  if (Array.isArray(picked) || picked === null) return null;
  return picked;
}

export const gitIsRepo = (path: string) =>
  invoke<boolean>("git_is_repo", { path });

export const gitDefaultBranch = (path: string) =>
  invoke<string>("git_default_branch", { path });
