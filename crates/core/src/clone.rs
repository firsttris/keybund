//! Cloning a repository into the right profile. `includeIf` does not apply
//! while the repository does not exist yet, so the clone passes the
//! profile's `core.sshCommand` explicitly.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::{Config, Profile};
use crate::render::ssh_command;
use crate::{process, Env, Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoUrl {
    pub host: String,
    /// Path on the host, e.g. `team/portal-frontend.git`.
    pub path: String,
    /// Directory name, e.g. `portal-frontend`.
    pub name: String,
}

/// Parses `git@host:path`, `ssh://[user@]host[:port]/path` and
/// `https://host/path`.
pub fn parse_url(url: &str) -> Result<RepoUrl> {
    let url = url.trim();
    let invalid = || Error::InvalidUrl(url.to_string());
    let (host, path) = if let Some(rest) = url.split_once("://").map(|(_, r)| r) {
        let (authority, path) = rest.split_once('/').ok_or_else(invalid)?;
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = host.split(':').next().unwrap_or(host);
        (host.to_string(), path.to_string())
    } else if let Some((left, path)) = url.split_once(':') {
        let host = left.rsplit('@').next().unwrap_or(left);
        (host.to_string(), path.to_string())
    } else {
        return Err(invalid());
    };
    let path = path.trim_matches('/').to_string();
    let last = path.rsplit('/').next().unwrap_or(&path);
    let name = last.strip_suffix(".git").unwrap_or(last).to_string();
    if host.is_empty() || name.is_empty() || host.contains(char::is_whitespace) {
        return Err(invalid());
    }
    Ok(RepoUrl { host, path, name })
}

/// Profiles that have the URL's host (by hostname or alias).
pub fn matching_profiles<'a>(config: &'a Config, url: &RepoUrl) -> Vec<&'a Profile> {
    config
        .active_profiles()
        .filter(|p| {
            p.hosts.iter().any(|h| h.hostname.eq_ignore_ascii_case(&url.host) || h.alias.as_deref() == Some(&url.host))
        })
        .collect()
}

/// `<first profile folder>/<repo name>`.
pub fn target_dir(env: &Env, profile: &Profile, url: &RepoUrl) -> Result<PathBuf> {
    let folder = profile.folders.first().ok_or_else(|| Error::NoFolder(profile.id.clone()))?;
    Ok(env.expand(folder).join(&url.name))
}

/// Clones `url` for `profile` into `target` (or the default target dir).
pub fn clone(env: &Env, profile: &Profile, url: &str, target: Option<PathBuf>) -> Result<PathBuf> {
    let parsed = parse_url(url)?;
    let target = match target {
        Some(t) => t,
        None => target_dir(env, profile, &parsed)?,
    };
    if target.exists() {
        return Err(Error::TargetExists(env.contract(&target)));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut cmd = env.command("git");
    cmd.arg("clone")
        .arg("-c")
        .arg(format!("core.sshCommand={}", ssh_command(&profile.key.path)))
        .arg(url.trim())
        .arg(&target)
        .env("GIT_TERMINAL_PROMPT", "0");
    process::stdout(cmd)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        let u = parse_url("git@github.com:firsttris/keybund.git").unwrap();
        assert_eq!((u.host.as_str(), u.name.as_str()), ("github.com", "keybund"));
        let u = parse_url("ssh://git@gitlab.kunde.de:2222/team/portal-frontend.git").unwrap();
        assert_eq!(
            (u.host.as_str(), u.path.as_str(), u.name.as_str()),
            ("gitlab.kunde.de", "team/portal-frontend.git", "portal-frontend")
        );
        let u = parse_url("https://github.com/firsttris/keybund").unwrap();
        assert_eq!(u.name, "keybund");
        let u = parse_url("meinserver:repos/tool.git").unwrap();
        assert_eq!(u.host, "meinserver");
        assert!(parse_url("nonsense").is_err());
    }

    #[test]
    fn target_is_first_folder() {
        let env = Env::with_home("/home/t");
        let p = Profile { id: "k".into(), folders: vec!["~/dev/k/".into()], ..Default::default() };
        let u = parse_url("git@github.com:a/b.git").unwrap();
        assert_eq!(target_dir(&env, &p, &u).unwrap(), PathBuf::from("/home/t/dev/k/b"));
    }
}
