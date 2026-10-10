//! Data model. `profiles.toml` is the single source of truth; every
//! generated config is derived from it.

use serde::{Deserialize, Serialize};

/// The whole content of `profiles.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Config {
    /// Profile whose key is used outside of all project folders.
    pub default_profile: Option<String>,
    pub profiles: Vec<Profile>,
    pub settings: Settings,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Profile {
    /// Stable identifier, used for file names (`<id>.gitconfig`).
    pub id: String,
    pub name: String,
    /// Color name for the UI (e.g. `emerald`, `blue`, `violet`).
    pub color: Option<String>,
    pub key: KeyRef,
    pub git: Option<GitIdentity>,
    /// Project folders, normalized with a trailing `/`.
    pub folders: Vec<String>,
    pub archived: bool,
    pub hosts: Vec<HostEntry>,
}

/// Reference to a private key. Only the path is stored; fingerprint and
/// creation date are read from the `.pub` file when needed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct KeyRef {
    pub path: String,
    pub comment: Option<String>,
}

impl KeyRef {
    pub fn pub_path(&self) -> String {
        format!("{}.pub", self.path)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
    /// Sign commits and tags with the profile's SSH key.
    pub sign: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HostEntry {
    /// SSH alias (e.g. `meinserver`). Hosts with an alias get their own
    /// `Host` block in the SSH config.
    pub alias: Option<String>,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub provider: Provider,
}

impl HostEntry {
    /// Name to use on the command line: the alias if there is one.
    pub fn target_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.hostname)
    }

    /// Patterns of the `Host` block Keybund writes for this host, empty if it
    /// gets none. A server matches by alias and by its hostname/IP, so
    /// `ssh meinserver` and `ssh 1.2.3.4` use the same key. Git hosting
    /// providers only get a block for an alias: a block for `github.com`
    /// would apply to every profile on github.com.
    pub fn ssh_patterns(&self) -> Vec<String> {
        let mut patterns: Vec<String> = self.alias.iter().cloned().collect();
        if self.provider == Provider::Generic && !patterns.iter().any(|p| p.eq_ignore_ascii_case(&self.hostname)) {
            patterns.push(self.hostname.clone());
        }
        patterns
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitlab,
    Bitbucket,
    #[default]
    Generic,
}

/// User preferences (`[settings]` in `profiles.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Settings {
    /// Parent folder of new projects (`~/dev`).
    pub projects_dir: String,
    /// Folder for new keys (`~/.ssh`).
    pub keys_dir: String,
    /// Write the `Include` in `~/.ssh/config` with the full path instead of
    /// `~`: libssh before 0.10.2 (Dolphin/KIO on older distributions) does
    /// not expand `~` there and would not see Keybund's Host blocks.
    pub absolute_ssh_include: bool,
    /// Keep the host keys of each profile's servers in its own known_hosts
    /// file (removed with the profile).
    pub known_hosts_per_profile: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            projects_dir: "~/dev".into(),
            keys_dir: "~/.ssh".into(),
            absolute_ssh_include: false,
            known_hosts_per_profile: true,
        }
    }
}

impl Settings {
    /// Folder of a new project, with a trailing `/`.
    pub fn project_folder(&self, id: &str) -> String {
        format!("{}/{id}/", self.projects_dir.trim_end_matches('/'))
    }

    /// Path of a new key.
    pub fn key_path(&self, id: &str) -> String {
        format!("{}/{id}_ed25519", self.keys_dir.trim_end_matches('/'))
    }
}

impl Config {
    pub fn profile(&self, id: &str) -> crate::Result<&Profile> {
        self.profiles.iter().find(|p| p.id == id).ok_or_else(|| crate::Error::ProfileNotFound(id.to_string()))
    }

    pub fn profile_mut(&mut self, id: &str) -> crate::Result<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.id == id).ok_or_else(|| crate::Error::ProfileNotFound(id.to_string()))
    }

    /// Profiles that take part in config generation.
    pub fn active_profiles(&self) -> impl Iterator<Item = &Profile> {
        self.profiles.iter().filter(|p| !p.archived)
    }

    pub fn default_entry(&self) -> Option<&Profile> {
        let id = self.default_profile.as_deref()?;
        self.active_profiles().find(|p| p.id == id)
    }
}
