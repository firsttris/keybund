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
import { LANGS, type Lang, rich, setLang, storedLang, systemLang } from "@/lib/i18n";
import { openExternal } from "@/lib/open";
import {
  toastSaved,
  usePaths,
  useProfiles,
  useScanKeys,
  useSetDefault,
  useSettings,
  useUpdateSettings,
} from "@/lib/queries";
import { setTheme, storedTheme, systemTheme, type Theme } from "@/lib/theme";
import { formatAge } from "@/lib/utils";
import { m } from "@/paraglide/messages";

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
      <h1 className="m-0 text-[22px] font-semibold tracking-tight">{m.sidebar_settings()}</h1>
      <div className="grid max-w-[960px] grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4">
        <Card aria-labelledby="default-title">
          <CardTitle id="default-title">{m.settings_defaultTitle()}</CardTitle>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="default-profile">{m.settings_defaultLabel()}</Label>
            <Select
              id="default-profile"
              value={profiles.data?.default_profile ?? ""}
              onChange={(e) => setDefault.mutate(e.target.value || null)}
            >
              <option value="">{m.settings_defaultNone()}</option>
              {active.map((p) => (
                <option key={p.profile.id} value={p.profile.id}>
                  {p.profile.name}
                </option>
              ))}
            </Select>
            <FieldHint>{m.settings_defaultHint()}</FieldHint>
          </div>
        </Card>

        <SettingsCard />

        <LanguageCard />
        <ThemeCard />

        <Card className="col-span-full" aria-labelledby="import-title">
          <CardTitle id="import-title">{m.settings_keysTitle()}</CardTitle>
          <FieldHint>
            {rich(m.settings_keysHint, {
              link: (
                <Link to="/import" className="text-foreground">
                  {m.settings_keysHintLink()}
                </Link>
              ),
            })}
          </FieldHint>
          {keys.isPending && <Spinner />}
          <ul className="m-0 flex list-none flex-col p-0">
            {keys.data?.map((k) => (
              <li
                key={k.key.path}
                className="flex flex-wrap items-center gap-3 border-b border-border-subtle py-3 last:border-b-0"
              >
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="font-mono text-[13px]">{k.key.path}</span>
                  <span className="truncate font-mono text-xs text-muted-foreground">
                    {k.key.fingerprint} {k.key.comment ? `· ${k.key.comment}` : ""}{" "}
                    {k.key.age_days != null ? `· ${formatAge(k.key.age_days)}` : ""}
                  </span>
                </div>
                {k.used_by ? (
                  <Link to="/profile/$id" params={{ id: k.used_by }} className="text-xs text-muted-foreground">
                    {m.settings_usedBy({ name: names[k.used_by] ?? k.used_by })}
                  </Link>
                ) : (
                  <Link to="/new" search={{ key: k.key.path }} className="text-[13px] font-medium text-foreground">
                    {m.settings_takeOver()}
                  </Link>
                )}
              </li>
            ))}
          </ul>
        </Card>

        <details className="group col-span-full">
          <summary className="cursor-pointer py-1 text-[13px] font-medium text-muted-foreground select-none hover:text-foreground">
            {m.settings_advanced()}
          </summary>
          <div className="mt-3 grid grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4">
            <AdvancedCard />

            <Card aria-labelledby="paths-title">
              <CardTitle id="paths-title">{m.settings_paths()}</CardTitle>
              <p className="m-0 -mt-2 text-[13px] text-muted-foreground">{m.settings_pathsHint()}</p>
              {paths.data && (
                <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-[13px]">
                  {[
                    [m.settings_pathKeys(), paths.data.profiles_file],
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

            <Card aria-labelledby="remove-title">
              <CardTitle id="remove-title">{m.uninstall_title()}</CardTitle>
              <FieldHint>{m.settings_removeHint()}</FieldHint>
              <Link to="/uninstall" className="self-start text-[13px] font-medium text-foreground">
                {m.settings_remove()}
              </Link>
            </Card>
          </div>
        </details>

        <Card aria-labelledby="about-title">
          <CardTitle id="about-title">{m.settings_about()}</CardTitle>
          <p className="m-0 text-[13px] leading-relaxed text-muted-foreground">
            Keybund {version.data ? `${version.data} ` : ""}by Tristan Teufel ·{" "}
            <button
              type="button"
              className="text-foreground underline-offset-4 hover:underline"
              onClick={() => openExternal(REPO_URL)}
            >
              {m.settings_source()}
            </button>
            <br />
            {m.settings_license()}{" "}
            <button
              type="button"
              className="text-foreground underline-offset-4 hover:underline"
              onClick={() => openExternal("https://teufel-it.de")}
            >
              teufel-it.de
            </button>
          </p>
        </Card>
      </div>
    </>
  );
}

/** A card that shows why its settings could not be loaded instead of spinning forever. */
function LoadFailed({ id, title, error }: { id: string; title: string; error: unknown }) {
  return (
    <Card aria-labelledby={id}>
      <CardTitle id={id}>{title}</CardTitle>
      <p role="alert" className="m-0 text-[13px] text-danger">
        {errorMessage(error)}
      </p>
    </Card>
  );
}

function SettingsCard() {
  const settings = useSettings();
  if (settings.error) return <LoadFailed id="settings-title" title={m.settings_newKeys()} error={settings.error} />;
  if (!settings.data) return <Spinner />;
  // Remounts with fresh values after saving.
  // Remounts only when the folders change (not when a switch under "Erweitert" is flipped).
  return <FoldersForm key={`${settings.data.projects_dir}|${settings.data.keys_dir}`} saved={settings.data} />;
}

const mono = "font-mono text-[13px]";

/** Where new keys and their git folders go. */
function FoldersForm({ saved }: { saved: Settings }) {
  const update = useUpdateSettings();
  const [draft, setDraft] = useState(saved);
  const set = (patch: Partial<Settings>) => setDraft((d) => ({ ...d, ...patch }));
  const changed = draft.projects_dir !== saved.projects_dir || draft.keys_dir !== saved.keys_dir;
  return (
    <Card aria-labelledby="settings-title">
      <CardTitle id="settings-title">{m.settings_newKeys()}</CardTitle>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          update.mutate(
            { projects_dir: draft.projects_dir, keys_dir: draft.keys_dir },
            { onSuccess: () => toast.success(m.settings_saved()) },
          );
        }}
      >
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="projects-dir">{m.settings_projectsDir()}</Label>
          <Input
            id="projects-dir"
            className={mono}
            value={draft.projects_dir}
            onChange={(e) => set({ projects_dir: e.target.value })}
          />
          <FieldHint>
            {m.settings_projectsDirHint({
              example: `${draft.projects_dir.replace(/\/+$/, "")}/${m.settings_exampleFolder()}/`,
            })}
          </FieldHint>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="keys-dir">{m.settings_keysDir()}</Label>
          <Input
            id="keys-dir"
            className={mono}
            value={draft.keys_dir}
            onChange={(e) => set({ keys_dir: e.target.value })}
          />
          <FieldHint>{m.settings_keysDirHint()}</FieldHint>
        </div>
        <ErrorSlot message={update.isError && errorMessage(update.error)} />
        <Button type="submit" className="self-end" disabled={!changed || update.isPending}>
          {m.common_save()}
        </Button>
      </form>
    </Card>
  );
}

/** Switches for special setups; each one is saved and applied right away. */
function AdvancedCard() {
  const settings = useSettings();
  const update = useUpdateSettings();
  // Dolphin and its libssh only exist on Linux.
  const linux = usePaths().data?.platform !== "macos";
  const s = settings.data;
  if (settings.error) return <LoadFailed id="advanced-title" title={m.settings_special()} error={settings.error} />;
  if (!s) return <Spinner />;
  const toggle = (patch: Partial<Settings>) =>
    update.mutate(patch, { onSuccess: toastSaved, onError: (e) => toast.error(errorMessage(e)) });
  return (
    <Card aria-labelledby="advanced-title">
      <CardTitle id="advanced-title">{m.settings_special()}</CardTitle>
      <label className="flex items-start justify-between gap-4">
        <span className="flex flex-col gap-0.5">
          <span className="text-[13px] font-medium">{m.settings_knownHosts()}</span>
          <span className="text-xs text-muted-foreground">{m.settings_knownHostsHint()}</span>
        </span>
        <Switch
          aria-label={m.settings_knownHostsLabel()}
          checked={s.known_hosts_per_profile}
          disabled={update.isPending}
          onCheckedChange={(v) => toggle({ known_hosts_per_profile: v })}
        />
      </label>
      {linux && (
        <label className="flex items-start justify-between gap-4">
          <span className="flex flex-col gap-0.5">
            <span className="text-[13px] font-medium">{m.settings_absoluteInclude()}</span>
            <span className="text-xs text-muted-foreground">{m.settings_absoluteIncludeHint()}</span>
          </span>
          <Switch
            aria-label={m.settings_absoluteInclude()}
            checked={s.absolute_ssh_include}
            disabled={update.isPending}
            onCheckedChange={(v) => toggle({ absolute_ssh_include: v })}
          />
        </label>
      )}
    </Card>
  );
}

/** The UI language: the system's, or a fixed one. Switches right away. */
function LanguageCard() {
  const [choice, setChoice] = useState<Lang | "">(() => storedLang() ?? "");
  const system = LANGS.find((l) => l.id === systemLang())!.label;
  return (
    <Card aria-labelledby="language-title">
      <CardTitle id="language-title">{m.settings_language()}</CardTitle>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="language">{m.settings_languageLabel()}</Label>
        <Select
          id="language"
          value={choice}
          onChange={(e) => {
            const lang = (e.target.value || null) as Lang | null;
            setChoice(lang ?? "");
            setLang(lang);
          }}
        >
          <option value="">{m.settings_languageSystem({ language: system })}</option>
          {LANGS.map((l) => (
            <option key={l.id} value={l.id}>
              {l.label}
            </option>
          ))}
        </Select>
        <FieldHint>{m.settings_languageHint()}</FieldHint>
      </div>
    </Card>
  );
}

function ThemeCard() {
  const [choice, setChoice] = useState<Theme | "">(() => storedTheme() ?? "");
  const label = { light: m.settings_themeLight, dark: m.settings_themeDark };
  return (
    <Card aria-labelledby="theme-title">
      <CardTitle id="theme-title">{m.settings_theme()}</CardTitle>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="theme">{m.settings_themeLabel()}</Label>
        <Select
          id="theme"
          value={choice}
          onChange={(e) => {
            const theme = (e.target.value || null) as Theme | null;
            setChoice(theme ?? "");
            setTheme(theme);
          }}
        >
          <option value="">{m.settings_themeSystem({ theme: label[systemTheme()]() })}</option>
          <option value="light">{label.light()}</option>
          <option value="dark">{label.dark()}</option>
        </Select>
        <FieldHint>{m.settings_themeHint()}</FieldHint>
      </div>
    </Card>
  );
}
