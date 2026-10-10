import { Link } from "@tanstack/react-router";
import { AlertTriangle, Plus, Server } from "lucide-react";
import type { ReactNode } from "react";
import { ColorDot } from "@/components/color-dot";
import { ConfigStatus } from "@/components/pending-banner";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import type { ProfileView } from "@/lib/bindings";
import { groupProfiles } from "@/lib/categories";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn, formatRelative, hostSummary } from "@/lib/utils";

export function HomePage() {
  const { data, isPending } = useProfiles();
  const doctor = useDoctor();
  const problems = doctor.data?.checks.filter((c) => c.status !== "ok") ?? [];
  const groups = groupProfiles(data?.profiles ?? []);
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
              Neues Profil
            </Link>
            <Link to={data && data.import_available > 0 ? "/import" : "/settings"} className={cn(buttonVariants({ variant: "outline" }), "no-underline")}>
              Vorhandene Keys importieren
            </Link>
          </div>
        </div>
      ) : (
        <>
          {groups.standard && (
            <Section title="Standard" hint="Gilt überall, wo kein anderes Profil passt.">
              <ProfileCard view={groups.standard} />
            </Section>
          )}
          {groups.projects.length > 0 && (
            <Section title="Projekte & Konten" hint="Gelten in ihren Projektordnern.">
              {groups.projects.map((v) => (
                <ProfileCard key={v.profile.id} view={v} />
              ))}
            </Section>
          )}
          {groups.servers.length > 0 && (
            <Section title="Server" hint="Gelten für ssh, scp und sftp auf diesen Hosts.">
              {groups.servers.map(({ host, profile }) => (
                <li key={`${profile.id}-${host.alias ?? host.hostname}`}>
                  <Link to="/profile/$id" params={{ id: profile.id }} className={card}>
                    <span className="flex items-center gap-2.5">
                      <Server aria-hidden className="size-3.5 text-muted-foreground" />
                      <span className="flex-1 font-mono text-[13px] font-medium">{host.alias ?? host.hostname}</span>
                    </span>
                    <span className="font-mono text-xs text-muted-foreground">
                      {host.user ? `${host.user}@` : ""}
                      {host.hostname}
                      {host.port && host.port !== 22 ? `:${host.port}` : ""}
                    </span>
                    <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <ColorDot color={profile.color} className="size-2" />
                      Key von {profile.name}
                    </span>
                  </Link>
                </li>
              ))}
            </Section>
          )}
        </>
      )}
      <ConfigStatus />
    </>
  );
}

const card =
  "flex h-full flex-col gap-2 rounded-xl border border-border bg-card p-4 text-foreground no-underline hover:border-zinc-600";

function Section({ title, hint, children }: { title: string; hint: string; children: ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-2.5">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
        <h2 className="m-0 text-[15px] font-semibold">{title}</h2>
        <span className="text-xs text-muted-foreground">{hint}</span>
      </div>
      <ul className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3 p-0">{children}</ul>
    </section>
  );
}

function ProfileCard({ view: { profile, last_test } }: { view: ProfileView }) {
  return (
    <li>
      <Link to="/profile/$id" params={{ id: profile.id }} className={card}>
        <span className="flex items-center gap-2.5">
          <ColorDot color={profile.color} className="size-2.5" />
          <span className="flex-1 font-medium">{profile.name}</span>
        </span>
        <span className="font-mono text-xs text-muted-foreground">{profile.folders[0] ?? profile.key.path}</span>
        <span className="text-xs text-muted-foreground">
          {hostSummary(profile) || "–"}
          {last_test && ` · ${last_test.success ? "verbunden" : "Test fehlgeschlagen"} ${formatRelative(last_test.at)}`}
        </span>
      </Link>
    </li>
  );
}
