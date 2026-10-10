import { Check, Copy, Info, X } from "lucide-react";
import { useState } from "react";
import type { ProbeResult } from "@/lib/bindings";
import { cn } from "@/lib/utils";

/**
 * What happened during a connection test, in plain words: which config
 * applied, as which user ssh logged in, which key it offered and what the
 * server said. Plus the command to try it yourself and the raw output.
 */
export function ProbeDetails({ result }: { result: ProbeResult }) {
  const [copied, setCopied] = useState(false);
  return (
    <details className="group rounded-lg border border-border bg-background text-[13px]" open={!result.success}>
      <summary className="cursor-pointer list-none px-3 py-2 text-muted-foreground select-none hover:text-foreground [&::-webkit-details-marker]:hidden">
        <span className="mr-1.5 inline-block transition-transform group-open:rotate-90">›</span>
        Details: Was beim Test passiert ist
      </summary>
      <div className="flex flex-col gap-3 border-t border-border px-3 py-3">
        {result.steps.length > 0 ? (
          <ol aria-label="Ablauf" className="m-0 flex list-none flex-col gap-1.5 p-0">
            {result.steps.map((step, i) => (
              <li key={i} className="flex items-start gap-2">
                {step.ok === true ? (
                  <Check aria-label="ok" className="mt-0.5 size-3.5 shrink-0 text-success" strokeWidth={2.6} />
                ) : step.ok === false ? (
                  <X aria-label="fehlgeschlagen" className="mt-0.5 size-3.5 shrink-0 text-danger" strokeWidth={2.6} />
                ) : (
                  <Info aria-hidden className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
                )}
                <span className={cn("[overflow-wrap:anywhere]", step.ok === false && "text-red-300")}>{step.text}</span>
              </li>
            ))}
          </ol>
        ) : (
          <span className="text-muted-foreground">ssh hat keine Einzelheiten geliefert.</span>
        )}
        <div className="flex flex-col gap-1">
          <span className="text-xs text-muted-foreground">Selbst ausprobieren im Terminal (mit -v für alle Einzelheiten):</span>
          <div className="flex items-start gap-2 rounded-md bg-muted px-2.5 py-2">
            <code className="min-w-0 flex-1 font-mono text-xs break-all">{result.command}</code>
            <button
              type="button"
              aria-label="Befehl kopieren"
              className="text-muted-foreground hover:text-foreground"
              onClick={() => {
                void navigator.clipboard?.writeText(result.command).catch(() => undefined);
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }}
            >
              {copied ? <Check aria-hidden className="size-3.5" /> : <Copy aria-hidden className="size-3.5" />}
            </button>
          </div>
        </div>
        {result.log && (
          <details>
            <summary className="cursor-pointer text-xs text-muted-foreground hover:text-foreground">Komplette Ausgabe von ssh</summary>
            <pre className="m-0 mt-2 max-h-72 overflow-auto rounded-md bg-muted p-2.5 font-mono text-[11.5px] leading-relaxed text-muted-foreground">
              {result.log}
            </pre>
          </details>
        )}
      </div>
    </details>
  );
}
