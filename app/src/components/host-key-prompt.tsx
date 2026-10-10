import { ShieldQuestion } from "lucide-react";
import { useEffect, useState } from "react";
import { setHostKeyAsker, type UnknownHostKey } from "@/lib/host-keys";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogTitle,
} from "./ui/dialog";

/** Asks the user to confirm the host key on the first contact with a server. */
export function HostKeyPrompt() {
  const [pending, setPending] = useState<{ check: UnknownHostKey; resolve: (ok: boolean) => void } | null>(null);

  useEffect(() => {
    setHostKeyAsker((check) => new Promise((resolve) => setPending({ check, resolve })));
    return () => setHostKeyAsker(null);
  }, []);

  const answer = (ok: boolean) => {
    pending?.resolve(ok);
    setPending(null);
  };

  return (
    <AlertDialog open={!!pending} onOpenChange={(open) => !open && answer(false)}>
      {pending && (
        <AlertDialogContent>
          <AlertDialogTitle className="flex items-center gap-2">
            <ShieldQuestion aria-hidden className="size-4 text-warning" />
            Erste Verbindung zu {pending.check.host}
          </AlertDialogTitle>
          <AlertDialogDescription>
            Der Server weist sich mit diesem Host-Key aus. Vergleiche den Fingerprint mit dem Server
            (ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub) oder den Angaben deines Hosters. Nur wenn er stimmt,
            ist es wirklich dein Server.
          </AlertDialogDescription>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {pending.check.keys.map((k) => (
              <li key={k.fingerprint} className="flex flex-col gap-0.5 rounded-lg bg-muted px-3 py-2">
                <span className="text-xs text-muted-foreground">{k.key_type}</span>
                <code className="font-mono text-[12.5px] break-all">{k.fingerprint}</code>
              </li>
            ))}
          </ul>
          <span className="text-xs text-muted-foreground">Wird gespeichert in {pending.check.file}</span>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => answer(false)}>Abbrechen</AlertDialogCancel>
            <AlertDialogAction onClick={() => answer(true)}>Fingerprint stimmt</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
