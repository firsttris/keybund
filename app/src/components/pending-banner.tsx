import { Link } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useProfiles } from "@/lib/queries";
import { formatDay } from "@/lib/utils";
import { m } from "@/paraglide/messages";

/**
 * Footer line. Changes in Keybund are written right away, so a difference
 * means a file was edited or deleted outside Keybund.
 */
export function ConfigStatus() {
  const { data } = useProfiles();
  if (!data) return null;
  if (data.pending_changes === 0) {
    return (
      <footer className="mt-auto flex flex-wrap items-center gap-x-2 gap-y-1 pt-2 text-xs text-muted-foreground">
        <span aria-hidden className="size-[7px] rounded-full bg-emerald-500" />
        <span>{m.status_upToDate()}</span>
        {data.last_applied != null && <span>· {m.status_lastWritten({ when: formatDay(data.last_applied) })}</span>}
      </footer>
    );
  }
  return (
    <footer className="mt-auto flex flex-wrap items-center gap-3 rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs">
      <RefreshCw aria-hidden className="size-3.5 text-warning" />
      <span className="flex-1">{m.status_pending({ n: data.pending_changes })}</span>
      <Link to="/apply" className="font-medium text-foreground underline-offset-4 hover:underline">
        {m.status_review()}
      </Link>
    </footer>
  );
}
