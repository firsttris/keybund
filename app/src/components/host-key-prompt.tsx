import { ShieldQuestion } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { setHostKeyAsker, type UnknownHostKey } from "@/lib/host-keys";
import { m } from "@/paraglide/messages";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogTitle,
} from "./ui/dialog";

type Question = { check: UnknownHostKey; resolve: (ok: boolean) => void };

/**
 * Asks the user to confirm the host key on the first contact with a server.
 * Questions queue up (two tests started back to back ask one after the
 * other); none is lost, every test gets its answer.
 */
export function HostKeyPrompt() {
  const [queue, setQueue] = useState<Question[]>([]);
  // The cleanup needs the questions still open when the prompt goes away.
  const open = useRef<Question[]>([]);
  open.current = queue;

  useEffect(() => {
    setHostKeyAsker((check) => new Promise((resolve) => setQueue((q) => [...q, { check, resolve }])));
    return () => {
      setHostKeyAsker(null);
      // Nobody can answer any more: decline, so no test waits forever.
      for (const q of open.current) q.resolve(false);
    };
  }, []);

  const head = queue[0];
  // Per question and idempotent: a button click also fires Radix's onOpenChange(false).
  const answer = (question: Question, ok: boolean) => {
    question.resolve(ok);
    setQueue((q) => q.filter((x) => x !== question));
  };

  return (
    <AlertDialog open={!!head} onOpenChange={(isOpen) => !isOpen && head && answer(head, false)}>
      {head && (
        <AlertDialogContent>
          <AlertDialogTitle className="flex items-center gap-2">
            <ShieldQuestion aria-hidden className="size-4 text-warning" />
            {m.hostKey_title({ host: head.check.host })}
          </AlertDialogTitle>
          <AlertDialogDescription>{m.hostKey_explain()}</AlertDialogDescription>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {head.check.keys.map((k) => (
              <li key={k.fingerprint} className="flex flex-col gap-0.5 rounded-lg bg-muted px-3 py-2">
                <span className="text-xs text-muted-foreground">{k.key_type}</span>
                <code className="font-mono text-[12.5px] break-all">{k.fingerprint}</code>
              </li>
            ))}
          </ul>
          <span className="text-xs text-muted-foreground">{m.hostKey_storedIn({ file: head.check.file })}</span>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => answer(head, false)}>{m.common_cancel()}</AlertDialogCancel>
            <AlertDialogAction onClick={() => answer(head, true)}>{m.hostKey_confirm()}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
