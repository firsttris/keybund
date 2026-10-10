import { Check } from "lucide-react";
import { toast } from "sonner";
import { DiffView } from "@/components/diff-view";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { errorMessage } from "@/lib/api";
import { useApplyChanges, usePlan } from "@/lib/queries";

export function ApplyPage() {
  const plan = usePlan();
  const apply = useApplyChanges();
  const changes = plan.data?.changes ?? [];
  return (
    <>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex flex-col gap-1.5">
          <h1 className="m-0 text-[22px] font-semibold tracking-tight">Configs neu schreiben</h1>
          <p className="m-0 max-w-[640px] text-muted-foreground">
            Was du in Keybund änderst, wird sofort geschrieben. Diese Dateien passen trotzdem nicht zu deinen Keys – meist, weil
            sie von Hand geändert oder gelöscht wurden. Keybund schreibt nur seine eigenen Dateien und je eine Include-Zeile in
            ~/.ssh/config und ~/.gitconfig; von deinen Dateien gibt es vorher ein Backup.
          </p>
        </div>
        <Button
          size="lg"
          disabled={changes.length === 0 || apply.isPending}
          onClick={() =>
            apply.mutate(
              changes.map((c) => c.diff),
              {
                onSuccess: (r) =>
                  toast.success(`${r.written.length + r.deleted.length} Datei(en) geschrieben`, {
                    description: r.backups.length ? `Backup: ${r.backups.join(", ")}` : undefined,
                  }),
              },
            )
          }
        >
          {apply.isPending && <Spinner className="size-3.5" />}
          Neu schreiben
        </Button>
      </div>
      {plan.isPending && <Spinner />}
      {apply.isError && (
        <div className="flex flex-wrap items-center gap-3">
          <FieldError>{errorMessage(apply.error)}</FieldError>
          <Button variant="outline" size="sm" onClick={() => (apply.reset(), plan.refetch())}>
            Vorschau neu laden
          </Button>
        </div>
      )}
      {plan.isSuccess && changes.length === 0 && (
        <p role="status" className="m-0 flex items-center gap-2 text-success">
          <Check aria-hidden className="size-4" />
          Alle Configs passen zu deinen Keys.
        </p>
      )}
      {changes.length > 0 && <DiffView changes={changes} />}
    </>
  );
}
