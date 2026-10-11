import { Link } from "@tanstack/react-router";
import { AlertTriangle, CheckCircle2, Folder, GitBranch, Info, Plus, Server, Star, XCircle } from "lucide-react";
import { ColorDot } from "@/components/color-dot";
import { HowItWorks } from "@/components/how-it-works";
import { ConfigStatus } from "@/components/pending-banner";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { errorMessage } from "@/lib/api";
import type { ProfileView } from "@/lib/bindings";
import { activeKeys, gitAccounts, servers, usageParts } from "@/lib/categories";
import { rich, t } from "@/lib/i18n";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn, formatRelative, providerLabel } from "@/lib/utils";
import { m } from "@/paraglide/messages";

export function HomePage() {
  const { data, isPending } = useProfiles();
  const keys = activeKeys(data?.profiles ?? []);

  if (isPending) return <Spinner />;
  return (
    <>
      <h1 className="m-0 text-[26px] font-semibold tracking-tight">{m.home_title()}</h1>
      <Todo />
      <HowItWorks forceOpen={keys.length === 0} />
      {keys.length === 0 ? (
        <div className="flex max-w-[520px] flex-col gap-3 rounded-xl border border-dashed border-border p-8">
          <span className="text-[15px] font-medium">{m.sidebar_noKeys()}</span>
          <p className="m-0 text-muted-foreground">{m.home_emptyText()}</p>
          <div className="flex flex-wrap gap-2">
            <Link to="/new" className={cn(buttonVariants(), "no-underline")}>
              <Plus aria-hidden />
              {m.sidebar_newKey()}
            </Link>
            <Link
              to={data && data.import_available > 0 ? "/import" : "/settings"}
              className={cn(buttonVariants({ variant: "outline" }), "no-underline")}
            >
              {m.home_takeOver()}
            </Link>
          </div>
        </div>
      ) : (
        <section aria-labelledby="keys-title" className="flex flex-col gap-3">
          <h2 id="keys-title" className="m-0 text-[15px] font-semibold">
            {m.home_yourKeys()}
          </h2>
          <ul
            aria-label={m.sidebar_keys()}
            className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3 p-0"
          >
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
            <span className="font-medium">{v.profile.name}:</span> {t(v.last_test.message)}
          </>
        ),
        link: (
          <Link to="/profile/$id" params={{ id: v.profile.id }} className={linkClass}>
            {m.doctor_fix()}
          </Link>
        ),
      });
    }
  }
  if (doctor.isError) {
    items.push({
      id: "doctor-failed",
      tone: "danger",
      text: <span className="font-medium">{m.home_doctorFailed({ error: errorMessage(doctor.error) })}</span>,
      link: (
        <Link to="/doctor" className={linkClass}>
          {m.home_toDoctor()}
        </Link>
      ),
    });
  }
  for (const c of doctor.data?.checks.filter((c) => c.status !== "ok") ?? []) {
    items.push({
      id: `doctor-${c.id}`,
      tone: c.status === "error" ? "danger" : "warn",
      text: <span className="font-medium">{t(c.title)}</span>,
      link: (
        <Link to="/doctor" className={linkClass}>
          {m.home_toDoctor()}
        </Link>
      ),
    });
  }
  if (data.import_available > 0) {
    items.push({
      id: "import",
      tone: "info",
      text: rich((i) => m.home_importAvailable({ ...i, n: data.import_available }), {
        file: <span className="font-mono">~/.ssh/config</span>,
      }),
      link: (
        <Link to="/import" className={linkClass}>
          {m.home_takeOverShort()}
        </Link>
      ),
    });
  }
  for (const v of keys) {
    if (usageParts(v).length === 0) {
      items.push({
        id: `unused-${v.profile.id}`,
        tone: "info",
        text: rich(m.home_unused, { name: <span className="font-medium">{v.profile.name}</span> }),
        link: (
          <Link to="/profile/$id" params={{ id: v.profile.id }} className={linkClass}>
            {m.home_decide()}
          </Link>
        ),
      });
    }
  }
  if (keys.length === 0 && items.length === 0) return null;
  if (items.length === 0) {
    return (
      <p
        role="status"
        className="m-0 flex items-center gap-2.5 rounded-[10px] border border-emerald-500/30 bg-emerald-500/5 px-4 py-3 text-[13px]"
      >
        <CheckCircle2 aria-hidden className="size-4 text-success" />
        {m.home_allFine()}
      </p>
    );
  }
  const icon = { danger: XCircle, warn: AlertTriangle, info: Info };
  const color = { danger: "text-danger", warn: "text-warning", info: "text-blue-400" };
  return (
    <section aria-labelledby="todo-title" className="flex flex-col gap-2">
      <h2 id="todo-title" className="m-0 text-[15px] font-semibold">
        {m.home_todo()}
      </h2>
      <ul
        aria-labelledby="todo-title"
        className="m-0 flex list-none flex-col overflow-hidden rounded-[10px] border border-border p-0"
      >
        {items.map((item) => {
          const Icon = icon[item.tone];
          return (
            <li
              key={item.id}
              className="flex flex-wrap items-center gap-3 border-b border-border-subtle bg-card px-4 py-3 last:border-b-0"
            >
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
    ...(view.is_default ? [{ icon: Star, text: m.usage_defaultForGit() }] : []),
    ...profile.folders.map((f) => ({ icon: Folder, text: f })),
    ...gitAccounts(profile).map((h) => ({
      icon: GitBranch,
      text: `${providerLabel(h.provider)}${h.alias ? ` (${h.alias})` : ""}`,
    })),
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
              {(last_test.success ? m.home_connected : m.home_testFailed)({ when: formatRelative(last_test.at) })}
            </span>
          )}
        </span>
        <span className="font-mono text-xs text-muted-foreground">{profile.key.path}</span>
        {usages.length > 0 ? (
          <ul aria-label={m.home_usedFor({ name: profile.name })} className="m-0 flex list-none flex-col gap-1.5 p-0">
            {usages.map(({ icon: Icon, text }) => (
              <li key={text} className="flex min-w-0 items-center gap-2 text-[13px] text-secondary-foreground">
                <Icon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="truncate">{text}</span>
              </li>
            ))}
          </ul>
        ) : (
          <span className="text-[13px] text-warning">{m.home_unusedCard()}</span>
        )}
      </Link>
    </li>
  );
}
