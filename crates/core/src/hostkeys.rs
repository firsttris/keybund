//! Host keys: Keybund never trusts a server's key silently.
//!
//! GitHub, GitLab and Bitbucket publish their keys; those are pinned here and
//! added without asking. For any other host the user sees the fingerprints
//! (from `ssh-keyscan`) and confirms them before the first connection.
//! Servers of a profile get their own known_hosts file
//! (`~/.ssh/known_hosts.d/<id>`, see [`crate::render::ssh_config`]).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::keygen::PublicKey;
use crate::model::{HostEntry, Profile, Provider, Settings};
use crate::{fsutil, process, render, Env, Error, Result};

/// Published host keys (ed25519 and ECDSA) of the big git hosts, as of 2026:
/// <https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints>,
/// <https://docs.gitlab.com/user/gitlab_com/#ssh-host-keys-fingerprints>,
/// <https://bitbucket.org/site/ssh>.
pub const PINNED: &[(&str, &str)] = &[
    ("github.com", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"),
    (
        "github.com",
        "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg=",
    ),
    ("gitlab.com", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAfuCHKVTjquxvt6CM6tdG4SLp1Btn/nOeHHE5UOzRdf"),
    (
        "gitlab.com",
        "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBFSMqzJeV9rUzU4kWitGjeR4PWSa29SPqJ1fVkhtj3Hw9xjLVXVYrU9QlYWrOLXBpQ6KWjbjTDTdDkoohFzgbEY=",
    ),
    ("bitbucket.org", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIazEu89wgQZ4bqs3d63QSMzYVa0MuJ2e2gKTKqu+UUO"),
    (
        "bitbucket.org",
        "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBPIQmuzMBuKdWeF4+a2sjSSpBK0iqitSQ+5BM9KhpexuGt20JpTVM7u5BDZngncgrqDMbWdxMWWOGtZ9UgbqgZE=",
    ),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HostKey {
    pub key_type: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HostKeyCheck {
    /// In a known_hosts file already.
    Known,
    /// A published key of GitHub/GitLab/Bitbucket, added just now.
    Pinned,
    /// First contact: the user has to confirm one of these.
    Unknown { host: String, file: String, keys: Vec<HostKey> },
    /// No answer from the host; the connection will tell why.
    Unreachable { host: String },
}

/// The name ssh looks up in known_hosts: `host` or `[host]:port`.
pub fn known_name(host: &HostEntry) -> String {
    let name = host.hostname.to_ascii_lowercase();
    match host.port {
        Some(port) if port != 22 => format!("[{name}]:{port}"),
        _ => name,
    }
}

/// The known_hosts files ssh itself uses for `host`, from `ssh -G`:
/// `(user files, global files)`. The user's own `UserKnownHostsFile` (under
/// `Host *`, say) counts – a key stored anywhere else would never be read.
/// `None` when ssh cannot tell (not installed, a broken config).
fn ssh_known_hosts(env: &Env, host: &HostEntry) -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
    let out = env.ssh_g(host.target_name())?;
    let list = |key: &str| -> Vec<PathBuf> {
        out.lines()
            .find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix(' ')))
            .map(|v| v.split_whitespace().map(|p| env.from_ssh(p)).collect())
            .unwrap_or_default()
    };
    let user = list("userknownhostsfile");
    (!user.is_empty()).then(|| (user, list("globalknownhostsfile")))
}

/// Where a new key of `host` is stored: the key's own file for its servers,
/// otherwise the first file ssh reads – the one ssh itself would add to.
pub fn target_file(env: &Env, settings: &Settings, profile: &Profile, host: &HostEntry) -> PathBuf {
    if settings.known_hosts_per_profile && host.provider == Provider::Generic {
        return env.expand(&render::profile_known_hosts(&profile.id));
    }
    ssh_known_hosts(env, host).and_then(|(user, _)| user.into_iter().next()).unwrap_or_else(|| env.known_hosts())
}

/// Every file a key of `host` may already be in: the target and the files
/// ssh reads – so a key stored per key before the setting was switched off
/// (no longer read by ssh) does not count as known.
fn files(env: &Env, settings: &Settings, profile: &Profile, host: &HostEntry) -> Vec<PathBuf> {
    let mut files = vec![target_file(env, settings, profile, host)];
    match ssh_known_hosts(env, host) {
        Some((user, global)) => {
            files.extend(user);
            // In a test home the machine's files stay out, as before.
            if env.global_known_hosts.is_some() {
                files.extend(global);
            }
        }
        None => {
            files.push(env.known_hosts());
            files.extend(env.global_known_hosts.clone());
        }
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|f| seen.insert(f.clone()));
    files
}

fn is_known_in(env: &Env, file: &Path, name: &str) -> bool {
    if !file.exists() {
        return false;
    }
    let mut cmd = env.command("ssh-keygen");
    cmd.arg("-F").arg(name).arg("-f").arg(file);
    process::output(cmd).is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}

/// `true` if some known_hosts file has a key for `host`.
pub fn is_known(env: &Env, settings: &Settings, profile: &Profile, host: &HostEntry) -> bool {
    let name = known_name(host);
    files(env, settings, profile, host).iter().any(|f| is_known_in(env, f, &name))
}

/// The pinned keys of a well-known host on the default port.
pub fn pinned(host: &HostEntry) -> Vec<&'static str> {
    if host.port.is_some_and(|p| p != 22) {
        return Vec::new();
    }
    let name = host.hostname.to_ascii_lowercase();
    PINNED.iter().filter(|(h, _)| *h == name).map(|(_, k)| *k).collect()
}

/// Parses `ssh-keyscan` output into `(key, "type blob")`.
pub fn parse_scan(output: &str) -> Vec<(HostKey, String)> {
    output
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let _name = parts.next()?;
            let key = PublicKey::parse(&format!("{} {}", parts.next()?, parts.next()?))?;
            Some((HostKey { key_type: key.key_type.clone(), fingerprint: key.fingerprint() }, key.without_comment()))
        })
        .collect()
}

/// The key types asked for, one `ssh-keyscan` round each, until one answers.
/// ssh-keyscan opens a connection per key type (five by default), and
/// sshd 9.8+ blocks an address that keeps connecting without logging in
/// (`PerSourcePenalties`, noauth): a test, a check and a confirmation in a
/// row could lock the user out for a while. One Ed25519 key is enough –
/// ssh prefers the host key type it already knows (`HostKeyAlgorithms`
/// ordering in sshconnect2.c) and learns the others via `UpdateHostKeys`.
const SCAN_ROUNDS: [&str; 2] = ["ed25519", "ecdsa,rsa"];

/// Asks the host for its keys (`ssh-keyscan`).
pub fn scan(env: &Env, host: &HostEntry) -> Result<Vec<(HostKey, String)>> {
    for types in SCAN_ROUNDS {
        let mut cmd = env.command("ssh-keyscan");
        cmd.args(["-T", "5", "-t", types]);
        if let Some(port) = host.port {
            cmd.arg("-p").arg(port.to_string());
        }
        cmd.arg("--").arg(&host.hostname);
        let out = process::output(cmd)?;
        let keys = parse_scan(&String::from_utf8_lossy(&out.stdout));
        if !keys.is_empty() {
            return Ok(keys);
        }
        // No SSH banner at all: the host is not reachable, asking again only waits.
        let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        if !said.contains("SSH-") {
            break;
        }
    }
    Ok(Vec::new())
}

fn append(path: &Path, name: &str, keys: &[String]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fsutil::ensure_dir(dir, Some(0o700))?;
    }
    // A hand-edited file may lack the final newline; the first key would glue onto its last line.
    let needs_newline = std::fs::read(path).ok().is_some_and(|c| !c.is_empty() && !c.ends_with(b"\n"));
    let existed = path.exists();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| Error::io(path, e))?;
    if needs_newline {
        writeln!(file).map_err(|e| Error::io(path, e))?;
    }
    for key in keys {
        writeln!(file, "{name} {key}").map_err(|e| Error::io(path, e))?;
    }
    // A file Keybund creates is private; the user's own known_hosts keeps its mode.
    if existed {
        Ok(())
    } else {
        fsutil::set_mode(path, 0o600)
    }
}

/// Checks the host key before connecting: known, pinned (added now), or
/// unknown with the fingerprints to confirm.
pub fn check(env: &Env, settings: &Settings, profile: &Profile, host: &HostEntry) -> Result<HostKeyCheck> {
    if is_known(env, settings, profile, host) {
        return Ok(HostKeyCheck::Known);
    }
    let pinned = pinned(host);
    if !pinned.is_empty() {
        let keys: Vec<String> = pinned.iter().map(|k| k.to_string()).collect();
        append(&target_file(env, settings, profile, host), &known_name(host), &keys)?;
        return Ok(HostKeyCheck::Pinned);
    }
    let keys = scan(env, host)?;
    if keys.is_empty() {
        return Ok(HostKeyCheck::Unreachable { host: host.hostname.clone() });
    }
    Ok(HostKeyCheck::Unknown {
        host: known_name(host),
        file: env.contract(&target_file(env, settings, profile, host)),
        keys: keys.into_iter().map(|(k, _)| k).collect(),
    })
}

/// Stores the host's keys after the user confirmed `fingerprints`. The host
/// is asked again; only keys with a confirmed fingerprint are stored.
pub fn trust(
    env: &Env,
    settings: &Settings,
    profile: &Profile,
    host: &HostEntry,
    fingerprints: &[String],
) -> Result<usize> {
    let keys: Vec<String> = scan(env, host)?
        .into_iter()
        .filter(|(k, _)| fingerprints.contains(&k.fingerprint))
        .map(|(_, line)| line)
        .collect();
    if keys.is_empty() {
        return Err(Error::HostKeyChanged(host.hostname.clone()));
    }
    append(&target_file(env, settings, profile, host), &known_name(host), &keys)?;
    Ok(keys.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(hostname: &str, port: Option<u16>, provider: Provider) -> HostEntry {
        HostEntry { hostname: hostname.into(), port, provider, ..Default::default() }
    }

    #[test]
    fn names_files_and_pins() {
        assert_eq!(known_name(&host("GitHub.com", None, Provider::Github)), "github.com");
        assert_eq!(known_name(&host("1.2.3.4", Some(22), Provider::Generic)), "1.2.3.4");
        assert_eq!(known_name(&host("1.2.3.4", Some(2222), Provider::Generic)), "[1.2.3.4]:2222");

        let env = Env::with_home("/home/t");
        let p = Profile { id: "kunde".into(), ..Default::default() };
        let mut s = Settings::default();
        let server = host("1.2.3.4", None, Provider::Generic);
        assert_eq!(target_file(&env, &s, &p, &server), PathBuf::from("/home/t/.ssh/known_hosts.d/kunde"));
        assert_eq!(target_file(&env, &s, &p, &host("github.com", None, Provider::Github)), env.known_hosts());
        s.known_hosts_per_profile = false;
        assert_eq!(target_file(&env, &s, &p, &server), env.known_hosts());

        assert_eq!(pinned(&host("github.com", None, Provider::Github)).len(), 2);
        assert!(pinned(&host("github.com", Some(443), Provider::Github)).is_empty());
        assert!(pinned(&host("gitlab.kunde.de", None, Provider::Gitlab)).is_empty());
    }

    #[test]
    fn pinned_keys_match_the_published_fingerprints() {
        let fp = |line: &str| PublicKey::parse(line).unwrap().fingerprint();
        let fps: Vec<(&str, String)> = PINNED.iter().map(|(h, k)| (*h, fp(k))).collect();
        for expected in [
            ("github.com", "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"),
            ("github.com", "SHA256:p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM"),
            ("gitlab.com", "SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8"),
            ("gitlab.com", "SHA256:HbW3g8zUjNSksFbqTiUWPWg2Bq1x8xdGUrliXFzSnUw"),
            ("bitbucket.org", "SHA256:ybgmFkzwOSotHTHLJgHO0QN8L0xErw6vd0VhFA9m3SM"),
            ("bitbucket.org", "SHA256:FC73VB6C4OQLSCrjEayhMp9UMxS97caD/Yyi2bhW/J0"),
        ] {
            assert!(fps.contains(&(expected.0, expected.1.to_string())), "{expected:?}");
        }
    }

    #[test]
    fn parses_keyscan_output() {
        let out = "# 1.2.3.4:22 SSH-2.0-OpenSSH_9.6\n\
                   1.2.3.4 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n\
                   [1.2.3.4]:2222 kaputt\n";
        let keys = parse_scan(out);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].0.key_type, "ssh-ed25519");
        assert_eq!(keys[0].0.fingerprint, "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU");
        assert!(keys[0].1.starts_with("ssh-ed25519 AAAA"));
    }

    #[test]
    fn pinned_hosts_are_added_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        let p = Profile { id: "p".into(), ..Default::default() };
        let gh = host("github.com", None, Provider::Github);
        let s = Settings::default();
        if std::process::Command::new("ssh-keygen").arg("-?").output().is_err() {
            return;
        }
        assert!(!is_known(&env, &s, &p, &gh));
        assert_eq!(check(&env, &s, &p, &gh).unwrap(), HostKeyCheck::Pinned);
        let text = std::fs::read_to_string(env.known_hosts()).unwrap();
        assert!(text.starts_with("github.com ssh-ed25519 AAAA"), "{text}");
        assert_eq!(check(&env, &s, &p, &gh).unwrap(), HostKeyCheck::Known);
    }
}
