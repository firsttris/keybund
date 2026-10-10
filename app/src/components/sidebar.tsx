import { Link, useRouterState } from "@tanstack/react-router";
import { Activity, Archive, Crosshair, Download, KeyRound, Plus, Search, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import { useDoctor, useProfiles } from "@/lib/queries";
import { cn, hostSummary } from "@/lib/utils";
import { ColorDot } from "./color-dot";
import { buttonVariants } from "./ui/button";
import { Input } from "./ui/input";

const navItem =
  "flex min-h-10 items-center gap-2.5 rounded-lg px-2.5 text-[13px] text-secondary-foreground no-underline hover:bg-muted data-[status=active]:bg-muted data-[status=active]:font-medium data-[status=active]:text-foreground [&_svg]:size-[15px]";

export function Sidebar() {
  const { data } = useProfiles();
  const doctor = useDoctor();
  const [filter, setFilter] = useState("");
  const path = useRouterState({ select: (s) => s.location.pathname });
  const problems = doctor.data?.checks.filter((c) => c.status !== "ok").length ?? 0;
  const matches = (name: string) => name.toLowerCase().includes(filter.trim().toLowerCase());
  const active = data?.profiles.filter((p) => !p.profile.archived && matches(p.profile.name)) ?? [];
  const archived = data?.profiles.filter((p) => p.profile.archived && matches(p.profile.name)) ?? [];

  return (
    <aside className="flex flex-[1_1_248px] flex-col gap-5 border-r border-border bg-card px-3 py-4 max-[808px]:border-r-0 max-[808px]:border-b">
      <Link to="/" className="flex items-center gap-2.5 px-2 py-1 text-foreground no-underline">
        <span className="flex size-[30px] items-center justify-center rounded-lg bg-primary text-background">
          <KeyRound aria-hidden className="size-4" strokeWidth={2.2} />
        </span>
        <span className="flex flex-col">
          <span className="text-[15px] font-semibold tracking-tight">Keybund</span>
          <span className="text-xs text-muted-foreground">SSH-Keys pro Projekt</span>
        </span>
      </Link>

      <Link to="/new" className={cn(buttonVariants(), "no-underline")}>
        <Plus aria-hidden />
        Neues Projekt
      </Link>

      <div className="relative">
        <label htmlFor="profile-search" className="sr-only">
          Profile durchsuchen
        </label>
        <Search aria-hidden className="pointer-events-none absolute top-[11px] left-[11px] size-[15px] text-muted-foreground" />
        <Input
          id="profile-search"
          placeholder="Profil suchen …"
          className="pl-[34px]"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>

      <nav aria-label="Profile" className="flex flex-col gap-0.5">
        <span className="px-2 pb-1.5 text-xs font-medium text-muted-foreground">Profile</span>
        {active.map(({ profile, last_test }) => {
          const current = path === `/profile/${profile.id}`;
          return (
            <Link
              key={profile.id}
              to="/profile/$id"
              params={{ id: profile.id }}
              aria-current={current ? "page" : undefined}
              className={navItem}
            >
              <ColorDot color={profile.color} />
              <span className="flex-1 truncate">{profile.name}</span>
              {last_test && !last_test.success ? (
                <span className="text-xs text-danger" title={last_test.message}>
                  Test fehlgeschlagen
                </span>
              ) : (
                <span className="text-xs font-normal text-muted-foreground">{hostSummary(profile)}</span>
              )}
            </Link>
          );
        })}
        {data && active.length === 0 && (
          <span className="px-2.5 text-xs text-muted-foreground">{filter ? "Kein Treffer" : "Noch keine Profile"}</span>
        )}
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
        <Link to="/which" className={navItem}>
          <Crosshair aria-hidden />
          Welcher Key?
        </Link>
        <Link to="/clone" className={navItem}>
          <Download aria-hidden />
          Repo klonen
        </Link>
        <Link to="/doctor" className={navItem}>
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
