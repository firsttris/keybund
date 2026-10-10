//! Provider specifics: display names, SSH user and the page where a
//! public key is added.

use crate::model::Provider;

impl Provider {
    pub const ALL: [Provider; 4] = [Provider::Github, Provider::Gitlab, Provider::Bitbucket, Provider::Generic];

    pub fn display_name(self) -> &'static str {
        match self {
            Provider::Github => "GitHub",
            Provider::Gitlab => "GitLab",
            Provider::Bitbucket => "Bitbucket",
            Provider::Generic => "Server",
        }
    }

    pub fn default_hostname(self) -> Option<&'static str> {
        match self {
            Provider::Github => Some("github.com"),
            Provider::Gitlab => Some("gitlab.com"),
            Provider::Bitbucket => Some("bitbucket.org"),
            Provider::Generic => None,
        }
    }

    /// SSH user for Git hosting providers.
    pub fn git_user(self) -> Option<&'static str> {
        match self {
            Provider::Generic => None,
            _ => Some("git"),
        }
    }

    pub fn is_git_host(self) -> bool {
        self != Provider::Generic
    }

    /// Guesses the provider from a hostname.
    pub fn detect(hostname: &str) -> Provider {
        let h = hostname.to_ascii_lowercase();
        if h == "github.com" || h.ends_with(".github.com") || h.starts_with("github.") {
            Provider::Github
        } else if h == "bitbucket.org" || h.starts_with("bitbucket.") {
            Provider::Bitbucket
        } else if h.contains("gitlab") {
            Provider::Gitlab
        } else {
            Provider::Generic
        }
    }
}

/// URL of the "add SSH key" page for a provider and host. Works for
/// self-hosted GitLab and GitHub Enterprise as well.
pub fn add_key_url(provider: Provider, hostname: &str) -> Option<String> {
    match provider {
        Provider::Github => Some(format!("https://{hostname}/settings/ssh/new")),
        Provider::Gitlab => Some(format!("https://{hostname}/-/user_settings/ssh_keys")),
        Provider::Bitbucket => Some("https://bitbucket.org/account/settings/ssh-keys/".to_string()),
        Provider::Generic => None,
    }
}

/// URL of the page that lists SSH keys (for removing a key on offboarding).
pub fn keys_url(provider: Provider, hostname: &str) -> Option<String> {
    match provider {
        Provider::Github => Some(format!("https://{hostname}/settings/keys")),
        _ => add_key_url(provider, hostname),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(add_key_url(Provider::Github, "github.com").unwrap(), "https://github.com/settings/ssh/new");
        assert_eq!(
            add_key_url(Provider::Gitlab, "gitlab.kunde.de").unwrap(),
            "https://gitlab.kunde.de/-/user_settings/ssh_keys"
        );
        assert!(add_key_url(Provider::Generic, "1.2.3.4").is_none());
    }

    #[test]
    fn detection() {
        assert_eq!(Provider::detect("github.com"), Provider::Github);
        assert_eq!(Provider::detect("gitlab.kunde.de"), Provider::Gitlab);
        assert_eq!(Provider::detect("bitbucket.org"), Provider::Bitbucket);
        assert_eq!(Provider::detect("1.2.3.4"), Provider::Generic);
    }
}
