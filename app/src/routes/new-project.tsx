import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowLeft, ArrowRight, Check, ExternalLink, FolderOpen, GitBranch, Server } from "lucide-react";
import { useState } from "react";
import { PageFrame } from "@/components/app-shell";
import { ColorDot } from "@/components/color-dot";
import { DiffView } from "@/components/diff-view";
import { KeyUpload } from "@/components/key-upload";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorSlot, FieldError, FieldHint, Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { api, errorMessage } from "@/lib/api";
import type { CreatedProject, HostEntry, NewProject, ProfileDefaults, Provider } from "@/lib/bindings";
import { openExternal } from "@/lib/open";
import { useAddHost, useCreateProject, useInstallKey, useProfiles, useScanKeys, useTestConnection } from "@/lib/queries";
import { pickFolder } from "@/lib/tauri";
import { cn, COLOR_NAMES, defaultHostname, GIT_PROVIDERS, isValidEmail, PROFILE_COLORS, providerLabel, slugify } from "@/lib/utils";

const mono = "font-mono text-[13px]";

/** Providers whose API Keybund uses to add the key. */
const API_PROVIDERS: Provider[] = ["github", "gitlab"];

export type Kind = "git" | "server";
export type StepId = "kind" | "project" | "account" | "folder" | "git" | "server" | "key" | "test";

const LABELS: Record<StepId, string> = {
  kind: "Wofür?",
  project: "Projekt",
  account: "Git-Konto",
  folder: "Ordner",
  git: "Git-Identität",
  server: "Server",
  key: "SSH-Key",
  test: "Eintragen & testen",
};

/** The steps of each flow: Git projects are chosen by folder, servers by host. */
export function stepsFor(kind: Kind | null): StepId[] {
  return kind === "server"
    ? ["kind", "server", "key", "test"]
    : ["kind", "project", "account", "folder", "git", "key", "test"];
}

export interface Draft {
  kind: Kind | null;
  name: string;
  color: string;
  makeDefault: boolean;
  provider: Provider;
  hostname: string;
  alias: string;
  user: string;
  port: string;
  folder: string;
  /** Set once the user changed the folder; until then it follows the name. */
  folderEdited: boolean;
  gitName: string;
  gitEmail: string;
  sign: boolean;
  /** New key, a key file from ~/.ssh, or (servers) the key of an existing profile. */
  keyMode: "generate" | "existing" | "profile";
  existingPath: string;
  profileId: string;
  comment: string;
  /** Set once the user changed the key comment; until then it follows the name. */
  commentEdited: boolean;
  passphrase: string;
}

export function emptyDraft(existingKey?: string): Draft {
  return {
    kind: null,
    name: "",
    color: "blue",
    makeDefault: false,
    provider: "github",
    hostname: "github.com",
    alias: "",
    user: "",
    port: "",
    folder: "",
    folderEdited: false,
    gitName: "",
    gitEmail: "",
    sign: false,
    keyMode: existingKey ? "existing" : "generate",
    existingPath: existingKey ?? "",
    profileId: "",
    comment: "",
    commentEdited: false,
    passphrase: "",
  };
}

/** Name of the profile: for a server the name, else its alias or hostname. */
export function profileName(d: Draft): string {
  return d.kind === "server" ? d.name.trim() || d.alias.trim() || d.hostname.trim() : d.name.trim();
}

/**
 * Fills the values derived from the name (folder, key comment) unless the
 * user edited them, so renaming the project in step 1 updates them too.
 */
export function withDefaults(d: Draft, defaults?: ProfileDefaults | null): Draft {
  const id = slugify(profileName(d));
  const fresh = defaults && defaults.id === id ? defaults : null;
  return {
    ...d,
    folder: d.kind === "server" ? "" : d.folderEdited ? d.folder : id ? (fresh?.folder ?? `~/dev/${id}/`) : "",
    comment: d.commentEdited ? d.comment : (fresh?.key_comment ?? ""),
  };
}

/** The host the wizard creates. */
export function toHost(d: Draft): HostEntry {
  const server = d.kind === "server";
  return {
    provider: server ? "generic" : d.provider,
    hostname: d.hostname.trim(),
    alias: d.alias.trim() || null,
    user: server && d.user.trim() ? d.user.trim() : null,
    port: d.port ? Number(d.port) : null,
  };
}

/** Builds the command payload from the wizard state. */
export function toProject(d: Draft): NewProject {
  const name = profileName(d);
  const id = slugify(name);
  const server = d.kind === "server";
  const keyPath = d.keyMode === "existing" ? d.existingPath : `~/.ssh/${id}_ed25519`;
  return {
    profile: {
      id,
      name,
      color: d.color,
      key: { path: keyPath, comment: null },
      git: !server && d.gitEmail.trim() ? { name: d.gitName.trim(), email: d.gitEmail.trim(), sign: d.sign } : null,
      folders: !server && d.folder.trim() ? [d.folder.trim()] : [],
      archived: false,
      hosts: [toHost(d)],
    },
    key:
      d.keyMode === "existing"
        ? { type: "existing", path: d.existingPath }
        : { type: "generate", comment: d.comment.trim(), passphrase: d.passphrase || null },
    make_default: !server && d.makeDefault,
  };
}

/** Validation message for a step, or `null` if it is complete. */
export function stepError(step: StepId, d: Draft, idTaken = false): string | null {
  const name = profileName(d);
  switch (step) {
    case "kind":
      return d.kind ? null : "Bitte wählen, wofür der Key ist.";
    case "project":
      if (!name) return "Bitte einen Projektnamen eingeben.";
      if (!slugify(name)) return "Der Name braucht mindestens einen Buchstaben oder eine Ziffer.";
      if (idTaken) return `Ein Profil „${slugify(name)}“ gibt es schon.`;
      return null;
    case "account":
    case "server":
      if (!d.hostname.trim()) return step === "server" ? "Bitte Hostname oder IP eingeben." : "Bitte einen Hostnamen eingeben.";
      if (d.port && !/^\d+$/.test(d.port)) return "Der Port muss eine Zahl sein.";
      if (step === "server" && !slugify(name)) return "Der Name braucht mindestens einen Buchstaben oder eine Ziffer.";
      return null;
    case "folder":
      return d.folder.trim() ? null : "Bitte einen Projektordner wählen.";
    case "git":
      if (!d.gitName.trim()) return "Bitte einen Git-Namen eingeben.";
      if (!isValidEmail(d.gitEmail)) return "Bitte eine gültige E-Mail-Adresse eingeben.";
      return null;
    case "key":
      if (d.keyMode === "profile") return d.profileId ? null : "Bitte ein Profil wählen.";
      if (d.kind === "server" && idTaken) return `Ein Profil „${slugify(name)}“ gibt es schon – Namen ändern oder dessen Key verwenden.`;
      if (d.keyMode === "existing" && !d.existingPath) return "Bitte einen vorhandenen Key wählen.";
      if (d.keyMode === "generate" && !d.comment.trim()) return "Bitte einen Kommentar für den Key eingeben.";
      return null;
    default:
      return null;
  }
}

function stepValue(step: StepId, d: Draft, profileLabel: string): string {
  switch (step) {
    case "kind":
      return d.kind === "server" ? "Server" : "Git";
    case "project":
      return d.name;
    case "account":
      return d.hostname;
    case "folder":
      return d.folder;
    case "git":
      return d.gitEmail;
    case "server":
      return d.alias || d.hostname;
    case "key":
      return d.keyMode === "existing" ? d.existingPath : d.keyMode === "profile" ? profileLabel : "neuer Key";
    default:
      return "";
  }
}

export function NewProjectPage({ existingKey }: { existingKey?: string }) {
  const [rawDraft, setDraft] = useState<Draft>(() => emptyDraft(existingKey));
  const [index, setIndex] = useState(0);
  const [created, setCreated] = useState<CreatedProject | null>(null);
  const set = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));
  const profiles = useProfiles();

  const name = profileName(rawDraft);
  const defaults = useQuery({
    queryKey: ["defaults", name],
    queryFn: () => api.profileDefaults(name),
    enabled: name.length > 0,
  });
  const draft = withDefaults(rawDraft, defaults.data);
  const steps = stepsFor(draft.kind);
  const step = steps[index];
  const idTaken = defaults.data?.id === slugify(name) && !!defaults.data?.id_taken;
  const error = stepError(step, draft, idTaken);
  const [showError, setShowError] = useState(false);
  const lastIndex = steps.length - 1;
  const keyIndex = steps.indexOf("key");
  const profileLabel = profiles.data?.profiles.find((p) => p.profile.id === draft.profileId)?.profile.name ?? "";

  const next = () => {
    if (error) {
      setShowError(true);
      return;
    }
    setShowError(false);
    setIndex((i) => i + 1);
  };
  const back = () => {
    setShowError(false);
    setIndex((i) => Math.max(0, i - 1));
  };

  return (
    <PageFrame
      back={
        <Link to="/" className="flex min-h-9 items-center gap-2 text-[13px] text-secondary-foreground no-underline hover:text-foreground">
          <ArrowLeft aria-hidden className="size-[15px]" />
          Zurück zu den Profilen
        </Link>
      }
      aside={
        <span className="text-[13px] text-muted-foreground">
          Schritt {index + 1} von {steps.length}
        </span>
      }
    >
      <div className="flex w-full max-w-[980px] flex-wrap items-start gap-8">
        <ol aria-label="Schritte" className="m-0 flex flex-[1_1_220px] list-none flex-col gap-1 p-0">
          {steps.map((id, i) => {
            const done = i < index;
            const current = i === index;
            return (
              <li
                key={id}
                aria-current={current ? "step" : undefined}
                className={cn(
                  "flex min-h-10 items-center gap-3 text-[13px] text-muted-foreground",
                  current && "-mx-2.5 rounded-lg bg-muted px-2.5 font-medium text-foreground",
                )}
              >
                <span
                  className={cn(
                    "flex size-6 shrink-0 items-center justify-center rounded-full text-xs",
                    done && "bg-muted text-success",
                    current && "bg-primary font-semibold text-background",
                    !done && !current && "border border-zinc-700",
                  )}
                >
                  {done ? <Check aria-hidden className="size-[13px]" strokeWidth={3} /> : i + 1}
                </span>
                <span className="flex-1">{LABELS[id]}</span>
                {done && id !== "test" && (
                  <span className={cn("max-w-[140px] truncate text-secondary-foreground", (id === "folder" || id === "server") && "font-mono text-xs")}>
                    {stepValue(id, draft, profileLabel)}
                  </span>
                )}
              </li>
            );
          })}
        </ol>

        <section className="flex min-w-0 flex-[999_1_520px] flex-col gap-6 rounded-[14px] border border-border bg-card p-7 max-[640px]:p-5">
          {step === "kind" && <StepKind draft={draft} set={set} />}
          {step === "project" && <StepProject draft={draft} set={set} id={defaults.data?.id ?? slugify(draft.name)} />}
          {step === "account" && <StepAccount draft={draft} set={set} />}
          {step === "folder" && <StepFolder draft={draft} set={set} />}
          {step === "git" && <StepGit draft={draft} set={set} />}
          {step === "server" && <StepServer draft={draft} set={set} />}
          {step === "key" && (
            <StepKey
              draft={draft}
              set={set}
              onCreated={(c) => {
                setCreated(c);
                setIndex(lastIndex);
              }}
              canSubmit={!error}
              showError={() => setShowError(true)}
            />
          )}
          {step === "test" && created && (draft.kind === "server" ? <StepServerTest created={created} host={toHost(draft)} /> : <StepTest created={created} />)}

          {index < lastIndex && (
            <div className="flex flex-col gap-2">
              <ErrorSlot message={showError && error} />
              {index < keyIndex ? (
                <div className="flex flex-wrap justify-between gap-3">
                  <Button variant="outline" size="lg" onClick={back} disabled={index === 0}>
                    Zurück
                  </Button>
                  <Button size="lg" onClick={next}>
                    Weiter
                    <ArrowRight aria-hidden />
                  </Button>
                </div>
              ) : (
                <Button variant="outline" size="lg" className="self-start" onClick={back}>
                  Zurück
                </Button>
              )}
            </div>
          )}
        </section>
      </div>
    </PageFrame>
  );
}

type StepProps = { draft: Draft; set: (p: Partial<Draft>) => void };

function Heading({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h1 className="m-0 text-[22px] font-semibold tracking-tight">{title}</h1>
      {children && <p className="m-0 leading-normal text-muted-foreground">{children}</p>}
    </div>
  );
}

function StepKind({ draft, set }: StepProps) {
  const options: { kind: Kind; title: string; text: string; icon: React.ReactNode }[] = [
    {
      kind: "git",
      title: "Git-Projekt oder -Konto",
      text: "GitHub, GitLab, Bitbucket – der Key gilt für alle Repos in einem Ordner.",
      icon: <GitBranch aria-hidden className="size-5" />,
    },
    {
      kind: "server",
      title: "Server",
      text: "ssh, scp, rsync, sftp – der Key gilt für einen Host oder eine IP.",
      icon: <Server aria-hidden className="size-5" />,
    },
  ];
  return (
    <>
      <Heading title="Wofür ist der Key?">
        Git wählt den Key über den Ordner des Repos, ssh über den Server. Für alles andere gilt dein Standard-Profil.
      </Heading>
      <div role="radiogroup" aria-label="Wofür" className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
        {options.map((o) => (
          <button
            key={o.kind}
            role="radio"
            aria-checked={draft.kind === o.kind}
            onClick={() =>
              set(
                o.kind === "server"
                  ? { kind: "server", provider: "generic", hostname: draft.kind === "server" ? draft.hostname : "" }
                  : { kind: "git", provider: "github", hostname: "github.com", keyMode: draft.keyMode === "profile" ? "generate" : draft.keyMode },
              )
            }
            className={cn(
              "flex gap-3 rounded-[10px] border p-4 text-left",
              draft.kind === o.kind ? "border-foreground bg-muted" : "border-border bg-transparent",
            )}
          >
            <span className="mt-0.5 text-muted-foreground">{o.icon}</span>
            <span className="flex flex-col gap-1">
              <span className="font-medium">{o.title}</span>
              <span className="text-[12.5px] text-muted-foreground">{o.text}</span>
            </span>
          </button>
        ))}
      </div>
    </>
  );
}

function StepAccount({ draft, set }: StepProps) {
  return (
    <>
      <Heading title="Wo liegt der Code?">GitHub, GitLab (auch selbst gehostet) oder Bitbucket.</Heading>
      <div role="radiogroup" aria-label="Anbieter" className="grid grid-cols-[repeat(auto-fit,minmax(140px,1fr))] gap-3">
        {GIT_PROVIDERS.map((p) => (
          <button
            key={p}
            role="radio"
            aria-checked={draft.provider === p}
            onClick={() => set({ provider: p, hostname: defaultHostname(p) })}
            className={cn(
              "flex items-center gap-3 rounded-[10px] border p-4 text-left",
              draft.provider === p ? "border-foreground bg-muted" : "border-border bg-transparent",
            )}
          >
            <span className={cn("size-[18px] shrink-0 rounded-full border", draft.provider === p ? "border-[5px] border-foreground" : "border-zinc-600")} />
            <span className="font-medium">{providerLabel(p)}</span>
          </button>
        ))}
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-4">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="hostname">Hostname</Label>
          <Input id="hostname" className={mono} value={draft.hostname} placeholder="gitlab.firma.de" onChange={(e) => set({ hostname: e.target.value })} />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="port">
            Port <span className="font-normal text-muted-foreground">(optional)</span>
          </Label>
          <Input id="port" inputMode="numeric" className={mono} value={draft.port} placeholder="22" onChange={(e) => set({ port: e.target.value })} />
        </div>
      </div>
    </>
  );
}

function StepServer({ draft, set }: StepProps) {
  return (
    <>
      <Heading title="Welcher Server?">
        ssh, scp, rsync und sftp (auch in Dolphin) nehmen für diesen Server dann automatisch den richtigen Key – egal ob du
        ihn per Alias oder per IP ansprichst.
      </Heading>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-4">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="hostname">Hostname oder IP</Label>
          <Input id="hostname" autoFocus className={mono} value={draft.hostname} placeholder="192.168.1.5" onChange={(e) => set({ hostname: e.target.value })} />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="alias">
            SSH-Alias <span className="font-normal text-muted-foreground">(optional)</span>
          </Label>
          <Input id="alias" className={mono} value={draft.alias} placeholder="nas" onChange={(e) => set({ alias: e.target.value })} />
          <FieldHint>Danach reicht ssh {draft.alias || draft.hostname || "nas"} – auch in Dolphin.</FieldHint>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="user">Benutzer</Label>
          <Input id="user" className={mono} value={draft.user} onChange={(e) => set({ user: e.target.value })} />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="port">
            Port <span className="font-normal text-muted-foreground">(optional)</span>
          </Label>
          <Input id="port" inputMode="numeric" className={mono} value={draft.port} placeholder="22" onChange={(e) => set({ port: e.target.value })} />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="server-name">
            Name <span className="font-normal text-muted-foreground">(optional)</span>
          </Label>
          <Input id="server-name" value={draft.name} placeholder={draft.alias || draft.hostname || "NAS"} onChange={(e) => set({ name: e.target.value })} />
        </div>
      </div>
    </>
  );
}

function StepProject({ draft, set, id }: StepProps & { id: string }) {
  return (
    <>
      <Heading title="Neues Projekt">Jedes Projekt bekommt ein eigenes Profil mit eigenem SSH-Key.</Heading>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="project-name">Projektname</Label>
        <Input id="project-name" autoFocus value={draft.name} placeholder="Kunde C" onChange={(e) => set({ name: e.target.value })} />
        <FieldHint>
          {id ? (
            <>
              Profil-ID: <span className="font-mono">{id}</span>
            </>
          ) : (
            "Die Profil-ID wird aus dem Namen abgeleitet."
          )}
        </FieldHint>
      </div>
      <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
        <legend className="mb-2 text-[13px] font-medium">Farbe</legend>
        <div className="flex flex-wrap gap-2">
          {Object.keys(PROFILE_COLORS).map((c) => (
            <label key={c} className={cn("flex size-9 items-center justify-center rounded-lg border", draft.color === c ? "border-foreground" : "border-border")}>
              <input type="radio" name="color" value={c} checked={draft.color === c} onChange={() => set({ color: c })} className="sr-only" aria-label={COLOR_NAMES[c] ?? c} />
              <ColorDot color={c} className="size-3.5" />
            </label>
          ))}
        </div>
      </fieldset>
      <label className="flex items-center justify-between gap-3">
        <span className="flex flex-col gap-0.5">
          <span className="text-[13px] font-medium">Als Standard-Profil</span>
          <span className="text-xs text-muted-foreground">Key für Git in allen Ordnern ohne eigenes Profil</span>
        </span>
        <Switch aria-label="Als Standard-Profil" checked={draft.makeDefault} onCheckedChange={(v) => set({ makeDefault: v })} />
      </label>
    </>
  );
}

function StepFolder({ draft, set }: StepProps) {
  const generic = draft.provider === "generic";
  const browse = async () => {
    try {
      const picked = await pickFolder();
      if (picked) set({ folder: await api.normalizeFolder(picked), folderEdited: true });
    } catch {
      // Dialog not available (e.g. browser preview): keep the text field.
    }
  };
  return (
    <>
      <Heading title="Projektordner">
        Alle Repos unter diesem Ordner nutzen automatisch den Key und die Git-Identität dieses Profils
        {generic && " – für reine Server optional"}.
      </Heading>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="folder">Ordner</Label>
        <div className="flex gap-2">
          <Input id="folder" className={mono} value={draft.folder} placeholder="~/dev/projekt/" onChange={(e) => set({ folder: e.target.value, folderEdited: true })} />
          <Button variant="outline" onClick={browse}>
            <FolderOpen aria-hidden />
            Durchsuchen
          </Button>
        </div>
        <FieldHint>Wird angelegt, falls er noch nicht existiert. Unterordner dürfen nicht zu einem anderen Profil gehören.</FieldHint>
      </div>
    </>
  );
}

function StepGit({ draft, set }: StepProps) {
  const generic = draft.provider === "generic";
  return (
    <>
      <Heading title="Git-Identität">
        Name und E-Mail für Commits in diesem Projekt{generic && " – leer lassen, wenn der Server kein Git-Remote ist"}.
      </Heading>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-4">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="git-name">Name</Label>
          <Input id="git-name" value={draft.gitName} onChange={(e) => set({ gitName: e.target.value })} />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="git-email">E-Mail</Label>
          <Input id="git-email" type="email" value={draft.gitEmail} onChange={(e) => set({ gitEmail: e.target.value })} />
        </div>
      </div>
      <label className="flex items-center justify-between gap-3">
        <span className="flex flex-col gap-0.5">
          <span className="text-[13px] font-medium">Commits signieren</span>
          <span className="text-xs text-muted-foreground">mit dem SSH-Key dieses Profils (gpg.format = ssh)</span>
        </span>
        <Switch aria-label="Commits signieren" checked={draft.sign} onCheckedChange={(v) => set({ sign: v })} />
      </label>
    </>
  );
}

function StepKey({
  draft,
  set,
  onCreated,
  canSubmit,
  showError,
}: StepProps & { onCreated: (c: CreatedProject) => void; canSubmit: boolean; showError: () => void }) {
  const server = draft.kind === "server";
  const project = toProject(draft);
  const host = toHost(draft);
  const scan = useScanKeys();
  const profiles = useProfiles();
  const unused = scan.data?.filter((k) => !k.used_by) ?? [];
  const active = profiles.data?.profiles.filter((p) => !p.profile.archived) ?? [];
  const viaProfile = draft.keyMode === "profile";
  const preview = useQuery({
    queryKey: viaProfile
      ? ["preview-host", draft.profileId, JSON.stringify(host)]
      : ["preview", JSON.stringify({ ...project, key: { ...project.key, passphrase: null } })],
    queryFn: () => (viaProfile ? api.previewAddHost(draft.profileId, host) : api.previewNewProject(project)),
    enabled: canSubmit,
  });
  const create = useCreateProject();
  const addHost = useAddHost();
  const pending = create.isPending || addHost.isPending;
  const failure = create.error ?? addHost.error;
  const submit = () => {
    if (!canSubmit) return showError();
    if (viaProfile) {
      addHost.mutate(
        { id: draft.profileId, host },
        {
          onSuccess: (view) =>
            onCreated({
              profile: view,
              report: { written: [], deleted: [], backups: [] },
              public_key: view.key.public_key ?? "",
              open_error: null,
            }),
        },
      );
    } else {
      // GitHub and GitLab get the key via their API in the next step; for
      // the others the page to add it by hand opens right away.
      create.mutate({ project, openPage: !server && !API_PROVIDERS.includes(draft.provider) }, { onSuccess: onCreated });
    }
  };
  const provider = providerLabel(draft.provider);
  const modes: { mode: Draft["keyMode"]; title: string; text: string }[] = [
    { mode: "generate", title: "Neuen Key erzeugen", text: `ed25519 · ${project.profile.key.path}` },
    ...(server && active.length > 0 ? [{ mode: "profile" as const, title: "Key eines Profils verwenden", text: "z. B. dein privater Key" }] : []),
    { mode: "existing", title: "Vorhandenen Key verwenden", text: "aus ~/.ssh importieren" },
  ];

  return (
    <>
      <Heading title={server ? `SSH-Key für ${profileName(draft)}` : `SSH-Key für ${draft.name.trim()}`}>
        {server
          ? "Ein eigener Key pro Server – oder ein Key, den du schon hast. Danach trägt Keybund ihn auf dem Server ein."
          : API_PROVIDERS.includes(draft.provider)
            ? `Jedes Projekt bekommt seinen eigenen Key. Danach trägt Keybund ihn in dein ${provider}-Konto ein – du meldest dich dafür nur im Browser an.`
            : `Jedes Projekt bekommt seinen eigenen Key. Nach dem Anwenden liegt der Pubkey in der Zwischenablage und ${provider} öffnet sich direkt auf der richtigen Seite.`}
      </Heading>

      <div role="radiogroup" aria-label="Key" className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3">
        {modes.map(({ mode, title, text }) => (
          <button
            key={mode}
            role="radio"
            aria-checked={draft.keyMode === mode}
            onClick={() => set({ keyMode: mode })}
            className={cn(
              "flex gap-3 rounded-[10px] border p-4 text-left",
              draft.keyMode === mode ? "border-foreground bg-muted" : "border-border bg-transparent",
            )}
          >
            <span className={cn("mt-px size-[18px] shrink-0 rounded-full border", draft.keyMode === mode ? "border-[5px] border-foreground" : "border-zinc-600")} />
            <span className="flex flex-col gap-1">
              <span className="font-medium">{title}</span>
              <span className={cn("text-[12.5px] text-muted-foreground", mode === "generate" && "font-mono")}>{text}</span>
            </span>
          </button>
        ))}
      </div>

      {draft.keyMode === "generate" && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="key-comment">Kommentar</Label>
            <Input id="key-comment" className={mono} value={draft.comment} onChange={(e) => set({ comment: e.target.value, commentEdited: true })} />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="key-pass">
              Passphrase <span className="font-normal text-muted-foreground">(optional)</span>
            </Label>
            <Input id="key-pass" type="password" autoComplete="new-password" value={draft.passphrase} onChange={(e) => set({ passphrase: e.target.value })} />
            <FieldHint>Wird beim ersten Gebrauch einmal abgefragt, danach hält der Agent den Key.</FieldHint>
          </div>
        </div>
      )}
      {draft.keyMode === "profile" && (
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="key-profile">Profil</Label>
          <Select id="key-profile" value={draft.profileId} onChange={(e) => set({ profileId: e.target.value })}>
            <option value="">Bitte wählen …</option>
            {active.map((p) => (
              <option key={p.profile.id} value={p.profile.id}>
                {p.profile.name} · {p.profile.key.path}
              </option>
            ))}
          </Select>
          <FieldHint>Der Server wird diesem Profil zugeordnet – kein neues Profil, kein neuer Key.</FieldHint>
        </div>
      )}
      {draft.keyMode === "existing" && (
        <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
          <legend className="mb-2 text-[13px] font-medium">Key aus ~/.ssh</legend>
          {scan.isPending && <Spinner />}
          {scan.isSuccess && unused.length === 0 && <FieldHint>Keine ungenutzten Keys gefunden.</FieldHint>}
          {unused.map((k) => (
            <label key={k.key.path} className={cn("flex items-center gap-3 rounded-lg border p-3", draft.existingPath === k.key.path ? "border-foreground" : "border-border")}>
              <input type="radio" name="existing-key" checked={draft.existingPath === k.key.path} onChange={() => set({ existingPath: k.key.path })} />
              <span className="flex min-w-0 flex-col gap-0.5">
                <span className={mono}>{k.key.path}</span>
                <span className="truncate font-mono text-xs text-muted-foreground">{k.key.fingerprint}</span>
              </span>
            </label>
          ))}
        </fieldset>
      )}

      {preview.isPending && canSubmit && <Spinner label="Berechne Vorschau" />}
      {preview.isError && <FieldError>{errorMessage(preview.error)}</FieldError>}
      {preview.data && <DiffView changes={preview.data.changes} />}
      {failure && <FieldError>{errorMessage(failure)}</FieldError>}

      <Button size="lg" className="self-end" onClick={submit} disabled={pending || preview.isError}>
        {pending && <Spinner className="size-3.5" />}
        {server ? "Anwenden" : "Anwenden & Pubkey kopieren"}
        <ArrowRight aria-hidden />
      </Button>
    </>
  );
}

function StepTest({ created }: { created: CreatedProject }) {
  const test = useTestConnection();
  const { profile } = created.profile;
  const link = created.profile.add_key_urls[0];
  const viaApi = !!link && API_PROVIDERS.includes(link.provider);
  const result = test.data;
  return (
    <>
      <Heading title="Key eintragen & testen">
        {viaApi
          ? `Keybund trägt den Key in dein ${providerLabel(link.provider)}-Konto ein. Oder von Hand: Der Pubkey liegt schon in der Zwischenablage.`
          : `Der Pubkey liegt in der Zwischenablage. Füge ihn bei ${link ? providerLabel(link.provider) : "deinem Anbieter"} ein und klicke dann auf „Ich hab ihn eingetragen“.`}
      </Heading>
      {viaApi && (
        <section aria-label="Automatisch eintragen" className="flex flex-col gap-3">
          <KeyUpload profile={profile} host={link.host} manual={false} onUploaded={() => test.mutate({ id: profile.id })} />
        </section>
      )}
      <section aria-label="Von Hand eintragen" className="flex flex-col gap-3">
        {viaApi && <span className="text-[13px] font-medium">Von Hand</span>}
        <div className="flex items-center gap-2 rounded-lg border border-border bg-background py-1 pr-1 pl-3.5">
          <code className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-secondary-foreground">{created.public_key}</code>
        </div>
        {link && created.open_error && (
          <div role="alert" className="flex flex-col gap-1 rounded-[10px] border border-amber-500/35 bg-amber-500/5 p-3.5 text-[13px]">
            <span className="font-medium">Die {providerLabel(link.provider)}-Seite hat sich nicht geöffnet</span>
            <span className="text-muted-foreground">{created.open_error}</span>
            <span className="font-mono text-xs break-all text-secondary-foreground">{link.url}</span>
          </div>
        )}
        <div className="flex flex-wrap gap-2">
          {link && (
            <Button variant="outline" onClick={() => openExternal(link.url)}>
              <ExternalLink aria-hidden />
              {providerLabel(link.provider)}-Seite öffnen
            </Button>
          )}
          <Button variant={viaApi ? "outline" : "default"} onClick={() => test.mutate({ id: profile.id })} disabled={test.isPending}>
            {test.isPending && <Spinner className="size-3.5" />}
            Ich hab ihn eingetragen
          </Button>
        </div>
      </section>
      <div aria-live="polite">
        {result?.success && (
          <div className="flex flex-col gap-3 rounded-[10px] border border-emerald-500/30 bg-emerald-500/5 p-4">
            <span className="font-medium text-success">{result.message}</span>
            <span className="text-[13px] text-muted-foreground">Fertig – {profile.name} ist eingerichtet.</span>
            <Link to="/profile/$id" params={{ id: profile.id }} className="self-start text-[13px] font-medium text-foreground">
              Zum Profil
            </Link>
          </div>
        )}
        {result && !result.success && (
          <div className="flex flex-col gap-2 rounded-[10px] border border-red-400/30 bg-red-500/5 p-4">
            <span className="font-medium text-danger">{result.message}</span>
            <span className="text-[13px] text-muted-foreground">Prüfe, ob der Key gespeichert ist, und teste erneut.</span>
          </div>
        )}
        {test.isError && <FieldError>{errorMessage(test.error)}</FieldError>}
      </div>
      {!result?.success && (
        <Link to="/profile/$id" params={{ id: profile.id }} className="text-[13px] text-muted-foreground">
          Später testen
        </Link>
      )}
    </>
  );
}

function StepServerTest({ created, host }: { created: CreatedProject; host: HostEntry }) {
  const install = useInstallKey();
  const test = useTestConnection();
  const { profile } = created.profile;
  const name = host.alias ?? host.hostname;
  const installed = install.data;
  const result = test.data;
  return (
    <>
      <Heading title="Key auf dem Server eintragen">
        Keybund meldet sich an {name} an – mit deinem Passwort oder einem Key, der dort schon eingetragen ist – und trägt den
        Key von „{profile.name}“ in ~/.ssh/authorized_keys ein.
      </Heading>
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => install.mutate({ id: profile.id, host: name })} disabled={install.isPending}>
          {install.isPending && <Spinner className="size-3.5" />}
          Key auf {name} eintragen
        </Button>
        <Button variant="outline" onClick={() => test.mutate({ id: profile.id, host: name })} disabled={test.isPending}>
          {test.isPending && <Spinner className="size-3.5" />}
          Verbindung testen
        </Button>
      </div>
      <div aria-live="polite" className="flex flex-col gap-3">
        {installed && (
          <div className={cn("rounded-[10px] border p-4", installed.success ? "border-emerald-500/30 bg-emerald-500/5" : "border-red-400/30 bg-red-500/5")}>
            <span className={cn("font-medium", installed.success ? "text-success" : "text-danger")}>{installed.message}</span>
          </div>
        )}
        {install.isError && <FieldError>{errorMessage(install.error)}</FieldError>}
        {result && (
          <div className={cn("flex flex-col gap-2 rounded-[10px] border p-4", result.success ? "border-emerald-500/30 bg-emerald-500/5" : "border-red-400/30 bg-red-500/5")}>
            <span className={cn("font-medium", result.success ? "text-success" : "text-danger")}>{result.message}</span>
            {result.success && <span className="text-[13px] text-muted-foreground">Fertig – ssh {name} nimmt jetzt diesen Key.</span>}
          </div>
        )}
        {test.isError && <FieldError>{errorMessage(test.error)}</FieldError>}
      </div>
      <Link to="/profile/$id" params={{ id: profile.id }} className="text-[13px] font-medium text-foreground">
        Zum Profil
      </Link>
    </>
  );
}
