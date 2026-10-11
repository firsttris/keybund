use std::fmt;
use std::path::Path;

use crate::i18n::Msg;
use crate::msg;

/// Central error type. Its text is a [`Msg`] (`Error::msg`): the app
/// translates the key, the CLI renders it with `Display`.
#[derive(Debug)]
pub enum Error {
    NoHome,
    ProfileNotFound(String),
    DuplicateId(String),
    InvalidId(String),
    MissingName(String),
    SignWithoutEmail(String),
    /// `field` is a field id (`name`, `key`, `gitName`, `gitEmail`, `folder`,
    /// `hostname`, `alias`, `user`), shown through the message `field_<id>`.
    InvalidValue {
        field: String,
        value: String,
    },
    DefaultProfileMissing(String),
    NestedFolders(String, String),
    DuplicateAlias(String),
    KeyMissing(String),
    KeyExists(String),
    InvalidPublicKey(String),
    ToolMissing(String),
    ToolFailed {
        tool: String,
        message: String,
    },
    ConfigParse(String),
    InvalidUrl(String),
    /// Only `https://` links are opened in the browser.
    NotHttps(String),
    TargetExists(String),
    NoFolder(String),
    NoHost(String),
    Archived(String),
    StalePlan(String),
    InvalidPath(String),
    HostKeyChanged(String),
    Network {
        host: String,
        message: String,
    },
    Io {
        path: String,
        source: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.msg().fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Error {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Error::Io { path: path.as_ref().display().to_string(), source }
    }

    /// The text for people, as a catalog key with arguments.
    pub fn msg(&self) -> Msg {
        match self {
            Error::NoHome => msg!("error_noHome"),
            Error::ProfileNotFound(id) => msg!("error_profileNotFound", id = id),
            Error::DuplicateId(id) => msg!("error_duplicateId", id = id),
            Error::InvalidId(id) => msg!("error_invalidId", id = id),
            Error::MissingName(id) => msg!("error_missingName", id = id),
            Error::SignWithoutEmail(id) => msg!("error_signWithoutEmail", id = id),
            Error::InvalidValue { field, value } => {
                msg!("error_invalidValue", value = value).nested("field", Msg::new(format!("field_{field}")))
            }
            Error::DefaultProfileMissing(id) => msg!("error_defaultProfileMissing", id = id),
            Error::NestedFolders(a, b) => msg!("error_nestedFolders", a = a, b = b),
            Error::DuplicateAlias(host) => msg!("error_duplicateAlias", host = host),
            Error::KeyMissing(path) => msg!("error_keyMissing", path = path),
            Error::KeyExists(path) => msg!("error_keyExists", path = path),
            Error::InvalidPublicKey(path) => msg!("error_invalidPublicKey", path = path),
            Error::ToolMissing(tool) => msg!("error_toolMissing", tool = tool),
            Error::ToolFailed { tool, message } => msg!("error_toolFailed", tool = tool, detail = message),
            Error::ConfigParse(detail) => msg!("error_configParse", detail = detail),
            Error::InvalidUrl(url) => msg!("error_invalidUrl", url = url),
            Error::NotHttps(url) => msg!("error_notHttps", url = url),
            Error::TargetExists(path) => msg!("error_targetExists", path = path),
            Error::NoFolder(id) => msg!("error_noFolder", id = id),
            Error::NoHost(id) => msg!("error_noHost", id = id),
            Error::Archived(id) => msg!("error_archived", id = id),
            Error::StalePlan(path) => msg!("error_stalePlan", path = path),
            Error::InvalidPath(path) => msg!("error_invalidPath", path = path),
            Error::HostKeyChanged(host) => msg!("error_hostKeyChanged", host = host),
            Error::Network { host, message } => msg!("error_network", host = host, detail = message),
            Error::Io { path, source } => msg!("error_io", path = path, detail = source),
        }
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
            Error::NotHttps(_) => "not_https",
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
    fn kinds_are_unique_and_every_message_is_translated() {
        let io = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let all = [
            Error::NoHome,
            Error::ProfileNotFound("a".into()),
            Error::DuplicateId("a".into()),
            Error::InvalidId("a".into()),
            Error::MissingName("a".into()),
            Error::SignWithoutEmail("a".into()),
            Error::InvalidValue { field: "alias".into(), value: "b".into() },
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
            Error::NotHttps("a".into()),
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
        // Every error has its text in both catalogs.
        for e in &all {
            let m = e.msg();
            for lang in [crate::i18n::Lang::De, crate::i18n::Lang::En] {
                let text = m.render(lang);
                assert!(!text.contains('{') && text != m.key, "{} in {lang:?}: {text}", m.key);
            }
        }
        assert_eq!(Error::io("/x", io()).to_string(), "Dateizugriff auf /x fehlgeschlagen: denied");
    }
}
