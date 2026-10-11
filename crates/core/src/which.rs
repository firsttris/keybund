//! "Welcher Key wird hier benutzt?" – asks ssh and git directly what they
//! would use, instead of guessing from Keybund's own data.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Msg;
use crate::model::{Config, KeyRef, Profile};
use crate::msg;
use crate::{gitenv, keygen, process, Env, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum WhichMode {
    Folder,
    Host,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WhichResult {
    pub query: String,
    pub mode: WhichMode,
    pub key_path: Option<String>,
    pub fingerprint: Option<String>,
    pub profile_id: Option<String>,
    /// Where the setting comes from, e.g. a gitconfig file or `ssh -G`.
    pub source: Option<String>,
    /// How it got there, e.g. `includeIf "gitdir:~/dev/kunde/"`.
    pub via: Option<Msg>,
    pub identities_only: Option<bool>,
    pub git_name: Option<String>,
    pub git_email: Option<String>,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// Whether the key is loaded in ssh-agent (`None`: no agent).
    pub agent_loaded: Option<bool>,
    pub notes: Vec<Msg>,
}

impl WhichResult {
    fn new(query: &str, mode: WhichMode) -> Self {
        WhichResult {
            query: query.to_string(),
            mode,
            key_path: None,
            fingerprint: None,
            profile_id: None,
            source: None,
            via: None,
            identities_only: None,
            git_name: None,
            git_email: None,
            hostname: None,
            user: None,
            port: None,
            agent_loaded: None,
            notes: Vec::new(),
        }
    }
}

/// Resolves a folder or host. Without a query, the current directory is used.
pub fn which(env: &Env, config: &Config, query: &str) -> Result<WhichResult> {
    let query = query.trim();
    let looks_like_path =
        query.is_empty() || query.starts_with(['/', '~', '.']) || query.contains('/') || Path::new(query).exists();
    let mut result = if looks_like_path { which_folder(env, config, query)? } else { which_host(env, config, query)? };

    if let Some(key) = &result.key_path {
        let info = keygen::key_info(env, &KeyRef { path: key.clone(), comment: None });
        result.fingerprint = info.fingerprint.clone();
        if !info.private_exists {
            result.notes.push(msg!("which_keyFileMissing", path = key));
        }
        match (agent_fingerprints(env), &info.fingerprint) {
            (Some(loaded), Some(fp)) => result.agent_loaded = Some(loaded.contains(fp)),
            (None, _) => result.notes.push(msg!("which_noAgent")),
            _ => {}
        }
    }
    Ok(result)
}

fn which_folder(env: &Env, config: &Config, query: &str) -> Result<WhichResult> {
    let dir = if query.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| env.home.clone())
    } else {
        let p = env.expand(query);
        if p.is_relative() {
            std::env::current_dir().map(|c| c.join(&p)).unwrap_or(p)
        } else {
            p
        }
    };
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let mut result = WhichResult::new(&env.contract(&dir), WhichMode::Folder);

    let in_repo = dir.is_dir() && git_in_repo(env, &dir);
    // git matches `gitdir:` against the repository's git directory – for a
    // linked worktree that lives with the main repo, not with the checkout.
    let folder_match = match in_repo.then(|| git_dir(env, &dir)).flatten() {
        Some(git_dir) => profile_for_folder(env, config, &git_dir),
        None => profile_for_folder(env, config, &dir),
    };

    if in_repo {
        if let Some((origin, value)) = git_get(env, &dir, "core.sshCommand") {
            result.key_path = identity_from_ssh_command(&value).map(|k| contract_str(env, &k));
            result.identities_only = Some(value.contains("IdentitiesOnly=yes"));
            result.source = Some(origin);
        }
        result.git_name = git_get(env, &dir, "user.name").map(|(_, v)| v);
        result.git_email = git_get(env, &dir, "user.email").map(|(_, v)| v);
        if let Some((p, folder)) = &folder_match {
            result.via = Some(msg!(
                "common_text",
                text = format!("includeIf \"{}\"", crate::render::gitdir_condition(folder, env.platform))
            ));
            if result.key_path.as_deref().map(|k| same_key(env, k, &p.key.path)) == Some(false) {
                result.notes.push(msg!("which_localOverride", name = &p.name));
            }
        }
        if result.key_path.is_none() {
            result.notes.push(msg!("which_noSshCommand"));
        }
    } else if let Some((p, folder)) = &folder_match {
        result.key_path = Some(p.key.path.clone());
        result.identities_only = Some(true);
        result.source = Some(env.contract(&env.profile_gitconfig(&p.id)));
        result.via = Some(msg!(
            "common_text",
            text = format!("includeIf \"{}\"", crate::render::gitdir_condition(folder, env.platform))
        ));
        // Empty fields of the key fall back to ~/.gitconfig, like git does.
        let global = gitenv::global_identity(env);
        let own = |v: Option<&String>| v.filter(|v| !v.trim().is_empty()).cloned();
        result.git_name = own(p.git.as_ref().map(|g| &g.name)).or(global.name);
        result.git_email = own(p.git.as_ref().map(|g| &g.email)).or(global.email);
        result.notes.push(msg!("which_notARepo"));
    } else if let Some(def) = config.default_entry() {
        result.key_path = Some(def.key.path.clone());
        result.identities_only = Some(true);
        result.source = Some(env.contract(&env.index_gitconfig()));
        result.via = Some(msg!("which_viaDefault"));
        result.notes.push(msg!("which_defaultApplies"));
    } else {
        result.notes.push(msg!("which_noKey"));
    }

    // GIT_SSH_COMMAND beats every core.sshCommand, including Keybund's.
    let overrides = gitenv::detect(env);
    if let Some(o) = overrides.iter().find(|o| o.name == "GIT_SSH_COMMAND") {
        result.key_path = identity_from_ssh_command(&o.value).map(|k| contract_str(env, &k));
        result.identities_only = Some(o.value.contains("IdentitiesOnly=yes"));
        result.source = Some(o.origin.clone());
        result.via = Some(msg!("common_text", text = "GIT_SSH_COMMAND"));
        result.notes.insert(0, msg!("which_gitSshCommand", origin = &o.origin, value = &o.value));
    }
    if let Some(o) = overrides.iter().find(|o| o.name == "GIT_CONFIG_GLOBAL") {
        result.notes.insert(0, msg!("which_gitConfigGlobal", origin = &o.origin, value = &o.value));
    }

    result.profile_id = result
        .key_path
        .as_deref()
        .and_then(|k| profile_for_key(env, config, k))
        .or(folder_match.map(|(p, _)| p))
        .map(|p| p.id.clone());
    Ok(result)
}

fn which_host(env: &Env, config: &Config, host: &str) -> Result<WhichResult> {
    let mut result = WhichResult::new(host, WhichMode::Host);
    // `--` inside: a query starting with `-` is a host, not an option.
    let out = match env.ssh_g(host) {
        Some(out) => out,
        None => {
            let mut cmd = env.command("ssh");
            cmd.args(["-G", "--", host]);
            process::stdout(cmd)?
        }
    };
    let mut identities = Vec::new();
    for line in out.lines() {
        let Some((k, v)) = line.split_once(' ') else { continue };
        match k {
            "hostname" => result.hostname = Some(v.to_string()),
            "user" => result.user = Some(v.to_string()),
            "port" => result.port = v.parse().ok(),
            "identitiesonly" => result.identities_only = Some(v == "yes"),
            "identityfile" => identities.push(env.from_ssh(v).display().to_string()),
            _ => {}
        }
    }
    result.source = Some("ssh -G".into());

    // ssh lists its default identity files when none is configured; prefer
    // the one that belongs to a profile.
    let by_profile = identities.iter().find_map(|i| profile_for_key(env, config, i).map(|p| (i, p)));
    let existing: Vec<&String> = identities.iter().filter(|i| env.expand(&contract_str(env, i)).exists()).collect();
    if let Some((identity, p)) = by_profile {
        result.key_path = Some(contract_str(env, identity));
        result.profile_id = Some(p.id.clone());
        if p.hosts.iter().any(|h| h.ssh_patterns().iter().any(|s| s.eq_ignore_ascii_case(host))) {
            result.source = Some(env.contract(&env.ssh_keybund_conf()));
            result.via = Some(msg!("common_text", text = format!("Host {host}")));
        }
    } else {
        result.key_path = existing.first().map(|i| contract_str(env, i));
    }
    // ssh tries every listed key that exists (agent-loaded ones first), so
    // the key named above is not necessarily the one that logs in.
    if existing.len() > 1 && result.identities_only != Some(true) {
        result.notes.push(msg!("which_triesKeys", n = existing.len()));
    }

    let hostname = result.hostname.clone().unwrap_or_else(|| host.to_string());
    let git_profiles: Vec<&Profile> = config
        .active_profiles()
        .filter(|p| p.hosts.iter().any(|h| h.alias.is_none() && h.hostname.eq_ignore_ascii_case(&hostname)))
        .collect();
    if !git_profiles.is_empty() {
        let names: Vec<&str> = git_profiles.iter().map(|p| p.name.as_str()).collect();
        result.notes.push(msg!("which_folderDecides", host = &hostname, names = names.join(", ")));
    }
    Ok(result)
}

/// Fingerprints of the keys loaded in ssh-agent; `None` if no agent runs.
pub fn agent_fingerprints(env: &Env) -> Option<Vec<String>> {
    agent_fingerprints_at(env, None)
}

/// The keys of the agent at `socket` (`None`: the own `SSH_AUTH_SOCK`).
pub fn agent_fingerprints_at(env: &Env, socket: Option<&str>) -> Option<Vec<String>> {
    let mut cmd = env.command("ssh-add");
    cmd.arg("-l");
    if let Some(sock) = socket {
        cmd.env("SSH_AUTH_SOCK", sock);
    }
    let out = process::output(cmd).ok()?;
    match out.status.code() {
        Some(0) => Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.split_whitespace().nth(1).map(str::to_string))
                .collect(),
        ),
        // 1: agent runs but has no identities.
        Some(1) => Some(Vec::new()),
        _ => None,
    }
}

fn git_in_repo(env: &Env, dir: &Path) -> bool {
    let mut cmd = env.command("git");
    cmd.arg("-C").arg(dir).args(["rev-parse", "--git-dir"]);
    process::output(cmd).map(|o| o.status.success()).unwrap_or(false)
}

/// The absolute git directory of the repository at `dir`.
fn git_dir(env: &Env, dir: &Path) -> Option<PathBuf> {
    let mut cmd = env.command("git");
    cmd.arg("-C").arg(dir).args(["rev-parse", "--absolute-git-dir"]);
    process::stdout(cmd).ok().map(|s| PathBuf::from(s.trim())).filter(|p| p.is_absolute())
}

/// `git config --show-origin --get <key>` → (origin, value). With `-z` git
/// prints the origin raw (otherwise a non-ASCII home like `/home/jörg`
/// comes back C-quoted); a relative origin (`.git/config`) is anchored at `dir`.
fn git_get(env: &Env, dir: &Path, key: &str) -> Option<(String, String)> {
    let mut cmd = env.command("git");
    cmd.arg("-C").arg(dir).args(["config", "-z", "--show-origin", "--get", key]);
    let out = process::stdout(cmd).ok()?;
    let mut parts = out.split('\0');
    let origin = parts.next()?;
    let value = parts.next()?;
    let origin = origin.strip_prefix("file:").unwrap_or(origin);
    let origin =
        if Path::new(origin).is_absolute() { origin.to_string() } else { dir.join(origin).display().to_string() };
    Some((contract_str(env, &origin), value.to_string()))
}

/// Extracts the `-i` argument from an ssh command line.
pub fn identity_from_ssh_command(cmd: &str) -> Option<String> {
    let words = split_shell_words(cmd);
    let mut it = words.iter();
    while let Some(w) = it.next() {
        if w == "-i" {
            return it.next().cloned();
        }
        if let Some(rest) = w.strip_prefix("-i") {
            if !rest.is_empty() && !w.starts_with("-i ") {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Minimal sh word splitting (single/double quotes, backslash).
fn split_shell_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                has = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '"' => {
                has = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => cur.extend(chars.next()),
                        c => cur.push(c),
                    }
                }
            }
            '\\' => {
                has = true;
                cur.extend(chars.next());
            }
            c if c.is_whitespace() => {
                if has {
                    words.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                has = true;
                cur.push(c);
            }
        }
    }
    if has {
        words.push(cur);
    }
    words
}

fn contract_str(env: &Env, path: &str) -> String {
    env.contract(&env.expand(path))
}

fn same_key(env: &Env, a: &str, b: &str) -> bool {
    env.expand(a) == env.expand(b)
}

fn profile_for_key<'a>(env: &Env, config: &'a Config, key: &str) -> Option<&'a Profile> {
    config.profiles.iter().find(|p| same_key(env, &p.key.path, key))
}

/// Profile whose folder contains `dir`, with the matching folder.
pub fn profile_for_folder<'a>(env: &Env, config: &'a Config, dir: &Path) -> Option<(&'a Profile, String)> {
    let mut dir_s = dir.display().to_string();
    if !dir_s.ends_with('/') {
        dir_s.push('/');
    }
    config
        .active_profiles()
        .flat_map(|p| p.folders.iter().map(move |f| (p, f)))
        .filter(|(_, f)| {
            let expanded: PathBuf = env.expand(f);
            let mut e = expanded.display().to_string();
            if !e.ends_with('/') {
                e.push('/');
            }
            // macOS: `gitdir/i:`, the same folder in other case still matches.
            if env.platform.case_insensitive_paths() {
                dir_s.to_lowercase().starts_with(&e.to_lowercase())
            } else {
                dir_s.starts_with(&e)
            }
        })
        .max_by_key(|(_, f)| f.len())
        .map(|(p, f)| (p, f.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_identity() {
        assert_eq!(identity_from_ssh_command("ssh -i ~/.ssh/a -o IdentitiesOnly=yes").as_deref(), Some("~/.ssh/a"));
        assert_eq!(identity_from_ssh_command("ssh -i '/x y/a'").as_deref(), Some("/x y/a"));
        assert_eq!(identity_from_ssh_command("ssh -i~/.ssh/b").as_deref(), Some("~/.ssh/b"));
        assert_eq!(identity_from_ssh_command("ssh -o X=y"), None);
    }

    #[test]
    fn folder_matching_uses_longest_prefix() {
        let env = Env::with_home("/home/t");
        let cfg = Config {
            profiles: vec![
                Profile { id: "a".into(), folders: vec!["~/dev/a/".into()], ..Default::default() },
                Profile { id: "b".into(), folders: vec!["~/dev/ab/".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let (p, f) = profile_for_folder(&env, &cfg, Path::new("/home/t/dev/ab/repo")).unwrap();
        assert_eq!((p.id.as_str(), f.as_str()), ("b", "~/dev/ab/"));
        assert!(profile_for_folder(&env, &cfg, Path::new("/home/t/dev/abc")).is_none());
        assert_eq!(profile_for_folder(&env, &cfg, Path::new("/home/t/dev/a")).unwrap().0.id, "a");
    }
}
