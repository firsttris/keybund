import { useMutation } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowLeft, KeyRound } from "lucide-react";
import { useState } from "react";
import { PageFrame } from "@/components/app-shell";
import { ColorDot } from "@/components/color-dot";
import { DoctorPanel } from "@/components/doctor-panel";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { FieldError } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { api, errorMessage } from "@/lib/api";
import type { WhichResult } from "@/lib/bindings";
import { useProfiles } from "@/lib/queries";

export function WhichPage({ initial }: { initial?: string }) {
  const profiles = useProfiles();
  const active = profiles.data?.profiles.filter((p) => !p.profile.archived) ?? [];
  const examples = [
    ...active.flatMap((p) => p.profile.hosts.filter((h) => h.alias).map((h) => h.alias!)),
    ...active.slice(0, 2).flatMap((p) => p.profile.folders.slice(0, 1)),
  ].slice(0, 3);
  const [query, setQuery] = useState(initial ?? "");
  const resolve = useMutation({ mutationFn: (q: string) => api.which(q) });
  const run = (q: string) => {
    setQuery(q);
    if (q.trim()) resolve.mutate(q.trim());
  };

  return (
    <PageFrame
      back={
        <Link to="/" className="flex min-h-9 items-center gap-2 text-[13px] text-secondary-foreground no-underline hover:text-foreground">
          <ArrowLeft aria-hidden className="size-[15px]" />
          Profile
        </Link>
      }
    >
      <div className="flex w-full max-w-[1040px] flex-wrap items-start gap-6">
        <section className="flex min-w-0 flex-[999_1_520px] flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <h1 className="m-0 text-[22px] font-semibold tracking-tight">Welcher Key?</h1>
            <p className="m-0 text-muted-foreground">Ordner oder Host eingeben. Keybund zeigt, was ssh und git dort wirklich verwenden.</p>
          </div>
          <form
            className="flex flex-wrap gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              run(query);
            }}
          >
            <label htmlFor="which-input" className="sr-only">
              Ordner oder Host
            </label>
            <Input
              id="which-input"
              className="h-10 flex-[1_1_280px] font-mono"
              placeholder="~/dev/projekt/repo oder meinserver"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <Button type="submit" size="lg" disabled={!query.trim() || resolve.isPending}>
              Auflösen
            </Button>
          </form>
          {examples.length > 0 && (
            <div className="flex flex-wrap gap-2 text-xs">
              <span className="self-center text-muted-foreground">Beispiele:</span>
              {examples.map((ex) => (
                <button
                  key={ex}
                  onClick={() => run(ex)}
                  className="h-[30px] rounded-full border border-border bg-transparent px-2.5 font-mono text-xs text-secondary-foreground hover:bg-muted"
                >
                  {ex}
                </button>
              ))}
            </div>
          )}
          {resolve.isPending && <Spinner label="Löse auf" />}
          {resolve.isError && <FieldError>{errorMessage(resolve.error)}</FieldError>}
          {resolve.data && <WhichCard result={resolve.data} />}
        </section>
        <DoctorPanel className="min-w-0 flex-[1_1_340px]" />
      </div>
    </PageFrame>
  );
}

function WhichCard({ result }: { result: WhichResult }) {
  const profiles = useProfiles();
  const profile = profiles.data?.profiles.find((p) => p.profile.id === result.profile_id)?.profile;
  const rows: [string, React.ReactNode][] = [];
  if (result.source) rows.push(["Quelle", <span className="font-mono text-[12.5px] [overflow-wrap:anywhere]">{result.source}{result.via ? ` · via ${result.via}` : ""}</span>]);
  if (result.git_email) rows.push(["Git-Identität", `${result.git_name ?? ""} <${result.git_email}>`]);
  if (result.hostname) rows.push(["Ziel", <span className="font-mono text-[12.5px]">{result.user ? `${result.user}@` : ""}{result.hostname}{result.port ? `:${result.port}` : ""}</span>]);
  if (result.identities_only != null) rows.push(["IdentitiesOnly", result.identities_only ? "ja, nur dieser Key wird angeboten" : "nein, ssh probiert mehrere Keys"]);
  if (result.agent_loaded != null)
    rows.push([
      "Agent",
      <span className="flex items-center gap-2">
        <span aria-hidden className={result.agent_loaded ? "size-[7px] rounded-full bg-emerald-500" : "size-[7px] rounded-full bg-amber-500"} />
        {result.agent_loaded ? "geladen, keine Passphrase-Abfrage nötig" : "nicht geladen"}
      </span>,
    ]);

  return (
    <div data-testid="which-result" className="overflow-hidden rounded-xl border border-border bg-card">
      <div className="flex flex-wrap items-center gap-3 border-b border-border px-5 py-[18px]">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-[10px] bg-blue-500/15 text-blue-300">
          <KeyRound aria-hidden className="size-[18px]" />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="font-mono text-sm font-medium">{result.key_path?.split("/").pop() ?? "kein fester Key"}</span>
          <span className="text-[13px] text-muted-foreground">
            {result.mode === "folder" ? "für git push, pull und fetch in diesem Ordner" : `für ssh ${result.query}`}
          </span>
        </div>
        {profile && (
          <Link
            to="/profile/$id"
            params={{ id: profile.id }}
            className="flex min-h-7 items-center gap-2 rounded-md border border-border px-2.5 py-1 text-xs font-medium text-secondary-foreground no-underline"
          >
            <ColorDot color={profile.color} />
            Profil {profile.name}
          </Link>
        )}
      </div>
      <dl className="m-0 grid grid-cols-[minmax(130px,180px)_minmax(0,1fr)]">
        {rows.map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="border-b border-border-subtle px-5 py-3 text-[13px] text-muted-foreground">{label}</dt>
            <dd className="m-0 border-b border-border-subtle px-5 py-3 text-[13px]">{value}</dd>
          </div>
        ))}
      </dl>
      {result.notes.length > 0 && (
        <ul className="m-0 flex list-none flex-col gap-1 px-5 py-3 text-[13px] text-muted-foreground">
          {result.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
