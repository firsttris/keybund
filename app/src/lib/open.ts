import { toast } from "sonner";
import { m } from "@/paraglide/messages";
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
    toast.error(m.toast_browserFailed(), {
      description: `${errorMessage(e)} · ${url}`,
      action: { label: m.toast_copyLink(), onClick: () => void navigator.clipboard?.writeText(url) },
      duration: 15_000,
    });
    return false;
  }
}
