import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { useState } from "react";
import { DiffView } from "@/components/diff-view";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { api, errorMessage } from "@/lib/api";
import type { UninstallOptions } from "@/lib/bindings";

export function UninstallPage() {
  const [opts, setOpts] = useState<UninstallOptions>({ restore_backups: false, purge: false });
  const client = useQueryClient();
  const preview = useQuery({ queryKey: ["uninstall", opts], queryFn: () => api.uninstallPreview(opts) });
  const run = useMutation({
    mutationFn: (diffs: string[]) => api.uninstallApply(opts, diffs),
    onSuccess: () => client.invalidateQueries(),
  });
  const changes = preview.data?.plan.changes ?? [];

  return (
    <>
      <div className="flex flex-col gap-1.5">
        <h1 className="m-0 text-[22px] font-semibold tracking-tight">Keybund entfernen</h1>
        <p className="m-0 max-w-[720px] leading-normal text-muted-foreground">
          Entfernt alles, was Keybund geschrieben hat: die Include-Zeilen in deiner ~/.ssh/config und ~/.gitconfig und
          Keybunds eigene Dateien. Deine Keys in ~/.ssh bleiben unverändert.
        </p>
      </div>
      {run.isSuccess ? (
        <p role="status" className="m-0 flex items-center gap-2 text-success">
          <Check aria-hidden className="size-4" />
          Keybund ist entfernt.{" "}
          {opts.purge
            ? "Auch die Liste deiner Keys ist gelöscht – die Key-Dateien in ~/.ssh bleiben."
            : "Keybund kennt deine Keys noch – „Configs neu schreiben“ richtet alles wieder ein."}
        </p>
      ) : (
        <>
          <Card className="max-w-[720px] gap-3">
            {preview.data?.backups_available && (
              <label className="flex items-start gap-3 text-[13px]">
                <input
                  type="checkbox"
                  className="mt-0.5"
                  checked={opts.restore_backups}
                  onChange={(e) => setOpts((o) => ({ ...o, restore_backups: e.target.checked }))}
                />
                <span className="flex flex-col gap-0.5">
                  <span className="font-medium">Originale aus dem Backup wiederherstellen</span>
                  <span className="text-muted-foreground">
                    ~/.ssh/config und ~/.gitconfig wie vor Keybund – bringt auch übernommene Einträge zurück, spätere eigene
                    Änderungen an diesen Dateien gehen verloren.
                  </span>
                </span>
              </label>
            )}
            <label className="flex items-start gap-3 text-[13px]">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={opts.purge}
                onChange={(e) => setOpts((o) => ({ ...o, purge: e.target.checked }))}
              />
              <span className="flex flex-col gap-0.5">
                <span className="font-medium">Auch Keybunds Liste der Keys löschen</span>
                <span className="text-muted-foreground">~/.config/keybund/profiles.toml – ohne lässt sich Keybund später wieder einrichten.</span>
              </span>
            </label>
          </Card>
          {preview.isPending && <Spinner />}
          {preview.data && changes.length === 0 && <p className="m-0 text-muted-foreground">Nichts zu entfernen.</p>}
          {changes.length > 0 && <DiffView changes={changes} />}
          {run.isError && <FieldError>{errorMessage(run.error)}</FieldError>}
          {changes.length > 0 && (
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button variant="destructive" className="self-start">
                  Keybund entfernen
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogTitle>Keybund wirklich entfernen?</AlertDialogTitle>
                <AlertDialogDescription>
                  Danach wählt Git den Key nicht mehr automatisch. Deine Keys bleiben, die Änderungen siehst du in der Vorschau.
                </AlertDialogDescription>
                <AlertDialogFooter>
                  <AlertDialogCancel>Abbrechen</AlertDialogCancel>
                  <AlertDialogAction onClick={() => run.mutate(changes.map((c) => c.diff))}>Entfernen</AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          )}
        </>
      )}
    </>
  );
}
