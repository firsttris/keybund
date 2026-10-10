import { AlertTriangle, Check, X } from "lucide-react";
import type { Check as CheckT } from "@/lib/bindings";
import { useDoctor, useFixDoctor } from "@/lib/queries";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Card, CardHeader, CardTitle } from "./ui/card";
import { Spinner } from "./ui/spinner";

function Problem({ check }: { check: CheckT }) {
  const fix = useFixDoctor();
  const error = check.status === "error";
  return (
    <li
      className={
        error
          ? "flex flex-col gap-2.5 rounded-[10px] border border-red-400/35 bg-red-500/5 p-3.5"
          : "flex flex-col gap-2.5 rounded-[10px] border border-amber-500/35 bg-amber-500/5 p-3.5"
      }
    >
      <div className="flex gap-2.5">
        {error ? (
          <X aria-hidden className="mt-px size-4 shrink-0 text-danger" />
        ) : (
          <AlertTriangle aria-hidden className="mt-px size-4 shrink-0 text-warning" />
        )}
        <div className="flex flex-col gap-1">
          <span className="text-[13px] font-medium">{check.title}</span>
          {check.detail && <span className="text-[12.5px] leading-normal text-secondary-foreground">{check.detail}</span>}
        </div>
      </div>
      {check.fix && (
        <Button
          size="sm"
          className="self-end"
          disabled={fix.isPending}
          onClick={() => fix.mutate(check.fix!)}
          aria-label={`${check.title} beheben`}
        >
          {fix.isPending ? <Spinner className="size-3" /> : null}
          Beheben
        </Button>
      )}
    </li>
  );
}

export function DoctorPanel({ className }: { className?: string }) {
  const doctor = useDoctor();
  const checks = doctor.data?.checks ?? [];
  const problems = checks.filter((c) => c.status !== "ok");
  const ok = checks.filter((c) => c.status === "ok");
  return (
    <Card className={className} aria-labelledby="doctor-title">
      <CardHeader>
        <CardTitle id="doctor-title">Doctor</CardTitle>
        {doctor.isSuccess &&
          (problems.length ? (
            <Badge variant="warning">{problems.length === 1 ? "1 Hinweis" : `${problems.length} Hinweise`}</Badge>
          ) : (
            <Badge variant="success">Alles in Ordnung</Badge>
          ))}
      </CardHeader>
      {doctor.isPending && <Spinner />}
      {problems.length > 0 && (
        <ul aria-label="Probleme" className="m-0 flex list-none flex-col gap-2.5 p-0">
          {problems.map((c) => (
            <Problem key={c.id} check={c} />
          ))}
        </ul>
      )}
      <ul aria-label="Bestanden" className="m-0 flex list-none flex-col p-0">
        {ok.map((c) => (
          <li key={c.id} className="flex min-h-10 items-center gap-2.5 border-b border-border-subtle text-[13px] last:border-b-0">
            <Check aria-hidden className="size-[15px] text-success" strokeWidth={2.6} />
            <span className="flex-1">{c.title}</span>
            {c.detail && <span className="text-xs text-muted-foreground">{c.detail}</span>}
          </li>
        ))}
      </ul>
      <Button variant="outline" onClick={() => doctor.refetch()} disabled={doctor.isFetching}>
        Erneut prüfen
      </Button>
    </Card>
  );
}
