import { Link, useNavigate } from "@tanstack/react-router";
import {
  Archive,
  Check,
  Copy,
  ExternalLink,
  Folder,
  FolderOpen,
  GitBranch,
  KeyRound,
  Server,
  Star,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { ColorDot } from "@/components/color-dot";
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
import type { HostEntry, LastTest, Profile, ProfileView, Provider } from "@/lib/bindings";
import { gitAccounts, servers } from "@/lib/categories";
import { rich, t } from "@/lib/i18n";
import { openExternal } from "@/lib/open";
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
import {
  defaultHostname,
  formatAge,
  formatDate,
  formatRelative,
  GIT_PROVIDERS,
  isValidEmail,
  providerLabel,
} from "@/lib/utils";
import { m } from "@/paraglide/messages";

const mono = "font-mono text-[13px]";

export function ProfilePage({ id }: { id: string }) {
  const { data: view, isPending, error } = useProfile(id);
  if (isPending) return <Spinner />;
  if (error) return <p role="alert">{errorMessage(error)}</p>;
  if (!view) {
    return (
      <div className="flex flex-col gap-3">
        <h1 className="m-0 text-xl font-semibold">{m.key_notFound()}</h1>
        <Link to="/">{m.key_toOverview()}</Link>
      </div>
    );
  }
  return <KeyDetail key={view.profile.id} view={view} />;
}

function KeyDetail({ view }: { view: ProfileView }) {
  const { profile, key } = view;
  const copy = useCopyPublicKey();
  const [copied, setCopied] = useState(false);
  const usesGit = profile.folders.length > 0 || view.is_default || gitAccounts(profile).length > 0;

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
          {m.sidebar_keys()}
        </Link>
        <span aria-hidden>/</span>
        <span className="text-foreground">{profile.name}</span>
      </div>

      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-3">
            <ColorDot color={profile.color} className="size-3" />
            <h1 className="m-0 text-[26px] font-semibold tracking-tight">{profile.name}</h1>
            {view.is_default && <Badge variant="outline">{m.usage_default()}</Badge>}
            {profile.archived && <Badge variant="muted">{m.key_archived()}</Badge>}
          </div>
          <p className="m-0 text-muted-foreground">
            <span className="font-mono text-[13px] text-secondary-foreground">{key.path}</span>
            {key.created && ` · ${m.key_created({ when: formatAge(key.age_days) })}`}
          </p>
        </div>
        {!profile.archived && (
          <Button onClick={onCopy} className="min-w-[156px]" disabled={!key.public_key}>
            {copied ? <Check aria-hidden strokeWidth={2.4} /> : <Copy aria-hidden />}
            {copied ? m.key_copied() : m.key_copyPubkey()}
          </Button>
        )}
      </header>

      <Tabs defaultValue="overview" className="flex flex-col gap-6">
        <TabsList aria-label={m.key_view()}>
          <TabsTrigger value="overview">{m.home_title()}</TabsTrigger>
          <TabsTrigger value="config">{m.key_generatedConfig()}</TabsTrigger>
        </TabsList>
        <TabsContent value="overview">
          <div className="grid grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4 max-[640px]:grid-cols-1">
            <UsageCard view={view} />
            <KeyCard view={view} onCopy={onCopy} />
            {/* A stored identity stays visible (and clearable) even without folders. */}
            {profile.folders.length > 0 || profile.git ? (
              <GitCard key={JSON.stringify(profile.git)} profile={profile} />
            ) : (
              usesGit && <GlobalIdentityCard />
            )}
            {!profile.archived && <DangerCard view={view} />}
          </div>
        </TabsContent>
        <TabsContent value="config">
          <GeneratedConfig id={profile.id} archived={profile.archived} />
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
  const update = useUpdateProfile(profile.id);
  const setDefault = useSetDefault();
  const [adding, setAdding] = useState<Adding>(null);
  const accounts = gitAccounts(profile);
  const serverList = servers(profile);
  const empty = profile.folders.length === 0 && profile.hosts.length === 0 && !view.is_default;
  const same = (a: HostEntry, b: HostEntry) =>
    a.provider === b.provider && a.hostname === b.hostname && a.alias === b.alias;
  const removeHost = (h: HostEntry) => update.mutate((p) => ({ ...p, hosts: p.hosts.filter((x) => !same(x, h)) }));
  const addHost = (host: HostEntry) =>
    update.mutate((p) => ({ ...p, hosts: [...p.hosts, host] }), { onSuccess: () => setAdding(null) });

  return (
    <Card className="col-span-full" aria-labelledby="usage-title">
      <CardTitle id="usage-title">{m.usage_title()}</CardTitle>
      <CardDescription>{m.usage_intro()}</CardDescription>

      {empty && !adding && !profile.archived && (
        <EmptyUsage onPick={setAdding} canDefault onDefault={() => setDefault.mutate(profile.id)} />
      )}

      {!empty && (
        <ul aria-label={m.usage_list()} className="m-0 flex list-none flex-col gap-2 p-0">
          {view.is_default && (
            <UsageRow
              icon={<Star aria-hidden className="size-4 text-amber-500 dark:text-amber-400" />}
              title={m.usage_defaultForGit()}
              detail={m.usage_defaultDetail()}
              actions={
                !profile.archived && (
                  <Button variant="ghost" size="sm" onClick={() => setDefault.mutate(null)}>
                    {m.usage_unsetDefault()}
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
              detail={m.usage_folderDetail()}
              actions={
                !profile.archived && (
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={m.usage_removeFolder({ folder: f })}
                    onClick={() => update.mutate((p) => ({ ...p, folders: p.folders.filter((x) => x !== f) }))}
                  >
                    <Trash2 aria-hidden />
                  </Button>
                )
              }
            />
          ))}
          {accounts.map((h) => (
            <AccountRow
              key={`a-${h.alias ?? h.hostname}`}
              profile={profile}
              host={h}
              addKeyUrl={view.add_key_urls.find((l) => l.host === h.hostname)?.url ?? null}
              lastTest={lastTestOf(view, h)}
              onRemove={() => removeHost(h)}
            />
          ))}
          {serverList.map((h) => (
            <ServerRow
              key={`s-${h.alias ?? h.hostname}`}
              profile={profile}
              host={h}
              lastTest={lastTestOf(view, h)}
              onRemove={() => removeHost(h)}
            />
          ))}
        </ul>
      )}

      {adding === "folder" && <FolderForm profile={profile} onDone={() => setAdding(null)} />}
      {adding === "git" && (
        <HostForm kind="git" saving={update.isPending} onCancel={() => setAdding(null)} onSave={addHost} />
      )}
      {adding === "server" && (
        <HostForm kind="server" saving={update.isPending} onCancel={() => setAdding(null)} onSave={addHost} />
      )}

      {!empty && !adding && !profile.archived && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] text-muted-foreground">{m.usage_add()}</span>
          <Button variant="outline" size="sm" onClick={() => setAdding("folder")}>
            <Folder aria-hidden />
            {m.usage_folder()}
          </Button>
          <Button variant="outline" size="sm" onClick={() => setAdding("git")}>
            <GitBranch aria-hidden />
            {m.usage_account()}
          </Button>
          <Button variant="outline" size="sm" onClick={() => setAdding("server")}>
            <Server aria-hidden />
            {m.usage_server()}
          </Button>
          {!view.is_default && (
            <Button variant="ghost" size="sm" onClick={() => setDefault.mutate(profile.id)}>
              <Star aria-hidden />
              {m.usage_makeDefault()}
            </Button>
          )}
        </div>
      )}
      {update.isError && <FieldError>{errorMessage(update.error)}</FieldError>}
    </Card>
  );
}

function EmptyUsage({
  onPick,
  canDefault,
  onDefault,
}: {
  onPick: (a: Adding) => void;
  canDefault: boolean;
  onDefault: () => void;
}) {
  const tiles: { kind: Adding; icon: React.ReactNode; title: string; text: string }[] = [
    {
      kind: "folder",
      icon: <Folder aria-hidden className="size-5" />,
      title: m.usage_folder(),
      text: m.usage_folderTile(),
    },
    {
      kind: "git",
      icon: <GitBranch aria-hidden className="size-5" />,
      title: m.usage_account(),
      text: m.usage_accountTile(),
    },
    {
      kind: "server",
      icon: <Server aria-hidden className="size-5" />,
      title: m.usage_server(),
      text: m.usage_serverTile(),
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
            className="flex gap-3 rounded-[10px] border border-border bg-transparent p-4 text-left hover:border-zinc-400 dark:hover:border-zinc-500"
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
        <button
          type="button"
          className="self-start text-[13px] text-muted-foreground underline-offset-2 hover:underline"
          onClick={onDefault}
        >
          {m.usage_orDefault()}
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

/** The stored result of the host's last test (also from the CLI or an earlier session). */
function lastTestOf(view: ProfileView, host: HostEntry): LastTest | null {
  return view.host_tests.find((t) => t.host === (host.alias ?? host.hostname)) ?? null;
}

/** The one thing to do when the server does not know the key yet. */
type Fix = { text: string; label: string; onClick: () => void; busy?: boolean };

function TestResult({
  test,
  fix,
  lastTest,
}: {
  test: ReturnType<typeof useTestConnection>;
  fix?: Fix | null;
  lastTest?: LastTest | null;
}) {
  if (test.isPending) return <Spinner label={m.test_running()} />;
  if (test.isError) return <FieldError>{errorMessage(test.error)}</FieldError>;
  const result = test.data;
  if (!result) {
    // Not tested in this session: what the last test of this host said.
    if (!lastTest) return null;
    return (
      <span
        data-testid="last-test"
        className={`flex items-center gap-2 text-xs ${lastTest.success ? "text-muted-foreground" : "text-danger"}`}
      >
        {lastTest.success ? <Check aria-hidden className="size-3.5" /> : <X aria-hidden className="size-3.5" />}
        {m.test_last({ when: formatRelative(lastTest.at), result: t(lastTest.message) })}
      </span>
    );
  }
  return (
    <div aria-live="polite" className="flex flex-col gap-2">
      <span
        className={`flex items-center gap-2 text-[13px] ${result.success ? "text-success" : "text-danger"}`}
        data-testid="connection-status"
      >
        {result.success ? <Check aria-hidden className="size-4" /> : <X aria-hidden className="size-4" />}
        {t(result.message)}
      </span>
      {result.key_rejected && fix && (
        <div className="flex flex-wrap items-center gap-3 rounded-[10px] border border-border bg-background p-3 text-[13px]">
          <span className="min-w-[220px] flex-1 text-muted-foreground">{fix.text}</span>
          <Button size="sm" onClick={fix.onClick} disabled={fix.busy}>
            {fix.busy && <Spinner className="size-3" />}
            {fix.label}
          </Button>
        </div>
      )}
      {result.other_key && (
        <div
          role="alert"
          className="flex flex-col gap-1.5 rounded-[10px] border border-amber-500/35 bg-amber-500/5 p-3.5 text-[13px]"
        >
          <span className="font-medium">{m.test_otherKeyTitle()}</span>
          <span className="text-muted-foreground">
            {rich(m.test_otherKeyText, { key: <span className="font-mono">{result.used_key}</span> })}
          </span>
          <span className="flex gap-3">
            <Link to="/import" className="font-medium text-foreground">
              {m.test_takeOverConfig()}
            </Link>
            <Link to="/doctor" className="text-foreground">
              {m.home_toDoctor()}
            </Link>
          </span>
        </div>
      )}
      <ProbeDetails result={result} />
    </div>
  );
}

function AccountRow({
  profile,
  host,
  addKeyUrl,
  lastTest,
  onRemove,
}: {
  profile: Profile;
  host: HostEntry;
  addKeyUrl: string | null;
  lastTest: LastTest | null;
  onRemove: () => void;
}) {
  const test = useTestConnection();
  const copy = useCopyPublicKey();
  // The pubkey was copied and the page opened: now it is the user's turn.
  const [pasting, setPasting] = useState(false);
  const name = host.alias ?? host.hostname;
  const provider = providerLabel(host.provider);
  const signs = host.provider === "github" && !!profile.git?.sign;
  const addKey = async () => {
    // Without the pubkey in the clipboard there is nothing to paste (the toast says why).
    const copied = await copy.mutateAsync(profile.id).then(
      () => true,
      () => false,
    );
    if (!copied) return;
    if (addKeyUrl) await openExternal(addKeyUrl);
    setPasting(true);
  };
  const testNow = () =>
    test.mutate({ id: profile.id, host: name }, { onSuccess: (r) => r.success && setPasting(false) });
  return (
    <UsageRow
      icon={<GitBranch aria-hidden className="size-4 text-muted-foreground" />}
      title={rich(m.account_title, {
        provider,
        name: <span className="font-mono font-normal text-secondary-foreground">{name}</span>,
      })}
      detail={
        host.alias
          ? m.account_aliasDetail({ alias: host.alias, host: host.hostname, provider })
          : m.account_detail({ provider })
      }
      actions={
        !profile.archived && (
          <>
            <Button variant="ghost" size="sm" onClick={addKey} title={m.account_addKeyHint({ provider })}>
              {m.usage_addKey()}
            </Button>
            <Button variant="ghost" size="sm" disabled={test.isPending} onClick={testNow}>
              <Zap aria-hidden />
              {m.usage_test()}
            </Button>
            <Button variant="ghost" size="icon" aria-label={m.account_remove({ name })} onClick={onRemove}>
              <Trash2 aria-hidden />
            </Button>
          </>
        )
      }
    >
      {pasting && (
        <div
          role="status"
          className="flex flex-col gap-2 rounded-[10px] border border-border bg-background p-3.5 text-[13px]"
        >
          <span>
            <span className="font-medium">{m.account_copied()}</span>{" "}
            <span className="text-muted-foreground">{m.account_pasteHint({ provider, name: profile.name })}</span>
          </span>
          {signs && <span className="text-muted-foreground">{m.account_signingHint()}</span>}
          <span className="flex flex-wrap gap-2">
            <Button size="sm" onClick={testNow} disabled={test.isPending}>
              {m.account_doneTest()}
            </Button>
            {addKeyUrl && (
              <Button variant="ghost" size="sm" onClick={() => openExternal(addKeyUrl)}>
                <ExternalLink aria-hidden />
                {m.account_reopen()}
              </Button>
            )}
          </span>
        </div>
      )}
      <TestResult
        test={test}
        lastTest={lastTest}
        fix={
          pasting
            ? null
            : {
                text: m.account_fixText({ provider }),
                label: m.account_fixLabel({ provider }),
                onClick: addKey,
              }
        }
      />
    </UsageRow>
  );
}

function ServerRow({
  profile,
  host,
  lastTest,
  onRemove,
}: {
  profile: Profile;
  host: HostEntry;
  lastTest: LastTest | null;
  onRemove: () => void;
}) {
  const install = useInstallKey();
  const test = useTestConnection();
  const name = host.alias ?? host.hostname;
  const result = install.data ?? null;
  const target = `${host.user ? `${host.user}@` : ""}${host.hostname}${host.port && host.port !== 22 ? `:${host.port}` : ""}`;
  // Test right after installing, so the row shows whether it worked.
  const installKey = () =>
    install.mutate(
      { id: profile.id, host: name },
      { onSuccess: (r) => r.success && test.mutate({ id: profile.id, host: name }) },
    );
  return (
    <UsageRow
      icon={<Server aria-hidden className="size-4 text-muted-foreground" />}
      title={rich(m.server_title, {
        name: <span className="font-mono font-normal text-secondary-foreground">{name}</span>,
      })}
      detail={m.server_detail({ name, target })}
      actions={
        !profile.archived && (
          <>
            <Button variant="ghost" size="sm" disabled={install.isPending} onClick={installKey}>
              {install.isPending && <Spinner className="size-3" />}
              {m.usage_addKey()}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={test.isPending}
              onClick={() => test.mutate({ id: profile.id, host: name })}
            >
              <Zap aria-hidden />
              {m.usage_test()}
            </Button>
            <Button variant="ghost" size="icon" aria-label={m.server_remove({ name })} onClick={onRemove}>
              <Trash2 aria-hidden />
            </Button>
          </>
        )
      }
    >
      <div aria-live="polite" className="flex flex-col gap-1 text-xs empty:hidden">
        {install.isPending && <span className="text-muted-foreground">{m.server_installing({ name })}</span>}
        {result && <span className={result.success ? "text-success" : "text-danger"}>{t(result.message)}</span>}
        {install.isError && <span className="text-danger">{errorMessage(install.error)}</span>}
      </div>
      <TestResult
        test={test}
        lastTest={lastTest}
        fix={{
          text: m.server_fixText({ name }),
          label: m.server_fixLabel({ name }),
          onClick: installKey,
          busy: install.isPending,
        }}
      />
    </UsageRow>
  );
}

/** Adds a git folder; asks for the git identity and the hoster if missing. */
function FolderForm({ profile, onDone }: { profile: Profile; onDone: () => void }) {
  const update = useUpdateProfile(profile.id);
  const settings = useSettings();
  const all = useProfiles();
  // Asked with the first folder only; later folders share the key's identity (card below).
  const needsIdentity = !profile.git && profile.folders.length === 0;
  const global = all.data?.global_identity;
  // Usually the same person: the name from ~/.gitconfig or another key.
  const knownName = global?.name ?? all.data?.profiles.find((p) => p.profile.git?.name)?.profile.git?.name ?? "";
  const needsAccount = gitAccounts(profile).length === 0;
  const [folder, setFolder] = useState("");
  const [name, setName] = useState<string | null>(null);
  const [email, setEmail] = useState<string | null>(null);
  const [provider, setProvider] = useState<Provider | "">("github");
  const suggestion = `${settings.data?.projects_dir ?? "~/dev"}/${profile.id}/`;
  const gitName = name ?? knownName;
  const gitEmail = email ?? global?.email ?? "";
  const identityOk = !needsIdentity || (!!gitName.trim() && isValidEmail(gitEmail));
  const error = !folder.trim() || identityOk ? null : m.folder_errorIdentity();
  const save = () => {
    const value = folder.trim() || suggestion;
    // The same as ~/.gitconfig: nothing of its own, the key follows the global identity.
    const sameAsGlobal = gitName.trim() === (global?.name ?? "") && gitEmail.trim() === (global?.email ?? "");
    const identity =
      needsIdentity && !sameAsGlobal ? { name: gitName.trim(), email: gitEmail.trim(), sign: false } : null;
    const account: HostEntry | null =
      needsAccount && provider
        ? { provider, hostname: defaultHostname(provider), alias: null, user: null, port: null }
        : null;
    update.mutate(
      (p) => ({
        ...p,
        folders: [...p.folders, value],
        git: identity ?? p.git,
        hosts: account && gitAccounts(p).length === 0 ? [...p.hosts, account] : p.hosts,
      }),
      { onSuccess: onDone },
    );
  };
  const browse = async () => {
    const picked = await pickFolder();
    if (picked) setFolder(await api.normalizeFolder(picked));
  };
  return (
    <form
      aria-label={m.folder_formLabel()}
      className="flex flex-col gap-3 rounded-lg border border-border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (identityOk) save();
      }}
    >
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="new-folder">{m.folder_label()}</Label>
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
            {m.folder_browse()}
          </Button>
        </div>
        <FieldHint>{m.folder_hint({ folder: suggestion })}</FieldHint>
      </div>
      {needsIdentity && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="folder-git-name">{m.folder_gitName()}</Label>
            <Input id="folder-git-name" value={gitName} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="folder-git-mail">{m.folder_gitEmail()}</Label>
            <Input id="folder-git-mail" type="email" value={gitEmail} onChange={(e) => setEmail(e.target.value)} />
          </div>
          {global?.email && <FieldHint className="col-span-full">{m.folder_identityHint()}</FieldHint>}
        </div>
      )}
      {needsAccount && (
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="folder-provider">{m.folder_provider()}</Label>
          <Select id="folder-provider" value={provider} onChange={(e) => setProvider(e.target.value as Provider | "")}>
            {GIT_PROVIDERS.map((p) => (
              <option key={p} value={p}>
                {providerLabel(p)}
              </option>
            ))}
            <option value="">{m.folder_providerLater()}</option>
          </Select>
          <FieldHint>{m.folder_providerHint()}</FieldHint>
        </div>
      )}
      <ErrorSlot message={error} />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onDone}>
          {m.common_cancel()}
        </Button>
        <Button size="sm" type="submit" disabled={update.isPending || !identityOk}>
          {m.folder_save()}
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
          <CardTitle id="key-title">{m.keyCard_title()}</CardTitle>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {!key.private_exists ? (
            <Badge variant="danger">{m.keyCard_missing()}</Badge>
          ) : key.age_days != null && key.age_days > 730 ? (
            <Badge variant="warning">{m.keyCard_old()}</Badge>
          ) : null}
          {key.private_exists && view.in_agent === true && <Badge variant="success">{m.keyCard_inAgent()}</Badge>}
          {key.private_exists && view.in_agent === false && (
            <Badge variant="muted" title={m.keyCard_notInAgentHint()}>
              {m.keyCard_notInAgent()}
            </Badge>
          )}
        </div>
      </CardHeader>
      <p className="m-0 text-[13px] leading-normal text-muted-foreground">{m.keyCard_intro()}</p>
      <details className="group">
        <summary className="cursor-pointer text-[13px] text-muted-foreground select-none hover:text-foreground">
          {m.keyCard_show()}
        </summary>
        <div className="mt-3 flex flex-col gap-4">
          <p className="m-0 text-[13px] leading-normal text-muted-foreground">
            {rich(m.keyCard_explain, {
              public: <span className="text-foreground">{m.keyCard_publicPart()}</span>,
              private: <span className="text-foreground">{m.keyCard_privatePart()}</span>,
              path: <span className="font-mono">{key.path}</span>,
            })}
          </p>
          {key.public_key && (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-background py-1 pr-1 pl-3.5">
              <code
                data-testid="public-key"
                className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-secondary-foreground"
              >
                {key.public_key}
              </code>
              <Button
                aria-label={m.key_copyPubkey()}
                size="icon"
                variant="secondary"
                onClick={onCopy}
                disabled={view.profile.archived}
              >
                <Copy aria-hidden />
              </Button>
            </div>
          )}
          <dl className="m-0 grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-4">
            {[
              [m.keyCard_type(), key.key_type?.replace("ssh-", "") ?? "–", true],
              [m.keyCard_file(), key.path, true],
              [m.keyCard_fingerprint(), key.fingerprint ?? "–", true],
              [
                m.keyCard_created(),
                key.created ? `${formatDate(key.created)} · ${formatAge(key.age_days)}` : "–",
                false,
              ],
            ].map(([label, value, isMono]) => (
              <div key={label as string} className="flex min-w-0 flex-col gap-1">
                <dt className="text-xs text-muted-foreground">{label}</dt>
                <dd className={isMono ? `${mono} m-0 break-all` : "m-0 text-[13px]"}>{value}</dd>
              </div>
            ))}
          </dl>
        </div>
      </details>
    </Card>
  );
}

export function HostForm({
  kind,
  saving = false,
  onSave,
  onCancel,
}: {
  kind: "git" | "server";
  /** A save is running: the form closes on success, until then a second submit would add the host twice. */
  saving?: boolean;
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
      aria-label={server ? m.hostForm_server() : m.hostForm_account()}
      className="flex flex-col gap-3 rounded-lg border border-border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (saving || !hostname.trim()) return;
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
            <Label htmlFor="host-provider">{m.hostForm_provider()}</Label>
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
          <Label htmlFor="host-name">{server ? m.hostForm_hostOrIp() : m.hostForm_hostname()}</Label>
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
            {m.hostForm_alias()} <span className="font-normal text-muted-foreground">{m.hostForm_optional()}</span>
          </Label>
          <Input
            id="host-alias"
            className={mono}
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder={server ? "nas" : m.hostForm_aliasPlaceholder()}
          />
        </div>
        {server && (
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="host-user">{m.hostForm_user()}</Label>
            <Input id="host-user" className={mono} value={user} onChange={(e) => setUser(e.target.value)} />
          </div>
        )}
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="host-port">{m.hostForm_port()}</Label>
          <Input
            id="host-port"
            inputMode="numeric"
            pattern="[0-9]*"
            className={mono}
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder="22"
          />
        </div>
      </div>
      <FieldHint>{server ? m.hostForm_serverAliasHint() : m.hostForm_accountAliasHint()}</FieldHint>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          {m.common_cancel()}
        </Button>
        <Button size="sm" type="submit" disabled={saving}>
          {saving && <Spinner className="size-3" />}
          {server ? m.hostForm_saveServer() : m.hostForm_saveAccount()}
        </Button>
      </div>
    </form>
  );
}

/** Git uses the identity of ~/.gitconfig wherever a key sets none. */
function useGlobalIdentity() {
  return useProfiles().data?.global_identity ?? { name: null, email: null };
}

function formatIdentity(id: { name: string | null; email: string | null }): string | null {
  if (!id.name && !id.email) return null;
  return [id.name, id.email && `<${id.email}>`].filter(Boolean).join(" ");
}

/** A key without git folders has no identity of its own. */
function GlobalIdentityCard() {
  const global = formatIdentity(useGlobalIdentity());
  return (
    <Card aria-labelledby="git-title">
      <CardTitle id="git-title">{m.which_identity()}</CardTitle>
      <CardDescription>{global ? m.identity_globalApplies() : m.identity_globalMissing()}</CardDescription>
      {global && <span className="font-mono text-[13px]">{global}</span>}
      <FieldHint>{m.identity_ownViaFolder()}</FieldHint>
    </Card>
  );
}

/** Remounted (via `key`) whenever the saved identity changes, so the fields never show stale values. */
function GitCard({ profile }: { profile: Profile }) {
  const update = useUpdateProfile(profile.id);
  const global = useGlobalIdentity();
  const git = profile.git ?? { name: "", email: "", sign: false };
  const [name, setName] = useState(git.name);
  const [email, setEmail] = useState(git.email);
  const dirty = name !== git.name || email !== git.email;
  const emailOk = email === "" || isValidEmail(email);
  // Nothing of its own: the key simply follows ~/.gitconfig.
  const orNull = (g: { name: string; email: string; sign: boolean }) => (g.name || g.email || g.sign ? g : null);
  const save = () =>
    update.mutate((p) => ({
      ...p,
      git: orNull({ name: name.trim(), email: email.trim(), sign: p.git?.sign ?? false }),
    }));
  // The switch changes only signing, never half-typed name or e-mail.
  const setSign = (sign: boolean) =>
    update.mutate((p) => ({ ...p, git: orNull({ ...(p.git ?? { name: "", email: "" }), sign }) }));
  return (
    <Card aria-labelledby="git-title">
      <CardTitle id="git-title">{m.which_identity()}</CardTitle>
      <CardDescription>{m.identity_intro()}</CardDescription>
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
              {m.newKey_name()}
            </Label>
            <Input
              id="git-name"
              value={name}
              placeholder={global.name ?? ""}
              onChange={(e) => setName(e.target.value)}
              disabled={profile.archived}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="git-mail" className="text-xs font-normal text-muted-foreground">
              {m.identity_email()}
            </Label>
            <Input
              id="git-mail"
              type="email"
              value={email}
              placeholder={global.email ?? ""}
              aria-invalid={!emailOk}
              onChange={(e) => setEmail(e.target.value)}
              disabled={profile.archived}
            />
          </div>
        </div>
        <div className="flex items-start justify-between gap-3">
          <ErrorSlot message={!emailOk && m.identity_invalidEmail()} />
          <Button size="sm" type="submit" disabled={!dirty || !emailOk || update.isPending || profile.archived}>
            {m.identity_save()}
          </Button>
        </div>
      </form>
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-col gap-0.5">
          <span id="sign-label" className="text-[13px] font-medium">
            {m.identity_sign()}
          </span>
          <span className="text-xs text-muted-foreground">
            {m.identity_signHint()}
            {!git.email && ` ${m.identity_signNeedsEmail()}`}
          </span>
        </div>
        <Switch
          aria-labelledby="sign-label"
          checked={git.sign}
          disabled={profile.archived || update.isPending || (!git.email && !git.sign)}
          onCheckedChange={setSign}
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
      <CardTitle id="manage-title">{m.manage_title()}</CardTitle>
      <CardDescription>{m.manage_intro()}</CardDescription>
      <div className="flex flex-wrap gap-2">
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="destructive">
              <Archive aria-hidden />
              {m.manage_archive()}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogTitle>{m.manage_confirmTitle({ name: profile.name })}</AlertDialogTitle>
            <AlertDialogDescription>{m.manage_confirmText()}</AlertDialogDescription>
            <AlertDialogFooter>
              <AlertDialogCancel>{m.common_cancel()}</AlertDialogCancel>
              <AlertDialogAction
                onClick={() =>
                  archive.mutate(profile.id, {
                    onSuccess: (res) => {
                      toast.success(m.manage_archived({ name: profile.name }), {
                        description: res.remove_key_urls.length
                          ? m.manage_removeAtProvider({ url: res.remove_key_urls[0] })
                          : undefined,
                      });
                      navigate({ to: "/" });
                    },
                  })
                }
              >
                {m.manage_archive()}
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
  if (path.endsWith("keybund.conf")) return m.config_explainSsh();
  if (path.endsWith("index.gitconfig")) return m.config_explainIndex();
  if (path.endsWith(".gitconfig")) return m.config_explainKey();
  return "";
}

function GeneratedConfig({ id, archived }: { id: string; archived: boolean }) {
  const { data, isPending } = useGeneratedConfig(id);
  if (isPending) return <Spinner />;
  if (archived) {
    return <p className="m-0 max-w-[760px] text-[13px] leading-normal text-muted-foreground">{m.config_archived()}</p>;
  }
  return (
    <div className="flex flex-col gap-4">
      <p className="m-0 max-w-[760px] text-[13px] leading-normal text-muted-foreground">{m.config_intro()}</p>
      {data?.map((f) => (
        <section key={f.path} className="overflow-hidden rounded-xl border border-border bg-card">
          <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="font-mono text-[12.5px] text-secondary-foreground">{f.path}</span>
              <span className="text-xs text-muted-foreground">{explainFile(f.path)}</span>
            </span>
            <span className="shrink-0 text-xs text-muted-foreground">
              {f.excerpt ? m.config_excerpt() : m.config_managed()}
            </span>
          </div>
          <pre className="m-0 overflow-x-auto p-4 font-mono text-[13px] leading-[1.7] text-secondary-foreground">
            {f.content.trimEnd()}
          </pre>
        </section>
      ))}
    </div>
  );
}
