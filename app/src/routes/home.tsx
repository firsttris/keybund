import { Link } from "@tanstack/react-router";
import { AlertTriangle, Plus } from "lucide-react";
import { ColorDot } from "@/components/color-dot";
import { ConfigStatus } from "@/components/pending-banner";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn, formatRelative, hostSummary } from "@/lib/utils";

export function HomePage() {
  const { data, isPending } = useProfiles();
  const doctor = useDoctor();
  const problems = doctor.data?.checks.filter((c) => c.status !== "ok") ?? [];
  const active = data?.profiles.filter((p) => !p.profile.archived) ?? [];

  if (isPending) return <Spinner />;
  return (
    <>
      {problems.length > 0 && (
        <div role="alert" className="flex flex-wrap items-center gap-3 rounded-[10px] border border-amber-500/35 bg-amber-500/5 px-4 py-3">
          <AlertTriangle aria-hidden className="size-4 text-warning" />
          <span className="flex-1 text-[13px]">
            <span className="font-medium">{problems[0].title}</span>
            {problems.length > 1 && <span className="text-muted-foreground"> und {problems.length - 1} weitere Hinweise</span>}
          </span>
          <Link to="/doctor" className="text-[13px] font-medium text-foreground">
            Zum Doctor
          </Link>
        </div>
      )}
      {data && data.import_available > 0 && (
        <div className="flex flex-wrap items-center gap-3 rounded-[10px] border border-blue-500/35 bg-blue-500/5 px-4 py-3">
          <span className="flex-1 text-[13px]">
            In deiner <span className="font-mono">~/.ssh/config</span> stehen{" "}
            {data.import_available === 1 ? "ein Eintrag" : `${data.import_available} Einträge`} mit eigenem Key, die Keybund
            übernehmen kann.
          </span>
          <Link to="/import" className="text-[13px] font-medium text-foreground">
            Übernehmen
          </Link>
        </div>
      )}
      <h1 className="m-0 text-[26px] font-semibold tracking-tight">Profile</h1>
      {active.length === 0 ? (
        <div className="flex max-w-[520px] flex-col gap-3 rounded-xl border border-dashed border-border p-8">
          <span className="text-[15px] font-medium">Noch keine Profile</span>
          <p className="m-0 text-muted-foreground">
            Lege für jedes Projekt ein Profil an: eigener Key, eigene Git-Identität, automatisch richtig in jedem Ordner.
          </p>
          <div className="flex flex-wrap gap-2">
            <Link to="/new" className={cn(buttonVariants(), "no-underline")}>
              <Plus aria-hidden />
              Neues Projekt
            </Link>
            <Link to={data && data.import_available > 0 ? "/import" : "/settings"} className={cn(buttonVariants({ variant: "outline" }), "no-underline")}>
              Vorhandene Keys importieren
            </Link>
          </div>
        </div>
      ) : (
        <ul className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3 p-0">
          {active.map(({ profile, last_test, is_default }) => (
            <li key={profile.id}>
              <Link
                to="/profile/$id"
                params={{ id: profile.id }}
                className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4 text-foreground no-underline hover:border-zinc-600"
              >
                <span className="flex items-center gap-2.5">
                  <ColorDot color={profile.color} className="size-2.5" />
                  <span className="flex-1 font-medium">{profile.name}</span>
                  {is_default && <span className="text-xs text-muted-foreground">Default</span>}
                </span>
                <span className="font-mono text-xs text-muted-foreground">{profile.folders[0] ?? profile.hosts[0]?.alias ?? "–"}</span>
                <span className="text-xs text-muted-foreground">
                  {hostSummary(profile)}
                  {last_test && ` · ${last_test.success ? "verbunden" : "Test fehlgeschlagen"} ${formatRelative(last_test.at)}`}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
      <ConfigStatus />
    </>
  );
}
