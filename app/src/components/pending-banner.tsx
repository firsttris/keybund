import { Link } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useProfiles } from "@/lib/queries";
import { plural } from "@/lib/utils";

/** Footer line: configs in sync, or a link to the pending diff. */
export function ConfigStatus() {
  const { data } = useProfiles();
  if (!data) return null;
  if (data.pending_changes === 0) {
    return (
      <footer className="mt-auto flex items-center gap-2 pt-2 text-xs text-muted-foreground">
        <span aria-hidden className="size-[7px] rounded-full bg-emerald-500" />
        Configs aktuell
      </footer>
    );
  }
  return (
    <footer className="mt-auto flex flex-wrap items-center gap-3 rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs">
      <RefreshCw aria-hidden className="size-3.5 text-warning" />
      <span className="flex-1">{plural(data.pending_changes, "Datei ist", "Dateien sind")} nicht aktuell</span>
      <Link to="/apply" className="font-medium text-foreground underline-offset-4 hover:underline">
        Vorschau &amp; anwenden
      </Link>
    </footer>
  );
}
