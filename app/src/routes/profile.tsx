import { Link, useNavigate } from "@tanstack/react-router";
import { Archive, Check, Copy, ExternalLink, Folder, KeyRound, Server, Star, Trash2, X, Zap } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { ColorDot } from "@/components/color-dot";
import { ConfigStatus } from "@/components/pending-banner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle, ItemRow } from "@/components/ui/card";
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
import { FieldError, Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, errorMessage } from "@/lib/api";
import type { HostEntry, Profile, ProfileView, Provider } from "@/lib/bindings";
import {
  useArchiveProfile,
  useCopyPublicKey,
  useGeneratedConfig,
  useProfile,
  useSetDefault,
  useTestConnection,
  useUpdateProfile,
} from "@/lib/queries";
import { defaultHostname, formatAge, formatDate, formatRelative, hostLabel, isValidEmail, PROVIDERS, providerLabel } from "@/lib/utils";

const mono = "font-mono text-[13px]";

export function ProfilePage({ id }: { id: string }) {
  const { data: view, isPending, error } = useProfile(id);
  if (isPending) return <Spinner />;
  if (error) return <p role="alert">{errorMessage(error)}</p>;
  if (!view) {
    return (
      <div className="flex flex-col gap-3">
        <h1 className="m-0 text-xl font-semibold">Profil nicht gefunden</h1>
        <Link to="/">Zur Übersicht</Link>
      </div>
    );
  }
  return <ProfileDetail key={view.profile.id} view={view} />;
}

function ProfileDetail({ view }: { view: ProfileView }) {
  const { profile, key } = view;
  const copy = useCopyPublicKey();
  const [copied, setCopied] = useState(false);
  const test = useTestConnection();
  const primary = profile.hosts[0];
  const folder = profile.folders[0];

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
          Profile
        </Link>
        <span aria-hidden>/</span>
        <span className="text-foreground">{profile.name}</span>
      </div>

      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-3">
            <ColorDot color={profile.color} className="size-3" />
            <h1 className="m-0 text-[26px] font-semibold tracking-tight">{profile.name}</h1>
            {primary && <Badge>{providerLabel(primary.provider)}</Badge>}
            {profile.archived ? <Badge variant="muted">Archiviert</Badge> : <Badge variant="success">Aktiv</Badge>}
            {view.is_default && <Badge variant="outline">Default</Badge>}
          </div>
          <p className="m-0 text-muted-foreground">
            {folder ? (
              <>
                alles unter <span className="font-mono text-secondary-foreground">{folder}</span> nutzt diesen Key
              </>
            ) : (
              "kein Projektordner – gilt nur für die Hosts"
            )}
          </p>
        </div>
        {!profile.archived && (
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => test.mutate(profile.id)} disabled={test.isPending || !primary}>
              <Zap aria-hidden />
              Verbindung testen
            </Button>
            {view.add_key_urls[0] && (
              <Button variant="outline" onClick={() => api.openUrl(view.add_key_urls[0].url)}>
                <ExternalLink aria-hidden />
                Bei {providerLabel(view.add_key_urls[0].provider)} hinzufügen
              </Button>
            )}
            <Button onClick={onCopy} className="min-w-[156px]" disabled={!key.public_key}>
              {copied ? <Check aria-hidden strokeWidth={2.4} /> : <Copy aria-hidden />}
              {copied ? "Kopiert" : "Pubkey kopieren"}
            </Button>
          </div>
        )}
      </header>

      <Tabs defaultValue="overview" className="flex flex-col gap-6">
        <TabsList aria-label="Ansicht">
          <TabsTrigger value="overview">Übersicht</TabsTrigger>
          <TabsTrigger value="config">Generierte Config</TabsTrigger>
        </TabsList>
        <TabsContent value="overview">
          <div className="grid grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4 max-[640px]:grid-cols-1">
            <KeyCard view={view} onCopy={onCopy} />
            <ConnectionCard view={view} test={test} />
            <HostsCard profile={profile} links={view.add_key_urls.map((l) => l.url)} />
            <FoldersCard profile={profile} />
            <GitCard profile={profile} />
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

function KeyCard({ view, onCopy }: { view: ProfileView; onCopy: () => void }) {
  const { key } = view;
  return (
    <Card className="col-span-full" aria-labelledby="key-title">
      <CardHeader>
        <div className="flex items-center gap-2.5">
          <KeyRound aria-hidden className="size-[18px] text-muted-foreground" />
          <CardTitle id="key-title">SSH-Key</CardTitle>
        </div>
        {!key.private_exists ? (
          <Badge variant="danger">Key-Datei fehlt</Badge>
        ) : key.age_days != null && key.age_days > 730 ? (
          <Badge variant="warning">Älter als 2 Jahre – rotieren</Badge>
        ) : null}
      </CardHeader>
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
    </Card>
  );
}

function ConnectionCard({ view, test }: { view: ProfileView; test: ReturnType<typeof useTestConnection> }) {
  const primary = view.profile.hosts[0];
  const result = test.data;
  const last = view.last_test;
  const provider = primary ? providerLabel(primary.provider) : "Der Host";
  return (
    <Card aria-labelledby="conn-title">
      <CardTitle id="conn-title">Verbindung</CardTitle>
      <div aria-live="polite" className="flex min-h-11 items-center gap-3">
        {test.isPending ? (
          <>
            <Spinner className="size-9" label="Teste Verbindung" />
            <div className="flex flex-col gap-0.5">
              <span className="font-medium">Teste Verbindung …</span>
              <span className="font-mono text-[13px] text-muted-foreground">ssh -T {primary?.alias ?? primary?.hostname}</span>
            </div>
          </>
        ) : test.isError ? (
          <StatusLine ok={false} title="Test nicht möglich" detail={errorMessage(test.error)} />
        ) : result ? (
          <StatusLine ok={result.success} title={result.message} detail={`${result.host} · gerade eben getestet`} />
        ) : last ? (
          <StatusLine ok={last.success} title={last.message} detail={`${primary?.hostname ?? ""} · ${formatRelative(last.at)} getestet`} />
        ) : (
          <span className="text-[13px] text-muted-foreground">Noch nicht getestet.</span>
        )}
      </div>
      {result && !result.success && (
        <pre className="m-0 overflow-x-auto rounded-lg bg-background p-3 font-mono text-xs text-muted-foreground">{result.output}</pre>
      )}
      <p className="m-0 text-[13px] leading-normal text-muted-foreground">
        Der Test nutzt nur diesen Key (IdentitiesOnly). Ein Erfolg heißt: {provider} kennt den Key und ordnet ihn deinem
        Account zu.
      </p>
    </Card>
  );
}

function StatusLine({ ok, title, detail }: { ok: boolean; title: string; detail: string }) {
  return (
    <>
      <span
        className={
          ok
            ? "flex size-9 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-success"
            : "flex size-9 shrink-0 items-center justify-center rounded-full bg-red-500/15 text-danger"
        }
      >
        {ok ? <Check aria-hidden className="size-[18px]" strokeWidth={2.4} /> : <X aria-hidden className="size-[18px]" strokeWidth={2.4} />}
      </span>
      <div className="flex flex-col gap-0.5">
        <span className="font-medium" data-testid="connection-status">
          {title}
        </span>
        <span className="text-[13px] text-muted-foreground">{detail}</span>
      </div>
    </>
  );
}

function HostsCard({ profile, links }: { profile: Profile; links: string[] }) {
  const update = useUpdateProfile();
  const [adding, setAdding] = useState(false);
  const removeHost = (index: number) => update.mutate({ ...profile, hosts: profile.hosts.filter((_, i) => i !== index) });
  return (
    <Card aria-labelledby="hosts-title">
      <CardHeader>
        <CardTitle id="hosts-title">Hosts</CardTitle>
        {!profile.archived && !adding && (
          <Button variant="ghost" size="sm" onClick={() => setAdding(true)}>
            + Host
          </Button>
        )}
      </CardHeader>
      {profile.hosts.map((h, i) => {
        const link = links.find((l) => l.includes(h.hostname));
        return (
          <ItemRow key={`${h.alias}-${h.hostname}-${i}`}>
            <Server aria-hidden className="size-4 text-muted-foreground" />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span className={mono}>{h.alias ?? h.hostname}</span>
              <span className="text-xs text-muted-foreground">{hostLabel(h)}</span>
            </div>
            {link && !profile.archived && (
              <button
                className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-[13px] text-foreground hover:bg-background"
                onClick={() => api.openUrl(link)}
              >
                Key eintragen
                <ExternalLink aria-hidden className="size-[13px]" />
              </button>
            )}
            {!profile.archived && profile.hosts.length > 1 && (
              <Button variant="ghost" size="icon" aria-label={`Host ${h.alias ?? h.hostname} entfernen`} onClick={() => removeHost(i)}>
                <Trash2 aria-hidden />
              </Button>
            )}
          </ItemRow>
        );
      })}
      {profile.hosts.length === 0 && <p className="m-0 text-[13px] text-muted-foreground">Kein Host eingetragen.</p>}
      {adding && (
        <HostForm
          onCancel={() => setAdding(false)}
          onSave={(host) =>
            update.mutate({ ...profile, hosts: [...profile.hosts, host] }, { onSuccess: () => setAdding(false) })
          }
        />
      )}
    </Card>
  );
}

export function HostForm({ onSave, onCancel }: { onSave: (h: HostEntry) => void; onCancel: () => void }) {
  const [provider, setProvider] = useState<Provider>("github");
  const [hostname, setHostname] = useState(defaultHostname("github"));
  const [alias, setAlias] = useState("");
  const [user, setUser] = useState("");
  const [port, setPort] = useState("");
  const generic = provider === "generic";
  return (
    <form
      aria-label="Host hinzufügen"
      className="flex flex-col gap-3 rounded-lg border border-border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (!hostname.trim()) return;
        onSave({
          provider,
          hostname: hostname.trim(),
          alias: generic && alias.trim() ? alias.trim() : null,
          user: generic && user.trim() ? user.trim() : null,
          port: port ? Number(port) : null,
        });
      }}
    >
      <div className="grid grid-cols-2 gap-3">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-provider">Provider</Label>
          <Select
            id="host-provider"
            value={provider}
            onChange={(e) => {
              const p = e.target.value as Provider;
              setProvider(p);
              setHostname(defaultHostname(p));
            }}
          >
            {PROVIDERS.map((p) => (
              <option key={p} value={p}>
                {providerLabel(p)}
              </option>
            ))}
          </Select>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-name">Hostname</Label>
          <Input id="host-name" className={mono} value={hostname} onChange={(e) => setHostname(e.target.value)} required />
        </div>
        {generic && (
          <>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="host-alias">Alias</Label>
              <Input id="host-alias" className={mono} value={alias} onChange={(e) => setAlias(e.target.value)} placeholder="meinserver" />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="host-user">Benutzer</Label>
              <Input id="host-user" className={mono} value={user} onChange={(e) => setUser(e.target.value)} />
            </div>
          </>
        )}
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-port">Port</Label>
          <Input id="host-port" inputMode="numeric" pattern="[0-9]*" className={mono} value={port} onChange={(e) => setPort(e.target.value)} placeholder="22" />
        </div>
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={onCancel}>
          Abbrechen
        </Button>
        <Button size="sm" type="submit">
          Host speichern
        </Button>
      </div>
    </form>
  );
}

function FoldersCard({ profile }: { profile: Profile }) {
  const update = useUpdateProfile();
  const [adding, setAdding] = useState(false);
  const [folder, setFolder] = useState("");
  const save = () => {
    if (!folder.trim()) return;
    update.mutate(
      { ...profile, folders: [...profile.folders, folder.trim()] },
      {
        onSuccess: () => {
          setAdding(false);
          setFolder("");
        },
      },
    );
  };
  return (
    <Card aria-labelledby="folders-title">
      <CardHeader>
        <CardTitle id="folders-title">Projektordner</CardTitle>
        {!profile.archived && !adding && (
          <Button variant="ghost" size="sm" onClick={() => setAdding(true)}>
            + Ordner
          </Button>
        )}
      </CardHeader>
      {profile.folders.map((f) => (
        <ItemRow key={f}>
          <Folder aria-hidden className="size-4 text-muted-foreground" />
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className={mono}>{f}</span>
            <span className="text-xs text-muted-foreground">includeIf gitdir · alle Repos darunter</span>
          </div>
          {!profile.archived && (
            <Button
              variant="ghost"
              size="icon"
              aria-label={`Ordner ${f} entfernen`}
              onClick={() => update.mutate({ ...profile, folders: profile.folders.filter((x) => x !== f) })}
            >
              <Trash2 aria-hidden />
            </Button>
          )}
        </ItemRow>
      ))}
      {profile.folders.length === 0 && <p className="m-0 text-[13px] text-muted-foreground">Kein Ordner – Git nutzt den Key nur per Default.</p>}
      {adding && (
        <form
          className="flex flex-wrap gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <label htmlFor="new-folder" className="sr-only">
            Neuer Ordner
          </label>
          <Input id="new-folder" className={`${mono} flex-1`} placeholder="~/dev/projekt/" value={folder} onChange={(e) => setFolder(e.target.value)} autoFocus />
          <Button size="sm" className="h-[38px]" type="submit">
            Speichern
          </Button>
        </form>
      )}
      {update.isError && <FieldError>{errorMessage(update.error)}</FieldError>}
    </Card>
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
        {!emailOk && <FieldError>Bitte eine gültige E-Mail-Adresse eingeben.</FieldError>}
        {dirty && (
          <Button size="sm" type="submit" className="self-end" disabled={!emailOk || update.isPending}>
            Identität speichern
          </Button>
        )}
      </form>
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-col gap-0.5">
          <span id="sign-label" className="text-[13px] font-medium">
            Commits signieren
          </span>
          <span className="text-xs text-muted-foreground">mit diesem SSH-Key (gpg.format = ssh)</span>
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
  const setDefault = useSetDefault();
  const navigate = useNavigate();
  const { profile } = view;
  return (
    <Card aria-labelledby="manage-title">
      <CardTitle id="manage-title">Verwalten</CardTitle>
      <div className="flex flex-wrap gap-2">
        {!view.is_default && (
          <Button variant="outline" onClick={() => setDefault.mutate(profile.id, { onSuccess: () => toast.success(`${profile.name} ist jetzt Default`) })}>
            <Star aria-hidden />
            Als Default setzen
          </Button>
        )}
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
              Der Key wandert nach ~/.ssh/archive/, die Configs für das Profil werden beim nächsten Anwenden entfernt.
              Denk daran, den Key auch beim Provider zu löschen.
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

function GeneratedConfig({ id }: { id: string }) {
  const { data, isPending } = useGeneratedConfig(id);
  if (isPending) return <Spinner />;
  return (
    <div className="flex flex-col gap-4">
      {data?.map((f) => (
        <section key={f.path} className="overflow-hidden rounded-xl border border-border bg-card">
          <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <span className="font-mono text-[12.5px] text-secondary-foreground">{f.path}</span>
            <span className="text-xs text-muted-foreground">{f.excerpt ? "Auszug" : "verwaltet von Keybund"}</span>
          </div>
          <pre className="m-0 overflow-x-auto p-4 font-mono text-[13px] leading-[1.7] text-secondary-foreground">{f.content.trimEnd()}</pre>
        </section>
      ))}
    </div>
  );
}
