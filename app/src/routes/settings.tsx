import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { getVersion } from "@tauri-apps/api/app";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ErrorSlot, FieldHint, Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { errorMessage } from "@/lib/api";
import type { Settings } from "@/lib/bindings";
import { openExternal } from "@/lib/open";
import { usePaths, useProfiles, useScanKeys, useSetDefault, useSettings, useUpdateSettings } from "@/lib/queries";
import { formatAge } from "@/lib/utils";

const REPO_URL = "https://github.com/firsttris/keybund";

export function SettingsPage() {
  const profiles = useProfiles();
  const setDefault = useSetDefault();
  const paths = usePaths();
  const keys = useScanKeys();
  const version = useQuery({ queryKey: ["version"], queryFn: () => getVersion() });
  const active = profiles.data?.profiles.filter((p) => !p.profile.archived) ?? [];
  const names = Object.fromEntries((profiles.data?.profiles ?? []).map((p) => [p.profile.id, p.profile.name]));

  return (
    <>
      <h1 className="m-0 text-[22px] font-semibold tracking-tight">Einstellungen</h1>
      <div className="grid max-w-[960px] grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4">
        <Card aria-labelledby="default-title">
          <CardTitle id="default-title">Standard-Key für Git</CardTitle>
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
            <FieldHint>
              Für alle Git-Repos, die in keinem Projektordner liegen, z. B. ein schnell geklontes Repo in ~/Downloads. Ohne
              Standard-Key probiert ssh einfach deine Keys durch.
            </FieldHint>
          </div>
        </Card>

        <SettingsCard />

        <Card aria-labelledby="paths-title">
          <CardTitle id="paths-title">Pfade</CardTitle>
          <p className="m-0 -mt-2 text-[13px] text-muted-foreground">
            Diese Dateien liest und schreibt Keybund. Deine eigenen (SSH, Git) bekommen nur je eine Include-Zeile.
          </p>
          {paths.data && (
            <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-[13px]">
              {[
                ["Keys (Keybund)", paths.data.profiles_file],
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

        <Card aria-labelledby="about-title">
          <CardTitle id="about-title">Über Keybund</CardTitle>
          <p className="m-0 text-[13px] leading-relaxed text-muted-foreground">
            Keybund {version.data ? `${version.data} ` : ""}by Tristan Teufel ·{" "}
            <button className="text-foreground underline-offset-4 hover:underline" onClick={() => openExternal(REPO_URL)}>
              Quellcode auf GitHub
            </button>
            <br />
            Lizenz: GPL-3.0 · eine kommerzielle Lizenz gibt es über{" "}
            <button className="text-foreground underline-offset-4 hover:underline" onClick={() => openExternal("https://teufel-it.de")}>
              teufel-it.de
            </button>
          </p>
        </Card>

        <Card aria-labelledby="remove-title">
          <CardTitle id="remove-title">Keybund entfernen</CardTitle>
          <FieldHint>Nimmt alle Änderungen von Keybund zurück, mit Vorschau. Deine Keys bleiben.</FieldHint>
          <Link to="/uninstall" className="self-start text-[13px] font-medium text-foreground">
            Entfernen …
          </Link>
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
                    Als Key übernehmen
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

function SettingsCard() {
  const settings = useSettings();
  if (!settings.data) return <Spinner />;
  // Remounts with fresh values after saving.
  return <SettingsForm key={JSON.stringify(settings.data)} saved={settings.data} />;
}

function SettingsForm({ saved }: { saved: Settings }) {
  const update = useUpdateSettings();
  const [draft, setDraft] = useState(saved);
  const set = (patch: Partial<Settings>) => setDraft((d) => ({ ...d, ...patch }));
  const changed = JSON.stringify(draft) !== JSON.stringify(saved);
  const mono = "font-mono text-[13px]";
  return (
    <Card aria-labelledby="settings-title">
      <CardTitle id="settings-title">Neue Profile & Configs</CardTitle>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          update.mutate(draft, { onSuccess: () => toast.success("Einstellungen gespeichert") });
        }}
      >
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="projects-dir">Ordner für Projekte</Label>
          <Input id="projects-dir" className={mono} value={draft.projects_dir} onChange={(e) => set({ projects_dir: e.target.value })} />
          <FieldHint>Neue Projekte schlägt der Wizard hier vor, z. B. {draft.projects_dir.replace(/\/+$/, "")}/kunde-c/.</FieldHint>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="keys-dir">Ordner für Keys</Label>
          <Input id="keys-dir" className={mono} value={draft.keys_dir} onChange={(e) => set({ keys_dir: e.target.value })} />
        </div>
        <label className="flex items-start justify-between gap-4">
          <span className="flex flex-col gap-0.5">
            <span className="text-[13px] font-medium">Server-Fingerprints pro Key (known_hosts)</span>
            <span className="text-xs text-muted-foreground">
              ssh merkt sich, woran es deine Server erkennt. Mit dieser Option liegt das pro Key in einer eigenen Datei und
              verschwindet beim Archivieren mit.
            </span>
          </span>
          <Switch
            aria-label="Server-Fingerprints pro Key"
            checked={draft.known_hosts_per_profile}
            onCheckedChange={(v) => set({ known_hosts_per_profile: v })}
          />
        </label>
        <label className="flex items-start justify-between gap-4">
          <span className="flex flex-col gap-0.5">
            <span className="text-[13px] font-medium">Include mit vollem Pfad</span>
            <span className="text-xs text-muted-foreground">Statt ~ in ~/.ssh/config – nötig für Dolphin/KIO mit libssh vor 0.10.2 (der Doctor meldet das).</span>
          </span>
          <Switch
            aria-label="Include mit vollem Pfad"
            checked={draft.absolute_ssh_include}
            onCheckedChange={(v) => set({ absolute_ssh_include: v })}
          />
        </label>
        <ErrorSlot message={update.isError && errorMessage(update.error)} />
        <Button type="submit" className="self-end" disabled={!changed || update.isPending}>
          Einstellungen speichern
        </Button>
      </form>
    </Card>
  );
}
