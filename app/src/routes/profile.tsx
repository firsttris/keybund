import { Link, useNavigate } from "@tanstack/react-router";
import { Archive, Check, Copy, Folder, FolderOpen, GitBranch, KeyRound, Server, Star, Trash2, X, Zap } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { ColorDot } from "@/components/color-dot";
import { KeyUpload } from "@/components/key-upload";
import { ConfigStatus } from "@/components/pending-banner";
import { ProbeDetails } from "@/components/probe-details";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ErrorSlot, FieldError, FieldHint, Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, errorMessage } from "@/lib/api";
import type { HostEntry, Profile, ProfileView, Provider } from "@/lib/bindings";
import { gitAccounts, servers } from "@/lib/categories";
import {
  useArchiveProfile,
  useCopyPublicKey,
  useGeneratedConfig,
  useInstallKey,
  useProfile,
  useProfiles,
  useSetDefault,
  useSettings,
  useTestConnection,
  useUpdateProfile,
} from "@/lib/queries";
import { pickFolder } from "@/lib/tauri";
import { defaultHostname, formatAge, formatDate, GIT_PROVIDERS, isValidEmail, providerLabel } from "@/lib/utils";

const mono = "font-mono text-[13px]";

export function ProfilePage({ id }: { id: string }) {
  const { data: view, isPending, error } = useProfile(id);
  if (isPending) return <Spinner />;
  if (error) return <p role="alert">{errorMessage(error)}</p>;
  if (!view) {
    return (
      <div className="flex flex-col gap-3">
        <h1 className="m-0 text-xl font-semibold">Key nicht gefunden</h1>
        <Link to="/">Zur Übersicht</Link>
      </div>
    );
  }
  return <KeyDetail key={view.profile.id} view={view} />;
}

function KeyDetail({ view }: { view: ProfileView }) {
  const { profile, key } = view;
  const copy = useCopyPublicKey();
  const [copied, setCopied] = useState(false);
  const usesGit = profile.folders.length > 0 || view.is_default || gitAccounts(profile).length > 0 || !!profile.git;

  const onCopy = () =>
    copy.mutate(profile.id, {
      onSuccess: () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1600);
      },
    });

  return (
    <>
      <div className="flex items-center gap-2 text-[13px] text-muted-foreground">
        <Link to="/" className="text-muted-foreground no-underline hover:text-foreground">
          Keys
        </Link>
        <span aria-hidden>/</span>
        <span className="text-foreground">{profile.name}</span>
      </div>

      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-3">
            <ColorDot color={profile.color} className="size-3" />
            <h1 className="m-0 text-[26px] font-semibold tracking-tight">{profile.name}</h1>
            {view.is_default && <Badge variant="outline">Standard</Badge>}
            {profile.archived && <Badge variant="muted">Archiviert</Badge>}
          </div>
          <p className="m-0 text-muted-foreground">
            <span className="font-mono text-[13px] text-secondary-foreground">{key.path}</span>
            {key.created && ` · erstellt ${formatAge(key.age_days)}`}
          </p>
        </div>
        {!profile.archived && (
          <Button onClick={onCopy} className="min-w-[156px]" disabled={!key.public_key}>
            {copied ? <Check aria-hidden strokeWidth={2.4} /> : <Copy aria-hidden />}
            {copied ? "Kopiert" : "Pubkey kopieren"}
          </Button>
        )}
      </header>

      <Tabs defaultValue="overview" className="flex flex-col gap-6">
        <TabsList aria-label="Ansicht">
          <TabsTrigger value="overview">Übersicht</TabsTrigger>
          <TabsTrigger value="config">Generierte Config</TabsTrigger>
        </TabsList>
        <TabsContent value="overview">
          <div className="grid grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4 max-[640px]:grid-cols-1">
            <UsageCard view={view} />
            <KeyCard view={view} onCopy={onCopy} />
            {usesGit && <GitCard profile={profile} />}
            {!profile.archived && <DangerCard view={view} />}
          </div>
        </TabsContent>
        <TabsContent value="config">
          <GeneratedConfig id={profile.id} />
        </TabsContent>
      </Tabs>

      <ConfigStatus />
    </>
  );
}

type Adding = "folder" | "git" | "server" | null;

/** The heart of the page: everything this key is used for, in one list. */
function UsageCard({ view }: { view: ProfileView }) {
  const { profile } = view;
  const update = useUpdateProfile();
  const setDefault = useSetDefault();
  const [adding, setAdding] = useState<Adding>(null);
  const accounts = gitAccounts(profile);
  const serverList = servers(profile);
  const empty = profile.folders.length === 0 && profile.hosts.length === 0 && !view.is_default;
  const removeHost = (h: HostEntry) => update.mutate({ ...profile, hosts: profile.hosts.filter((x) => x !== h) });
  const addHost = (host: HostEntry) =>
    update.mutate({ ...profile, hosts: [...profile.hosts, host] }, { onSuccess: () => setAdding(null) });

  return (
    <Card className="col-span-full" aria-labelledby="usage-title">
      <CardTitle id="usage-title">Wo wird dieser Key benutzt?</CardTitle>
      <CardDescription>
        Keybund sagt Git und ssh, wann sie diesen Key nehmen sollen – danach passiert alles automatisch, auch ohne dass Keybund
        läuft.
      </CardDescription>

      {empty && !adding && !profile.archived && <EmptyUsage onPick={setAdding} canDefault onDefault={() => setDefault.mutate(profile.id)} />}

      {!empty && (
        <ul aria-label="Verwendungen" className="m-0 flex list-none flex-col gap-2 p-0">
          {view.is_default && (
            <UsageRow
              icon={<Star aria-hidden className="size-4 text-amber-400" />}
              title="Standard für Git"
              detail="Git nimmt diesen Key in allen Ordnern, für die kein anderer Key gilt – z. B. für ein schnell geklontes Repo."
              actions={
                !profile.archived && (
                  <Button variant="ghost" size="sm" onClick={() => setDefault.mutate(null)}>
                    Nicht mehr Standard
                  </Button>
                )
              }
            />
          )}
          {profile.folders.map((f) => (
            <UsageRow
              key={f}
              icon={<Folder aria-hidden className="size-4 text-muted-foreground" />}
              title={<span className={mono}>{f}</span>}
              detail="Git-Repos in diesem Ordner (mit allen Unterordnern) nutzen diesen Key und deine Git-Identität unten."
              actions={
                !profile.archived && (
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={`Ordner ${f} entfernen`}
                    onClick={() => update.mutate({ ...profile, folders: profile.folders.filter((x) => x !== f) })}
                  >
                    <Trash2 aria-hidden />
                  </Button>
                )
              }
            />
          ))}
          {accounts.map((h) => (
            <AccountRow key={`a-${h.alias ?? h.hostname}`} profile={profile} host={h} onRemove={() => removeHost(h)} />
          ))}
          {serverList.map((h) => (
            <ServerRow key={`s-${h.alias ?? h.hostname}`} profile={profile} host={h} onRemove={() => removeHost(h)} />
          ))}
        </ul>
      )}

      {adding === "folder" && <FolderForm profile={profile} onDone={() => setAdding(null)} />}
      {adding === "git" && <HostForm kind="git" onCancel={() => setAdding(null)} onSave={addHost} />}
      {adding === "server" && <HostForm kind="server" onCancel={() => setAdding(null)} onSave={addHost} />}

      {!empty && !adding && !profile.archived && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] text-muted-foreground">Hinzufügen:</span>
          <Button variant="outline" size="sm" onClick={() => setAdding("folder")}>
            <Folder aria-hidden />
            Git-Ordner
          </Button>
          <Button variant="outline" size="sm" onClick={() => setAdding("git")}>
            <GitBranch aria-hidden />
            Git-Konto
          </Button>
          <Button variant="outline" size="sm" onClick={() => setAdding("server")}>
            <Server aria-hidden />
            Server
          </Button>
          {!view.is_default && (
            <Button variant="ghost" size="sm" onClick={() => setDefault.mutate(profile.id)}>
              <Star aria-hidden />
              Als Standard
            </Button>
          )}
        </div>
      )}
      {update.isError && <FieldError>{errorMessage(update.error)}</FieldError>}
    </Card>
  );
}

function EmptyUsage({ onPick, canDefault, onDefault }: { onPick: (a: Adding) => void; canDefault: boolean; onDefault: () => void }) {
  const tiles: { kind: Adding; icon: React.ReactNode; title: string; text: string }[] = [
    {
      kind: "folder",
      icon: <Folder aria-hidden className="size-5" />,
      title: "Git-Ordner",
      text: "Alle Repos in einem Ordner, z. B. ~/dev/kunde-c/ – mit eigenem Namen und E-Mail in den Commits.",
    },
    {
      kind: "git",
      icon: <GitBranch aria-hidden className="size-5" />,
      title: "Git-Konto",
      text: "GitHub, GitLab oder Bitbucket – Keybund trägt den Key dort ein, damit du pushen kannst.",
    },
    {
      kind: "server",
      icon: <Server aria-hidden className="size-5" />,
      title: "Server",
      text: "ssh, scp und sftp zu einem Rechner oder einer IP – auch in Dolphin.",
    },
  ];
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3">
        {tiles.map((t) => (
          <button
            key={t.kind}
            type="button"
            onClick={() => onPick(t.kind)}
            className="flex gap-3 rounded-[10px] border border-border bg-transparent p-4 text-left hover:border-zinc-500"
          >
            <span className="mt-0.5 text-muted-foreground">{t.icon}</span>
            <span className="flex flex-col gap-1">
              <span className="font-medium">{t.title}</span>
              <span className="text-[12.5px] text-muted-foreground">{t.text}</span>
            </span>
          </button>
        ))}
      </div>
      {canDefault && (
        <button type="button" className="self-start text-[13px] text-muted-foreground underline-offset-2 hover:underline" onClick={onDefault}>
          Oder: als Standard für Git benutzen (überall, wo kein anderer Key gilt)
        </button>
      )}
    </div>
  );
}

function UsageRow({
  icon,
  title,
  detail,
  actions,
  children,
}: {
  icon: React.ReactNode;
  title: React.ReactNode;
  detail: React.ReactNode;
  actions?: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <li className="flex flex-col gap-3 rounded-lg bg-muted p-3">
      <div className="flex flex-wrap items-center gap-3">
        {icon}
        <div className="flex min-w-[200px] flex-1 flex-col gap-0.5">
          <span className="text-[13px] font-medium">{title}</span>
          <span className="text-xs leading-normal text-muted-foreground">{detail}</span>
        </div>
        {actions && <div className="flex flex-wrap items-center gap-1">{actions}</div>}
      </div>
      {children}
    </li>
  );
}

function TestResult({ test }: { test: ReturnType<typeof useTestConnection> }) {
  if (test.isPending) return <Spinner label="Teste Verbindung" />;
  if (test.isError) return <FieldError>{errorMessage(test.error)}</FieldError>;
  const result = test.data;
  if (!result) return null;
  return (
    <div aria-live="polite" className="flex flex-col gap-2">
      <span className={`flex items-center gap-2 text-[13px] ${result.success ? "text-success" : "text-danger"}`} data-testid="connection-status">
        {result.success ? <Check aria-hidden className="size-4" /> : <X aria-hidden className="size-4" />}
        {result.message}
      </span>
      {result.other_key && (
        <div role="alert" className="flex flex-col gap-1.5 rounded-[10px] border border-amber-500/35 bg-amber-500/5 p-3.5 text-[13px]">
          <span className="font-medium">Dieser Key wurde nicht benutzt</span>
          <span className="text-muted-foreground">
            ssh hat zusätzlich <span className="font-mono">{result.used_key}</span> angeboten (aus deiner ~/.ssh/config), und
            damit hat die Anmeldung geklappt. Trage diesen Key beim Anbieter ein und entferne den alten Eintrag.
          </span>
          <span className="flex gap-3">
            <Link to="/import" className="font-medium text-foreground">
              Bestehende Config übernehmen
            </Link>
            <Link to="/doctor" className="text-foreground">
              Zum Doctor
            </Link>
          </span>
        </div>
      )}
      <ProbeDetails result={result} />
    </div>
  );
}

function AccountRow({ profile, host, onRemove }: { profile: Profile; host: HostEntry; onRemove: () => void }) {
  const test = useTestConnection();
  const [uploading, setUploading] = useState(false);
  const name = host.alias ?? host.hostname;
  const provider = providerLabel(host.provider);
  return (
    <UsageRow
      icon={<GitBranch aria-hidden className="size-4 text-muted-foreground" />}
      title={
        <>
          {provider}-Konto <span className="font-mono font-normal text-secondary-foreground">{name}</span>
        </>
      }
      detail={
        host.alias
          ? `Repos mit git@${host.alias}:… nutzen diesen Key (${host.hostname}). Der Key muss in deinem ${provider}-Konto stehen.`
          : `Damit ${provider} dich erkennt, muss der Key in deinem Konto stehen.`
      }
      actions={
        !profile.archived && (
          <>
            <Button variant="ghost" size="sm" aria-expanded={uploading} onClick={() => setUploading(!uploading)}>
              {uploading ? "Schließen" : "Key eintragen"}
            </Button>
            <Button variant="ghost" size="sm" disabled={test.isPending} onClick={() => test.mutate({ id: profile.id, host: name })}>
              <Zap aria-hidden />
              Testen
            </Button>
            <Button variant="ghost" size="icon" aria-label={`Konto ${name} entfernen`} onClick={onRemove}>
              <Trash2 aria-hidden />
            </Button>
          </>
        )
      }
    >
      {uploading && <KeyUpload profile={profile} host={name} onUploaded={() => test.mutate({ id: profile.id, host: name })} />}
      <TestResult test={test} />
    </UsageRow>
  );
}

function ServerRow({ profile, host, onRemove }: { profile: Profile; host: HostEntry; onRemove: () => void }) {
  const install = useInstallKey();
  const test = useTestConnection();
  const name = host.alias ?? host.hostname;
  const result = install.data ?? null;
  const target = `${host.user ? `${host.user}@` : ""}${host.hostname}${host.port && host.port !== 22 ? `:${host.port}` : ""}`;
  return (
    <UsageRow
      icon={<Server aria-hidden className="size-4 text-muted-foreground" />}
      title={
        <>
          Server <span className="font-mono font-normal text-secondary-foreground">{name}</span>
        </>
      }
      detail={`ssh ${name} verbindet zu ${target} – mit diesem Key, auch scp, sftp und rsync.`}
      actions={
        !profile.archived && (
          <>
            <Button variant="ghost" size="sm" disabled={install.isPending} onClick={() => install.mutate({ id: profile.id, host: name })}>
              {install.isPending && <Spinner className="size-3" />}
              Key eintragen
            </Button>
            <Button variant="ghost" size="sm" disabled={test.isPending} onClick={() => test.mutate({ id: profile.id, host: name })}>
              <Zap aria-hidden />
              Testen
            </Button>
            <Button variant="ghost" size="icon" aria-label={`Server ${name} entfernen`} onClick={onRemove}>
              <Trash2 aria-hidden />
            </Button>
          </>
        )
      }
    >
      <div aria-live="polite" className="flex flex-col gap-1 text-xs empty:hidden">
        {install.isPending && <span className="text-muted-foreground">Anmeldung an {name} – ggf. Passwort eingeben …</span>}
        {result && <span className={result.success ? "text-success" : "text-danger"}>{result.message}</span>}
        {install.isError && <span className="text-danger">{errorMessage(install.error)}</span>}
      </div>
      <TestResult test={test} />
    </UsageRow>
  );
}

/** Adds a project folder; asks for the git identity and the hoster if missing. */
function FolderForm({ profile, onDone }: { profile: Profile; onDone: () => void }) {
  const update = useUpdateProfile();
  const settings = useSettings();
  const all = useProfiles();
  const needsIdentity = !profile.git;
  // Usually the same person: take the name from another key.
  const knownName = all.data?.profiles.find((p) => p.profile.git?.name)?.profile.git?.name ?? "";
  const needsAccount = gitAccounts(profile).length === 0;
  const [folder, setFolder] = useState("");
  const [name, setName] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [provider, setProvider] = useState<Provider | "">("github");
  const suggestion = `${settings.data?.projects_dir ?? "~/dev"}/${profile.id}/`;
  const gitName = name ?? knownName;
  const error =
    !folder.trim() ? null : needsIdentity && (!gitName.trim() || !isValidEmail(email)) ? "Bitte Name und eine gültige E-Mail eingeben." : null;
  const save = () => {
    const value = folder.trim() || suggestion;
    const next: Profile = { ...profile, folders: [...profile.folders, value] };
    if (needsIdentity) next.git = { name: gitName.trim(), email: email.trim(), sign: false };
    if (needsAccount && provider) {
      next.hosts = [...profile.hosts, { provider, hostname: defaultHostname(provider), alias: null, user: null, port: null }];
    }
    update.mutate(next, { onSuccess: onDone });
  };
  const browse = async () => {
    const picked = await pickFolder();
    if (picked) setFolder(await api.normalizeFolder(picked));
  };
  return (
    <form
      aria-label="Git-Ordner hinzufügen"
      className="flex flex-col gap-3 rounded-lg border border-border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!error && (!needsIdentity || (gitName.trim() && isValidEmail(email)))) save();
      }}
    >
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="new-folder">Ordner</Label>
        <div className="flex flex-wrap gap-2">
          <Input
            id="new-folder"
            className={`${mono} min-w-[220px] flex-1`}
            placeholder={suggestion}
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            autoFocus
          />
          <Button type="button" variant="outline" onClick={browse}>
            <FolderOpen aria-hidden />
            Durchsuchen
          </Button>
        </div>
        <FieldHint>Leer lassen für {suggestion} – wird angelegt, falls er fehlt.</FieldHint>
      </div>
      {needsIdentity && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="folder-git-name">Name in Commits</Label>
            <Input id="folder-git-name" value={gitName} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="folder-git-mail">E-Mail in Commits</Label>
            <Input id="folder-git-mail" type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
          </div>
        </div>
      )}
      {needsAccount && (
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="folder-provider">Wo liegt der Code?</Label>
          <Select id="folder-provider" value={provider} onChange={(e) => setProvider(e.target.value as Provider | "")}>
            {GIT_PROVIDERS.map((p) => (
              <option key={p} value={p}>
                {providerLabel(p)}
              </option>
            ))}
            <option value="">Später / woanders</option>
          </Select>
          <FieldHint>Dann kann Keybund den Key gleich dort eintragen. Selbst gehostet? Später unter „Git-Konto“.</FieldHint>
        </div>
      )}
      <ErrorSlot message={error} />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onDone}>
          Abbrechen
        </Button>
        <Button size="sm" type="submit" disabled={update.isPending || (needsIdentity && (!gitName.trim() || !isValidEmail(email)))}>
          Ordner speichern
        </Button>
      </div>
      {update.isError && <FieldError>{errorMessage(update.error)}</FieldError>}
    </form>
  );
}

function KeyCard({ view, onCopy }: { view: ProfileView; onCopy: () => void }) {
  const { key } = view;
  return (
    <Card className="col-span-full" aria-labelledby="key-title">
      <CardHeader>
        <div className="flex items-center gap-2.5">
          <KeyRound aria-hidden className="size-[18px] text-muted-foreground" />
          <CardTitle id="key-title">Der Key</CardTitle>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {!key.private_exists ? (
            <Badge variant="danger">Key-Datei fehlt</Badge>
          ) : key.age_days != null && key.age_days > 730 ? (
            <Badge variant="warning">Älter als 2 Jahre – rotieren</Badge>
          ) : null}
          {key.private_exists && view.in_agent === true && <Badge variant="success">Im Agent geladen</Badge>}
          {key.private_exists && view.in_agent === false && (
            <Badge variant="muted" title="Der ssh-agent merkt sich den Key nach dem ersten Gebrauch – dann fragt niemand mehr nach der Passphrase.">
              Nicht im Agent
            </Badge>
          )}
        </div>
      </CardHeader>
      <p className="m-0 text-[13px] leading-normal text-muted-foreground">
        Ein SSH-Key hat zwei Teile: den <span className="text-foreground">öffentlichen Teil</span> („Pubkey“, unten) gibst du an
        GitHub, GitLab oder einen Server weiter. Der <span className="text-foreground">private Teil</span>{" "}
        <span className="font-mono">{key.path}</span> bleibt auf deinem Rechner – damit beweist ssh, dass du es bist.
      </p>
      {key.public_key && (
        <div className="flex items-center gap-2 rounded-lg border border-border bg-background py-1 pr-1 pl-3.5">
          <code data-testid="public-key" className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-secondary-foreground">
            {key.public_key}
          </code>
          <Button aria-label="Pubkey kopieren" size="icon" variant="secondary" onClick={onCopy} disabled={view.profile.archived}>
            <Copy aria-hidden />
          </Button>
        </div>
      )}
      <dl className="m-0 grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-4">
        {[
          ["Typ", key.key_type?.replace("ssh-", "") ?? "–", true],
          ["Datei", key.path, true],
          ["Fingerprint", key.fingerprint ?? "–", true],
          ["Erstellt", key.created ? `${formatDate(key.created)} · ${formatAge(key.age_days)}` : "–", false],
        ].map(([label, value, isMono]) => (
          <div key={label as string} className="flex min-w-0 flex-col gap-1">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className={isMono ? `${mono} m-0 break-all` : "m-0 text-[13px]"}>{value}</dd>
          </div>
        ))}
      </dl>
    </Card>
  );
}

export function HostForm({
  kind,
  onSave,
  onCancel,
}: {
  kind: "git" | "server";
  onSave: (h: HostEntry) => void;
  onCancel: () => void;
}) {
  const server = kind === "server";
  const [provider, setProvider] = useState<Provider>(server ? "generic" : "github");
  const [hostname, setHostname] = useState(server ? "" : defaultHostname("github"));
  const [alias, setAlias] = useState("");
  const [user, setUser] = useState("");
  const [port, setPort] = useState("");
  return (
    <form
      aria-label={server ? "Server hinzufügen" : "Git-Konto hinzufügen"}
      className="flex flex-col gap-3 rounded-lg border border-border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!hostname.trim()) return;
        onSave({
          provider,
          hostname: hostname.trim(),
          alias: alias.trim() || null,
          user: server && user.trim() ? user.trim() : null,
          port: port ? Number(port) : null,
        });
      }}
    >
      <div className="grid grid-cols-2 gap-3">
        {!server && (
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="host-provider">Anbieter</Label>
            <Select
              id="host-provider"
              value={provider}
              onChange={(e) => {
                const p = e.target.value as Provider;
                setProvider(p);
                setHostname(defaultHostname(p));
              }}
            >
              {GIT_PROVIDERS.map((p) => (
                <option key={p} value={p}>
                  {providerLabel(p)}
                </option>
              ))}
            </Select>
          </div>
        )}
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-name">{server ? "Hostname oder IP" : "Hostname"}</Label>
          <Input
            id="host-name"
            className={mono}
            value={hostname}
            placeholder={server ? "192.168.1.5" : undefined}
            onChange={(e) => setHostname(e.target.value)}
            required
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-alias">
            Alias <span className="font-normal text-muted-foreground">(optional)</span>
          </Label>
          <Input
            id="host-alias"
            className={mono}
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder={server ? "nas" : "github-kunde"}
          />
        </div>
        {server && (
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="host-user">Benutzer</Label>
            <Input id="host-user" className={mono} value={user} onChange={(e) => setUser(e.target.value)} />
          </div>
        )}
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-port">Port</Label>
          <Input id="host-port" inputMode="numeric" pattern="[0-9]*" className={mono} value={port} onChange={(e) => setPort(e.target.value)} placeholder="22" />
        </div>
      </div>
      <FieldHint>
        {server
          ? "Mit einem Alias tippst du später nur ssh nas statt ssh admin@192.168.1.5."
          : "Ein Alias ist nur nötig, wenn du beim selben Anbieter mehrere Konten hast: Dann klonst du mit git@github-kunde:org/repo."}
      </FieldHint>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          Abbrechen
        </Button>
        <Button size="sm" type="submit">
          {server ? "Server speichern" : "Konto speichern"}
        </Button>
      </div>
    </form>
  );
}

function GitCard({ profile }: { profile: Profile }) {
  const update = useUpdateProfile();
  const git = profile.git ?? { name: "", email: "", sign: false };
  const [name, setName] = useState(git.name);
  const [email, setEmail] = useState(git.email);
  const dirty = name !== git.name || email !== git.email;
  const emailOk = email === "" || isValidEmail(email);
  const save = (sign = git.sign) => update.mutate({ ...profile, git: { name: name.trim(), email: email.trim(), sign } });
  return (
    <Card aria-labelledby="git-title">
      <CardTitle id="git-title">Git-Identität</CardTitle>
      <CardDescription>Name und E-Mail, die in deinen Commits stehen – in den Git-Ordnern dieses Keys.</CardDescription>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (emailOk) save();
        }}
      >
        <div className="grid grid-cols-2 gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="git-name" className="text-xs font-normal text-muted-foreground">
              Name
            </Label>
            <Input id="git-name" value={name} onChange={(e) => setName(e.target.value)} disabled={profile.archived} />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="git-mail" className="text-xs font-normal text-muted-foreground">
              E-Mail
            </Label>
            <Input id="git-mail" type="email" value={email} aria-invalid={!emailOk} onChange={(e) => setEmail(e.target.value)} disabled={profile.archived} />
          </div>
        </div>
        <div className="flex items-start justify-between gap-3">
          <ErrorSlot message={!emailOk && "Bitte eine gültige E-Mail-Adresse eingeben."} />
          <Button size="sm" type="submit" disabled={!dirty || !emailOk || update.isPending || profile.archived}>
            Identität speichern
          </Button>
        </div>
      </form>
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-col gap-0.5">
          <span id="sign-label" className="text-[13px] font-medium">
            Commits signieren
          </span>
          <span className="text-xs text-muted-foreground">
            Mit diesem Key unterschreiben – GitHub und GitLab zeigen die Commits dann als „Verified“, wenn der Key dort auch als
            Signing-Key eingetragen ist.
          </span>
        </div>
        <Switch
          aria-labelledby="sign-label"
          checked={git.sign}
          disabled={profile.archived || !git.email || update.isPending}
          onCheckedChange={(sign) => save(sign)}
        />
      </div>
    </Card>
  );
}

function DangerCard({ view }: { view: ProfileView }) {
  const archive = useArchiveProfile();
  const navigate = useNavigate();
  const { profile } = view;
  return (
    <Card aria-labelledby="manage-title">
      <CardTitle id="manage-title">Verwalten</CardTitle>
      <CardDescription>
        Archivieren legt den Key still: Er wandert nach ~/.ssh/archive/ und die Regeln für Git und ssh werden entfernt – z. B.
        wenn ein Kundenprojekt endet.
      </CardDescription>
      <div className="flex flex-wrap gap-2">
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="destructive">
              <Archive aria-hidden />
              Archivieren
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogTitle>„{profile.name}“ archivieren?</AlertDialogTitle>
            <AlertDialogDescription>
              Der Key wandert nach ~/.ssh/archive/, die Regeln für Git und ssh werden beim nächsten Anwenden entfernt.
              Denk daran, den Key auch bei GitHub & Co. und auf Servern zu löschen.
            </AlertDialogDescription>
            <AlertDialogFooter>
              <AlertDialogCancel>Abbrechen</AlertDialogCancel>
              <AlertDialogAction
                onClick={() =>
                  archive.mutate(profile.id, {
                    onSuccess: (res) => {
                      toast.success(`${profile.name} archiviert`, {
                        description: res.remove_key_urls.length ? `Key beim Provider entfernen: ${res.remove_key_urls[0]}` : undefined,
                      });
                      navigate({ to: "/apply" });
                    },
                  })
                }
              >
                Archivieren
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </Card>
  );
}

/** What a generated file does, in one sentence. */
function explainFile(path: string): string {
  if (path.endsWith("keybund.conf")) return "Sagt ssh, welcher Key für welchen Server oder Alias gilt.";
  if (path.endsWith("index.gitconfig")) return "Sagt Git, welche Einstellungen in welchem Ordner gelten.";
  if (path.endsWith(".gitconfig")) return "Name, E-Mail und Key für Git in den Ordnern dieses Keys.";
  return "";
}

function GeneratedConfig({ id }: { id: string }) {
  const { data, isPending } = useGeneratedConfig(id);
  if (isPending) return <Spinner />;
  return (
    <div className="flex flex-col gap-4">
      <p className="m-0 max-w-[760px] text-[13px] leading-normal text-muted-foreground">
        So sieht dieser Key für Git und ssh aus. Keybund schreibt diese Dateien selbst – du musst nichts davon anfassen. Sie zeigen,
        woran Git und ssh erkennen, welcher Key gerade dran ist.
      </p>
      {data?.map((f) => (
        <section key={f.path} className="overflow-hidden rounded-xl border border-border bg-card">
          <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="font-mono text-[12.5px] text-secondary-foreground">{f.path}</span>
              <span className="text-xs text-muted-foreground">{explainFile(f.path)}</span>
            </span>
            <span className="shrink-0 text-xs text-muted-foreground">{f.excerpt ? "Auszug" : "verwaltet von Keybund"}</span>
          </div>
          <pre className="m-0 overflow-x-auto p-4 font-mono text-[13px] leading-[1.7] text-secondary-foreground">{f.content.trimEnd()}</pre>
        </section>
      ))}
    </div>
  );
}
