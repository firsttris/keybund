use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};
use keybund_core::apply::{self, ChangeKind, Plan};
use keybund_core::doctor::{self, Status};
use keybund_core::keygen::{self, GenerateOptions};
use keybund_core::model::{Config, GitIdentity, HostEntry, KeyRef, Profile, Provider};
use keybund_core::{archive as archive_mod, clone as clone_mod, probe, provider, store, which as which_mod, Env};

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
        println!("Noch keine Profile. Mit `keybund new` ein Projekt anlegen.");
        return Ok(ExitCode::SUCCESS);
    }
    for p in config.profiles.iter().filter(|p| all || !p.archived) {
        let info = keygen::key_info(env, &p.key);
        let mut title = format!("{} {}", style(&p.name).bold(), style(format!("({})", p.id)).dim());
        if config.default_profile.as_deref() == Some(p.id.as_str()) {
            title.push_str(&format!(" {}", style("default").cyan()));
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
    if let Some(g) = &p.git {
        ui::row("Git", &format!("{} <{}>", g.name, g.email));
        ui::row("Signieren", if g.sign { "ja" } else { "nein" });
    }
    for f in &p.folders {
        ui::row("Ordner", f);
    }
    for h in &p.hosts {
        ui::row("Host", &host_label(h));
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
    /// Profil-ID (Standard: aus dem Namen abgeleitet)
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
    /// Projektordner (wird angelegt, falls nötig)
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
    /// Als Default-Profil setzen
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
        None if tty => ask("Projektname", None)?,
        None => require(None, "--name")?,
    };
    let id = args.id.clone().unwrap_or_else(|| store::slugify(&name));
    if !store::is_valid_id(&id) {
        bail!("Ungültige Profil-ID „{id}“ – mit --id angeben");
    }
    if config.profiles.iter().any(|p| p.id == id) {
        bail!("Profil „{id}“ existiert bereits");
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
    let host = HostEntry { alias, hostname, user, port: args.port, provider };

    // 3. Projektordner
    let mut folders = args.folder.clone();
    if folders.is_empty() && tty && provider.is_git_host() {
        folders.push(ask("Projektordner", Some(format!("~/dev/{id}/")))?);
    }
    let folders: Vec<String> = folders
        .iter()
        .map(|f| {
            let path = env.expand(f.trim());
            std::fs::create_dir_all(&path).with_context(|| format!("Ordner {f} kann nicht angelegt werden"))?;
            Ok(store::normalize_folder(env, f))
        })
        .collect::<Result<_>>()?;

    // 4. Git-Identität
    let git = if provider.is_git_host() || args.git_email.is_some() {
        let global_name = git_global(env, "user.name");
        let git_name = match args.git_name.clone() {
            Some(n) => n,
            None if tty => ask("Git-Name", global_name)?,
            None => require(global_name, "--git-name")?,
        };
        let git_email = match args.git_email.clone() {
            Some(e) => e,
            None if tty => ask("Git-E-Mail", None)?,
            None => require(None, "--git-email")?,
        };
        let sign = if args.sign || !tty {
            args.sign
        } else {
            Confirm::with_theme(&theme()).with_prompt("Commits mit diesem Key signieren?").default(false).interact()?
        };
        Some(GitIdentity { name: git_name, email: git_email, sign })
    } else {
        None
    };

    // 5. Key
    let key = match args.key.clone() {
        Some(path) => {
            let path = env.contract(&env.expand(&path));
            keygen::read_public(env, &KeyRef { path: path.clone(), comment: None })?;
            KeyRef { path, comment: None }
        }
        None => {
            let path = keygen::default_key_path(&id);
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
    if args.default || config.default_profile.is_none() && config.profiles.len() == 1 {
        config.default_profile = Some(id.clone());
    }
    store::save(env, &config)?;
    ui::success(&format!("Profil „{name}“ gespeichert"));

    // 6. Diff anzeigen und anwenden
    if !args.no_apply {
        let plan = apply::plan(env, &config)?;
        if !confirm_and_apply(env, &plan, args.yes || !tty)? {
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
        ui::hint(&format!(
            "Key auf dem Server eintragen: ssh-copy-id -i {} {}",
            profile.key.pub_path(),
            host.target_name()
        ));
    }

    // 8. „Ich hab ihn eingetragen“ → Verbindungstest
    if tty
        && Confirm::with_theme(&theme())
            .with_prompt("Key eingetragen? Verbindung jetzt testen")
            .default(true)
            .interact()?
    {
        return run_probe(env, &profile);
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

fn git_global(env: &Env, key: &str) -> Option<String> {
    let out = env.command("git").args(["config", "--global", "--get", key]).output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!v.is_empty()).then_some(v)
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

pub fn test(env: &Env, id: &str) -> Result<ExitCode> {
    let config = store::load(env)?;
    run_probe(env, config.profile(id)?)
}

fn run_probe(env: &Env, profile: &Profile) -> Result<ExitCode> {
    if profile.hosts.is_empty() {
        bail!("Profil „{}“ hat keinen Host", profile.id);
    }
    let mut ok = true;
    for host in &profile.hosts {
        let r = probe::probe(env, profile, host)?;
        if r.success {
            ui::success(&format!("{}: {}", r.host, r.message));
        } else {
            ok = false;
            ui::error(&format!("{}: {}", r.host, r.message));
            ui::hint(&r.command);
            for line in r.output.lines().take(5) {
                ui::hint(line);
            }
        }
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
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
        ui::row("Profil", &name);
    }
    if let Some(src) = &r.source {
        let via = r.via.as_deref().map(|v| format!(" · via {v}")).unwrap_or_default();
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
        ui::hint(n);
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
                [] => bail!("Kein Profil hat den Host {} – mit --profile wählen", parsed.host),
                many if interactive() => {
                    let items: Vec<&str> = many.iter().map(|p| p.name.as_str()).collect();
                    let idx = Select::with_theme(&theme()).with_prompt("Profil").items(&items).default(0).interact()?;
                    many[idx]
                }
                many => {
                    let ids: Vec<&str> = many.iter().map(|p| p.id.as_str()).collect();
                    bail!("Mehrere Profile nutzen {}: {} – mit --profile wählen", parsed.host, ids.join(", "))
                }
            }
        }
    };
    let target = match dest {
        Some(d) => d,
        None => clone_mod::target_dir(env, profile, &parsed)?,
    };
    ui::hint(&format!("Profil „{}“ → {}", profile.name, env.contract(&target)));
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
    let yes = yes || !interactive();
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
        ui::status(c.status, &c.title);
        if c.status != Status::Ok {
            if let Some(d) = &c.detail {
                ui::hint(d);
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

pub fn set_default(env: &Env, id: &str) -> Result<ExitCode> {
    let mut config = store::load(env)?;
    config.profile(id)?;
    config.default_profile = Some(id.to_string());
    store::save(env, &config)?;
    ui::success(&format!("Default-Profil: {id}"));
    ui::hint("Mit `keybund apply` übernehmen.");
    Ok(ExitCode::SUCCESS)
}

pub fn archive(env: &Env, id: &str, yes: bool) -> Result<ExitCode> {
    let mut config: Config = store::load(env)?;
    let name = config.profile(id)?.name.clone();
    if !yes
        && interactive()
        && !Confirm::with_theme(&theme())
            .with_prompt(format!("Profil „{name}“ archivieren? Der Key wandert nach ~/.ssh/archive/"))
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
