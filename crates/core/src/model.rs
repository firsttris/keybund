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
