import { Link, useRouterState } from "@tanstack/react-router";
import { Activity, Archive, Crosshair, Download, KeyRound, Plus, Search, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import type { ProfileView } from "@/lib/bindings";
import { activeKeys, usageParts } from "@/lib/categories";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { ColorDot } from "./color-dot";
import { buttonVariants } from "./ui/button";
import { Input } from "./ui/input";

const navItem =
  "flex min-h-10 items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-secondary-foreground no-underline hover:bg-muted data-[status=active]:bg-muted data-[status=active]:font-medium data-[status=active]:text-foreground [&_svg]:size-[15px]";

export function Sidebar() {
  const { data } = useProfiles();
  const doctor = useDoctor();
  const [filter, setFilter] = useState("");
  const path = useRouterState({ select: (s) => s.location.pathname });
  const problems = doctor.data?.checks.filter((c) => c.status !== "ok").length ?? 0;
  const q = filter.trim().toLowerCase();
  const matches = (...texts: (string | null | undefined)[]) => !q || texts.some((t) => t?.toLowerCase().includes(q));
  const keys = activeKeys(data?.profiles ?? []).filter((v) =>
    matches(v.profile.name, v.profile.key.path, ...v.profile.folders, ...v.profile.hosts.flatMap((h) => [h.alias, h.hostname])),
  );
  const archived = (data?.profiles ?? []).filter((v) => v.profile.archived && matches(v.profile.name));
  const nothing = data && keys.length === 0;
  const current = (id: string) => (path === `/profile/${id}` ? "page" : undefined);

  const keyLink = (view: ProfileView) => {
    const { profile, last_test } = view;
    const parts = usageParts(view);
    return (
      <Link key={profile.id} to="/profile/$id" params={{ id: profile.id }} aria-current={current(profile.id)} className={navItem}>
        <ColorDot color={profile.color} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate">{profile.name}</span>
          {last_test && !last_test.success ? (
            <span className="truncate text-xs font-normal text-danger" title={last_test.message}>
              Test fehlgeschlagen
            </span>
          ) : (
            <span className={cn("truncate text-xs font-normal", parts.length ? "text-muted-foreground" : "text-warning")}>
              {parts.length ? parts.join(" · ") : "noch nicht benutzt"}
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
          <span className="text-xs text-muted-foreground">Der richtige Key, automatisch</span>
        </span>
      </Link>

      <Link to="/new" className={cn(buttonVariants(), "no-underline")}>
        <Plus aria-hidden />
        Neuer Key
      </Link>

      <div className="relative">
        <label htmlFor="profile-search" className="sr-only">
          Keys durchsuchen
        </label>
        <Search aria-hidden className="pointer-events-none absolute top-[11px] left-[11px] size-[15px] text-muted-foreground" />
        <Input
          id="profile-search"
          placeholder="Key, Ordner oder Server …"
          className="pl-[34px]"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>

      <nav aria-label="Keys" className="flex flex-col gap-0.5">
        <span className="px-2 pb-1.5 text-xs font-medium text-muted-foreground">Deine Keys</span>
        {keys.map(keyLink)}
        {nothing && <span className="px-2.5 text-xs text-muted-foreground">{filter ? "Kein Treffer" : "Noch keine Keys"}</span>}
      </nav>

      {archived.length > 0 && (
        <nav aria-label="Archiv" className="flex flex-col gap-0.5">
          <span className="px-2 pb-1.5 text-xs font-medium text-muted-foreground">Archiv</span>
          {archived.map(({ profile }) => (
            <Link key={profile.id} to="/profile/$id" params={{ id: profile.id }} className={cn(navItem, "text-muted-foreground")}>
              <Archive aria-hidden />
              <span className="flex-1 truncate">{profile.name}</span>
            </Link>
          ))}
        </nav>
      )}

      <nav aria-label="Werkzeuge" className="mt-auto flex flex-col gap-0.5 border-t border-border pt-3.5">
        <Link to="/which" className={navItem} title="Zeigt für einen Ordner oder Server, welcher Key dort benutzt wird – und warum">
          <Crosshair aria-hidden />
          Welcher Key?
        </Link>
        <Link to="/clone" className={navItem} title="Klont ein Repo direkt in den Ordner des passenden Keys">
          <Download aria-hidden />
          Repo klonen
        </Link>
        <Link to="/doctor" className={navItem} title="Prüft, ob ssh und Git richtig eingerichtet sind">
          <Activity aria-hidden />
          <span className="flex-1">Doctor</span>
          {problems > 0 && (
            <span
              aria-label={`${problems} Hinweise`}
              className="flex h-5 min-w-5 items-center justify-center rounded-full bg-amber-500/15 px-1.5 text-[11.5px] font-semibold text-warning"
            >
              {problems}
            </span>
          )}
        </Link>
        <Link to="/settings" className={navItem}>
          <SlidersHorizontal aria-hidden />
          Einstellungen
        </Link>
      </nav>
    </aside>
  );
}
