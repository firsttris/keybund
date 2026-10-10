use std::path::Path;

/// Central error type. The messages are meant to be shown to the user as-is,
/// by the CLI as well as by the GUI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Home-Verzeichnis nicht gefunden (HOME ist nicht gesetzt)")]
    NoHome,
    #[error("Profil „{0}“ existiert nicht")]
    ProfileNotFound(String),
    #[error("Profil-ID „{0}“ ist doppelt vergeben")]
    DuplicateId(String),
    #[error("Ungültige Profil-ID „{0}“ (erlaubt sind a–z, 0–9, - und _)")]
    InvalidId(String),
    #[error("Profil „{0}“ braucht einen Namen")]
    MissingName(String),
    #[error("Default-Profil „{0}“ existiert nicht")]
    DefaultProfileMissing(String),
    #[error("Projektordner überschneiden sich: {0} und {1}")]
    NestedFolders(String, String),
    #[error("Host-Alias „{0}“ ist doppelt vergeben")]
    DuplicateAlias(String),
    #[error("Key fehlt: {0}")]
    KeyMissing(String),
    #[error("Key existiert bereits: {0}")]
    KeyExists(String),
    #[error("Ungültiger Public Key in {0}")]
    InvalidPublicKey(String),
    #[error("Programm „{0}“ nicht gefunden – ist es installiert?")]
    ToolMissing(String),
    #[error("{tool} fehlgeschlagen: {message}")]
    ToolFailed { tool: String, message: String },
    #[error("profiles.toml ist fehlerhaft: {0}")]
    ConfigParse(String),
    #[error("Ungültige Repo-URL: {0}")]
    InvalidUrl(String),
    #[error("Zielordner existiert bereits: {0}")]
    TargetExists(String),
    #[error("Profil „{0}“ hat keinen Projektordner")]
    NoFolder(String),
    #[error("Profil „{0}“ hat keinen Host")]
    NoHost(String),
    #[error("Profil „{0}“ ist archiviert")]
    Archived(String),
    #[error("Die Konfiguration hat sich seit der Vorschau geändert: {0}")]
    StalePlan(String),
    #[error("Dateizugriff auf {path} fehlgeschlagen: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Error::Io { path: path.as_ref().display().to_string(), source }
    }

    /// Stable machine-readable identifier, e.g. for the GUI.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::NoHome => "no_home",
            Error::ProfileNotFound(_) => "profile_not_found",
            Error::DuplicateId(_) => "duplicate_id",
            Error::InvalidId(_) => "invalid_id",
            Error::MissingName(_) => "missing_name",
            Error::DefaultProfileMissing(_) => "default_profile_missing",
            Error::NestedFolders(..) => "nested_folders",
            Error::DuplicateAlias(_) => "duplicate_alias",
            Error::KeyMissing(_) => "key_missing",
            Error::KeyExists(_) => "key_exists",
            Error::InvalidPublicKey(_) => "invalid_public_key",
            Error::ToolMissing(_) => "tool_missing",
            Error::ToolFailed { .. } => "tool_failed",
            Error::ConfigParse(_) => "config_parse",
            Error::InvalidUrl(_) => "invalid_url",
            Error::TargetExists(_) => "target_exists",
            Error::NoFolder(_) => "no_folder",
            Error::NoHost(_) => "no_host",
            Error::Archived(_) => "archived",
            Error::StalePlan(_) => "stale_plan",
            Error::Io { .. } => "io",
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
