import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { Activity, Archive, Crosshair, Download, KeyRound, Plus, Search, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import type { ProfileView } from "@/lib/bindings";
import { activeKeys, usageParts } from "@/lib/categories";
import { t } from "@/lib/i18n";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";
import { ColorDot } from "./color-dot";
import { buttonVariants } from "./ui/button";
import { Input } from "./ui/input";

const navItem =
  "flex min-h-10 items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-secondary-foreground no-underline hover:bg-muted data-[status=active]:bg-muted data-[status=active]:font-medium data-[status=active]:text-foreground [&_svg]:size-[15px]";

export function Sidebar() {
  const { data } = useProfiles();
  const doctor = useDoctor();
  const [filter, setFilter] = useState("");
  const navigate = useNavigate();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const problems = doctor.data?.checks.filter((c) => c.status !== "ok").length ?? 0;
  const q = filter.trim().toLowerCase();
  const matches = (...texts: (string | null | undefined)[]) => !q || texts.some((t) => t?.toLowerCase().includes(q));
  const keys = activeKeys(data?.profiles ?? []).filter((v) =>
    matches(
      v.profile.name,
      v.profile.key.path,
      ...v.profile.folders,
      ...v.profile.hosts.flatMap((h) => [h.alias, h.hostname]),
    ),
  );
  const archived = (data?.profiles ?? []).filter((v) => v.profile.archived && matches(v.profile.name));
  const nothing = data && keys.length === 0;
  const current = (id: string) => (path === `/profile/${id}` ? "page" : undefined);

  const keyLink = (view: ProfileView) => {
    const { profile, last_test } = view;
    const parts = usageParts(view);
    return (
      <Link
        key={profile.id}
        to="/profile/$id"
        params={{ id: profile.id }}
        aria-current={current(profile.id)}
        className={navItem}
      >
        <ColorDot color={profile.color} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate">{profile.name}</span>
          {last_test && !last_test.success ? (
            <span className="truncate text-xs font-normal text-danger" title={t(last_test.message)}>
              {m.sidebar_testFailed()}
            </span>
          ) : (
            <span
              className={cn("truncate text-xs font-normal", parts.length ? "text-muted-foreground" : "text-warning")}
            >
              {parts.length ? parts.join(" · ") : m.sidebar_unused()}
            </span>
          )}
        </span>
      </Link>
    );
  };

  return (
    <aside className="flex flex-[1_1_248px] flex-col gap-5 border-r border-border bg-card px-3 py-4 max-[808px]:border-r-0 max-[808px]:border-b min-[809px]:sticky min-[809px]:top-0 min-[809px]:h-screen min-[809px]:self-start min-[809px]:overflow-y-auto">
      <Link to="/" className="flex items-center gap-2.5 px-2 py-1 text-foreground no-underline">
        <span className="flex size-[30px] items-center justify-center rounded-lg bg-primary text-background">
          <KeyRound aria-hidden className="size-4" strokeWidth={2.2} />
        </span>
        <span className="flex flex-col">
          <span className="text-[15px] font-semibold tracking-tight">Keybund</span>
          <span className="text-xs text-muted-foreground">{m.app_tagline()}</span>
        </span>
      </Link>

      <Link to="/new" className={cn(buttonVariants(), "no-underline")}>
        <Plus aria-hidden />
        {m.sidebar_newKey()}
      </Link>

      <div className="relative">
        <label htmlFor="profile-search" className="sr-only">
          {m.sidebar_search()}
        </label>
        <Search
          aria-hidden
          className="pointer-events-none absolute top-[11px] left-[11px] size-[15px] text-muted-foreground"
        />
        <Input
          id="profile-search"
          placeholder={m.sidebar_searchPlaceholder()}
          className="pl-[34px]"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && q) navigate({ to: "/which", search: { q: filter.trim() } });
            if (e.key === "Escape") setFilter("");
          }}
        />
      </div>

      <nav aria-label={m.sidebar_keys()} className="flex flex-col gap-0.5">
        <span className="px-2 pb-1.5 text-xs font-medium text-muted-foreground">{m.sidebar_yourKeys()}</span>
        {keys.map(keyLink)}
        {nothing && (
          <span className="px-2.5 text-xs text-muted-foreground">
            {filter ? m.sidebar_noMatch() : m.sidebar_noKeys()}
          </span>
        )}
        {q && (
          <Link to="/which" search={{ q: filter.trim() }} className={cn(navItem, "text-muted-foreground")}>
            <Crosshair aria-hidden />
            <span className="min-w-0 flex-1 truncate">{m.sidebar_whichFor({ query: filter.trim() })}</span>
          </Link>
        )}
      </nav>

      {archived.length > 0 && (
        <nav aria-label={m.sidebar_archive()} className="flex flex-col gap-0.5">
          <span className="px-2 pb-1.5 text-xs font-medium text-muted-foreground">{m.sidebar_archive()}</span>
          {archived.map(({ profile }) => (
            <Link
              key={profile.id}
              to="/profile/$id"
              params={{ id: profile.id }}
              className={cn(navItem, "text-muted-foreground")}
            >
              <Archive aria-hidden />
              <span className="flex-1 truncate">{profile.name}</span>
            </Link>
          ))}
        </nav>
      )}

      <nav aria-label={m.sidebar_tools()} className="mt-auto flex flex-col gap-0.5 border-t border-border pt-3.5">
        <Link to="/clone" className={navItem} title={m.sidebar_cloneHint()}>
          <Download aria-hidden />
          {m.sidebar_clone()}
        </Link>
        <Link to="/doctor" className={navItem} title={m.sidebar_doctorHint()}>
          <Activity aria-hidden />
          <span className="flex-1">Doctor</span>
          {problems > 0 && (
            <span className="flex h-5 min-w-5 items-center justify-center rounded-full bg-amber-500/15 px-1.5 text-[11.5px] font-semibold text-warning">
              <span aria-hidden>{problems}</span>
              <span className="sr-only">{m.sidebar_hints({ n: problems })}</span>
            </span>
          )}
        </Link>
        <Link to="/settings" className={navItem}>
          <SlidersHorizontal aria-hidden />
          {m.sidebar_settings()}
        </Link>
      </nav>
    </aside>
  );
}
