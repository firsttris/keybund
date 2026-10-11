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
    /// Write the `Include` with the full path (old libssh, Dolphin) and apply.
    AbsoluteSshInclude,
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
                Ok(()) => checks.push(Check::new("profiles", Status::Ok, format!("{} Keys geladen", c.profiles.len()))),
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
    checks.extend(global_identity_file_check(env));
    checks.extend(own_ssh_command_check(env, &config));
    checks.extend(key_age_checks(env, &config));
    checks.extend(folder_checks(env, &config));
    let dolphin = process::find_program("dolphin").is_some() || kio_sftp_installed();
    checks.extend(dolphin_check(dolphin, libssh_version(), &config));
    Report { checks }
}

/// libssh before 0.10.2 does not expand `~` in `Include`.
const LIBSSH_TILDE_INCLUDE: (u32, u32, u32) = (0, 10, 2);

const LIB_DIRS: [&str; 6] = [
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/usr/lib64",
    "/usr/lib",
    "/lib/x86_64-linux-gnu",
    "/lib64",
];

/// libssh's version from its file name: `libssh.so.4.M.P` is libssh `0.(M+1).P`.
pub fn libssh_from_soname(name: &str) -> Option<(u32, u32, u32)> {
    let rest = name.strip_prefix("libssh.so.4.")?;
    let mut parts = rest.split('.');
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    Some((0, minor + 1, patch))
}

/// The installed libssh (used by Dolphin/KIO for sftp://), if found.
fn libssh_version() -> Option<(u32, u32, u32)> {
    LIB_DIRS.iter().find_map(|dir| {
        std::fs::read_dir(dir).ok()?.flatten().find_map(|e| libssh_from_soname(&e.file_name().to_string_lossy()))
    })
}

fn kio_sftp_installed() -> bool {
    LIB_DIRS.iter().any(|dir| {
        ["qt6/plugins/kf6/kio/kio_sftp.so", "qt5/plugins/kf5/kio/sftp.so", "plugins/kf5/kio/sftp.so"]
            .iter()
            .any(|p| std::path::Path::new(dir).join(p).exists())
    })
}

/// Dolphin/KIO reads the ssh config with libssh. Only checked when Dolphin
/// or the sftp plugin is installed.
pub fn dolphin_check(installed: bool, libssh: Option<(u32, u32, u32)>, config: &Config) -> Option<Check> {
    if !installed {
        return None;
    }
    let Some(v) = libssh else {
        return Some(Check::new("dolphin", Status::Ok, "Dolphin gefunden, libssh-Version unbekannt").detail(
            "Zum Prüfen sftp://<alias> in Dolphin öffnen: Es sollte ohne Passwort mit dem Key aus Keybund verbinden.",
        ));
    };
    let version = format!("{}.{}.{}", v.0, v.1, v.2);
    Some(if v >= LIBSSH_TILDE_INCLUDE || config.settings.absolute_ssh_include {
        Check::new("dolphin", Status::Ok, format!("Dolphin (libssh {version}) liest die Keybund-Config"))
    } else {
        Check::new("dolphin", Status::Warn, format!("Dolphin sieht die Keybund-Config nicht (libssh {version})"))
            .detail(
                "libssh vor 0.10.2 versteht ~ in Include nicht – sftp:// in Dolphin nutzt dann nicht den Key aus Keybund. \
                 Abhilfe: Include mit vollem Pfad schreiben.",
            )
            .fix(Fix::AbsoluteSshInclude)
    })
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
        // sshd (StrictModes) only rejects ~/.ssh when others can write to it.
        checks.push(if mode & 0o022 == 0 {
            Check::new("ssh-dir-perms", Status::Ok, format!("~/.ssh hat Rechte {mode:o}"))
        } else {
            Check::new("ssh-dir-perms", Status::Error, format!("~/.ssh hat Rechte {mode:o} statt 700"))
                .detail("Andere dürfen in ~/.ssh schreiben – sshd verweigert dann die Anmeldung mit Keys.")
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
    let index = env.contract(&env.index_gitconfig());
    checks.push(if apply::git_include_last(&git, &index) {
        Check::new("git-include", Status::Ok, "~/.gitconfig bindet Keybund ein")
    } else if apply::git_include_present(&git, &index) {
        Check::new("git-include", Status::Warn, "Keybund steht nicht mehr am Ende von ~/.gitconfig")
            .detail("Was danach steht (z. B. von git config --global), überschreibt die Einstellungen deiner Keys.")
            .fix(Fix::Apply)
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
                "GIT_SSH_COMMAND ist in {} gesetzt („{}“). Git benutzt dann in jedem Repo diesen Befehl statt des Keys aus Keybund.",
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
                .detail(format!("{} wird zusätzlich zum Key aus Keybund angeboten – die Anmeldung kann mit dem falschen Key gelingen. Mit „Bestehende SSH-Config übernehmen“ in Keybund übernehmen, dann entfernen.", b.identity_files[0])),
            (false, true) => Check::new(&format!("ssh-config-{host}"), Status::Warn, format!("Doppelter Eintrag für {host} in ~/.ssh/config"))
                .detail("Keybund schreibt diesen Host-Block selbst, der alte kann weg.")
                .fix(Fix::CleanupSshConfig),
            (false, false) => continue,
        };
        checks.push(check);
    }
    checks
}

/// `IdentityFile` lines in `~/.ssh/config` that apply to every host (global,
/// `Host *`, `Match`): ssh adds them to Keybund's hosts as well, and a key
/// loaded in the agent is offered first – the login can then succeed with
/// the wrong account. IdentitiesOnly does not stop that.
pub fn global_identity_files(ssh_config: &str) -> Vec<String> {
    let mut wide = true; // before the first Host/Match: global
    let mut out = Vec::new();
    for line in ssh_config.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let (key, value) = t.split_once(|c: char| c.is_whitespace() || c == '=').unwrap_or((t, ""));
        let value = value.trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim();
        match key.to_ascii_lowercase().as_str() {
            "host" => wide = value.split_whitespace().any(|p| p.contains(['*', '?'])),
            "match" => wide = true,
            "identityfile" if wide => out.push(value.trim_matches('"').to_string()),
            _ => {}
        }
    }
    out
}

fn global_identity_file_check(env: &Env) -> Vec<Check> {
    let ssh = fsutil::read_optional(&env.ssh_config()).ok().flatten().unwrap_or_default();
    let files = global_identity_files(&ssh);
    if files.is_empty() {
        return Vec::new();
    }
    vec![Check::new("ssh-global-identity", Status::Warn, "IdentityFile für alle Hosts in ~/.ssh/config")
        .detail(format!(
            "{} wird bei jeder Verbindung zusätzlich angeboten, auch zu den Hosts deiner Keys. Ist er im ssh-agent, kommt er zuerst – die Anmeldung kann dann mit dem falschen Konto gelingen. Besser nur in den Host-Blöcken angeben, die ihn brauchen.",
            files.join(", ")
        ))]
}

/// The user's own `core.sshCommand` in `~/.gitconfig` is replaced by the
/// standard key outside the git folders – worth knowing.
fn own_ssh_command_check(env: &Env, config: &Config) -> Vec<Check> {
    let Some(def) = config.default_entry() else { return Vec::new() };
    let mut cmd = env.command("git");
    cmd.arg("config").arg("--file").arg(env.gitconfig()).args(["--get", "core.sshCommand"]);
    let Ok(own) = crate::process::stdout(cmd) else { return Vec::new() };
    let own = own.trim();
    if own.is_empty() {
        return Vec::new();
    }
    vec![Check::new("git-own-ssh-command", Status::Warn, "Standard-Key ersetzt dein core.sshCommand")
        .detail(format!(
            "~/.gitconfig setzt „{own}“, außerhalb der Git-Ordner gilt aber der Standard-Key „{}“. Entferne die Zeile oder nimm den Standard zurück.",
            def.name
        ))]
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
    vec![Check::new("folders", Status::Warn, "Git-Ordner existieren nicht").detail(missing.join(", "))]
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
            let include = render::ssh_include_line(env, &store::load(env)?.settings);
            fsutil::backup_once(&path)?;
            fsutil::atomic_write(&path, &apply::ensure_ssh_include(&content, &include), None)
        }
        Fix::Apply => {
            let config = store::load(env)?;
            apply::apply_config(env, &config).map(|_| ())
        }
        Fix::CleanupSshConfig => apply::apply(env, &import::plan_cleanup(env)?).map(|_| ()),
        Fix::AbsoluteSshInclude => {
            let mut config = store::load(env)?;
            config.settings.absolute_ssh_include = true;
            store::save(env, &config)?;
            apply::apply_config(env, &config).map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn finds_identity_files_for_every_host() {
        let cfg = "Include ~/.ssh/config.d/keybund.conf\nIdentityFile ~/.ssh/global\nHost github.com\n  IdentityFile ~/.ssh/gh\nHost *.corp *\n  IdentityFile ~/.ssh/star\nMatch host x\n  IdentityFile=~/.ssh/match\n";
        assert_eq!(global_identity_files(cfg), ["~/.ssh/global", "~/.ssh/star", "~/.ssh/match"]);
        assert!(global_identity_files("Host a\n  IdentityFile ~/.ssh/a\n").is_empty());
    }

    use super::*;

    #[test]
    fn dolphin_needs_libssh_0_10_2_for_the_include() {
        assert_eq!(libssh_from_soname("libssh.so.4.9.6"), Some((0, 10, 6)));
        assert_eq!(libssh_from_soname("libssh.so.4.8.7"), Some((0, 9, 7)));
        assert_eq!(libssh_from_soname("libssh.so.4"), None);
        assert_eq!(libssh_from_soname("libssh_threads.so.4.8.7"), None);

        let mut config = Config::default();
        assert!(dolphin_check(false, Some((0, 9, 6)), &config).is_none());
        assert_eq!(dolphin_check(true, Some((0, 10, 6)), &config).unwrap().status, Status::Ok);
        let old = dolphin_check(true, Some((0, 9, 6)), &config).unwrap();
        assert_eq!((old.status, old.fix), (Status::Warn, Some(Fix::AbsoluteSshInclude)));
        assert!(old.title.contains("libssh 0.9.6"));
        assert_eq!(dolphin_check(true, Some((0, 10, 1)), &config).unwrap().status, Status::Warn);
        config.settings.absolute_ssh_include = true;
        assert_eq!(dolphin_check(true, Some((0, 9, 6)), &config).unwrap().status, Status::Ok);
        assert!(dolphin_check(true, None, &config).unwrap().detail.unwrap().contains("sftp://"));
    }
}
