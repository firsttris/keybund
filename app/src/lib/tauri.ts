import { open } from "@tauri-apps/plugin-dialog";

/** Lets the user pick a directory; returns `null` if cancelled. */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  const result = await open({ directory: true, multiple: false, defaultPath });
  return typeof result === "string" ? result : null;
}
