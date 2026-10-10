import { Link } from "@tanstack/react-router";
import { Card, CardTitle } from "@/components/ui/card";
import { FieldHint, Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { usePaths, useProfiles, useScanKeys, useSetDefault } from "@/lib/queries";
import { formatAge } from "@/lib/utils";

export function SettingsPage() {
  const profiles = useProfiles();
  const setDefault = useSetDefault();
  const paths = usePaths();
  const keys = useScanKeys();
  const active = profiles.data?.profiles.filter((p) => !p.profile.archived) ?? [];
  const names = Object.fromEntries((profiles.data?.profiles ?? []).map((p) => [p.profile.id, p.profile.name]));

  return (
    <>
      <h1 className="m-0 text-[22px] font-semibold tracking-tight">Einstellungen</h1>
      <div className="grid max-w-[960px] grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4">
        <Card aria-labelledby="default-title">
          <CardTitle id="default-title">Default-Profil</CardTitle>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="default-profile">Key außerhalb aller Projektordner</Label>
            <Select
              id="default-profile"
              value={profiles.data?.default_profile ?? ""}
              onChange={(e) => setDefault.mutate(e.target.value || null)}
            >
              <option value="">Keins – ssh wählt selbst</option>
              {active.map((p) => (
                <option key={p.profile.id} value={p.profile.id}>
                  {p.profile.name}
                </option>
              ))}
            </Select>
            <FieldHint>Wird nach dem Anwenden als core.sshCommand in index.gitconfig gesetzt.</FieldHint>
          </div>
        </Card>

        <Card aria-labelledby="paths-title">
          <CardTitle id="paths-title">Pfade</CardTitle>
          {paths.data && (
            <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-[13px]">
              {[
                ["Profile", paths.data.profiles_file],
                ["SSH", paths.data.ssh_config],
                ["SSH (Keybund)", paths.data.ssh_keybund_conf],
                ["Git", paths.data.gitconfig],
                ["Git (Keybund)", paths.data.index_gitconfig],
              ].map(([k, v]) => (
                <div key={k} className="contents">
                  <dt className="text-muted-foreground">{k}</dt>
                  <dd className="m-0 font-mono text-[12.5px] [overflow-wrap:anywhere]">{v}</dd>
                </div>
              ))}
            </dl>
          )}
        </Card>

        <Card className="col-span-full" aria-labelledby="import-title">
          <CardTitle id="import-title">Vorhandene Keys</CardTitle>
          <FieldHint>
            Keys in ~/.ssh. Ein ungenutzter Key kann direkt zu einem neuen Projekt werden – oder du{" "}
            <Link to="/import" className="text-foreground">
              übernimmst deine bestehende ~/.ssh/config
            </Link>
            .
          </FieldHint>
          {keys.isPending && <Spinner />}
          <ul className="m-0 flex list-none flex-col p-0">
            {keys.data?.map((k) => (
              <li key={k.key.path} className="flex flex-wrap items-center gap-3 border-b border-border-subtle py-3 last:border-b-0">
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="font-mono text-[13px]">{k.key.path}</span>
                  <span className="truncate font-mono text-xs text-muted-foreground">
                    {k.key.fingerprint} {k.key.comment ? `· ${k.key.comment}` : ""} {k.key.age_days != null ? `· ${formatAge(k.key.age_days)}` : ""}
                  </span>
                </div>
                {k.used_by ? (
                  <Link to="/profile/$id" params={{ id: k.used_by }} className="text-xs text-muted-foreground">
                    genutzt von {names[k.used_by] ?? k.used_by}
                  </Link>
                ) : (
                  <Link to="/new" search={{ key: k.key.path }} className="text-[13px] font-medium text-foreground">
                    Als Projekt anlegen
                  </Link>
                )}
              </li>
            ))}
          </ul>
        </Card>
      </div>
    </>
  );
}
