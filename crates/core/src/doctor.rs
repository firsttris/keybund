//! Health checks for everything Keybund depends on. Each pitfall from the
//! plan has a check here; some come with an automatic fix.

use serde::{Deserialize, Serialize};

use crate::model::Config;
use crate::{apply, fsutil, gitenv, import, keygen, process, render, store, which, Env, Result};

/// Keys older than this get a rotation hint.
pub const OLD_KEY_DAYS: u32 = 2 * 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Error,
}

/// An automatic fix the user can trigger from a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fix {
    /// `chmod 700 ~/.ssh`, `chmod 600` on profile keys.
    Permissions,
    /// Move the Include line to the top of `~/.ssh/config`.
    MoveSshInclude,
    /// Write all pending config changes.
    Apply,
    /// Remove blocks from `~/.ssh/config` that a profile now covers.
    CleanupSshConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Check {
    pub id: String,
    pub status: Status,
    pub title: String,
    pub detail: Option<String>,
    pub fix: Option<Fix>,
}

impl Check {
    fn new(id: &str, status: Status, title: impl Into<String>) -> Self {
        Check { id: id.into(), status, title: title.into(), detail: None, fix: None }
    }
    fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    fn fix(mut self, f: Fix) -> Self {
        self.fix = Some(f);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    /// Number of warnings and errors.
    pub fn problems(&self) -> usize {
        self.checks.iter().filter(|c| c.status != Status::Ok).count()
    }
    pub fn has_errors(&self) -> bool {
        self.checks.iter().any(|c| c.status == Status::Error)
    }
}

/// Runs all checks.
pub fn run(env: &Env) -> Report {
    let mut checks = Vec::new();
    let config = match store::load(env) {
        Ok(c) => {
            match store::validate(&c) {
                Ok(()) => {
                    checks.push(Check::new("profiles", Status::Ok, format!("{} Profile geladen", c.profiles.len())))
                }
                Err(e) => checks
                    .push(Check::new("profiles", Status::Error, "profiles.toml ist ungültig").detail(e.to_string())),
            }
            c
        }
        Err(e) => {
            checks.push(
                Check::new("profiles", Status::Error, "profiles.toml kann nicht gelesen werden").detail(e.to_string()),
            );
            Config::default()
        }
    };

    checks.extend(tool_checks(env));
    checks.extend(permission_checks(env, &config));
    checks.extend(include_checks(env));
    checks.push(up_to_date_check(env, &config));
    checks.push(git_env_check(env));
    checks.push(agent_check(env));
    checks.extend(add_keys_check(env, &config));
    checks.extend(ssh_config_checks(env, &config));
    checks.extend(key_age_checks(env, &config));
    checks.extend(folder_checks(env, &config));
    Report { checks }
}

fn tool_checks(env: &Env) -> Vec<Check> {
    ["ssh", "ssh-keygen", "git"]
        .iter()
        .filter_map(|tool| {
            let mut cmd = env.command(tool);
            cmd.arg(if *tool == "git" { "--version" } else { "-V" });
            match process::output(cmd) {
                Err(crate::Error::ToolMissing(_)) => {
                    Some(Check::new(&format!("tool-{tool}"), Status::Error, format!("{tool} ist nicht installiert")))
                }
                _ => None,
            }
        })
        .collect()
}

fn permission_checks(env: &Env, config: &Config) -> Vec<Check> {
    let mut checks = Vec::new();
    if let Some(mode) = fsutil::mode(&env.ssh_dir()) {
        checks.push(if mode & 0o077 == 0 {
            Check::new("ssh-dir-perms", Status::Ok, "~/.ssh hat Rechte 700")
        } else {
            Check::new("ssh-dir-perms", Status::Error, format!("~/.ssh hat Rechte {mode:o} statt 700"))
                .detail("ssh verweigert Keys in zu offenen Verzeichnissen.")
                .fix(Fix::Permissions)
        });
    }
    let mut bad = Vec::new();
    let mut missing = Vec::new();
    for p in config.active_profiles() {
        let path = env.expand(&p.key.path);
        match fsutil::mode(&path) {
            Some(mode) if mode & 0o077 != 0 => bad.push(format!("{} ({mode:o})", p.key.path)),
            None if !path.exists() => missing.push(format!("{} ({})", p.key.path, p.name)),
            _ => {}
        }
        if path.exists() && !env.expand(&p.key.pub_path()).exists() {
            missing.push(format!("{} ({})", p.key.pub_path(), p.name));
        }
    }
    if !missing.is_empty() {
        checks.push(Check::new("keys-missing", Status::Error, "Key-Dateien fehlen").detail(missing.join(", ")));
    }
    if !config.profiles.is_empty() {
        checks.push(if bad.is_empty() {
            Check::new("key-perms", Status::Ok, "Private Keys haben Rechte 600")
        } else {
            Check::new("key-perms", Status::Error, "Private Keys mit zu offenen Rechten")
                .detail(bad.join(", "))
                .fix(Fix::Permissions)
        });
    }
    checks
}

fn include_checks(env: &Env) -> Vec<Check> {
    let mut checks = Vec::new();
    let ssh = fsutil::read_optional(&env.ssh_config()).ok().flatten().unwrap_or_default();
    checks.push(if apply::ssh_include_on_top(&ssh) {
        Check::new("ssh-include", Status::Ok, "Include steht ganz oben in ~/.ssh/config")
    } else if apply::ssh_include_present(&ssh) {
        Check::new("ssh-include", Status::Error, "Include steht nicht ganz oben")
            .detail("In ~/.ssh/config steht es unter einem Host-Block und gilt nur für diesen.")
            .fix(Fix::MoveSshInclude)
    } else {
        Check::new("ssh-include", Status::Warn, "Include fehlt in ~/.ssh/config").fix(Fix::Apply)
    });
    let git = fsutil::read_optional(&env.gitconfig()).ok().flatten().unwrap_or_default();
    checks.push(if apply::git_include_present(&git, &env.contract(&env.index_gitconfig())) {
        Check::new("git-include", Status::Ok, "~/.gitconfig bindet Keybund ein")
    } else {
        Check::new("git-include", Status::Warn, "Include fehlt in ~/.gitconfig").fix(Fix::Apply)
    });
    checks
}

fn up_to_date_check(env: &Env, config: &Config) -> Check {
    match apply::plan(env, config) {
        Ok(plan) if plan.is_empty() => Check::new("configs", Status::Ok, "Configs sind aktuell"),
        Ok(plan) => Check::new("configs", Status::Warn, format!("{} Datei(en) nicht aktuell", plan.changes.len()))
            .detail(plan.changes.iter().map(|c| c.path.as_str()).collect::<Vec<_>>().join(", "))
            .fix(Fix::Apply),
        Err(e) => Check::new("configs", Status::Error, "Configs können nicht berechnet werden").detail(e.to_string()),
    }
}

/// Variables that switch Keybund off for git, wherever they are set.
fn git_env_check(env: &Env) -> Check {
    let found = gitenv::detect(env);
    let Some(first) = found.first() else {
        return Check::new("git-env", Status::Ok, "Keine Umgebungsvariable übersteuert Git");
    };
    let detail = found
        .iter()
        .map(|o| match o.name.as_str() {
            "GIT_SSH_COMMAND" => format!(
                "GIT_SSH_COMMAND ist in {} gesetzt („{}“). Git benutzt dann in jedem Repo diesen Befehl statt des Profil-Keys.",
                o.origin, o.value
            ),
            _ => format!(
                "{} zeigt in {} auf {} – Git liest dann ~/.gitconfig mit dem Keybund-Include nicht.",
                o.name, o.origin, o.value
            ),
        })
        .collect::<Vec<_>>()
        .join(" ");
    Check::new("git-env", Status::Error, format!("{} übersteuert Keybund", first.name))
        .detail(format!("{detail} Zeile entfernen und neu anmelden bzw. ein neues Terminal öffnen."))
}

fn agent_check(env: &Env) -> Check {
    match which::agent_fingerprints(env) {
        Some(keys) => Check::new("agent", Status::Ok, "ssh-agent läuft").detail(format!("{} Keys geladen", keys.len())),
        None => Check::new("agent", Status::Warn, "Kein ssh-agent erreichbar")
            .detail("SSH_AUTH_SOCK ist nicht gesetzt oder der Agent läuft nicht. Keys mit Passphrase werden bei jeder Verbindung abgefragt."),
    }
}

/// Keys with a passphrase should be asked once and then kept by the agent.
/// Checks the files as written on disk (older Keybund versions did not set
/// `AddKeysToAgent` for git).
fn add_keys_check(env: &Env, config: &Config) -> Option<Check> {
    let active: Vec<_> = config.active_profiles().collect();
    if active.is_empty() {
        return None;
    }
    let mut missing = Vec::new();
    for p in &active {
        let path = env.profile_gitconfig(&p.id);
        let Some(content) = fsutil::read_optional(&path).ok().flatten() else { continue };
        let ok = content
            .lines()
            .filter(|l| l.trim_start().to_ascii_lowercase().starts_with("sshcommand"))
            .all(render::adds_keys_to_agent);
        if !ok {
            missing.push(p.name.clone());
        }
    }
    if let Some(conf) = fsutil::read_optional(&env.ssh_keybund_conf()).ok().flatten() {
        let hosts = conf.lines().filter(|l| l.trim_start().starts_with("Host ") && l.trim() != "Host *").count();
        let adds = conf.lines().filter(|l| l.trim().eq_ignore_ascii_case("AddKeysToAgent yes")).count();
        if adds < hosts {
            missing.push("~/.ssh/config.d/keybund.conf".into());
        }
    }
    Some(if missing.is_empty() {
        Check::new("add-keys", Status::Ok, "AddKeysToAgent aktiv")
    } else {
        Check::new("add-keys", Status::Warn, "AddKeysToAgent fehlt")
            .detail(format!(
                "Keys mit Passphrase werden bei jedem Zugriff neu abgefragt: {}. Configs neu schreiben.",
                missing.join(", ")
            ))
            .fix(Fix::Apply)
    })
}

/// `Host` blocks in the user's own `~/.ssh/config` that get in the way:
/// an `IdentityFile` for a host that profiles use via `core.sshCommand`
/// (github.com) is offered as well and can log in with the wrong key; a
/// block a profile already covers is a leftover from before the import.
fn ssh_config_checks(env: &Env, config: &Config) -> Vec<Check> {
    let ssh = fsutil::read_optional(&env.ssh_config()).ok().flatten().unwrap_or_default();
    let blocks = import::parse_ssh_config(&ssh);
    let migrated: Vec<&str> =
        import::migrated_blocks(env, config, &blocks).iter().map(|b| b.pattern.as_str()).collect();
    let shared: Vec<String> = config
        .active_profiles()
        .flat_map(|p| p.hosts.iter().filter(|h| h.alias.is_none()).map(|h| h.hostname.to_lowercase()))
        .collect();
    let mut checks = Vec::new();
    for b in blocks.iter().filter(|b| !b.identity_files.is_empty()) {
        let host = &b.pattern;
        let mixes = shared.contains(&host.to_lowercase());
        let covered = migrated.contains(&host.as_str());
        let check = match (mixes, covered) {
            (true, true) => Check::new(&format!("ssh-config-{host}"), Status::Warn, format!("Alter Eintrag für {host} in ~/.ssh/config"))
                .detail(format!("Er ist in Keybund übernommen, bietet aber weiter {} an – dadurch kann die Anmeldung mit dem falschen Key gelingen. Entfernen.", b.identity_files[0]))
                .fix(Fix::CleanupSshConfig),
            (true, false) => Check::new(&format!("ssh-config-{host}"), Status::Warn, format!("Globaler Eintrag für {host} in ~/.ssh/config"))
                .detail(format!("{} wird zusätzlich zum Profil-Key angeboten – die Anmeldung kann mit dem falschen Key gelingen. Mit „Bestehende SSH-Config übernehmen“ in Keybund übernehmen, dann entfernen.", b.identity_files[0])),
            (false, true) => Check::new(&format!("ssh-config-{host}"), Status::Warn, format!("Doppelter Eintrag für {host} in ~/.ssh/config"))
                .detail("Keybund schreibt diesen Host-Block selbst, der alte kann weg.")
                .fix(Fix::CleanupSshConfig),
            (false, false) => continue,
        };
        checks.push(check);
    }
    checks
}

fn key_age_checks(env: &Env, config: &Config) -> Vec<Check> {
    config
        .active_profiles()
        .filter_map(|p| {
            let info = keygen::key_info(env, &p.key);
            let days = info.age_days?;
            (days > OLD_KEY_DAYS).then(|| {
                Check::new(
                    &format!("key-age-{}", p.id),
                    Status::Warn,
                    format!("Key von „{}“ ist {} Jahre alt", p.name, days / 365),
                )
                .detail("Neuen Key erzeugen und den alten beim Provider entfernen.")
            })
        })
        .collect()
}

fn folder_checks(env: &Env, config: &Config) -> Vec<Check> {
    let missing: Vec<String> =
        config.active_profiles().flat_map(|p| p.folders.iter()).filter(|f| !env.expand(f).is_dir()).cloned().collect();
    if missing.is_empty() {
        return Vec::new();
    }
    vec![Check::new("folders", Status::Warn, "Projektordner existieren nicht").detail(missing.join(", "))]
}

/// Runs an automatic fix.
pub fn fix(env: &Env, fix: &Fix) -> Result<()> {
    match fix {
        Fix::Permissions => {
            if env.ssh_dir().exists() {
                fsutil::set_mode(&env.ssh_dir(), 0o700)?;
            }
            let config = store::load(env)?;
            for p in config.active_profiles() {
                let path = env.expand(&p.key.path);
                if path.exists() {
                    fsutil::set_mode(&path, 0o600)?;
                }
            }
            Ok(())
        }
        Fix::MoveSshInclude => {
            let path = env.ssh_config();
            let content = fsutil::read_optional(&path)?.unwrap_or_default();
            fsutil::backup_once(&path)?;
            fsutil::atomic_write(&path, &apply::ensure_ssh_include(&content), None)
        }
        Fix::Apply => {
            let config = store::load(env)?;
            apply::apply_config(env, &config).map(|_| ())
        }
        Fix::CleanupSshConfig => apply::apply(env, &import::plan_cleanup(env)?).map(|_| ()),
    }
}
