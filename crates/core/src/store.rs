//! Loading, validating and saving `profiles.toml`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fsutil;
use crate::model::{Config, GitIdentity, HostEntry, KeyRef, Profile, Provider};
use crate::{Env, Error, Result};

// The on-disk layout uses TOML-friendly singular names (`[[profile]]`,
// `[[profile.host]]`); the in-memory model uses plural field names.

#[derive(Serialize, Deserialize, Default)]
struct TomlConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_profile: Option<String>,
    #[serde(default, rename = "profile")]
    profiles: Vec<TomlProfile>,
}

#[derive(Serialize, Deserialize)]
struct TomlProfile {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    key: TomlKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git: Option<TomlGit>,
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    archived: bool,
    #[serde(default, rename = "host", skip_serializing_if = "Vec::is_empty")]
    hosts: Vec<TomlHost>,
}

#[derive(Serialize, Deserialize)]
struct TomlKey {
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct TomlGit {
    name: String,
    email: String,
    #[serde(default)]
    sign: bool,
}

#[derive(Serialize, Deserialize)]
struct TomlHost {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    hostname: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(default)]
    provider: Provider,
}

impl From<TomlProfile> for Profile {
    fn from(p: TomlProfile) -> Self {
        Profile {
            id: p.id,
            name: p.name,
            color: p.color,
            key: KeyRef { path: p.key.path, comment: p.key.comment },
            git: p.git.map(|g| GitIdentity { name: g.name, email: g.email, sign: g.sign }),
            folders: p.folders,
            archived: p.archived,
            hosts: p
                .hosts
                .into_iter()
                .map(|h| HostEntry {
                    alias: h.alias,
                    hostname: h.hostname,
                    user: h.user,
                    port: h.port,
                    provider: h.provider,
                })
                .collect(),
        }
    }
}

impl From<Profile> for TomlProfile {
    fn from(p: Profile) -> Self {
        TomlProfile {
            id: p.id,
            name: p.name,
            color: p.color,
            key: TomlKey { path: p.key.path, comment: p.key.comment },
            git: p.git.map(|g| TomlGit { name: g.name, email: g.email, sign: g.sign }),
            folders: p.folders,
            archived: p.archived,
            hosts: p
                .hosts
                .into_iter()
                .map(|h| TomlHost {
                    alias: h.alias,
                    hostname: h.hostname,
                    user: h.user,
                    port: h.port,
                    provider: h.provider,
                })
                .collect(),
        }
    }
}

pub fn parse(text: &str) -> Result<Config> {
    let raw: TomlConfig = toml::from_str(text).map_err(|e| Error::ConfigParse(e.to_string()))?;
    Ok(Config { default_profile: raw.default_profile, profiles: raw.profiles.into_iter().map(Into::into).collect() })
}

pub fn to_toml(config: &Config) -> String {
    let raw = TomlConfig {
        default_profile: config.default_profile.clone(),
        profiles: config.profiles.iter().cloned().map(Into::into).collect(),
    };
    toml::to_string_pretty(&raw).expect("profile data is always serializable")
}

/// Loads `profiles.toml`; a missing file yields an empty config.
pub fn load(env: &Env) -> Result<Config> {
    match fsutil::read_optional(&env.profiles_file())? {
        Some(text) => parse(&text),
        None => Ok(Config::default()),
    }
}

/// Validates and atomically writes `profiles.toml`.
pub fn save(env: &Env, config: &Config) -> Result<()> {
    validate(config)?;
    for p in config.active_profiles() {
        if !env.expand(&p.key.path).exists() {
            return Err(Error::KeyMissing(p.key.path.clone()));
        }
    }
    fsutil::ensure_dir(&env.config_dir, Some(0o700))?;
    let header = "# Keybund profiles – source of truth for all generated SSH and Git configs.\n\n";
    fsutil::atomic_write(&env.profiles_file(), &format!("{header}{}", to_toml(config)), Some(0o600))
}

/// Structural checks that do not touch the file system.
pub fn validate(config: &Config) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    let mut aliases: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &config.profiles {
        if !is_valid_id(&p.id) {
            return Err(Error::InvalidId(p.id.clone()));
        }
        if !ids.insert(p.id.as_str()) {
            return Err(Error::DuplicateId(p.id.clone()));
        }
        if p.name.trim().is_empty() {
            return Err(Error::MissingName(p.id.clone()));
        }
        if p.archived {
            continue;
        }
        for h in &p.hosts {
            for pattern in h.ssh_patterns() {
                if !aliases.insert(pattern.to_lowercase()) {
                    return Err(Error::DuplicateAlias(pattern));
                }
            }
        }
    }
    if let Some(def) = &config.default_profile {
        if !config.active_profiles().any(|p| &p.id == def) {
            return Err(Error::DefaultProfileMissing(def.clone()));
        }
    }
    // Folders must not be nested, neither within a profile nor across
    // profiles: includeIf would apply both and the later one wins silently.
    let folders: Vec<&str> = config.active_profiles().flat_map(|p| p.folders.iter().map(String::as_str)).collect();
    for (i, a) in folders.iter().enumerate() {
        for b in &folders[i + 1..] {
            if folder_contains(a, b) || folder_contains(b, a) {
                return Err(Error::NestedFolders(a.to_string(), b.to_string()));
            }
        }
    }
    Ok(())
}

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Derives an id from a display name: `Kunde C` → `kunde-c`.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let mapped = match c {
            'ä' => "ae".to_string(),
            'ö' => "oe".to_string(),
            'ü' => "ue".to_string(),
            'ß' => "ss".to_string(),
            c if c.is_ascii_alphanumeric() => c.to_string(),
            _ => "-".to_string(),
        };
        if mapped == "-" && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push_str(&mapped);
    }
    out.trim_end_matches('-').to_string()
}

/// `true` if folder `outer` contains (or equals) folder `inner`.
/// Both are expected with a trailing slash.
fn folder_contains(outer: &str, inner: &str) -> bool {
    inner.starts_with(outer)
}

/// Normalizes a project folder for `includeIf "gitdir:…"`: resolves
/// symlinks if the folder exists, writes it relative to `~` and appends the
/// trailing slash without which subfolders would not match.
pub fn normalize_folder(env: &Env, input: &str) -> String {
    let trimmed = input.trim();
    let expanded = env.expand(trimmed);
    let resolved = std::fs::canonicalize(&expanded).unwrap_or(expanded);
    let mut s = env.contract(Path::new(&resolved));
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
default_profile = "privat"

[[profile]]
id = "privat"
name = "Privat"
color = "emerald"
key = { path = "~/.ssh/privat_ed25519", comment = "tristan@zentrale" }
git = { name = "Tristan", email = "tristan@privat.de", sign = true }
folders = ["~/dev/privat/"]

[[profile.host]]
alias = "meinserver"
hostname = "1.2.3.4"
user = "tristan"
port = 22
provider = "generic"

[[profile]]
id = "sparkasse"
name = "S-Communication"
key = { path = "~/.ssh/sparkasse_ed25519", comment = "tristan@zentrale-sparkasse" }
git = { name = "Tristan", email = "tristan@kunde.de", sign = false }
folders = ["~/dev/sparkasse/"]
archived = false

[[profile.host]]
hostname = "gitlab.kunde.de"
provider = "gitlab"
"#;

    #[test]
    fn parses_plan_example() {
        let cfg = parse(SAMPLE).unwrap();
        assert_eq!(cfg.default_profile.as_deref(), Some("privat"));
        assert_eq!(cfg.profiles.len(), 2);
        let privat = &cfg.profiles[0];
        assert_eq!(privat.hosts[0].alias.as_deref(), Some("meinserver"));
        assert_eq!(privat.hosts[0].port, Some(22));
        assert!(privat.git.as_ref().unwrap().sign);
        assert_eq!(cfg.profiles[1].hosts[0].provider, Provider::Gitlab);
        validate(&cfg).unwrap();
    }

    #[test]
    fn roundtrip() {
        let cfg = parse(SAMPLE).unwrap();
        let again = parse(&to_toml(&cfg)).unwrap();
        assert_eq!(cfg, again);
    }

    #[test]
    fn rejects_nested_folders() {
        let mut cfg = parse(SAMPLE).unwrap();
        cfg.profiles[1].folders = vec!["~/dev/privat/kunde/".into()];
        assert!(matches!(validate(&cfg), Err(Error::NestedFolders(..))));
    }

    #[test]
    fn sibling_folders_with_common_prefix_are_fine() {
        let mut cfg = parse(SAMPLE).unwrap();
        cfg.profiles[1].folders = vec!["~/dev/privat2/".into()];
        validate(&cfg).unwrap();
    }

    #[test]
    fn rejects_duplicate_ids_and_bad_default() {
        let mut cfg = parse(SAMPLE).unwrap();
        cfg.profiles[1].id = "privat".into();
        assert!(matches!(validate(&cfg), Err(Error::DuplicateId(_))));
        let mut cfg = parse(SAMPLE).unwrap();
        cfg.default_profile = Some("nope".into());
        assert!(matches!(validate(&cfg), Err(Error::DefaultProfileMissing(_))));
        let mut cfg = parse(SAMPLE).unwrap();
        cfg.profiles[0].id = "Mit Leerzeichen".into();
        assert!(matches!(validate(&cfg), Err(Error::InvalidId(_))));
    }

    #[test]
    fn a_server_belongs_to_one_profile() {
        let mut cfg = parse(SAMPLE).unwrap();
        // meinserver is 1.2.3.4 in "privat"; the same IP in another profile is ambiguous.
        cfg.profiles[1].hosts.push(HostEntry { hostname: "1.2.3.4".into(), ..Default::default() });
        assert!(matches!(validate(&cfg), Err(Error::DuplicateAlias(h)) if h == "1.2.3.4"));
        // Two profiles on github.com are fine: no Host block for it.
        let mut cfg = parse(SAMPLE).unwrap();
        cfg.profiles[0].hosts.push(HostEntry {
            hostname: "github.com".into(),
            provider: Provider::Github,
            ..Default::default()
        });
        cfg.profiles[1].hosts.push(HostEntry {
            hostname: "github.com".into(),
            provider: Provider::Github,
            ..Default::default()
        });
        validate(&cfg).unwrap();
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Kunde C"), "kunde-c");
        assert_eq!(slugify("  Bundesagentur für Arbeit "), "bundesagentur-fuer-arbeit");
        assert_eq!(slugify("S-Communication"), "s-communication");
    }

    #[test]
    fn normalizes_folders() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path().canonicalize().unwrap());
        std::fs::create_dir_all(env.home.join("dev/real")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(env.home.join("dev/real"), env.home.join("dev/link")).unwrap();
        assert_eq!(normalize_folder(&env, "~/dev/missing"), "~/dev/missing/");
        assert_eq!(normalize_folder(&env, "~/dev/real/"), "~/dev/real/");
        #[cfg(unix)]
        assert_eq!(normalize_folder(&env, "~/dev/link"), "~/dev/real/");
    }

    #[test]
    fn save_requires_existing_key() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        let cfg = parse(SAMPLE).unwrap();
        assert!(matches!(save(&env, &cfg), Err(Error::KeyMissing(_))));
        std::fs::create_dir_all(env.home.join(".ssh")).unwrap();
        std::fs::write(env.home.join(".ssh/privat_ed25519"), "x").unwrap();
        std::fs::write(env.home.join(".ssh/sparkasse_ed25519"), "x").unwrap();
        save(&env, &cfg).unwrap();
        assert_eq!(load(&env).unwrap(), cfg);
    }
}
