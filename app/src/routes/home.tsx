import { Link } from "@tanstack/react-router";
import { AlertTriangle, Folder, GitBranch, Plus, Server, Star } from "lucide-react";
import { ColorDot } from "@/components/color-dot";
import { HowItWorks } from "@/components/how-it-works";
import { ConfigStatus } from "@/components/pending-banner";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import type { ProfileView } from "@/lib/bindings";
import { activeKeys, gitAccounts, servers } from "@/lib/categories";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn, formatRelative, providerLabel } from "@/lib/utils";

export function HomePage() {
  const { data, isPending } = useProfiles();
  const doctor = useDoctor();
  const problems = doctor.data?.checks.filter((c) => c.status !== "ok") ?? [];
  const keys = activeKeys(data?.profiles ?? []);

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
      <h1 className="m-0 text-[26px] font-semibold tracking-tight">Deine SSH-Keys</h1>
      <HowItWorks forceOpen={keys.length === 0} />
      {keys.length === 0 ? (
        <div className="flex max-w-[520px] flex-col gap-3 rounded-xl border border-dashed border-border p-8">
          <span className="text-[15px] font-medium">Noch keine Keys</span>
          <p className="m-0 text-muted-foreground">
            Leg einen neuen Key an oder übernimm die Keys, die du schon in ~/.ssh hast.
          </p>
          <div className="flex flex-wrap gap-2">
            <Link to="/new" className={cn(buttonVariants(), "no-underline")}>
              <Plus aria-hidden />
              Neuer Key
            </Link>
            <Link to={data && data.import_available > 0 ? "/import" : "/settings"} className={cn(buttonVariants({ variant: "outline" }), "no-underline")}>
              Vorhandene Keys übernehmen
            </Link>
          </div>
        </div>
      ) : (
        <ul aria-label="Keys" className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3 p-0">
          {keys.map((v) => (
            <KeyCard key={v.profile.id} view={v} />
          ))}
        </ul>
      )}
      <ConfigStatus />
    </>
  );
}

function KeyCard({ view }: { view: ProfileView }) {
  const { profile, last_test } = view;
  const usages = [
    ...(view.is_default ? [{ icon: Star, text: "Standard für Git" }] : []),
    ...profile.folders.map((f) => ({ icon: Folder, text: f })),
    ...gitAccounts(profile).map((h) => ({ icon: GitBranch, text: `${providerLabel(h.provider)}${h.alias ? ` (${h.alias})` : ""}` })),
    ...servers(profile).map((h) => ({ icon: Server, text: `ssh ${h.alias ?? h.hostname}` })),
  ];
  return (
    <li>
      <Link
        to="/profile/$id"
        params={{ id: profile.id }}
        className="flex h-full flex-col gap-3 rounded-xl border border-border bg-card p-4 text-foreground no-underline hover:border-zinc-600"
      >
        <span className="flex items-center gap-2.5">
          <ColorDot color={profile.color} className="size-2.5" />
          <span className="flex-1 font-medium">{profile.name}</span>
          {last_test && (
            <span className={cn("text-xs", last_test.success ? "text-muted-foreground" : "text-danger")}>
              {last_test.success ? "verbunden" : "Test fehlgeschlagen"} {formatRelative(last_test.at)}
            </span>
          )}
        </span>
        <span className="font-mono text-xs text-muted-foreground">{profile.key.path}</span>
        {usages.length > 0 ? (
          <ul aria-label={`${profile.name} wird benutzt für`} className="m-0 flex list-none flex-col gap-1.5 p-0">
            {usages.map(({ icon: Icon, text }) => (
              <li key={text} className="flex min-w-0 items-center gap-2 text-[13px] text-secondary-foreground">
                <Icon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="truncate">{text}</span>
              </li>
            ))}
          </ul>
        ) : (
          <span className="text-[13px] text-warning">Noch nirgends benutzt – öffnen und festlegen, wofür</span>
        )}
      </Link>
    </li>
  );
}
