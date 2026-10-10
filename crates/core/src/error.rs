use std::path::Path;

/// Central error type. The messages are meant to be shown to the user as-is,
/// by the CLI as well as by the GUI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Home-Verzeichnis nicht gefunden (HOME ist nicht gesetzt)")]
    NoHome,
    #[error("Key „{0}“ existiert nicht")]
    ProfileNotFound(String),
    #[error("Einen Key „{0}“ gibt es schon")]
    DuplicateId(String),
    #[error("Ungültige Key-ID „{0}“ (erlaubt sind a–z, 0–9, - und _)")]
    InvalidId(String),
    #[error("Key „{0}“ braucht einen Namen")]
    MissingName(String),
    #[error("Key „{0}“ soll Commits signieren, hat aber keine eigene E-Mail")]
    SignWithoutEmail(String),
    #[error("Ungültiger Wert für {field}: „{value}“")]
    InvalidValue { field: String, value: String },
    #[error("Standard-Key „{0}“ existiert nicht")]
    DefaultProfileMissing(String),
    #[error("Git-Ordner überschneiden sich: {0} und {1}")]
    NestedFolders(String, String),
    #[error("Host „{0}“ gehört schon zu einem anderen Key")]
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
    #[error("Key „{0}“ hat keinen Git-Ordner")]
    NoFolder(String),
    #[error("Key „{0}“ hat keinen Host")]
    NoHost(String),
    #[error("Key „{0}“ ist archiviert")]
    Archived(String),
    #[error("Die Konfiguration hat sich seit der Vorschau geändert: {0}")]
    StalePlan(String),
    #[error("Ungültiger Ordner „{0}“ – bitte mit / oder ~/ angeben")]
    InvalidPath(String),
    #[error("Der Host-Key von {0} ist nicht der bestätigte – Verbindung abgebrochen. Prüfen, ob der Server neu aufgesetzt wurde.")]
    HostKeyChanged(String),
    #[error("Keine Verbindung zu {host}: {message}")]
    Network { host: String, message: String },
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
            Error::SignWithoutEmail(_) => "sign_without_email",
            Error::InvalidValue { .. } => "invalid_value",
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
            Error::InvalidPath(_) => "invalid_path",
            Error::HostKeyChanged(_) => "host_key_changed",
            Error::Network { .. } => "network",
            Error::Io { .. } => "io",
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn kinds_are_unique_and_messages_readable() {
        let io = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let all = [
            Error::NoHome,
            Error::ProfileNotFound("a".into()),
            Error::DuplicateId("a".into()),
            Error::InvalidId("a".into()),
            Error::MissingName("a".into()),
            Error::SignWithoutEmail("a".into()),
            Error::InvalidValue { field: "a".into(), value: "b".into() },
            Error::DefaultProfileMissing("a".into()),
            Error::NestedFolders("a".into(), "b".into()),
            Error::DuplicateAlias("a".into()),
            Error::KeyMissing("a".into()),
            Error::KeyExists("a".into()),
            Error::InvalidPublicKey("a".into()),
            Error::ToolMissing("a".into()),
            Error::ToolFailed { tool: "a".into(), message: "b".into() },
            Error::ConfigParse("a".into()),
            Error::InvalidUrl("a".into()),
            Error::TargetExists("a".into()),
            Error::NoFolder("a".into()),
            Error::NoHost("a".into()),
            Error::Archived("a".into()),
            Error::StalePlan("a".into()),
            Error::InvalidPath("a".into()),
            Error::HostKeyChanged("a".into()),
            Error::Network { host: "a".into(), message: "b".into() },
            Error::io("/x", io()),
        ];
        let mut kinds: Vec<&str> = all.iter().map(Error::kind).collect();
        kinds.sort();
        kinds.dedup();
        assert_eq!(kinds.len(), all.len());
        assert!(all.iter().all(|e| !e.to_string().is_empty()));
        assert_eq!(Error::io("/x", io()).to_string(), "Dateizugriff auf /x fehlgeschlagen: denied");
    }
}
