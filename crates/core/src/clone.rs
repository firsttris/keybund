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
    // The URL goes to `git clone`: nothing that git could read as an option
    // or a special transport (`ext::`, local paths).
    if url.starts_with('-') || url.contains("::") || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid());
    }
    if let Some((scheme, _)) = url.split_once("://") {
        if !["ssh", "git+ssh", "ssh+git", "https", "http", "git"].contains(&scheme.to_ascii_lowercase().as_str()) {
            return Err(invalid());
        }
    }
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
    let host_ok = !host.is_empty()
        && !host.starts_with('-')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-_[]".contains(c));
    let name_ok = !name.is_empty() && name != "." && name != ".." && !name.contains([':', '\\']);
    if !host_ok || !name_ok {
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
    // `git -c` (not `clone -c`): the key is used for this clone only and not
    // stored in the repo, so includeIf keeps deciding later (key changes, archive).
    let mut cmd = env.command("git");
    cmd.arg("-c")
        .arg(format!("core.sshCommand={}", ssh_command(&profile.key.path)))
        .arg("clone")
        .arg("--")
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
    fn rejects_urls_git_would_read_as_options() {
        for bad in [
            "--upload-pack=id>/tmp/p;:a/x:b",
            "-oProxyCommand=x:a/b",
            "ext::sh -c touch% /tmp/x",
            "file:///etc/passwd",
            "git@host name:a/b.git",
            "git@-host:a/b.git",
            "git@host:a/..",
        ] {
            assert!(parse_url(bad).is_err(), "{bad}");
        }
    }

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
