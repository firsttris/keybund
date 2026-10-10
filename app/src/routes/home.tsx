import { Link } from "@tanstack/react-router";
import { AlertTriangle, CheckCircle2, Folder, GitBranch, Info, Plus, Server, Star, XCircle } from "lucide-react";
import { ColorDot } from "@/components/color-dot";
import { HowItWorks } from "@/components/how-it-works";
import { ConfigStatus } from "@/components/pending-banner";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import type { ProfileView } from "@/lib/bindings";
import { activeKeys, gitAccounts, servers, usageParts } from "@/lib/categories";
import { errorMessage } from "@/lib/api";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn, formatRelative, providerLabel } from "@/lib/utils";

export function HomePage() {
  const { data, isPending } = useProfiles();
  const keys = activeKeys(data?.profiles ?? []);

  if (isPending) return <Spinner />;
  return (
    <>
      <h1 className="m-0 text-[26px] font-semibold tracking-tight">Übersicht</h1>
      <Todo />
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
        <section aria-labelledby="keys-title" className="flex flex-col gap-3">
          <h2 id="keys-title" className="m-0 text-[15px] font-semibold">
            Deine SSH-Keys
          </h2>
          <ul aria-label="Keys" className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3 p-0">
            {keys.map((v) => (
              <KeyCard key={v.profile.id} view={v} />
            ))}
          </ul>
        </section>
      )}
      <ConfigStatus />
    </>
  );
}

type TodoItem = { id: string; tone: "danger" | "warn" | "info"; text: React.ReactNode; link: React.ReactNode };

const linkClass = "shrink-0 text-[13px] font-medium text-foreground";

/** What needs attention right now – or a calm "all good". */
function Todo() {
  const { data } = useProfiles();
  const doctor = useDoctor();
  // Only claim "all good" once the doctor has answered.
  if (!data || doctor.isPending) return null;
  const keys = activeKeys(data.profiles);
  const items: TodoItem[] = [];
  for (const v of keys) {
    if (v.last_test && !v.last_test.success) {
      items.push({
        id: `test-${v.profile.id}`,
        tone: "danger",
        text: (
          <>
            <span className="font-medium">{v.profile.name}:</span> {v.last_test.message}
          </>
        ),
        link: (
          <Link to="/profile/$id" params={{ id: v.profile.id }} className={linkClass}>
            Beheben
          </Link>
        ),
      });
    }
  }
  if (doctor.isError) {
    items.push({
      id: "doctor-failed",
      tone: "danger",
      text: <span className="font-medium">Die Prüfung von ssh und Git ist fehlgeschlagen: {errorMessage(doctor.error)}</span>,
      link: (
        <Link to="/doctor" className={linkClass}>
          Zum Doctor
        </Link>
      ),
    });
  }
  for (const c of doctor.data?.checks.filter((c) => c.status !== "ok") ?? []) {
    items.push({
      id: `doctor-${c.id}`,
      tone: c.status === "error" ? "danger" : "warn",
      text: <span className="font-medium">{c.title}</span>,
      link: (
        <Link to="/doctor" className={linkClass}>
          Zum Doctor
        </Link>
      ),
    });
  }
  if (data.import_available > 0) {
    items.push({
      id: "import",
      tone: "info",
      text: (
        <>
          In deiner <span className="font-mono">~/.ssh/config</span> stehen{" "}
          {data.import_available === 1 ? "ein Eintrag" : `${data.import_available} Einträge`} mit eigenem Key, die Keybund
          übernehmen kann.
        </>
      ),
      link: (
        <Link to="/import" className={linkClass}>
          Übernehmen
        </Link>
      ),
    });
  }
  for (const v of keys) {
    if (usageParts(v).length === 0) {
      items.push({
        id: `unused-${v.profile.id}`,
        tone: "info",
        text: (
          <>
            <span className="font-medium">{v.profile.name}</span> wird noch nirgends benutzt.
          </>
        ),
        link: (
          <Link to="/profile/$id" params={{ id: v.profile.id }} className={linkClass}>
            Festlegen
          </Link>
        ),
      });
    }
  }
  if (keys.length === 0 && items.length === 0) return null;
  if (items.length === 0) {
    return (
      <p role="status" className="m-0 flex items-center gap-2.5 rounded-[10px] border border-emerald-500/30 bg-emerald-500/5 px-4 py-3 text-[13px]">
        <CheckCircle2 aria-hidden className="size-4 text-success" />
        Alles in Ordnung – Git und ssh nehmen automatisch den richtigen Key.
      </p>
    );
  }
  const icon = { danger: XCircle, warn: AlertTriangle, info: Info };
  const color = { danger: "text-danger", warn: "text-warning", info: "text-blue-400" };
  return (
    <section aria-labelledby="todo-title" className="flex flex-col gap-2">
      <h2 id="todo-title" className="m-0 text-[15px] font-semibold">
        Zu tun
      </h2>
      <ul aria-labelledby="todo-title" className="m-0 flex list-none flex-col overflow-hidden rounded-[10px] border border-border p-0">
        {items.map((item) => {
          const Icon = icon[item.tone];
          return (
            <li key={item.id} className="flex flex-wrap items-center gap-3 border-b border-border-subtle bg-card px-4 py-3 last:border-b-0">
              <Icon aria-hidden className={cn("size-4 shrink-0", color[item.tone])} />
              <span className="min-w-[200px] flex-1 text-[13px]">{item.text}</span>
              {item.link}
            </li>
          );
        })}
      </ul>
    </section>
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
