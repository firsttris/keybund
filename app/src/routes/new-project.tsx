import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowLeft, ArrowRight, Check, ExternalLink, FolderOpen } from "lucide-react";
import { useState } from "react";
import { PageFrame } from "@/components/app-shell";
import { ColorDot } from "@/components/color-dot";
import { DiffView } from "@/components/diff-view";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorSlot, FieldError, FieldHint, Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { api, errorMessage } from "@/lib/api";
import type { CreatedProject, NewProject, ProfileDefaults, Provider } from "@/lib/bindings";
import { openExternal } from "@/lib/open";
import { useCreateProject, useScanKeys, useTestConnection } from "@/lib/queries";
import { pickFolder } from "@/lib/tauri";
import { cn, COLOR_NAMES, defaultHostname, isValidEmail, PROFILE_COLORS, PROVIDERS, providerLabel, slugify } from "@/lib/utils";

const STEPS = ["Projekt", "Host", "Ordner", "Git-Identität", "SSH-Key", "Eintragen & testen"] as const;
const mono = "font-mono text-[13px]";

export interface Draft {
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
  keyMode: "generate" | "existing";
  existingPath: string;
  comment: string;
  /** Set once the user changed the key comment; until then it follows the name. */
  commentEdited: boolean;
  passphrase: string;
}

export function emptyDraft(existingKey?: string): Draft {
  return {
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
    comment: "",
    commentEdited: false,
    passphrase: "",
  };
}

/**
 * Fills the values derived from the name (folder, key comment) unless the
 * user edited them, so renaming the project in step 1 updates them too.
 */
export function withDefaults(d: Draft, defaults?: ProfileDefaults | null): Draft {
  const id = slugify(d.name);
  const fresh = defaults && defaults.id === id ? defaults : null;
  return {
    ...d,
    folder: d.folderEdited ? d.folder : id ? (fresh?.folder ?? `~/dev/${id}/`) : "",
    comment: d.commentEdited ? d.comment : (fresh?.key_comment ?? ""),
  };
}

/** Builds the command payload from the wizard state. */
export function toProject(d: Draft): NewProject {
  const id = slugify(d.name);
  const generic = d.provider === "generic";
  const keyPath = d.keyMode === "existing" ? d.existingPath : `~/.ssh/${id}_ed25519`;
  return {
    profile: {
      id,
      name: d.name.trim(),
      color: d.color,
      key: { path: keyPath, comment: null },
      git: d.gitEmail.trim() ? { name: d.gitName.trim(), email: d.gitEmail.trim(), sign: d.sign } : null,
      folders: d.folder.trim() ? [d.folder.trim()] : [],
      archived: false,
      hosts: [
        {
          provider: d.provider,
          hostname: d.hostname.trim(),
          alias: generic && d.alias.trim() ? d.alias.trim() : null,
          user: generic && d.user.trim() ? d.user.trim() : null,
          port: d.port ? Number(d.port) : null,
        },
      ],
    },
    key:
      d.keyMode === "existing"
        ? { type: "existing", path: d.existingPath }
        : { type: "generate", comment: d.comment.trim(), passphrase: d.passphrase || null },
    make_default: d.makeDefault,
  };
}

/** Validation message for a step, or `null` if it is complete. */
export function stepError(step: number, d: Draft, idTaken = false): string | null {
  const generic = d.provider === "generic";
  switch (step) {
    case 0:
      if (!d.name.trim()) return "Bitte einen Projektnamen eingeben.";
      if (!slugify(d.name)) return "Der Name braucht mindestens einen Buchstaben oder eine Ziffer.";
      if (idTaken) return `Ein Profil „${slugify(d.name)}“ gibt es schon.`;
      return null;
    case 1:
      if (!d.hostname.trim()) return "Bitte einen Hostnamen eingeben.";
      if (generic && !d.alias.trim()) return "Server brauchen einen Alias, z. B. meinserver.";
      if (d.port && !/^\d+$/.test(d.port)) return "Der Port muss eine Zahl sein.";
      return null;
    case 2:
      if (!generic && !d.folder.trim()) return "Bitte einen Projektordner wählen.";
      return null;
    case 3:
      if (generic && !d.gitEmail.trim()) return null;
      if (!d.gitName.trim()) return "Bitte einen Git-Namen eingeben.";
      if (!isValidEmail(d.gitEmail)) return "Bitte eine gültige E-Mail-Adresse eingeben.";
      return null;
    case 4:
      if (d.keyMode === "existing" && !d.existingPath) return "Bitte einen vorhandenen Key wählen.";
      if (d.keyMode === "generate" && !d.comment.trim()) return "Bitte einen Kommentar für den Key eingeben.";
      return null;
    default:
      return null;
  }
}

function stepValue(step: number, d: Draft): string {
  return [d.name, d.hostname, d.folder, d.gitEmail, d.keyMode === "existing" ? d.existingPath : "neuer Key", ""][step] ?? "";
}

export function NewProjectPage({ existingKey }: { existingKey?: string }) {
  const [rawDraft, setDraft] = useState<Draft>(() => emptyDraft(existingKey));
  const [step, setStep] = useState(0);
  const [created, setCreated] = useState<CreatedProject | null>(null);
  const set = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));

  const defaults = useQuery({
    queryKey: ["defaults", rawDraft.name],
    queryFn: () => api.profileDefaults(rawDraft.name),
    enabled: rawDraft.name.trim().length > 0,
  });
  const draft = withDefaults(rawDraft, defaults.data);
  const error = stepError(step, draft, defaults.data?.id_taken);
  const [showError, setShowError] = useState(false);

  const next = () => {
    if (error) {
      setShowError(true);
      return;
    }
    setShowError(false);
    setStep((s) => s + 1);
  };
  const back = () => {
    setShowError(false);
    setStep((s) => Math.max(0, s - 1));
  };

  return (
    <PageFrame
      back={
        <Link to="/" className="flex min-h-9 items-center gap-2 text-[13px] text-secondary-foreground no-underline hover:text-foreground">
          <ArrowLeft aria-hidden className="size-[15px]" />
          Zurück zu den Profilen
        </Link>
      }
      aside={<span className="text-[13px] text-muted-foreground">Schritt {step + 1} von {STEPS.length}</span>}
    >
      <div className="flex w-full max-w-[980px] flex-wrap items-start gap-8">
        <ol aria-label="Schritte" className="m-0 flex flex-[1_1_220px] list-none flex-col gap-1 p-0">
          {STEPS.map((label, i) => {
            const done = i < step;
            const current = i === step;
            return (
              <li
                key={label}
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
                <span className="flex-1">{label}</span>
                {done && i < 4 && <span className={cn("max-w-[140px] truncate text-secondary-foreground", i === 2 && "font-mono text-xs")}>{stepValue(i, draft)}</span>}
              </li>
            );
          })}
        </ol>

        <section className="flex min-w-0 flex-[999_1_520px] flex-col gap-6 rounded-[14px] border border-border bg-card p-7 max-[640px]:p-5">
          {step === 0 && <StepProject draft={draft} set={set} id={defaults.data?.id ?? slugify(draft.name)} />}
          {step === 1 && <StepHost draft={draft} set={set} />}
          {step === 2 && <StepFolder draft={draft} set={set} />}
          {step === 3 && <StepGit draft={draft} set={set} />}
          {step === 4 && <StepKey draft={draft} set={set} onCreated={(c) => {
                setCreated(c);
                setStep(5);
              }} canSubmit={!error} showError={() => setShowError(true)} />}
          {step === 5 && created && <StepTest created={created} />}

          {step < 5 && (
            <div className="flex flex-col gap-2">
              <ErrorSlot message={showError && error} />
              {step < 4 ? (
                <div className="flex flex-wrap justify-between gap-3">
                  <Button variant="outline" size="lg" onClick={back} disabled={step === 0}>
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
          <span className="text-[13px] font-medium">Als Default-Profil</span>
          <span className="text-xs text-muted-foreground">Key für alles außerhalb der Projektordner</span>
        </span>
        <Switch aria-label="Als Default-Profil" checked={draft.makeDefault} onCheckedChange={(v) => set({ makeDefault: v })} />
      </label>
    </>
  );
}

function StepHost({ draft, set }: StepProps) {
  const generic = draft.provider === "generic";
  return (
    <>
      <Heading title="Wo liegt der Code?">GitHub, GitLab (auch selbst gehostet), Bitbucket oder ein eigener Server.</Heading>
      <div role="radiogroup" aria-label="Provider" className="grid grid-cols-[repeat(auto-fit,minmax(140px,1fr))] gap-3">
        {PROVIDERS.map((p) => (
          <button
            key={p}
            role="radio"
            aria-checked={draft.provider === p}
            onClick={() => set({ provider: p, hostname: defaultHostname(p) || (draft.provider === "generic" ? draft.hostname : "") })}
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
          <Input id="hostname" className={mono} value={draft.hostname} placeholder={generic ? "1.2.3.4" : "gitlab.firma.de"} onChange={(e) => set({ hostname: e.target.value })} />
        </div>
        {generic && (
          <>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="alias">SSH-Alias</Label>
              <Input id="alias" className={mono} value={draft.alias} placeholder="meinserver" onChange={(e) => set({ alias: e.target.value })} />
              <FieldHint>Danach reicht ssh {draft.alias || "meinserver"} – auch in Dolphin.</FieldHint>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="user">Benutzer</Label>
              <Input id="user" className={mono} value={draft.user} onChange={(e) => set({ user: e.target.value })} />
            </div>
          </>
        )}
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="port">Port <span className="font-normal text-muted-foreground">(optional)</span></Label>
          <Input id="port" inputMode="numeric" className={mono} value={draft.port} placeholder="22" onChange={(e) => set({ port: e.target.value })} />
        </div>
      </div>
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
  const project = toProject(draft);
  const scan = useScanKeys();
  const unused = scan.data?.filter((k) => !k.used_by) ?? [];
  const preview = useQuery({
    queryKey: ["preview", JSON.stringify({ ...project, key: { ...project.key, passphrase: null } })],
    queryFn: () => api.previewNewProject(project),
    enabled: canSubmit,
  });
  const create = useCreateProject();
  const submit = () => {
    if (!canSubmit) return showError();
    create.mutate({ project, openPage: true }, { onSuccess: onCreated });
  };
  const provider = providerLabel(draft.provider);

  return (
    <>
      <Heading title={`SSH-Key für ${draft.name.trim()}`}>
        Jedes Projekt bekommt seinen eigenen Key. Nach dem Anwenden liegt der Pubkey in der Zwischenablage
        {draft.provider !== "generic" && ` und ${provider} öffnet sich direkt auf der richtigen Seite`}.
      </Heading>

      <div role="radiogroup" aria-label="Key" className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
        {(["generate", "existing"] as const).map((mode) => (
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
              <span className="font-medium">{mode === "generate" ? "Neuen Key erzeugen" : "Vorhandenen Key verwenden"}</span>
              <span className={cn("text-[12.5px] text-muted-foreground", mode === "generate" && "font-mono")}>
                {mode === "generate" ? `ed25519 · ${project.profile.key.path}` : "aus ~/.ssh importieren"}
              </span>
            </span>
          </button>
        ))}
      </div>

      {draft.keyMode === "generate" ? (
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
      ) : (
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
      {create.isError && <FieldError>{errorMessage(create.error)}</FieldError>}

      <Button size="lg" className="self-end" onClick={submit} disabled={create.isPending || preview.isError}>
        {create.isPending && <Spinner className="size-3.5" />}
        Anwenden &amp; Pubkey kopieren
        <ArrowRight aria-hidden />
      </Button>
    </>
  );
}

function StepTest({ created }: { created: CreatedProject }) {
  const test = useTestConnection();
  const { profile } = created.profile;
  const link = created.profile.add_key_urls[0];
  const result = test.data;
  return (
    <>
      <Heading title="Key eintragen & testen">
        Der Pubkey liegt in der Zwischenablage.{" "}
        {link ? `Füge ihn bei ${providerLabel(link.provider)} ein und klicke dann auf „Ich hab ihn eingetragen“.` : "Trage ihn auf dem Server in ~/.ssh/authorized_keys ein (z. B. mit ssh-copy-id)."}
      </Heading>
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
        <Button onClick={() => test.mutate(profile.id)} disabled={test.isPending}>
          {test.isPending && <Spinner className="size-3.5" />}
          Ich hab ihn eingetragen
        </Button>
      </div>
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
