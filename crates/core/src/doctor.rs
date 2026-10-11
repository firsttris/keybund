//! Health checks for everything Keybund depends on. Each pitfall from the
//! plan has a check here; some come with an automatic fix.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::env::Platform;
use crate::i18n::Msg;
use crate::model::Config;
use crate::msg;
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
    pub title: Msg,
    pub detail: Option<Msg>,
    pub fix: Option<Fix>,
}

impl Check {
    fn new(id: &str, status: Status, title: Msg) -> Self {
        Check { id: id.into(), status, title, detail: None, fix: None }
    }
    fn detail(mut self, d: Msg) -> Self {
        self.detail = Some(d);
        self
    }
    /// A detail that is a plain list (paths, names) and needs no translation.
    fn list(self, items: &[String]) -> Self {
        self.detail(msg!("common_text", text = items.join(", ")))
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
                    checks.push(Check::new("profiles", Status::Ok, msg!("doctor_profiles_ok", n = c.profiles.len())))
                }
                Err(e) => {
                    checks.push(Check::new("profiles", Status::Error, msg!("doctor_profiles_invalid")).detail(e.msg()))
                }
            }
            c
        }
        Err(e) => {
            checks.push(Check::new("profiles", Status::Error, msg!("doctor_profiles_unreadable")).detail(e.msg()));
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
    // Dolphin and KIO only exist on Linux desktops.
    if env.platform == Platform::Linux {
        let dolphin = process::find_program("dolphin").is_some() || kio_sftp_installed();
        checks.extend(dolphin_check(dolphin, libssh_version(), &config));
    }
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
        return Some(
            Check::new("dolphin", Status::Ok, msg!("doctor_dolphin_unknown"))
                .detail(msg!("doctor_dolphin_unknown_detail")),
        );
    };
    let version = format!("{}.{}.{}", v.0, v.1, v.2);
    Some(if v >= LIBSSH_TILDE_INCLUDE || config.settings.absolute_ssh_include {
        Check::new("dolphin", Status::Ok, msg!("doctor_dolphin_ok", version = &version))
    } else {
        Check::new("dolphin", Status::Warn, msg!("doctor_dolphin_old", version = &version))
            .detail(msg!("doctor_dolphin_old_detail"))
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
                    Some(Check::new(&format!("tool-{tool}"), Status::Error, msg!("doctor_toolMissing", tool = tool)))
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
            Check::new("ssh-dir-perms", Status::Ok, msg!("doctor_sshDir_ok", mode = format!("{mode:o}")))
        } else {
            Check::new("ssh-dir-perms", Status::Error, msg!("doctor_sshDir_bad", mode = format!("{mode:o}")))
                .detail(msg!("doctor_sshDir_bad_detail"))
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
        checks.push(Check::new("keys-missing", Status::Error, msg!("doctor_keysMissing")).list(&missing));
    }
    if !config.profiles.is_empty() {
        checks.push(if bad.is_empty() {
            Check::new("key-perms", Status::Ok, msg!("doctor_keyPerms_ok"))
        } else {
            Check::new("key-perms", Status::Error, msg!("doctor_keyPerms_bad")).list(&bad).fix(Fix::Permissions)
        });
    }
    // What the ssh client itself refuses: a config file (also an included
    // one) that others may write to, or that belongs to someone else
    // ("Bad owner or permissions", readconf.c) – then no Host block applies.
    let unsafe_files: Vec<String> = ssh_config_files(env)
        .into_iter()
        .filter(|f| f.exists() && !config_file_ok(env, f))
        .map(|f| env.contract(&f))
        .collect();
    if env.ssh_config().exists() {
        checks.push(if unsafe_files.is_empty() {
            Check::new("config-perms", Status::Ok, msg!("doctor_sshConfigPerms_ok"))
        } else {
            Check::new("config-perms", Status::Error, msg!("doctor_sshConfigPerms_bad"))
                .list(&unsafe_files)
                .detail(msg!("doctor_sshConfigPerms_bad_detail"))
                .fix(Fix::Permissions)
        });
    }
    checks
}

/// The files ssh reads for its configuration: the user's and Keybund's.
fn ssh_config_files(env: &Env) -> Vec<std::path::PathBuf> {
    vec![env.ssh_config(), env.ssh_keybund_conf()]
}

/// ssh's rule for a config file: owned by the user (or root) and not
/// writable by group or others.
fn config_file_ok(env: &Env, path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(meta) = std::fs::metadata(path) else { return true };
        let me = std::fs::metadata(&env.home).map(|m| m.uid()).unwrap_or(meta.uid());
        (meta.uid() == me || meta.uid() == 0) && meta.mode() & 0o022 == 0
    }
    #[cfg(not(unix))]
    {
        let _ = (env, path);
        true
    }
}

fn include_checks(env: &Env) -> Vec<Check> {
    let mut checks = Vec::new();
    let ssh = fsutil::read_optional(&env.ssh_config()).ok().flatten().unwrap_or_default();
    checks.push(if apply::ssh_include_on_top(&ssh) {
        Check::new("ssh-include", Status::Ok, msg!("doctor_sshInclude_ok"))
    } else if apply::ssh_include_present(&ssh) {
        Check::new("ssh-include", Status::Error, msg!("doctor_sshInclude_notTop"))
            .detail(msg!("doctor_sshInclude_notTop_detail"))
            .fix(Fix::MoveSshInclude)
    } else {
        Check::new("ssh-include", Status::Warn, msg!("doctor_sshInclude_missing")).fix(Fix::Apply)
    });
    let git = fsutil::read_optional(&env.gitconfig()).ok().flatten().unwrap_or_default();
    let index = env.contract(&env.index_gitconfig());
    checks.push(if apply::git_include_last(&git, &index) {
        Check::new("git-include", Status::Ok, msg!("doctor_gitInclude_ok"))
    } else if apply::git_include_present(&git, &index) {
        Check::new("git-include", Status::Warn, msg!("doctor_gitInclude_notLast"))
            .detail(msg!("doctor_gitInclude_notLast_detail"))
            .fix(Fix::Apply)
    } else {
        Check::new("git-include", Status::Warn, msg!("doctor_gitInclude_missing")).fix(Fix::Apply)
    });
    checks
}

fn up_to_date_check(env: &Env, config: &Config) -> Check {
    match apply::plan(env, config) {
        Ok(plan) if plan.is_empty() => Check::new("configs", Status::Ok, msg!("doctor_configs_ok")),
        Ok(plan) => Check::new("configs", Status::Warn, msg!("doctor_configs_stale", n = plan.changes.len()))
            .list(&plan.changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>())
            .fix(Fix::Apply),
        Err(e) => Check::new("configs", Status::Error, msg!("doctor_configs_error")).detail(e.msg()),
    }
}

/// Variables that switch Keybund off for git, wherever they are set.
fn git_env_check(env: &Env) -> Check {
    let found = gitenv::detect(env);
    let Some(first) = found.first() else {
        return Check::new("git-env", Status::Ok, msg!("doctor_gitEnv_ok"));
    };
    // The first one; the next shows up once that is gone.
    let detail = match first.name.as_str() {
        "GIT_SSH_COMMAND" => msg!("doctor_gitEnv_sshCommand", origin = &first.origin, value = &first.value),
        _ => msg!("doctor_gitEnv_config", name = &first.name, origin = &first.origin, value = &first.value),
    };
    Check::new("git-env", Status::Error, msg!("doctor_gitEnv_bad", name = &first.name)).detail(detail)
}

fn agent_check(env: &Env) -> Check {
    if let Some(keys) = which::agent_fingerprints(env) {
        return Check::new("agent", Status::Ok, msg!("doctor_agent_ok"))
            .detail(msg!("doctor_agent_keys", n = keys.len()));
    }
    // An agent started in .bashrc/.zshrc exists for the terminal only: git
    // there works, while the app, Dolphin and the IDE from the menu ask again.
    let own = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();
    let terminal = gitenv::login_shell_var(env, "SSH_AUTH_SOCK")
        .filter(|sock| *sock != own && which::agent_fingerprints_at(env, Some(sock)).is_some());
    match terminal {
        Some(sock) => Check::new("agent", Status::Warn, msg!("doctor_agent_terminalOnly"))
            .detail(msg!("doctor_agent_terminalOnly_detail", socket = sock)),
        None => Check::new("agent", Status::Warn, msg!("doctor_agent_missing")).detail(match env.platform {
            Platform::Linux => msg!("doctor_agent_missing_detail"),
            // launchd starts one for every login; without it something unset SSH_AUTH_SOCK.
            Platform::MacOs => msg!("doctor_agent_missing_detail_macos"),
        }),
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
        Check::new("add-keys", Status::Ok, msg!("doctor_addKeys_ok"))
    } else {
        Check::new("add-keys", Status::Warn, msg!("doctor_addKeys_missing"))
            .detail(msg!("doctor_addKeys_missing_detail", files = missing.join(", ")))
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
            (true, true) => {
                Check::new(&format!("ssh-config-{host}"), Status::Warn, msg!("doctor_sshConfig_old", host = host))
                    .detail(msg!("doctor_sshConfig_old_detail", key = &b.identity_files[0]))
                    .fix(Fix::CleanupSshConfig)
            }
            (true, false) => {
                Check::new(&format!("ssh-config-{host}"), Status::Warn, msg!("doctor_sshConfig_global", host = host))
                    .detail(msg!("doctor_sshConfig_global_detail", key = &b.identity_files[0]))
            }
            (false, true) => {
                Check::new(&format!("ssh-config-{host}"), Status::Warn, msg!("doctor_sshConfig_duplicate", host = host))
                    .detail(msg!("doctor_sshConfig_duplicate_detail"))
                    .fix(Fix::CleanupSshConfig)
            }
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
    vec![Check::new("ssh-global-identity", Status::Warn, msg!("doctor_globalIdentity"))
        .detail(msg!("doctor_globalIdentity_detail", files = files.join(", ")))]
}

/// The user's own `core.sshCommand` in `~/.gitconfig` is replaced by the
/// standard key outside the git folders – worth knowing.
fn own_ssh_command_check(env: &Env, config: &Config) -> Vec<Check> {
    let Some(def) = config.default_entry() else { return Vec::new() };
    // git reads both global files; ~/.gitconfig after ~/.config/git/config.
    let own = [env.xdg_gitconfig(), env.gitconfig()].into_iter().filter(|f| f.exists()).find_map(|file| {
        let mut cmd = env.command("git");
        cmd.arg("config").arg("--file").arg(file).args(["--get", "core.sshCommand"]);
        crate::process::stdout(cmd).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    });
    let Some(own) = own else { return Vec::new() };
    let own = own.as_str();
    vec![Check::new("git-own-ssh-command", Status::Warn, msg!("doctor_ownSshCommand")).detail(msg!(
        "doctor_ownSshCommand_detail",
        command = own,
        name = &def.name
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
                    msg!("doctor_keyAge", name = &p.name, years = days / 365),
                )
                .detail(msg!("doctor_keyAge_detail"))
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
    vec![Check::new("folders", Status::Warn, msg!("doctor_folders")).list(&missing)]
}

/// Runs an automatic fix.
pub fn fix(env: &Env, fix: &Fix) -> Result<()> {
    match fix {
        // Only what is wrong is touched: a 0400 key stays 0400, a private
        // directory keeps its mode.
        Fix::Permissions => {
            if fsutil::mode(&env.ssh_dir()).is_some_and(|m| m & 0o022 != 0) {
                fsutil::set_mode(&env.ssh_dir(), 0o700)?;
            }
            let config = store::load(env)?;
            for p in config.active_profiles() {
                let path = env.expand(&p.key.path);
                if fsutil::mode(&path).is_some_and(|m| m & 0o077 != 0) {
                    fsutil::set_mode(&path, 0o600)?;
                }
            }
            for file in ssh_config_files(env) {
                if let Some(mode) = fsutil::mode(&file).filter(|m| m & 0o022 != 0) {
                    fsutil::set_mode(&file, mode & !0o022)?;
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
        assert!(old.title.to_string().contains("libssh 0.9.6"));
        assert_eq!(dolphin_check(true, Some((0, 10, 1)), &config).unwrap().status, Status::Warn);
        config.settings.absolute_ssh_include = true;
        assert_eq!(dolphin_check(true, Some((0, 9, 6)), &config).unwrap().status, Status::Ok);
        assert!(dolphin_check(true, None, &config).unwrap().detail.unwrap().to_string().contains("sftp://"));
    }
}
