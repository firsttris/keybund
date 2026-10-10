//! Taking over an existing `~/.ssh/config`: every `Host` block with an
//! `IdentityFile` becomes a proposal for a profile that uses the existing
//! key (nothing is moved). After the import, the migrated blocks can be
//! removed from `~/.ssh/config`, so they no longer mix their key into the
//! profiles' connections.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::apply::{self, Plan};
use crate::model::{Config, HostEntry, KeyRef, Profile, Provider};
use crate::{fsutil, store, Env, Error, Result};

/// One `Host` block of an ssh config.
#[derive(Debug, Clone, PartialEq)]
pub struct HostBlock {
    /// The single host pattern, e.g. `github.com` or `pflegeabc`.
    pub pattern: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_files: Vec<String>,
    /// Comment line directly above the block, e.g. `Privater Account`.
    pub comment: Option<String>,
    /// Lines `[start, end)` of the block including its comment.
    pub start: usize,
    pub end: usize,
}

fn keyword_value(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let (k, v) = match t.find(|c: char| c.is_whitespace() || c == '=') {
        Some(i) => (&t[..i], t[i..].trim_start_matches(|c: char| c.is_whitespace() || c == '=')),
        None => (t, ""),
    };
    Some((k.to_ascii_lowercase(), v.trim().trim_matches('"').to_string()))
}

/// Parses the `Host` blocks with exactly one plain pattern (no wildcards,
/// no negation). `Match` blocks and global options are ignored.
pub fn parse_ssh_config(text: &str) -> Vec<HostBlock> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&i| keyword_value(lines[i]).is_some_and(|(k, _)| k == "host" || k == "match"))
        .collect();
    let mut blocks = Vec::new();
    for (n, &i) in starts.iter().enumerate() {
        let (keyword, value) = keyword_value(lines[i]).unwrap_or_default();
        let next = starts.get(n + 1).copied().unwrap_or(lines.len());
        // The block ends before blank lines and comments that lead into the next one.
        let mut end = next;
        while end > i + 1 && (lines[end - 1].trim().is_empty() || lines[end - 1].trim_start().starts_with('#')) {
            end -= 1;
        }
        if keyword != "host" {
            continue;
        }
        let patterns: Vec<&str> = value.split_whitespace().collect();
        if patterns.len() != 1 || patterns[0].contains(['*', '?', '!']) {
            continue;
        }
        // A comment directly above the Host line belongs to the block.
        let mut start = i;
        while start > 0 && lines[start - 1].trim_start().starts_with('#') {
            start -= 1;
        }
        let comment = (start < i)
            .then(|| lines[i - 1].trim_start().trim_start_matches('#').trim().to_string())
            .filter(|c| !c.is_empty());
        let mut block = HostBlock {
            pattern: patterns[0].to_string(),
            hostname: None,
            user: None,
            port: None,
            identity_files: Vec::new(),
            comment,
            start,
            end,
        };
        for line in &lines[i + 1..end] {
            match keyword_value(line) {
                Some((k, v)) if k == "hostname" => block.hostname = Some(v),
                Some((k, v)) if k == "user" => block.user = Some(v),
                Some((k, v)) if k == "port" => block.port = v.parse().ok(),
                Some((k, v)) if k == "identityfile" => block.identity_files.push(v),
                _ => {}
            }
        }
        blocks.push(block);
    }
    blocks
}

/// A profile Keybund proposes to create from existing ssh config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportProposal {
    /// Prefilled profile; name, id, color and folders can be changed.
    pub profile: Profile,
    /// Use as default profile (the key for the host outside of all folders).
    pub make_default: bool,
    /// The `Host` patterns this proposal replaces.
    pub blocks: Vec<String>,
    pub key_exists: bool,
    /// Hints for the user, e.g. that a folder is needed.
    pub notes: Vec<String>,
}

/// Turns a key path into a profile id: `~/.ssh/privat_ed25519` → `privat`.
fn id_from_key(path: &str) -> Option<String> {
    let stem = Path::new(path).file_name()?.to_string_lossy().into_owned();
    let stem = ["_ed25519", "_ecdsa", "_rsa", "_ed25519_sk"]
        .iter()
        .fold(stem, |s, suffix| s.trim_end_matches(suffix).to_string());
    if stem.starts_with("id_") || stem == "id" {
        return None;
    }
    Some(store::slugify(&stem)).filter(|s| !s.is_empty())
}

/// Proposals for all blocks of `ssh_config` that are not yet covered by a
/// profile, one per key.
pub fn proposals(env: &Env, config: &Config, ssh_config: &str) -> Vec<ImportProposal> {
    let blocks = parse_ssh_config(ssh_config);
    let mut by_key: BTreeMap<String, Vec<&HostBlock>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for b in &blocks {
        let Some(identity) = b.identity_files.first() else { continue };
        let key = env.contract(&env.expand(identity));
        let covered = config.profiles.iter().any(|p| {
            env.expand(&p.key.path) == env.expand(&key)
                && p.hosts.iter().any(|h| {
                    h.target_name().eq_ignore_ascii_case(&b.pattern) || h.hostname.eq_ignore_ascii_case(&b.pattern)
                })
        });
        if covered {
            continue;
        }
        if !by_key.contains_key(&key) {
            order.push(key.clone());
        }
        by_key.entry(key).or_default().push(b);
    }

    let mut taken: Vec<String> = config.profiles.iter().map(|p| p.id.clone()).collect();
    let mut default_given = config.default_profile.is_some();
    let mut out = Vec::new();
    for key in order {
        let blocks = &by_key[&key];
        let first = blocks[0];
        let name = first.comment.clone().unwrap_or_else(|| first.pattern.clone());
        let base = id_from_key(&key).unwrap_or_else(|| store::slugify(&first.pattern));
        let mut id = base.clone();
        let mut n = 2;
        while taken.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        taken.push(id.clone());

        let mut notes = Vec::new();
        let mut make_default = false;
        let hosts: Vec<HostEntry> = blocks
            .iter()
            .map(|b| {
                let hostname = b.hostname.clone().unwrap_or_else(|| b.pattern.clone());
                let provider = Provider::detect(&hostname);
                let same_name = hostname.eq_ignore_ascii_case(&b.pattern);
                // A plain `Host github.com` becomes the default profile for that
                // host (keys via core.sshCommand, no Host block, so it cannot mix
                // with other profiles on the same host). Only one can be default.
                let alias = if same_name && provider.is_git_host() && !default_given {
                    default_given = true;
                    make_default = true;
                    None
                } else {
                    Some(b.pattern.clone())
                };
                let user = b.user.clone().filter(|u| !(provider.is_git_host() && u == "git"));
                HostEntry { alias, hostname, user, port: b.port.filter(|p| *p != 22), provider }
            })
            .collect();
        if make_default {
            notes.push(format!(
                "Wird Default-Profil: gilt für {} in allen Ordnern ohne eigenes Profil",
                hosts.iter().find(|h| h.alias.is_none()).map(|h| h.hostname.as_str()).unwrap_or_default()
            ));
        }
        if blocks.iter().any(|b| b.identity_files.len() > 1) {
            notes.push("Mehrere IdentityFile-Einträge – übernommen wird der erste".into());
        }
        let key_exists = env.expand(&key).exists();
        if !key_exists {
            notes.push(format!("Key-Datei {key} existiert nicht"));
        }
        out.push(ImportProposal {
            profile: Profile {
                id,
                name,
                color: None,
                key: KeyRef { path: key.clone(), comment: None },
                git: None,
                folders: Vec::new(),
                archived: false,
                hosts,
            },
            make_default,
            blocks: blocks.iter().map(|b| b.pattern.clone()).collect(),
            key_exists,
            notes,
        });
    }
    out
}

/// Proposals for the current `~/.ssh/config`.
pub fn proposals_for(env: &Env) -> Result<Vec<ImportProposal>> {
    let config = store::load(env)?;
    let ssh = fsutil::read_optional(&env.ssh_config())?.unwrap_or_default();
    Ok(proposals(env, &config, &ssh))
}

/// Saves the chosen proposals as profiles. Configs are written by the next apply.
pub fn import(env: &Env, chosen: &[ImportProposal]) -> Result<Vec<String>> {
    let mut config = store::load(env)?;
    let mut ids = Vec::new();
    for proposal in chosen {
        let mut profile = proposal.profile.clone();
        profile.folders =
            profile.folders.iter().filter(|f| !f.trim().is_empty()).map(|f| store::normalize_folder(env, f)).collect();
        if config.profiles.iter().any(|p| p.id == profile.id) {
            return Err(Error::DuplicateId(profile.id));
        }
        if proposal.make_default {
            config.default_profile = Some(profile.id.clone());
        }
        ids.push(profile.id.clone());
        config.profiles.push(profile);
    }
    store::save(env, &config)?;
    Ok(ids)
}

/// Blocks of `ssh_config` that a profile now covers with the same key.
pub fn migrated_blocks<'a>(env: &Env, config: &Config, blocks: &'a [HostBlock]) -> Vec<&'a HostBlock> {
    blocks
        .iter()
        .filter(|b| {
            let Some(identity) = b.identity_files.first() else { return false };
            config.active_profiles().any(|p| {
                env.expand(&p.key.path) == env.expand(identity)
                    && p.hosts.iter().any(|h| {
                        h.alias.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(&b.pattern))
                            || (h.alias.is_none() && h.hostname.eq_ignore_ascii_case(&b.pattern))
                    })
            })
        })
        .collect()
}

/// `ssh_config` without the given blocks (and the blank line after each).
pub fn without_blocks(ssh_config: &str, remove: &[&HostBlock]) -> String {
    let lines: Vec<&str> = ssh_config.lines().collect();
    let mut drop = vec![false; lines.len()];
    for b in remove {
        let mut end = b.end;
        if end < lines.len() && lines[end].trim().is_empty() {
            end += 1;
        }
        for d in drop.iter_mut().take(end).skip(b.start) {
            *d = true;
        }
    }
    let kept: Vec<&str> = lines.iter().zip(&drop).filter(|(_, d)| !**d).map(|(l, _)| *l).collect();
    let mut out = kept.join("\n");
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The change to `~/.ssh/config` that removes all migrated blocks.
pub fn plan_cleanup(env: &Env) -> Result<Plan> {
    let config = store::load(env)?;
    let current = fsutil::read_optional(&env.ssh_config())?.unwrap_or_default();
    let blocks = parse_ssh_config(&current);
    let migrated = migrated_blocks(env, &config, &blocks);
    if migrated.is_empty() {
        return Ok(Plan::default());
    }
    apply::plan_user_file(env, &env.ssh_config(), without_blocks(&current, &migrated))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ssh config of the issue report.
    const USER_CONFIG: &str = "Include ~/.ssh/config.d/keybund.conf

# Privater Account
Host github.com
    HostName github.com
    User git
    IdentityFile ~/.ssh/privat_ed25519
    IdentitiesOnly yes

# Pflegeabc Account
Host pflegeabc
    HostName github.com
    User git
    IdentityFile ~/.ssh/pflegeabc/id_ed25519
    IdentitiesOnly yes

# S-Communication
Host gitlab.sfp.berlin
    HostName gitlab.sfp.berlin
    User git
    IdentityFile ~/.ssh/s-communication_ed25519
    IdentitiesOnly yes

Host *.internal !x.internal
    User admin

Match host foo
    IdentityFile ~/.ssh/foo
";

    #[test]
    fn parses_host_blocks() {
        let blocks = parse_ssh_config(USER_CONFIG);
        let patterns: Vec<&str> = blocks.iter().map(|b| b.pattern.as_str()).collect();
        assert_eq!(patterns, ["github.com", "pflegeabc", "gitlab.sfp.berlin"]);
        let gh = &blocks[0];
        assert_eq!(gh.comment.as_deref(), Some("Privater Account"));
        assert_eq!(gh.identity_files, ["~/.ssh/privat_ed25519"]);
        assert_eq!(gh.user.as_deref(), Some("git"));
        let lines: Vec<&str> = USER_CONFIG.lines().collect();
        assert_eq!(lines[gh.start], "# Privater Account");
        assert_eq!(lines[gh.end - 1], "    IdentitiesOnly yes");
        assert_eq!(blocks[1].hostname.as_deref(), Some("github.com"));
    }

    #[test]
    fn proposes_profiles_like_the_user_would() {
        let env = Env::with_home("/home/t");
        let p = proposals(&env, &Config::default(), USER_CONFIG);
        assert_eq!(p.len(), 3);

        let privat = &p[0];
        assert_eq!((privat.profile.id.as_str(), privat.profile.name.as_str()), ("privat", "Privater Account"));
        assert!(privat.make_default);
        assert_eq!(
            privat.profile.hosts,
            [HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() }]
        );

        let pflege = &p[1];
        assert_eq!(pflege.profile.id, "pflegeabc", "id_ed25519 says nothing, the alias does");
        assert!(!pflege.make_default);
        assert_eq!(pflege.profile.hosts[0].alias.as_deref(), Some("pflegeabc"));
        assert_eq!(pflege.profile.hosts[0].hostname, "github.com");
        assert_eq!(pflege.profile.key.path, "~/.ssh/pflegeabc/id_ed25519");

        let scomm = &p[2];
        assert_eq!(scomm.profile.id, "s-communication");
        assert_eq!(
            scomm.profile.hosts[0].alias.as_deref(),
            Some("gitlab.sfp.berlin"),
            "a second plain host keeps its Host block"
        );
        assert_eq!(scomm.profile.hosts[0].provider, Provider::Gitlab);
        assert!(scomm.profile.hosts[0].user.is_none());
    }

    #[test]
    fn existing_default_and_imported_blocks_are_respected() {
        let env = Env::with_home("/home/t");
        let mut config = Config::default();
        config.profiles.push(Profile {
            id: "privat".into(),
            name: "Privat".into(),
            key: KeyRef { path: "~/.ssh/privat_ed25519".into(), comment: None },
            hosts: vec![HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() }],
            ..Default::default()
        });
        config.default_profile = Some("privat".into());
        let p = proposals(&env, &config, USER_CONFIG);
        assert_eq!(p.iter().map(|x| x.profile.id.as_str()).collect::<Vec<_>>(), ["pflegeabc", "s-communication"]);
        assert!(p.iter().all(|x| !x.make_default));

        // The privat block is now covered by a profile and can be removed.
        let blocks = parse_ssh_config(USER_CONFIG);
        let migrated = migrated_blocks(&env, &config, &blocks);
        assert_eq!(migrated.iter().map(|b| b.pattern.as_str()).collect::<Vec<_>>(), ["github.com"]);
        let cleaned = without_blocks(USER_CONFIG, &migrated);
        assert!(cleaned.starts_with("Include ~/.ssh/config.d/keybund.conf\n\n# Pflegeabc Account\nHost pflegeabc\n"));
        assert!(!cleaned.contains("privat_ed25519"));
        assert!(!cleaned.contains("Privater Account"));
        assert!(cleaned.contains("Host *.internal"));
        assert!(cleaned.ends_with("    IdentityFile ~/.ssh/foo\n"));
    }

    #[test]
    fn ids_never_collide() {
        let env = Env::with_home("/home/t");
        let text = "Host a\n  IdentityFile ~/.ssh/work_ed25519\nHost b\n  HostName b.example.org\n  IdentityFile ~/.ssh/work2/work_ed25519\n";
        let p = proposals(&env, &Config::default(), text);
        assert_eq!(p.iter().map(|x| x.profile.id.as_str()).collect::<Vec<_>>(), ["work", "work-2"]);
    }

    #[test]
    fn one_profile_per_key_with_several_hosts() {
        let env = Env::with_home("/home/t");
        let text = "Host github.com\n  IdentityFile ~/.ssh/me_ed25519\nHost gitlab.com\n  IdentityFile ~/.ssh/me_ed25519\n  Port 2222\n";
        let p = proposals(&env, &Config::default(), text);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].blocks, ["github.com", "gitlab.com"]);
        assert_eq!(p[0].profile.hosts[1].alias.as_deref(), Some("gitlab.com"));
        assert_eq!(p[0].profile.hosts[1].port, Some(2222));
    }
}
