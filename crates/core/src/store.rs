//! Loading, validating and saving `profiles.toml`.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsutil;
use crate::model::{Config, GitIdentity, HostEntry, KeyRef, Profile, Provider, Settings};
use crate::{Env, Error, Result};

// The on-disk layout uses TOML-friendly singular names (`[[profile]]`,
// `[[profile.host]]`); the in-memory model uses plural field names.

#[derive(Serialize, Deserialize, Default)]
struct TomlConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_profile: Option<String>,
    #[serde(default, skip_serializing_if = "TomlSettings::is_empty")]
    settings: TomlSettings,
    #[serde(default, rename = "profile")]
    profiles: Vec<TomlProfile>,
}

/// Only values that differ from the defaults are written.
#[derive(Serialize, Deserialize, Default)]
struct TomlSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    projects_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keys_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    absolute_ssh_include: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    known_hosts_per_profile: Option<bool>,
}

impl TomlSettings {
    fn is_empty(&self) -> bool {
        self.projects_dir.is_none()
            && self.keys_dir.is_none()
            && self.absolute_ssh_include.is_none()
            && self.known_hosts_per_profile.is_none()
    }
}

impl From<TomlSettings> for Settings {
    fn from(t: TomlSettings) -> Self {
        let d = Settings::default();
        Settings {
            projects_dir: t.projects_dir.unwrap_or(d.projects_dir),
            keys_dir: t.keys_dir.unwrap_or(d.keys_dir),
            absolute_ssh_include: t.absolute_ssh_include.unwrap_or(d.absolute_ssh_include),
            known_hosts_per_profile: t.known_hosts_per_profile.unwrap_or(d.known_hosts_per_profile),
        }
    }
}

impl From<&Settings> for TomlSettings {
    fn from(s: &Settings) -> Self {
        let d = Settings::default();
        TomlSettings {
            projects_dir: (s.projects_dir != d.projects_dir).then(|| s.projects_dir.clone()),
            keys_dir: (s.keys_dir != d.keys_dir).then(|| s.keys_dir.clone()),
            absolute_ssh_include: (s.absolute_ssh_include != d.absolute_ssh_include).then_some(s.absolute_ssh_include),
            known_hosts_per_profile: (s.known_hosts_per_profile != d.known_hosts_per_profile)
                .then_some(s.known_hosts_per_profile),
        }
    }
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
    Ok(Config {
        default_profile: raw.default_profile,
        profiles: raw.profiles.into_iter().map(Into::into).collect(),
        settings: raw.settings.into(),
    })
}

pub fn to_toml(config: &Config) -> String {
    let raw = TomlConfig {
        default_profile: config.default_profile.clone(),
        settings: (&config.settings).into(),
        profiles: config.profiles.iter().cloned().map(Into::into).collect(),
    };
    toml::to_string_pretty(&raw).expect("profile data is always serializable")
}

/// Holds `~/.config/keybund/.lock` while a process changes profiles.toml
/// and the configs (load → change → save → apply). The app and the CLI
/// (or two CLI runs) would otherwise save each other's profiles away and
/// delete each other's gitconfigs as stale. Released when dropped.
pub struct WriteLock(#[allow(dead_code)] std::fs::File);

/// Takes the lock, waiting for another process to finish.
pub fn lock(env: &Env) -> Result<WriteLock> {
    fsutil::ensure_dir(&env.config_dir, Some(0o700))?;
    let path = env.config_dir.join(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| Error::io(&path, e))?;
    file.lock().map_err(|e| Error::io(&path, e))?;
    Ok(WriteLock(file))
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
    validate_in(env, config)?;
    // New or changed key paths must exist. A key file that disappeared later
    // must not block every other change (the doctor reports it).
    let previous = load(env).unwrap_or_default();
    for p in config.active_profiles() {
        let known = previous.active_profiles().any(|q| q.id == p.id && q.key.path == p.key.path);
        if !known && !env.expand(&p.key.path).exists() {
            return Err(Error::KeyMissing(p.key.path.clone()));
        }
    }
    fsutil::ensure_dir(&env.config_dir, Some(0o700))?;
    let header = "# Keybund profiles – source of truth for all generated SSH and Git configs.\n\n";
    let old = fsutil::read_optional(&env.profiles_file())?.unwrap_or_default();
    let body = keep_unknown(&to_toml(config), &old);
    fsutil::atomic_write(&env.profiles_file(), &format!("{header}{body}"), Some(0o600))
}

const KNOWN_TOP: [&str; 3] = ["default_profile", "settings", "profile"];
const KNOWN_SETTINGS: [&str; 4] = ["projects_dir", "keys_dir", "absolute_ssh_include", "known_hosts_per_profile"];
const KNOWN_PROFILE: [&str; 8] = ["id", "name", "color", "key", "git", "folders", "archived", "host"];

/// Keys Keybund does not know (hand-written, or from a newer version) survive
/// a save: they are copied from the old file into the new one – top level,
/// `[settings]` and per profile (matched by id).
fn keep_unknown(new_text: &str, old_text: &str) -> String {
    use toml::{Table, Value};
    let Ok(old) = old_text.parse::<Table>() else { return new_text.to_string() };
    let Ok(mut new) = new_text.parse::<Table>() else { return new_text.to_string() };
    let unknown = |t: &Table, known: &[&str]| -> Vec<(String, Value)> {
        t.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let mut changed = false;
    for (k, v) in unknown(&old, &KNOWN_TOP) {
        new.insert(k, v);
        changed = true;
    }
    if let Some(Value::Table(old_settings)) = old.get("settings") {
        let extra = unknown(old_settings, &KNOWN_SETTINGS);
        if !extra.is_empty() {
            let settings = new.entry("settings").or_insert_with(|| Value::Table(Table::new()));
            if let Value::Table(t) = settings {
                t.extend(extra);
                changed = true;
            }
        }
    }
    if let (Some(Value::Array(old_profiles)), Some(Value::Array(new_profiles))) =
        (old.get("profile"), new.get_mut("profile"))
    {
        for np in new_profiles.iter_mut() {
            let Value::Table(np) = np else { continue };
            let id = np.get("id").cloned();
            let Some(Value::Table(op)) = old_profiles.iter().find(|p| p.get("id") == id.as_ref()) else { continue };
            let extra = unknown(op, &KNOWN_PROFILE);
            if !extra.is_empty() {
                np.extend(extra);
                changed = true;
            }
        }
    }
    if changed {
        toml::to_string_pretty(&new).unwrap_or_else(|_| new_text.to_string())
    } else {
        new_text.to_string()
    }
}

/// Values end up verbatim in ssh and git configs: a line break would add
/// directives, a wildcard or space in a Host pattern would match other hosts.
fn check_values(p: &Profile) -> Result<()> {
    let invalid = |field: &str, value: &str| Error::InvalidValue { field: field.into(), value: value.into() };
    let printable = |field: &str, value: &str| {
        if value.chars().any(char::is_control) {
            Err(invalid(field, value))
        } else {
            Ok(())
        }
    };
    // A token for `Host` / `HostName` / `User`: no spaces, wildcards, quotes or leading '-'.
    let token = |field: &str, value: &str| {
        let bad = value.is_empty()
            || value.starts_with('-')
            || value.chars().any(|c| c.is_whitespace() || c.is_control() || "*?!,\"'#%[]".contains(c));
        if bad {
            Err(invalid(field, value))
        } else {
            Ok(())
        }
    };
    printable("name", &p.name)?;
    printable("key", &p.key.path)?;
    // ssh expands % and ${…} in IdentityFile, a quote would end the quoted
    // path, and a backslash is an escape that ssh swallows (\' → ').
    if p.key.path.contains(['"', '%', '\\']) || p.key.path.contains("${") {
        return Err(invalid("key", &p.key.path));
    }
    if let Some(g) = &p.git {
        printable("gitName", &g.name)?;
        printable("gitEmail", &g.email)?;
        // The e-mail is the principal in allowed_signers: a pattern line of
        // its own, where whitespace separates and , * ? ! are pattern syntax.
        if g.sign && !g.email.trim().is_empty() && g.email.chars().any(|c| c.is_whitespace() || ",*?!".contains(c)) {
            return Err(invalid("gitEmail", &g.email));
        }
    }
    for f in &p.folders {
        printable("folder", f)?;
        // gitdir: patterns are globs: * ? [ \ would match other folders; git
        // compares the real path, so `..` would never match anything.
        let dotted = f.split('/').any(|part| part == "." || part == "..");
        if f.contains(['"', '*', '?', '[', '\\']) || !(f.starts_with('/') || f.starts_with("~/")) || dotted {
            return Err(Error::InvalidPath(f.clone()));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for h in &p.hosts {
        token("hostname", &h.hostname)?;
        if let Some(a) = &h.alias {
            token("alias", a)?;
        }
        if let Some(u) = &h.user {
            token("user", u)?;
        }
        // The same account or server twice in one key: ssh would get two
        // identical blocks, the app two identical rows.
        let same = (h.hostname.to_lowercase(), h.alias.as_ref().map(|a| a.to_lowercase()));
        if !seen.insert(same) {
            return Err(Error::DuplicateHost(h.hostname.clone()));
        }
    }
    Ok(())
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
        check_values(p)?;
        // allowed_signers needs the e-mail as principal; ~/.gitconfig's is not known there.
        if p.git.as_ref().is_some_and(|g| g.sign && g.email.trim().is_empty()) {
            return Err(Error::SignWithoutEmail(p.id.clone()));
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

/// `true` if folder `outer` contains (or equals) folder `inner`: a path
/// test, so `~/dev/a` does not contain `~/dev/ab/`.
fn folder_contains(outer: &str, inner: &str) -> bool {
    let slashed = |f: &str| if f.ends_with('/') { f.to_string() } else { format!("{f}/") };
    slashed(inner).starts_with(&slashed(outer))
}

/// `validate` plus the checks that need the file system: nested folders
/// are compared as real paths, so `~/dev/a/` and `/home/me/dev/a/sub/`
/// count as nested too.
pub fn validate_in(env: &Env, config: &Config) -> Result<()> {
    validate(config)?;
    let folders: Vec<(&str, String)> = config
        .active_profiles()
        .flat_map(|p| p.folders.iter().map(|f| (f.as_str(), env.expand(f).display().to_string())))
        .collect();
    for (i, (a, pa)) in folders.iter().enumerate() {
        for (b, pb) in &folders[i + 1..] {
            if folder_contains(pa, pb) || folder_contains(pb, pa) {
                return Err(Error::NestedFolders(a.to_string(), b.to_string()));
            }
        }
    }
    Ok(())
}

/// Removes `.` and `..` components without touching the file system, like
/// a shell would resolve them lexically (`a/../b` → `b`).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Normalizes a project folder for `includeIf "gitdir:…"`: resolves
/// symlinks if the folder exists, writes it relative to `~` and appends the
/// trailing slash without which subfolders would not match.
/// A folder setting: absolute or below `~`, written with `~` and without a
/// trailing `/`.
pub fn normalize_dir(env: &Env, input: &str) -> Result<String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with('/') || trimmed == "~" || trimmed.starts_with("~/")) {
        return Err(Error::InvalidPath(trimmed.to_string()));
    }
    let s = env.contract(&env.expand(trimmed));
    Ok(if s.len() > 1 { s.trim_end_matches('/').to_string() } else { s })
}

pub fn normalize_folder(env: &Env, input: &str) -> String {
    let trimmed = input.trim();
    let expanded = lexical(&env.expand(trimmed));
    // Resolve symlinks of the part that exists (e.g. ~/dev -> ~/data/dev),
    // also for a folder that is only created later.
    let mut existing = expanded.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => break,
        }
    }
    let resolved = match std::fs::canonicalize(existing) {
        Ok(mut base) if expanded.is_absolute() => {
            base.extend(rest.iter().rev());
            base
        }
        _ => expanded.clone(),
    };
    let mut s = env.contract(Path::new(&resolved));
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_written_only_when_changed() {
        let mut cfg = Config::default();
        assert!(!to_toml(&cfg).contains("[settings]"));
        cfg.settings.projects_dir = "~/work".into();
        cfg.settings.known_hosts_per_profile = false;
        let text = to_toml(&cfg);
        assert!(text.contains("[settings]\nprojects_dir = \"~/work\"\nknown_hosts_per_profile = false"), "{text}");
        let back = parse(&text).unwrap();
        assert_eq!(back.settings, cfg.settings);
        assert_eq!(back.settings.key_path("x"), "~/.ssh/x_ed25519");
        assert_eq!(back.settings.project_folder("x"), "~/work/x/");
    }

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
id = "acme"
name = "Nordlicht-Media"
key = { path = "~/.ssh/acme_ed25519", comment = "tristan@zentrale-acme" }
git = { name = "Tristan", email = "tristan@kunde.de", sign = false }
folders = ["~/dev/acme/"]
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

        let mut cfg = Config::default();
        cfg.profiles.push(Profile {
            id: "s".into(),
            name: "S".into(),
            git: Some(GitIdentity { name: "T".into(), email: " ".into(), sign: true }),
            ..Default::default()
        });
        assert!(matches!(validate(&cfg), Err(Error::SignWithoutEmail(id)) if id == "s"));

        // Values ssh or git would misread.
        let host = |hostname: &str| Profile {
            id: "h".into(),
            name: "H".into(),
            hosts: vec![HostEntry { hostname: hostname.into(), ..Default::default() }],
            ..Default::default()
        };
        for bad in ["fe80::1%eth0", "[2001:db8::3]", "-oProxyCommand=x", "a b", "*"] {
            let cfg = Config { profiles: vec![host(bad)], ..Default::default() };
            assert!(matches!(validate(&cfg), Err(Error::InvalidValue { .. })), "{bad}");
        }
        let cfg = Config { profiles: vec![host("2001:db8::3")], ..Default::default() };
        validate(&cfg).unwrap();
        for bad in ["~/dev/[c]/", "~/dev/star*/", "dev/rel/"] {
            let p = Profile { id: "f".into(), name: "F".into(), folders: vec![bad.into()], ..Default::default() };
            let cfg = Config { profiles: vec![p], ..Default::default() };
            assert!(validate(&cfg).is_err(), "{bad}");
        }
        let p = Profile {
            id: "k".into(),
            name: "K".into(),
            key: KeyRef { path: "~/.ssh/100%done".into(), comment: None },
            ..Default::default()
        };
        assert!(validate(&Config { profiles: vec![p], ..Default::default() }).is_err());
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
    fn values_that_would_break_ssh_or_git_are_refused() {
        let with = |p: Profile| validate(&Config { profiles: vec![p], ..Default::default() });
        let base = || Profile { id: "k".into(), name: "K".into(), ..Default::default() };
        // A backslash is an escape for ssh: `\'` becomes `'`, the path changes silently.
        let p = Profile { key: KeyRef { path: "~/.ssh/a\\b".into(), comment: None }, ..base() };
        assert!(matches!(with(p), Err(Error::InvalidValue { field, .. }) if field == "key"));
        // An apostrophe is fine: the renderer quotes it.
        let p = Profile { key: KeyRef { path: "~/.ssh/o'brien".into(), comment: None }, ..base() };
        with(p).unwrap();
        // The signing e-mail is a principal pattern in allowed_signers.
        for email in ["a b@x.de", "a,b@x.de", "a*@x.de"] {
            let p = Profile { git: Some(GitIdentity { name: "T".into(), email: email.into(), sign: true }), ..base() };
            assert!(matches!(with(p), Err(Error::InvalidValue { field, .. }) if field == "gitEmail"), "{email}");
        }
        let p =
            Profile { git: Some(GitIdentity { name: "T".into(), email: "a b@x.de".into(), sign: false }), ..base() };
        with(p).unwrap();
        // git compares real paths: `..` in a gitdir pattern never matches.
        for folder in ["~/dev/../x/", "~/dev/./x/", "/home/t/.."] {
            let p = Profile { folders: vec![folder.into()], ..base() };
            assert!(matches!(with(p), Err(Error::InvalidPath(_))), "{folder}");
        }
        // The same account twice in one key.
        let host = |hostname: &str, alias: Option<&str>| HostEntry {
            hostname: hostname.into(),
            alias: alias.map(Into::into),
            provider: Provider::Github,
            ..Default::default()
        };
        let p = Profile { hosts: vec![host("github.com", None), host("GitHub.com", None)], ..base() };
        assert!(matches!(with(p), Err(Error::DuplicateHost(h)) if h == "GitHub.com"));
        let p = Profile { hosts: vec![host("github.com", None), host("github.com", Some("gh-work"))], ..base() };
        with(p).unwrap();
    }

    #[test]
    fn nested_folders_are_compared_as_paths() {
        // A prefix is not a parent.
        assert!(!folder_contains("~/dev/a", "~/dev/ab/"));
        assert!(folder_contains("~/dev/a", "~/dev/a/b/"));
        let env = Env::with_home("/home/t");
        let p = |id: &str, folder: &str| Profile {
            id: id.into(),
            name: id.into(),
            folders: vec![folder.into()],
            ..Default::default()
        };
        let cfg = Config { profiles: vec![p("a", "~/dev/a/"), p("b", "/home/t/dev/a/sub/")], ..Default::default() };
        validate(&cfg).unwrap();
        assert!(matches!(validate_in(&env, &cfg), Err(Error::NestedFolders(..))));
        let cfg = Config { profiles: vec![p("a", "~/dev/a/"), p("b", "/home/t/dev/ab/")], ..Default::default() };
        validate_in(&env, &cfg).unwrap();
    }

    #[test]
    fn folders_are_normalized_lexically_before_anything_exists() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path().canonicalize().unwrap());
        assert_eq!(normalize_folder(&env, "~/missing/../x"), "~/x/");
        assert_eq!(normalize_folder(&env, "~/dev/./y//"), "~/dev/y/");
        assert_eq!(normalize_folder(&env, "~/dev/z/"), "~/dev/z/");
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Kunde C"), "kunde-c");
        assert_eq!(slugify("  Grüne Wiese für Kinder "), "gruene-wiese-fuer-kinder");
        assert_eq!(slugify("Nordlicht-Media"), "nordlicht-media");
    }

    #[test]
    fn normalizes_folders() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path().canonicalize().unwrap());
        std::fs::create_dir_all(env.home.join("dev/real")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(env.home.join("dev/real"), env.home.join("dev/link")).unwrap();
        assert_eq!(normalize_folder(&env, "~/dev/missing"), "~/dev/missing/");
        assert_eq!(normalize_dir(&env, " ~/work/ ").unwrap(), "~/work");
        assert_eq!(normalize_dir(&env, &format!("{}/x", env.home.display())).unwrap(), "~/x");
        assert_eq!(normalize_dir(&env, "/srv/").unwrap(), "/srv");
        assert!(normalize_dir(&env, "work").is_err());
        assert_eq!(normalize_folder(&env, "~/dev/real/"), "~/dev/real/");
        #[cfg(unix)]
        assert_eq!(normalize_folder(&env, "~/dev/link"), "~/dev/real/");
    }

    #[test]
    fn unknown_keys_survive_a_save() {
        let old = "future = 1\n\n[settings]\nlater = \"x\"\n\n[[profile]]\nid = \"a\"\nname = \"A\"\nextra = \"keep\"\nkey = { path = \"~/.ssh/a\" }\n";
        let mut cfg = parse(old).unwrap();
        cfg.profiles[0].name = "A2".into();
        let out = keep_unknown(&to_toml(&cfg), old);
        let back: toml::Table = out.parse().unwrap();
        assert_eq!(back["future"].as_integer(), Some(1));
        assert_eq!(back["settings"]["later"].as_str(), Some("x"));
        assert_eq!(back["profile"][0]["extra"].as_str(), Some("keep"));
        assert_eq!(parse(&out).unwrap().profiles[0].name, "A2");
        // Nothing unknown: the output stays exactly as Keybund writes it.
        assert_eq!(keep_unknown(&to_toml(&cfg), &to_toml(&cfg)), to_toml(&cfg));
    }

    #[test]
    fn save_requires_existing_key() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        let cfg = parse(SAMPLE).unwrap();
        assert!(matches!(save(&env, &cfg), Err(Error::KeyMissing(_))));
        std::fs::create_dir_all(env.home.join(".ssh")).unwrap();
        std::fs::write(env.home.join(".ssh/privat_ed25519"), "x").unwrap();
        std::fs::write(env.home.join(".ssh/acme_ed25519"), "x").unwrap();
        save(&env, &cfg).unwrap();
        assert_eq!(load(&env).unwrap(), cfg);
        // A key file that vanished later does not block other changes.
        std::fs::remove_file(env.home.join(".ssh/acme_ed25519")).unwrap();
        let mut changed = cfg.clone();
        changed.profiles[0].name = "Privat neu".into();
        save(&env, &changed).unwrap();
        changed.profiles[0].key.path = "~/.ssh/gibtsnicht".into();
        assert!(matches!(save(&env, &changed), Err(Error::KeyMissing(_))));
    }
}
