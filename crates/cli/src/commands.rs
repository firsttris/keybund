use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Password, Select};
use keybund_core::apply::{self, ChangeKind, Plan};
use keybund_core::doctor::{self, Status};
use keybund_core::keygen::{self, GenerateOptions};
use keybund_core::model::{Config, GitIdentity, HostEntry, KeyRef, Profile, Provider, Settings};
use keybund_core::{
    archive as archive_mod, clone as clone_mod, history, import as import_mod, keyinstall, probe, provider, store,
    uninstall as uninstall_mod, which as which_mod, Env,
};

use crate::{system, ui};

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub fn list(env: &Env, all: bool) -> Result<ExitCode> {
    let config = store::load(env)?;
    if config.profiles.is_empty() {
        println!("Noch keine Keys. Mit `keybund new` einen anlegen.");
        return Ok(ExitCode::SUCCESS);
    }
    for p in config.profiles.iter().filter(|p| all || !p.archived) {
        let info = keygen::key_info(env, &p.key);
        let mut title = format!("{} {}", style(&p.name).bold(), style(format!("({})", p.id)).dim());
        if config.default_profile.as_deref() == Some(p.id.as_str()) {
            title.push_str(&format!(" {}", style("Standard").cyan()));
        }
        if p.archived {
            title.push_str(&format!(" {}", style("archiviert").yellow()));
        }
        println!("{title}");
        ui::row("Key", &p.key.path);
        ui::row("Fingerprint", info.fingerprint.as_deref().unwrap_or("– (.pub fehlt)"));
        if !p.folders.is_empty() {
            ui::row("Ordner", &p.folders.join(", "));
        }
        if !p.hosts.is_empty() {
            let hosts: Vec<String> = p.hosts.iter().map(host_label).collect();
            ui::row("Hosts", &hosts.join(", "));
        }
        println!();
    }
    Ok(ExitCode::SUCCESS)
}

fn host_label(h: &HostEntry) -> String {
    match &h.alias {
        Some(a) => format!("{a} → {}", h.hostname),
        None => format!("{} ({})", h.hostname, h.provider.display_name()),
    }
}

pub fn show(env: &Env, id: &str) -> Result<ExitCode> {
    let config = store::load(env)?;
    let p = config.profile(id)?;
    let info = keygen::key_info(env, &p.key);
    ui::heading(&p.name);
    ui::row("ID", &p.id);
    ui::row("Key", &p.key.path);
    ui::row("Typ", info.key_type.as_deref().unwrap_or("–"));
    ui::row("Fingerprint", info.fingerprint.as_deref().unwrap_or("–"));
    ui::row("Erstellt", info.created.as_deref().unwrap_or("–"));
    // Only the git folders use the key's identity; empty fields fall back to ~/.gitconfig.
    if !p.folders.is_empty() {
        let global = keybund_core::gitenv::global_identity(env);
        let own = |v: Option<&str>, fallback: Option<String>| match v.filter(|v| !v.trim().is_empty()) {
            Some(v) => v.to_string(),
            None => match fallback {
                Some(v) => format!("{v} (aus ~/.gitconfig)"),
                None => "nicht gesetzt – Git fragt beim ersten Commit".into(),
            },
        };
        let g = p.git.as_ref();
        ui::row("Git-Name", &own(g.map(|g| g.name.as_str()), global.name));
        ui::row("Git-E-Mail", &own(g.map(|g| g.email.as_str()), global.email));
        ui::row("Signieren", if g.is_some_and(|g| g.sign) { "ja" } else { "nein" });
    }
    for f in &p.folders {
        ui::row("Ordner", f);
    }
    for h in &p.hosts {
        ui::row("Host", &host_label(h));
    }
    let tests = history::load(env);
    for t in history::host_tests(&tests, p) {
        let when = format_millis(t.at);
        let host = t.host.as_deref().unwrap_or_default();
        ui::row("Letzter Test", &format!("{host}: {} ({when})", t.message));
    }
    if history::host_tests(&tests, p).is_empty() {
        if let Some(t) = history::summary(&tests, p) {
            ui::row("Letzter Test", &format!("{} ({})", t.message, format_millis(t.at)));
        }
    }
    if let Some(k) = &info.public_key {
        println!("\n{k}");
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Copy, Clone, ValueEnum)]
pub enum ProviderArg {
    Github,
    Gitlab,
    Bitbucket,
    Generic,
}

impl From<ProviderArg> for Provider {
    fn from(p: ProviderArg) -> Self {
        match p {
            ProviderArg::Github => Provider::Github,
            ProviderArg::Gitlab => Provider::Gitlab,
            ProviderArg::Bitbucket => Provider::Bitbucket,
            ProviderArg::Generic => Provider::Generic,
        }
    }
}

#[derive(Args)]
pub struct NewArgs {
    /// Anzeigename, z. B. „Kunde C“
    #[arg(long)]
    name: Option<String>,
    /// Key-ID (Standard: aus dem Namen abgeleitet)
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    color: Option<String>,
    #[arg(long, value_enum)]
    provider: Option<ProviderArg>,
    /// Hostname, z. B. github.com oder gitlab.kunde.de
    #[arg(long)]
    host: Option<String>,
    /// SSH-Alias für Server, z. B. meinserver
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    user: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    /// Git-Ordner (wird angelegt, falls nötig)
    #[arg(long)]
    folder: Vec<String>,
    #[arg(long)]
    git_name: Option<String>,
    #[arg(long)]
    git_email: Option<String>,
    /// Commits mit dem SSH-Key signieren
    #[arg(long)]
    sign: bool,
    /// Vorhandenen Key verwenden statt einen neuen zu erzeugen
    #[arg(long)]
    key: Option<String>,
    /// Kommentar für einen neuen Key
    #[arg(long)]
    comment: Option<String>,
    /// Als Standard-Key setzen
    #[arg(long)]
    default: bool,
    /// Ohne Rückfragen anwenden
    #[arg(long, short)]
    yes: bool,
    /// Configs nicht schreiben
    #[arg(long)]
    no_apply: bool,
    /// Provider-Seite nicht im Browser öffnen
    #[arg(long)]
    no_browser: bool,
}

fn ask(prompt: &str, default: Option<String>) -> Result<String> {
    let theme = theme();
    let mut input = Input::<String>::with_theme(&theme).with_prompt(prompt);
    if let Some(d) = default {
        input = input.default(d);
    }
    Ok(input.interact_text()?)
}

fn require<T>(value: Option<T>, flag: &str) -> Result<T> {
    value.with_context(|| format!("{flag} fehlt (ohne Terminal müssen alle Angaben als Optionen kommen)"))
}

pub fn new(env: &Env, args: NewArgs) -> Result<ExitCode> {
    let tty = interactive() && !args.yes;
    let mut config = store::load(env)?;

    // 1. Name
    let name = match args.name.clone() {
        Some(n) => n,
        None if tty => ask("Name des Keys", None)?,
        None => require(None, "--name")?,
    };
    let id = args.id.clone().unwrap_or_else(|| store::slugify(&name));
    if !store::is_valid_id(&id) {
        bail!("Ungültige Key-ID „{id}“ – mit --id angeben");
    }
    if config.profiles.iter().any(|p| p.id == id) {
        bail!("Einen Key „{id}“ gibt es schon");
    }

    // 2. Provider und Host
    let provider: Provider = match args.provider {
        Some(p) => p.into(),
        None if tty => {
            let items: Vec<&str> = Provider::ALL.iter().map(|p| p.display_name()).collect();
            let idx = Select::with_theme(&theme()).with_prompt("Provider").items(&items).default(0).interact()?;
            Provider::ALL[idx]
        }
        None => args.host.as_deref().map(Provider::detect).unwrap_or(Provider::Github),
    };
    let hostname = match args.host.clone() {
        Some(h) => h,
        None if tty => ask("Hostname", provider.default_hostname().map(str::to_string))?,
        None => require(provider.default_hostname().map(str::to_string), "--host")?,
    };
    let alias = match (&args.alias, provider) {
        (Some(a), _) => Some(a.clone()),
        (None, Provider::Generic) if tty => {
            Some(ask("SSH-Alias (z. B. meinserver)", Some(hostname.split('.').next().unwrap_or("server").into()))?)
        }
        _ => None,
    };
    let user = match (&args.user, provider) {
        (Some(u), _) => Some(u.clone()),
        (None, Provider::Generic) if tty => {
            let u = ask("Benutzer", Some(std::env::var("USER").unwrap_or_default()))?;
            (!u.is_empty()).then_some(u)
        }
        _ => None,
    };
    // Lower case like the app: ssh matches Host patterns case-sensitively.
    let host = HostEntry {
        alias: alias.map(|a| a.to_lowercase()),
        hostname: hostname.to_lowercase(),
        user,
        port: args.port,
        provider,
    };

    // 3. Projektordner
    let mut folders = args.folder.clone();
    if folders.is_empty() && tty && provider.is_git_host() {
        folders.push(ask("Git-Ordner", Some(config.settings.project_folder(&id)))?);
    }
    let folders: Vec<String> = folders.iter().map(|f| store::normalize_folder(env, f)).collect();

    // 4. Git-Identität: suggested from ~/.gitconfig; the same values mean
    // "follow ~/.gitconfig", so nothing is stored for the key.
    let git = if provider.is_git_host() || args.git_email.is_some() || args.git_name.is_some() {
        let global = keybund_core::gitenv::global_identity(env);
        let git_name = match args.git_name.clone() {
            Some(n) => n,
            None if tty => ask("Git-Name", global.name.clone())?,
            None => global.name.clone().unwrap_or_default(),
        };
        let git_email = match args.git_email.clone() {
            Some(e) => e,
            None if tty => ask("Git-E-Mail", global.email.clone())?,
            None => global.email.clone().unwrap_or_default(),
        };
        let sign = if args.sign || !tty {
            args.sign
        } else {
            Confirm::with_theme(&theme()).with_prompt("Commits mit diesem Key signieren?").default(false).interact()?
        };
        let same = |v: &str, g: &Option<String>| v.trim() == g.as_deref().unwrap_or_default();
        if !sign && same(&git_name, &global.name) && same(&git_email, &global.email) {
            None
        } else {
            Some(GitIdentity { name: git_name.trim().to_string(), email: git_email.trim().to_string(), sign })
        }
    } else {
        None
    };

    // Check everything before anything is created: a refused key must not
    // leave a key file or folders behind.
    let planned_key = match &args.key {
        Some(path) => env.contract(&env.expand(path)),
        None => config.settings.key_path(&id),
    };
    let mut candidate = config.clone();
    candidate.profiles.push(Profile {
        id: id.clone(),
        name: name.clone(),
        key: KeyRef { path: planned_key, comment: None },
        git: git.clone(),
        folders: folders.clone(),
        hosts: vec![host.clone()],
        ..Default::default()
    });
    store::validate_in(env, &candidate)?;

    // 5. Key
    let key = match args.key.clone() {
        Some(path) => {
            let path = env.contract(&env.expand(&path));
            keygen::read_public(env, &KeyRef { path: path.clone(), comment: None })?;
            KeyRef { path, comment: None }
        }
        None => {
            let path = config.settings.key_path(&id);
            let comment = args.comment.clone().unwrap_or_else(|| default_comment(&id));
            let passphrase = if tty {
                Password::with_theme(&theme())
                    .with_prompt("Passphrase (leer für keine)")
                    .with_confirmation("Wiederholen", "Passphrasen stimmen nicht überein")
                    .allow_empty_password(true)
                    .interact()?
            } else {
                std::env::var("KEYBUND_PASSPHRASE").unwrap_or_default()
            };
            let info = keygen::generate(
                env,
                &GenerateOptions { path: path.clone(), comment: comment.clone(), passphrase: Some(passphrase) },
            )?;
            ui::success(&format!("Key erzeugt: {path}"));
            ui::hint(info.fingerprint.as_deref().unwrap_or_default());
            KeyRef { path, comment: Some(comment) }
        }
    };
    // Folders only once the key is there: a refused key leaves nothing behind.
    for f in &folders {
        let path = env.expand(f);
        std::fs::create_dir_all(&path).with_context(|| format!("Ordner {f} kann nicht angelegt werden"))?;
    }

    let profile = Profile {
        id: id.clone(),
        name: name.clone(),
        color: args.color.clone(),
        key,
        git,
        folders,
        archived: false,
        hosts: vec![host.clone()],
    };
    config.profiles.push(profile.clone());
    // Only on request: a standard key replaces ssh's own key choice outside the git folders.
    if args.default {
        config.default_profile = Some(id.clone());
    }
    store::save(env, &config)?;
    ui::success(&format!("Key „{name}“ gespeichert"));

    // 6. Diff anzeigen und anwenden
    if !args.no_apply {
        let plan = apply::plan(env, &config)?;
        if !confirm_and_apply(env, &plan, args.yes)? {
            ui::warn("Configs nicht geschrieben – später mit `keybund apply`.");
            return Ok(ExitCode::SUCCESS);
        }
    }

    // 7. Pubkey in die Zwischenablage, Provider-Seite öffnen
    let public = keygen::read_public(env, &profile.key)?.line();
    match system::copy_to_clipboard(&public) {
        Ok(()) => ui::success("Pubkey liegt in der Zwischenablage"),
        Err(_) => {
            ui::warn("Zwischenablage nicht verfügbar, Pubkey:");
            println!("{public}");
        }
    }
    if let Some(url) = provider::add_key_url(host.provider, &host.hostname) {
        ui::hint(&format!("Key eintragen: {url}"));
        if tty && !args.no_browser {
            let _ = system::open_url(&url);
        }
    } else if provider == Provider::Generic {
        ui::hint(&format!("Key auf dem Server eintragen: keybund install-key {}", profile.id));
    }

    // 8. „Ich hab ihn eingetragen“ → Verbindungstest
    if tty
        && Confirm::with_theme(&theme())
            .with_prompt("Key eingetragen? Verbindung jetzt testen")
            .default(true)
            .interact()?
    {
        return run_probe(env, &profile, None);
    }
    Ok(ExitCode::SUCCESS)
}

fn default_comment(id: &str) -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let host = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
    if host.is_empty() {
        format!("{user}-{id}")
    } else {
        format!("{user}@{host}-{id}")
    }
}

fn print_plan(plan: &Plan) {
    for c in &plan.changes {
        let label = match c.kind {
            ChangeKind::Create => "neu",
            ChangeKind::Modify => "geändert",
            ChangeKind::Delete => "gelöscht",
        };
        ui::heading(&format!("{} · {label} (+{} −{})", c.path, c.added, c.removed));
        ui::diff(&c.diff);
        println!();
    }
}

/// Shows the plan and writes it after confirmation. Returns `false` if the
/// user declined.
fn confirm_and_apply(env: &Env, plan: &Plan, yes: bool) -> Result<bool> {
    if plan.is_empty() {
        ui::success("Configs sind aktuell");
        return Ok(true);
    }
    print_plan(plan);
    // A script or a pipe cannot answer: the files are only written with --yes.
    if !yes && !interactive() {
        ui::warn("Ohne Terminal nur mit --yes – Configs nicht geschrieben.");
        return Ok(false);
    }
    if !yes
        && !Confirm::with_theme(&theme())
            .with_prompt(format!("{} Datei(en) schreiben?", plan.changes.len()))
            .default(true)
            .interact()?
    {
        return Ok(false);
    }
    let report = apply::apply(env, plan)?;
    for b in &report.backups {
        ui::hint(&format!("Backup: {b}"));
    }
    ui::success(&format!(
        "{} Datei(en) geschrieben{}",
        report.written.len(),
        if report.deleted.is_empty() { String::new() } else { format!(", {} gelöscht", report.deleted.len()) }
    ));
    Ok(true)
}

pub fn copy(env: &Env, id: &str, print: bool) -> Result<ExitCode> {
    let config = store::load(env)?;
    let p = config.profile(id)?;
    let public = keygen::read_public(env, &p.key)?.line();
    if print {
        println!("{public}");
        return Ok(ExitCode::SUCCESS);
    }
    system::copy_to_clipboard(&public)?;
    ui::success(&format!("Pubkey von „{}“ kopiert", p.name));
    Ok(ExitCode::SUCCESS)
}

pub fn test(env: &Env, id: &str, host: Option<&str>) -> Result<ExitCode> {
    let config = store::load(env)?;
    run_probe(env, config.profile(id)?, host)
}

fn run_probe(env: &Env, profile: &Profile, only: Option<&str>) -> Result<ExitCode> {
    if profile.hosts.is_empty() {
        bail!("Key „{}“ hat keinen Host", profile.id);
    }
    let hosts: Vec<&HostEntry> = profile
        .hosts
        .iter()
        .filter(|h| {
            only.is_none_or(|name| h.target_name().eq_ignore_ascii_case(name) || h.hostname.eq_ignore_ascii_case(name))
        })
        .collect();
    if let (true, Some(name)) = (hosts.is_empty(), only) {
        bail!("Key „{}“ hat keinen Host {name}", profile.id);
    }
    let mut ok = true;
    let settings = store::load(env)?.settings;
    for host in hosts {
        // An unconfirmed host key fails this host, the others are still tested.
        if let Err(e) = host_key_ok(env, &settings, profile, host, false) {
            ok = false;
            ui::error(&format!("{}: {e}", host.target_name()));
            continue;
        }
        let r = probe::probe(env, profile, host)?;
        // Per host, like the app: the key's summary is computed from these.
        history::record(env, &profile.id, host.target_name(), &r)?;
        if r.success {
            ui::success(&format!("{}: {}", r.host, r.message));
        } else {
            ok = false;
            ui::error(&format!("{}: {}", r.host, r.message));
            print_steps(&r.steps);
            ui::hint(&format!("Befehl: {}", r.command));
            if r.key_rejected {
                ui::hint(&format!("Key eintragen: keybund install-key {} --host {}", profile.id, host.target_name()));
            }
        }
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// The steps of a connection test, as a short story.
fn print_steps(steps: &[probe::ProbeStep]) {
    for step in steps {
        let mark = match step.ok {
            Some(true) => style("✓").green(),
            Some(false) => style("✗").red(),
            None => style("·").dim(),
        };
        println!("    {mark} {}", step.text);
    }
}

/// `YYYY-MM-DD HH:MM` (UTC) of a timestamp in milliseconds.
fn format_millis(ms: f64) -> String {
    let secs = (ms / 1000.0) as u64;
    let time = secs % 86_400;
    format!("{} {:02}:{:02} UTC", keygen::format_date(secs), time / 3600, time % 3600 / 60)
}

/// Shows or changes a setting (`[settings]` in profiles.toml).
pub fn config(env: &Env, key: Option<&str>, value: Option<&str>) -> Result<ExitCode> {
    let mut config = store::load(env)?;
    let Some(key) = key else {
        let s = &config.settings;
        ui::row("projects-dir", &s.projects_dir);
        ui::row("keys-dir", &s.keys_dir);
        ui::row("absolute-ssh-include", &s.absolute_ssh_include.to_string());
        ui::row("known-hosts-per-profile", &s.known_hosts_per_profile.to_string());
        return Ok(ExitCode::SUCCESS);
    };
    const KEYS: [&str; 4] = ["projects-dir", "keys-dir", "absolute-ssh-include", "known-hosts-per-profile"];
    if !KEYS.contains(&key) {
        bail!("Unbekannte Einstellung „{key}“ ({})", KEYS.join(", "));
    }
    let Some(value) = value else {
        bail!("Wert fehlt: keybund config {key} <wert>");
    };
    let flag = |v: &str| match v {
        "true" | "ja" | "an" | "on" | "1" => Ok(true),
        "false" | "nein" | "aus" | "off" | "0" => Ok(false),
        _ => bail!("„{v}“ ist kein Ja/Nein-Wert (true/false)"),
    };
    let s = &mut config.settings;
    match key {
        "projects-dir" => s.projects_dir = store::normalize_dir(env, value)?,
        "keys-dir" => s.keys_dir = store::normalize_dir(env, value)?,
        "absolute-ssh-include" => s.absolute_ssh_include = flag(value)?,
        "known-hosts-per-profile" => s.known_hosts_per_profile = flag(value)?,
        _ => {
            bail!("Unbekannte Einstellung „{key}“ (projects-dir, keys-dir, absolute-ssh-include, known-hosts-per-profile)")
        }
    }
    store::save(env, &config)?;
    ui::success(&format!("{key} gespeichert"));
    if matches!(key, "absolute-ssh-include" | "known-hosts-per-profile") {
        ui::hint("Mit `keybund apply` übernehmen.");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn which(env: &Env, target: Option<&str>) -> Result<ExitCode> {
    let config = store::load(env)?;
    let r = which_mod::which(env, &config, target.unwrap_or(""))?;
    ui::heading(&r.query);
    ui::row("Key", r.key_path.as_deref().unwrap_or("– (ssh wählt selbst)"));
    if let Some(fp) = &r.fingerprint {
        ui::row("Fingerprint", fp);
    }
    if let Some(id) = &r.profile_id {
        let name = config.profile(id).map(|p| p.name.clone()).unwrap_or_else(|_| id.clone());
        ui::row("Key", &name);
    }
    if let Some(src) = &r.source {
        let via = r.via.as_ref().map(|v| format!(" · via {v}")).unwrap_or_default();
        ui::row("Quelle", &format!("{src}{via}"));
    }
    if let (Some(n), Some(e)) = (&r.git_name, &r.git_email) {
        ui::row("Git-Identität", &format!("{n} <{e}>"));
    }
    if let Some(h) = &r.hostname {
        let user = r.user.as_deref().map(|u| format!("{u}@")).unwrap_or_default();
        let port = r.port.map(|p| format!(":{p}")).unwrap_or_default();
        ui::row("Ziel", &format!("{user}{h}{port}"));
    }
    if let Some(io) = r.identities_only {
        ui::row("IdentitiesOnly", if io { "ja, nur dieser Key wird angeboten" } else { "nein" });
    }
    if let Some(loaded) = r.agent_loaded {
        ui::row("Agent", if loaded { "geladen" } else { "nicht geladen" });
    }
    for n in &r.notes {
        ui::hint(&n.to_string());
    }
    Ok(ExitCode::SUCCESS)
}

pub fn clone(env: &Env, url: &str, profile: Option<&str>, dest: Option<PathBuf>) -> Result<ExitCode> {
    let config = store::load(env)?;
    let parsed = clone_mod::parse_url(url)?;
    let profile: &Profile = match profile {
        Some(id) => config.profile(id)?,
        None => {
            let matches = clone_mod::matching_profiles(&config, &parsed);
            match matches.as_slice() {
                [only] => only,
                [] => bail!("Kein Key hat den Host {} – mit --key wählen", parsed.host),
                many if interactive() => {
                    let items: Vec<&str> = many.iter().map(|p| p.name.as_str()).collect();
                    let idx = Select::with_theme(&theme()).with_prompt("Key").items(&items).default(0).interact()?;
                    many[idx]
                }
                many => {
                    let ids: Vec<&str> = many.iter().map(|p| p.id.as_str()).collect();
                    bail!("Mehrere Keys nutzen {}: {} – mit --key wählen", parsed.host, ids.join(", "))
                }
            }
        }
    };
    let target = match dest {
        Some(d) => d,
        None => clone_mod::target_dir(env, profile, &parsed)?,
    };
    ui::hint(&format!("Key „{}“ → {}", profile.name, env.contract(&target)));
    let target = clone_mod::clone(env, profile, url, Some(target))?;
    ui::success(&format!("Geklont nach {}", env.contract(&target)));
    Ok(ExitCode::SUCCESS)
}

pub fn apply(env: &Env, dry_run: bool, yes: bool) -> Result<ExitCode> {
    let config = store::load(env)?;
    let plan = apply::plan(env, &config)?;
    if dry_run {
        if plan.is_empty() {
            ui::success("Configs sind aktuell");
        } else {
            print_plan(&plan);
            println!("{} Datei(en), +{} −{} Zeilen", plan.changes.len(), plan.added(), plan.removed());
        }
        return Ok(ExitCode::SUCCESS);
    }
    // Writing the user's own files needs a yes: asked, or --yes.
    if !yes && !interactive() && !plan.is_empty() {
        print_plan(&plan);
        bail!("Ohne Terminal nur mit --yes");
    }
    confirm_and_apply(env, &plan, yes)?;
    Ok(ExitCode::SUCCESS)
}

pub fn doctor(env: &Env, fix: bool) -> Result<ExitCode> {
    let mut report = doctor::run(env);
    if fix {
        let mut fixes: Vec<doctor::Fix> = report.checks.iter().filter_map(|c| c.fix.clone()).collect();
        fixes.dedup();
        for f in &fixes {
            doctor::fix(env, f)?;
        }
        if !fixes.is_empty() {
            report = doctor::run(env);
        }
    }
    for c in &report.checks {
        ui::status(c.status, &c.title.to_string());
        if c.status != Status::Ok {
            if let Some(d) = &c.detail {
                ui::hint(&d.to_string());
            }
            if c.fix.is_some() && !fix {
                ui::hint("→ automatisch behebbar mit `keybund doctor --fix`");
            }
        }
    }
    let problems = report.problems();
    println!();
    if problems == 0 {
        ui::success("Alles in Ordnung");
        Ok(ExitCode::SUCCESS)
    } else {
        ui::warn(&format!("{problems} Hinweis(e)"));
        Ok(ExitCode::FAILURE)
    }
}

pub fn keys(env: &Env) -> Result<ExitCode> {
    let config = store::load(env)?;
    let found = keygen::scan(env, &config)?;
    if found.is_empty() {
        println!("Keine Keys in ~/.ssh gefunden.");
    }
    for c in found {
        let used = c.used_by.map(|p| format!(" {}", style(format!("[{p}]")).cyan())).unwrap_or_default();
        println!("{}{used}", style(&c.key.path).bold());
        ui::row("Typ", c.key.key_type.as_deref().unwrap_or("–"));
        ui::row("Fingerprint", c.key.fingerprint.as_deref().unwrap_or("–"));
        if let Some(comment) = &c.key.comment {
            ui::row("Kommentar", comment);
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn set_default(env: &Env, id: Option<&str>) -> Result<ExitCode> {
    let mut config = store::load(env)?;
    if let Some(id) = id {
        if config.profile(id)?.archived {
            bail!("Key „{id}“ ist archiviert");
        }
    }
    config.default_profile = id.map(str::to_string);
    store::save(env, &config)?;
    match id {
        Some(id) => ui::success(&format!("Standard-Key: {id}")),
        None => ui::success("Kein Standard-Key mehr – außerhalb der Git-Ordner wählt ssh den Key selbst"),
    }
    // Like the app: the choice applies right away.
    let plan = apply::plan(env, &config)?;
    confirm_and_apply(env, &plan, true)?;
    Ok(ExitCode::SUCCESS)
}

pub fn archive(env: &Env, id: &str, yes: bool) -> Result<ExitCode> {
    let mut config: Config = store::load(env)?;
    let name = config.profile(id)?.name.clone();
    if !yes && !interactive() {
        bail!("Ohne Terminal nur mit --yes – archivieren verschiebt den Key");
    }
    if !yes
        && interactive()
        && !Confirm::with_theme(&theme())
            .with_prompt(format!("Key „{name}“ archivieren? Der Key wandert nach ~/.ssh/archive/"))
            .default(false)
            .interact()?
    {
        return Ok(ExitCode::SUCCESS);
    }
    let result = archive_mod::archive(env, &mut config, id)?;
    ui::success(&format!("„{name}“ archiviert, Key liegt unter {}", result.key_path));
    let plan = apply::plan(env, &config)?;
    confirm_and_apply(env, &plan, true)?;
    for url in &result.remove_key_urls {
        ui::hint(&format!("Key beim Provider entfernen: {url}"));
    }
    Ok(ExitCode::SUCCESS)
}

pub fn import(env: &Env, all: bool, yes: bool, keep_old: bool) -> Result<ExitCode> {
    let tty = interactive() && !yes;
    let proposals = import_mod::proposals_for(env)?;
    if proposals.is_empty() {
        ui::success("Nichts zu übernehmen – ~/.ssh/config enthält keine Einträge mit eigenem Key, die Keybund noch nicht kennt.");
    } else {
        ui::heading("Aus ~/.ssh/config übernehmbar:");
        for (i, p) in proposals.iter().enumerate() {
            let hosts: Vec<String> = p.profile.hosts.iter().map(host_label).collect();
            println!(
                "  {}. {} {}{}",
                i + 1,
                style(&p.profile.name).bold(),
                style(format!("({})", p.profile.id)).dim(),
                if p.make_default { format!(" {}", style("Default").cyan()) } else { String::new() }
            );
            ui::row("Key", &p.profile.key.path);
            ui::row("Hosts", &hosts.join(", "));
            for note in &p.notes {
                ui::hint(&note.to_string());
            }
        }
        let chosen: Vec<_> = if all || !tty {
            if !all && !yes {
                println!();
                ui::hint("Mit `keybund import --all` übernehmen.");
                return Ok(ExitCode::SUCCESS);
            }
            // A block whose key file is gone cannot become a key; the others still can.
            for p in proposals.iter().filter(|p| !p.key_exists) {
                ui::warn(&format!("{} übersprungen: Key-Datei {} fehlt", p.profile.name, p.profile.key.path));
            }
            proposals.iter().filter(|p| p.key_exists).cloned().collect()
        } else {
            let labels: Vec<String> =
                proposals.iter().map(|p| format!("{} ({})", p.profile.name, p.profile.key.path)).collect();
            let picked = MultiSelect::with_theme(&theme())
                .with_prompt("Welche übernehmen? (Leertaste wählt, Enter bestätigt)")
                .items(&labels)
                .defaults(&vec![true; labels.len()])
                .interact()?;
            picked.into_iter().map(|i| proposals[i].clone()).collect()
        };
        if chosen.is_empty() {
            return Ok(ExitCode::SUCCESS);
        }
        let ids = import_mod::import(env, &chosen)?;
        ui::success(&format!("{} Key(s) übernommen: {}", ids.len(), ids.join(", ")));
        let config = store::load(env)?;
        if !confirm_and_apply(env, &apply::plan(env, &config)?, yes)? {
            ui::warn("Configs nicht geschrieben – später mit `keybund apply`.");
            return Ok(ExitCode::SUCCESS);
        }
    }

    if keep_old {
        return Ok(ExitCode::SUCCESS);
    }
    let cleanup = import_mod::plan_cleanup(env)?;
    if cleanup.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    println!();
    ui::heading("Übernommene Einträge aus ~/.ssh/config entfernen");
    ui::hint("Solange sie dort stehen, bietet ssh ihre Keys zusätzlich an – die Anmeldung kann mit dem falschen Key gelingen.");
    println!();
    // Removing blocks from the user's own file needs a yes: asked, or --yes.
    if !yes && !interactive() {
        print_plan(&cleanup);
        ui::warn("Ohne Terminal nur mit --yes – alte Einträge bleiben vorerst.");
        return Ok(ExitCode::SUCCESS);
    }
    if !confirm_and_apply(env, &cleanup, yes)? {
        ui::warn("Alte Einträge bleiben – `keybund doctor --fix` entfernt sie später.");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn uninstall(env: &Env, restore_backups: bool, purge: bool, yes: bool) -> Result<ExitCode> {
    let opts = uninstall_mod::UninstallOptions { restore_backups, purge };
    let preview = uninstall_mod::preview(env, opts)?;
    if preview.plan.is_empty() {
        ui::success("Nichts zu entfernen – Keybund hat keine Spuren hinterlassen.");
        return Ok(ExitCode::SUCCESS);
    }
    print_plan(&preview.plan);
    if preview.backups_available && !restore_backups {
        ui::hint("Mit --restore-backups kommen ~/.ssh/config und ~/.gitconfig wieder so, wie sie vor Keybund waren.");
    }
    if !(yes || !interactive())
        && !Confirm::with_theme(&theme()).with_prompt("Keybund entfernen?").default(false).interact()?
    {
        return Ok(ExitCode::SUCCESS);
    }
    if !yes && !interactive() {
        bail!("Ohne Terminal nur mit --yes");
    }
    let report = uninstall_mod::uninstall(env, &preview.plan)?;
    ui::success(&format!(
        "Keybund entfernt: {} Datei(en) geändert, {} gelöscht",
        report.written.len(),
        report.deleted.len()
    ));
    ui::hint("Deine Keys in ~/.ssh sind unverändert.");
    if !purge {
        ui::hint(
            "Deine Keys stehen weiter in ~/.config/keybund/profiles.toml – `keybund apply` richtet alles wieder ein.",
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct EditArgs {
    /// Key-ID
    #[arg(value_name = "KEY")]
    profile: String,
    /// Neuer Anzeigename
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    color: Option<String>,
    /// Git-Ordner hinzufügen (wird angelegt, falls nötig)
    #[arg(long, value_name = "ORDNER")]
    add_folder: Vec<String>,
    /// Git-Ordner entfernen
    #[arg(long, value_name = "ORDNER")]
    remove_folder: Vec<String>,
    #[arg(long)]
    git_name: Option<String>,
    #[arg(long)]
    git_email: Option<String>,
    /// Commits signieren
    #[arg(long, conflicts_with = "no_sign")]
    sign: bool,
    /// Commits nicht signieren
    #[arg(long)]
    no_sign: bool,
    /// Host hinzufügen, z. B. gitlab.kunde.de
    #[arg(long, value_name = "HOSTNAME")]
    add_host: Option<String>,
    /// SSH-Alias für den neuen Host
    #[arg(long, requires = "add_host")]
    alias: Option<String>,
    #[arg(long, requires = "add_host")]
    user: Option<String>,
    #[arg(long, requires = "add_host")]
    port: Option<u16>,
    #[arg(long, value_enum, requires = "add_host")]
    provider: Option<ProviderArg>,
    /// Host entfernen (Alias oder Hostname)
    #[arg(long, value_name = "HOST")]
    remove_host: Vec<String>,
    /// Anderen, vorhandenen Key verwenden
    #[arg(long)]
    key: Option<String>,
    /// Ohne Rückfrage schreiben
    #[arg(long, short)]
    yes: bool,
    /// Nur profiles.toml ändern, Configs nicht schreiben
    #[arg(long)]
    no_apply: bool,
}

pub fn edit(env: &Env, args: EditArgs) -> Result<ExitCode> {
    let mut config = store::load(env)?;
    let before = config.profile(&args.profile)?.clone();
    let p = config.profile_mut(&args.profile)?;
    let mut changes: Vec<String> = Vec::new();

    if let Some(name) = args.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        changes.push(format!("Name: {name}"));
        p.name = name.to_string();
    }
    if let Some(color) = &args.color {
        changes.push(format!("Farbe: {color}"));
        p.color = Some(color.clone());
    }
    for folder in &args.add_folder {
        let normalized = store::normalize_folder(env, folder);
        if !p.folders.contains(&normalized) {
            changes.push(format!("+ Ordner {normalized}"));
            p.folders.push(normalized);
        }
    }
    for folder in &args.remove_folder {
        let normalized = store::normalize_folder(env, folder);
        let count = p.folders.len();
        p.folders.retain(|f| f != &normalized && f != folder);
        if p.folders.len() == count {
            bail!("Key „{}“ hat den Ordner {folder} nicht", p.id);
        }
        changes.push(format!("− Ordner {normalized}"));
    }
    if args.git_name.is_some() || args.git_email.is_some() || args.sign || args.no_sign {
        let mut git = p.git.clone().unwrap_or_default();
        if let Some(n) = &args.git_name {
            git.name = n.clone();
        }
        if let Some(e) = &args.git_email {
            git.email = e.clone();
        }
        if args.sign {
            git.sign = true;
        }
        if args.no_sign {
            git.sign = false;
        }
        // Empty name or e-mail: git keeps the one from ~/.gitconfig. Signing
        // needs the key's own e-mail (validated when saving).
        let shown = |v: &str, label: &str| {
            if v.trim().is_empty() {
                format!("{label} aus ~/.gitconfig")
            } else {
                v.to_string()
            }
        };
        changes.push(format!(
            "Git: {} <{}>{}",
            shown(&git.name, "Name"),
            shown(&git.email, "E-Mail"),
            if git.sign { ", signiert" } else { "" }
        ));
        let empty = git.name.trim().is_empty() && git.email.trim().is_empty() && !git.sign;
        p.git = if empty { None } else { Some(git) };
    }
    if let Some(hostname) = &args.add_host {
        let provider: Provider = args.provider.map(Into::into).unwrap_or_else(|| Provider::detect(hostname));
        let host = HostEntry {
            alias: args.alias.as_deref().map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty()),
            hostname: hostname.trim().to_lowercase(),
            user: args.user.as_deref().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()),
            port: args.port,
            provider,
        };
        changes.push(format!("+ Host {}", host_label(&host)));
        p.hosts.push(host);
    }
    for name in &args.remove_host {
        let count = p.hosts.len();
        // By alias or hostname, like test and trust.
        p.hosts.retain(|h| {
            !(h.alias.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(name)) || h.hostname.eq_ignore_ascii_case(name))
        });
        if p.hosts.len() == count {
            bail!("Key „{}“ hat keinen Host {name}", p.id);
        }
        changes.push(format!("− Host {name}"));
    }
    if let Some(key) = &args.key {
        let path = env.contract(&env.expand(key));
        keygen::read_public(env, &KeyRef { path: path.clone(), comment: None })?;
        changes.push(format!("Key: {path}"));
        p.key = KeyRef { path, comment: None };
    }

    if changes.is_empty() {
        show(env, &args.profile)?;
        println!();
        ui::hint("Nichts geändert – Optionen siehe `keybund edit --help`.");
        return Ok(ExitCode::SUCCESS);
    }
    if *p == before {
        ui::success("Keine Änderung");
        return Ok(ExitCode::SUCCESS);
    }
    store::save(env, &config)?;
    // Only after validation, so a rejected folder leaves nothing behind.
    for folder in &args.add_folder {
        let path = env.expand(folder.trim());
        std::fs::create_dir_all(&path).with_context(|| format!("Ordner {folder} kann nicht angelegt werden"))?;
    }
    for c in &changes {
        ui::row("", c);
    }
    ui::success(&format!("Key „{}“ gespeichert", args.profile));
    if args.no_apply {
        return Ok(ExitCode::SUCCESS);
    }
    let plan = apply::plan(env, &config)?;
    if !confirm_and_apply(env, &plan, args.yes)? {
        ui::warn("Configs nicht geschrieben – später mit `keybund apply`.");
    }
    Ok(ExitCode::SUCCESS)
}

/// Makes sure the host key is known before ssh connects: pinned keys are
/// added, unknown ones are shown and confirmed (or `keybund trust` is
/// suggested without a terminal).
fn host_key_ok(env: &Env, settings: &Settings, profile: &Profile, host: &HostEntry, yes: bool) -> Result<()> {
    use keybund_core::hostkeys::{self, HostKeyCheck};
    match hostkeys::check(env, settings, profile, host)? {
        HostKeyCheck::Known => Ok(()),
        // ssh reports the network problem itself.
        HostKeyCheck::Unreachable { .. } => Ok(()),
        HostKeyCheck::Pinned => {
            ui::hint(&format!("Veröffentlichten Host-Key von {} in ~/.ssh/known_hosts eingetragen", host.hostname));
            Ok(())
        }
        HostKeyCheck::Unknown { host: name, file, keys } => {
            println!("Erste Verbindung zu {name}. Host-Key laut Server:");
            for k in &keys {
                println!("  {} {}", k.key_type, style(&k.fingerprint).bold());
            }
            ui::hint("Vergleiche mit dem Server (ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub) oder dem Hoster.");
            let confirmed = yes
                || (interactive()
                    && Confirm::with_theme(&theme())
                        .with_prompt("Stimmt der Fingerprint?")
                        .default(false)
                        .interact()?);
            if !confirmed {
                bail!(
                    "Host-Key von {name} ist nicht bestätigt. Prüfen und bestätigen mit: keybund trust {} --host {}",
                    profile.id,
                    host.target_name()
                );
            }
            let fingerprints: Vec<String> = keys.iter().map(|k| k.fingerprint.clone()).collect();
            hostkeys::trust(env, settings, profile, host, &fingerprints)?;
            ui::success(&format!("Host-Key von {name} in {file} gespeichert"));
            Ok(())
        }
    }
}

/// `keybund trust`: confirms the host keys of a profile's hosts.
pub fn trust(env: &Env, id: &str, host: Option<&str>, yes: bool) -> Result<ExitCode> {
    let config = store::load(env)?;
    let p = config.profile(id)?;
    let targets: Vec<&HostEntry> = p
        .hosts
        .iter()
        .filter(|h| {
            host.is_none_or(|name| h.target_name().eq_ignore_ascii_case(name) || h.hostname.eq_ignore_ascii_case(name))
        })
        .collect();
    if targets.is_empty() {
        bail!("Key „{id}“ hat keinen Host {}", host.unwrap_or_default());
    }
    for target in targets {
        if keybund_core::hostkeys::is_known(env, &config.settings, p, target) {
            ui::success(&format!("{}: Host-Key ist bekannt", target.target_name()));
            continue;
        }
        if let keybund_core::hostkeys::HostKeyCheck::Unreachable { .. } =
            keybund_core::hostkeys::check(env, &config.settings, p, target)?
        {
            bail!("{} ist nicht erreichbar – Host-Key nicht geprüft", target.target_name());
        }
        host_key_ok(env, &config.settings, p, target, yes)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `keybund install-key`: puts the key on servers (like `ssh-copy-id`);
/// for GitHub, GitLab and Bitbucket it copies the pubkey and opens the page
/// where it is pasted.
pub fn install_key(env: &Env, id: &str, host: Option<&str>) -> Result<ExitCode> {
    let config = store::load(env)?;
    let p = config.profile(id)?;
    let targets: Vec<&HostEntry> = p
        .hosts
        .iter()
        .filter(|h| {
            host.is_none_or(|name| {
                h.ssh_patterns().iter().any(|s| s.eq_ignore_ascii_case(name)) || h.hostname.eq_ignore_ascii_case(name)
            })
        })
        .collect();
    if targets.is_empty() {
        match host {
            Some(name) => bail!("Key „{id}“ hat keinen Host {name}"),
            None => bail!("Key „{id}“ hat keinen Host"),
        }
    }
    let mut ok = true;
    for target in targets {
        let success = if target.provider.is_git_host() {
            add_to_account(env, p, target)?
        } else {
            // keyinstall connects with StrictHostKeyChecking=yes: confirm first.
            host_key_ok(env, &config.settings, p, target, false)?;
            ui::hint(&format!(
                "Melde mich an {} an (Passwort oder ein bereits eingetragener Key) …",
                target.target_name()
            ));
            let r = keyinstall::install(env, p, target)?;
            if r.success {
                ui::success(&r.message.to_string());
            } else {
                ui::error(&r.message.to_string());
                for line in r.output.lines().take(5) {
                    ui::hint(line);
                }
            }
            r.success
        };
        ok &= success;
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// GitHub & Co.: the pubkey goes into the clipboard and the page to paste
/// it opens in the browser – the account stays the user's business.
fn add_to_account(env: &Env, p: &Profile, host: &HostEntry) -> Result<bool> {
    let name = host.provider.display_name();
    let public = keygen::read_public(env, &p.key)?.line();
    let Some(url) = provider::add_key_url(host.provider, &host.hostname) else {
        bail!("Für {} kenne ich keine Seite zum Eintragen", host.hostname);
    };
    match system::copy_to_clipboard(&public) {
        Ok(()) => ui::success("Pubkey in der Zwischenablage"),
        Err(_) => println!("{public}"),
    }
    println!("Bei {name} als neuen SSH-Key einfügen: {url}");
    if p.git.as_ref().is_some_and(|g| g.sign) {
        ui::hint("Für signierte Commits dort zusätzlich als „Signing Key“ eintragen.");
    }
    system::open_url(&url).ok();
    ui::hint(&format!("Danach prüfen: keybund test {} --host {}", p.id, host.target_name()));
    Ok(true)
}
