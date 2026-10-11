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
import { m } from "@/paraglide/messages";
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
        <h1 className="m-0 text-[22px] font-semibold tracking-tight">{m.uninstall_title()}</h1>
        <p className="m-0 max-w-[720px] leading-normal text-muted-foreground">{m.uninstall_intro()}</p>
      </div>
      {run.isSuccess ? (
        <p role="status" className="m-0 flex items-center gap-2 text-success">
          <Check aria-hidden className="size-4" />
          {m.uninstall_done()} {opts.purge ? m.uninstall_donePurged() : m.uninstall_doneKept()}
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
                  <span className="font-medium">{m.uninstall_restore()}</span>
                  <span className="text-muted-foreground">{m.uninstall_restoreHint()}</span>
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
                <span className="font-medium">{m.uninstall_purge()}</span>
                <span className="text-muted-foreground">{m.uninstall_purgeHint()}</span>
              </span>
            </label>
          </Card>
          {preview.isPending && <Spinner />}
          {preview.isError && <FieldError>{errorMessage(preview.error)}</FieldError>}
          {preview.data && changes.length === 0 && <p className="m-0 text-muted-foreground">{m.uninstall_nothing()}</p>}
          {changes.length > 0 && <DiffView changes={changes} />}
          {run.isError && <FieldError>{errorMessage(run.error)}</FieldError>}
          {changes.length > 0 && (
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button variant="destructive" className="self-start">
                  {m.uninstall_title()}
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogTitle>{m.uninstall_confirmTitle()}</AlertDialogTitle>
                <AlertDialogDescription>
                  {m.uninstall_confirmText()}
                </AlertDialogDescription>
                <AlertDialogFooter>
                  <AlertDialogCancel>{m.common_cancel()}</AlertDialogCancel>
                  <AlertDialogAction onClick={() => run.mutate(changes.map((c) => c.diff))}>{m.uninstall_confirm()}</AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          )}
        </>
      )}
    </>
  );
}
