import { m } from "@/paraglide/messages";
import { ApiError, api } from "./api";
import type { HostKeyCheck } from "./bindings";

export type UnknownHostKey = Extract<HostKeyCheck, { state: "unknown" }>;
type Asker = (check: UnknownHostKey) => Promise<boolean>;

let asker: Asker | null = null;

/** Registered by `<HostKeyPrompt />`, which shows the confirmation dialog. */
export function setHostKeyAsker(next: Asker | null) {
  asker = next;
}

/**
 * Runs before every connection: pinned keys of GitHub & co. are added by
 * the backend, an unknown key has to be confirmed by the user. Throws if
 * the user declines.
 */
export async function ensureHostKey(id: string, host: string | null): Promise<void> {
  let check: HostKeyCheck;
  try {
    check = await api.hostKeyCheck(id, host);
  } catch (e) {
    // Without ssh-keyscan there is no check; ssh itself decides and the probe
    // says so. Every other failure (file access, unknown host …) is shown.
    if (e instanceof ApiError && e.kind === "tool_missing") return;
    throw e;
  }
  if (check.state !== "unknown") return;
  const confirmed = asker ? await asker(check) : false;
  if (!confirmed) throw new Error(m.hostKey_declined());
  await api.trustHostKey(
    id,
    host,
    check.keys.map((k) => k.fingerprint),
  );
}
