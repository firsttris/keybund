import { toast } from "sonner";
import { api, errorMessage } from "./api";

/**
 * Opens a link in the browser. If that fails, the user sees why and can
 * copy the link instead of a button that silently does nothing.
 */
export async function openExternal(url: string): Promise<boolean> {
  try {
    await api.openUrl(url);
    return true;
  } catch (e) {
    toast.error("Browser konnte nicht geöffnet werden", {
      description: `${errorMessage(e)} · ${url}`,
      action: { label: "Link kopieren", onClick: () => void navigator.clipboard?.writeText(url) },
      duration: 15_000,
    });
    return false;
  }
}
