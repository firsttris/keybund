//! Cloning a repository into the right profile's folder. Git would take the
//! folder's key there by itself – it reads the new repository's config,
//! includeIf included, before it connects (builtin/clone.c) – but the
//! profile's `core.sshCommand` is passed explicitly as well, so the clone
//! does not depend on the folder rule being written yet.

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

/// `[2001:db8::1]` at the start of `s`: the address and what follows `]`.
fn bracketed(s: &str) -> Option<(&str, &str)> {
    let inner = s.strip_prefix('[')?;
    let (address, rest) = inner.split_once(']')?;
    (!address.is_empty() && address.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.'))
        .then_some((address, rest))
}

/// `[user@]host` with an IPv6 address in brackets; returns the host and what
/// follows it.
fn host_part(s: &str) -> (&str, &str) {
    let after_user = s.rsplit_once('@').map_or(s, |(_, h)| h);
    match bracketed(after_user) {
        Some((address, rest)) => (address, rest),
        None => match after_user.split_once(':') {
            Some((host, rest)) => (host, rest),
            None => (after_user, ""),
        },
    }
}

/// Parses `git@host:path`, `ssh://[user@]host[:port]/path` and
/// `https://host/path`; an IPv6 host goes in brackets (`ssh://[2001:db8::1]:22/x`,
/// `git@[2001:db8::1]:x`).
pub fn parse_url(url: &str) -> Result<RepoUrl> {
    let url = url.trim();
    let invalid = || Error::InvalidUrl(url.to_string());
    // The URL goes to `git clone`: nothing that git could read as an option
    // or a special transport (`ext::`, local paths). `::` inside an IPv6
    // address in brackets is no transport.
    let outside_brackets: String = {
        let mut depth = 0;
        url.chars()
            .filter(|c| {
                match c {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                depth == 0 && *c != ']'
            })
            .collect()
    };
    if url.starts_with('-')
        || outside_brackets.contains("::")
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid());
    }
    if let Some((scheme, _)) = url.split_once("://") {
        if !["ssh", "git+ssh", "ssh+git", "https", "http", "git"].contains(&scheme.to_ascii_lowercase().as_str()) {
            return Err(invalid());
        }
    }
    let (host, path) = if let Some(rest) = url.split_once("://").map(|(_, r)| r) {
        let (authority, path) = rest.split_once('/').ok_or_else(invalid)?;
        let (host, port) = host_part(authority);
        if !(port.is_empty() || port.trim_start_matches(':').chars().all(|c| c.is_ascii_digit())) {
            return Err(invalid());
        }
        (host.to_string(), path.to_string())
    } else {
        // scp-like: `[user@]host:path`, the host may be `[v6]`.
        let (host, rest) = host_part(url);
        let path = rest.strip_prefix(':').unwrap_or(rest);
        if host.is_empty() || (path.is_empty() && !rest.starts_with(':')) || !url.contains(':') {
            return Err(invalid());
        }
        (host.to_string(), path.to_string())
    };
    let path = path.trim_matches('/').to_string();
    let last = path.rsplit('/').next().unwrap_or(&path);
    let name = last.strip_suffix(".git").unwrap_or(last).to_string();
    let host_ok = !host.is_empty()
        && !host.starts_with('-')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-_:".contains(c));
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
            p.hosts.iter().any(|h| {
                h.hostname.eq_ignore_ascii_case(&url.host)
                    || h.alias.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(&url.host))
            })
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
        .arg(format!("core.sshCommand={}", ssh_command(&profile.key.path, env.platform)))
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
    fn ipv6_hosts_and_aliases_in_capitals() {
        let u = parse_url("ssh://git@[2001:db8::1]:22/team/app.git").unwrap();
        assert_eq!((u.host.as_str(), u.path.as_str(), u.name.as_str()), ("2001:db8::1", "team/app.git", "app"));
        let u = parse_url("git@[2001:db8::1]:team/app.git").unwrap();
        assert_eq!((u.host.as_str(), u.name.as_str()), ("2001:db8::1", "app"));
        // Still no transport and no option through brackets.
        for bad in ["ext::[x]", "ssh://[2001:db8::1]:x/a", "[ext::sh]:a/b", "ssh://[::1]/a::b"] {
            if let Ok(u) = parse_url(bad) {
                assert!(
                    !u.host.contains("ext") && u.host.chars().all(|c| c.is_ascii_hexdigit() || c == ':'),
                    "{bad}: {u:?}"
                );
            }
        }
        assert!(parse_url("ext::sh -c x").is_err());
        // Aliases are stored in lower case; a URL typed in capitals still finds the key.
        let config = Config {
            profiles: vec![Profile {
                id: "k".into(),
                hosts: vec![crate::model::HostEntry {
                    alias: Some("github-kunde".into()),
                    hostname: "github.com".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let u = parse_url("GITHUB-KUNDE:org/repo.git").unwrap();
        assert_eq!(matching_profiles(&config, &u).len(), 1);
        let v6 = Config {
            profiles: vec![Profile {
                id: "v6".into(),
                hosts: vec![crate::model::HostEntry { hostname: "2001:db8::1".into(), ..Default::default() }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(matching_profiles(&v6, &parse_url("ssh://[2001:db8::1]/a.git").unwrap()).len(), 1);
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
