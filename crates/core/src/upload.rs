//! Adding a profile's public key to a GitHub or GitLab account via their API.
//!
//! The login uses the OAuth device flow: Keybund shows a short code, the user
//! confirms it in the browser (where they are already logged in) and Keybund
//! receives a token with just the scope for SSH keys. Keybund never sees a
//! password. Hosts without a registered OAuth app (self-hosted GitLab, GitHub
//! Enterprise) take a personal access token instead. Tokens are used for the
//! upload and then dropped, unless the user keeps them in the keyring
//! ([`crate::tokens`]).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::keygen::PublicKey;
use crate::model::Provider;
use crate::{Env, Error, Result};

const GRANT_DEVICE_CODE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// What the user sees to confirm the device login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// GitLab: the URL with the code filled in.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// One poll of the device flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    /// Not confirmed yet.
    Pending,
    /// Polled too fast: wait 5 seconds longer from now on.
    SlowDown,
    Token(String),
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UploadResult {
    pub success: bool,
    /// The key was already in the account.
    pub already_present: bool,
    /// The account the token belongs to (`login` / `username`).
    pub account: Option<String>,
    pub message: String,
    /// The token was rejected or lacks the scope: a new login helps.
    pub auth_failed: bool,
}

impl UploadResult {
    fn failed(message: String, account: Option<String>, auth_failed: bool) -> Self {
        UploadResult { success: false, already_present: false, account, message, auth_failed }
    }
}

/// The API of one git host.
#[derive(Debug, Clone)]
pub struct Api {
    pub provider: Provider,
    pub hostname: String,
    oauth_base: String,
    api_base: String,
    client_id: Option<String>,
}

impl Api {
    /// `None` for providers without a supported API (Bitbucket, servers).
    pub fn new(env: &Env, provider: Provider, hostname: &str) -> Option<Api> {
        let base = env.api.base_url.as_deref().map(|b| b.trim_end_matches('/').to_string());
        let host = hostname.to_ascii_lowercase();
        let (oauth_base, api_base, client_id) = match provider {
            Provider::Github => {
                let dotcom = host == "github.com";
                let oauth = base.clone().unwrap_or_else(|| format!("https://{host}"));
                let api = base.unwrap_or_else(|| {
                    if dotcom {
                        "https://api.github.com".to_string()
                    } else {
                        format!("https://{host}/api/v3")
                    }
                });
                (oauth, api, env.api.github_client_id.clone().filter(|_| dotcom))
            }
            Provider::Gitlab => {
                let oauth = base.unwrap_or_else(|| format!("https://{host}"));
                let api = format!("{oauth}/api/v4");
                (oauth, api, env.api.gitlab_client_id.clone().filter(|_| host == "gitlab.com"))
            }
            Provider::Bitbucket | Provider::Generic => return None,
        };
        Some(Api { provider, hostname: host, oauth_base, api_base, client_id })
    }

    /// Whether the login via browser (device flow) is available here.
    pub fn device_flow(&self) -> bool {
        self.client_id.is_some()
    }

    /// OAuth scopes the upload needs.
    pub fn scopes(&self, sign: bool) -> Vec<&'static str> {
        match self.provider {
            Provider::Github if sign => vec!["write:public_key", "admin:ssh_signing_key"],
            Provider::Github => vec!["write:public_key"],
            _ => vec!["api"],
        }
    }

    /// Page to create a personal access token with the right scopes.
    pub fn token_url(&self, sign: bool) -> String {
        let scopes = self.scopes(sign).join(",");
        match self.provider {
            Provider::Github => {
                format!("https://{}/settings/tokens/new?scopes={scopes}&description=Keybund", self.hostname)
            }
            _ => {
                format!("https://{}/-/user_settings/personal_access_tokens?name=Keybund&scopes={scopes}", self.hostname)
            }
        }
    }

    fn name(&self) -> &'static str {
        self.provider.display_name()
    }

    fn agent() -> ureq::Agent {
        let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!("keybund/", env!("CARGO_PKG_VERSION")))
            .tls_config(tls)
            .build()
            .into()
    }

    fn network(&self, e: ureq::Error) -> Error {
        Error::Network { host: self.hostname.clone(), message: e.to_string() }
    }

    fn read(&self, res: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<(u16, Value)> {
        let mut res = res.map_err(|e| self.network(e))?;
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().map_err(|e| self.network(e))?;
        // GitHub's OAuth endpoints answer form-encoded unless asked for JSON;
        // anything unparsable is kept as a string for the error message.
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok((status, value))
    }

    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<(u16, Value)> {
        self.read(Self::agent().post(url).header("Accept", "application/json").send_form(form.iter().copied()))
    }

    fn get(&self, url: &str, token: &str) -> Result<(u16, Value)> {
        self.read(
            Self::agent()
                .get(url)
                .header("Accept", "application/json")
                .header("Authorization", &format!("Bearer {token}"))
                .call(),
        )
    }

    fn post_json(&self, url: &str, token: &str, body: &Value) -> Result<(u16, Value)> {
        self.read(
            Self::agent()
                .post(url)
                .header("Accept", "application/json")
                .header("Authorization", &format!("Bearer {token}"))
                .send_json(body),
        )
    }

    fn unexpected(&self, status: u16, body: &Value) -> Error {
        Error::Api(format!("{} antwortet mit HTTP {status}: {}", self.hostname, message_of(body)))
    }

    /// Starts the device flow: show `user_code`, open `verification_uri`.
    pub fn start_device_flow(&self, sign: bool) -> Result<DeviceCode> {
        let client_id = self.client_id.as_deref().ok_or_else(|| {
            Error::Api(format!(
                "Für {} ist keine Anmeldung per Browser eingerichtet – bitte ein Token verwenden",
                self.hostname
            ))
        })?;
        let path = match self.provider {
            Provider::Github => "/login/device/code",
            _ => "/oauth/authorize_device",
        };
        let scope = self.scopes(sign).join(" ");
        let (status, body) =
            self.post_form(&format!("{}{path}", self.oauth_base), &[("client_id", client_id), ("scope", &scope)])?;
        if status != 200 {
            return Err(self.unexpected(status, &body));
        }
        serde_json::from_value(body.clone()).map_err(|_| self.unexpected(status, &body))
    }

    /// Asks once whether the user confirmed the code.
    pub fn poll_device_flow(&self, device_code: &str) -> Result<Poll> {
        let client_id = self.client_id.as_deref().unwrap_or_default();
        let path = match self.provider {
            Provider::Github => "/login/oauth/access_token",
            _ => "/oauth/token",
        };
        let (status, body) = self.post_form(
            &format!("{}{path}", self.oauth_base),
            &[("client_id", client_id), ("device_code", device_code), ("grant_type", GRANT_DEVICE_CODE)],
        )?;
        if let Some(token) = body.get("access_token").and_then(Value::as_str) {
            return Ok(Poll::Token(token.to_string()));
        }
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => Ok(Poll::SlowDown),
            Some("expired_token") => Ok(Poll::Expired),
            Some("access_denied") => Ok(Poll::Denied),
            _ => Err(self.unexpected(status, &body)),
        }
    }

    /// The account a token belongs to; `None` if the token is rejected.
    pub fn account(&self, token: &str) -> Result<Option<String>> {
        let (status, body) = self.get(&format!("{}/user", self.api_base), token)?;
        match status {
            200 => {
                Ok(["login", "username"].iter().find_map(|k| body.get(*k).and_then(Value::as_str)).map(str::to_string))
            }
            401 | 403 => Ok(None),
            _ => Err(self.unexpected(status, &body)),
        }
    }

    /// `type blob` of all keys at `path` (first page of 100 is plenty).
    fn keys(&self, token: &str, path: &str) -> Result<std::result::Result<Vec<String>, u16>> {
        let (status, body) = self.get(&format!("{}{path}?per_page=100", self.api_base), token)?;
        if status != 200 {
            return Ok(Err(status));
        }
        let keys = body.as_array().map(Vec::as_slice).unwrap_or_default();
        Ok(Ok(keys
            .iter()
            .filter_map(|k| k.get("key").and_then(Value::as_str))
            .filter_map(PublicKey::parse)
            .map(|k| k.without_comment())
            .collect()))
    }

    /// Adds `key` to the account of `token`, as signing key too if `sign`.
    pub fn upload(&self, token: &str, key: &PublicKey, title: &str, sign: bool) -> Result<UploadResult> {
        let name = self.name();
        let Some(account) = self.account(token)? else {
            return Ok(UploadResult::failed(
                format!("{name} hat das Token abgelehnt (abgelaufen, widerrufen oder ohne Berechtigung)"),
                None,
                true,
            ));
        };
        let at = Some(account.clone());
        let missing_scope = |scope: &str| {
            UploadResult::failed(
                format!("Dem Token fehlt die Berechtigung „{scope}“ – bitte neu anmelden"),
                at.clone(),
                true,
            )
        };
        let wanted = key.without_comment();
        let already = match self.keys(token, "/user/keys")? {
            Ok(keys) => keys.contains(&wanted),
            Err(401 | 403 | 404) => return Ok(missing_scope(self.scopes(false)[0])),
            Err(status) => return Err(Error::Api(format!("{} antwortet mit HTTP {status}", self.hostname))),
        };
        if !already {
            let body = match self.provider {
                Provider::Github => json!({ "title": title, "key": key.line() }),
                _ => {
                    json!({ "title": title, "key": key.line(), "usage_type": if sign { "auth_and_signing" } else { "auth" } })
                }
            };
            let (status, resp) = self.post_json(&format!("{}/user/keys", self.api_base), token, &body)?;
            match status {
                200 | 201 => {}
                401 | 403 | 404 => return Ok(missing_scope(self.scopes(false)[0])),
                400 | 422 => return Ok(UploadResult::failed(rejected(name, &resp), at, false)),
                _ => return Err(self.unexpected(status, &resp)),
            }
        }
        let mut message = if already {
            format!("Der Key war bei {name} schon eingetragen (@{account})")
        } else {
            format!("Key bei {name} eingetragen (@{account})")
        };
        if sign && self.provider == Provider::Github {
            message.push_str(&self.upload_signing_key(token, key, title)?);
        }
        Ok(UploadResult { success: true, already_present: already, account: at, message, auth_failed: false })
    }

    /// GitHub keeps signing keys separately. Problems here do not fail the
    /// upload, they are added to the message.
    fn upload_signing_key(&self, token: &str, key: &PublicKey, title: &str) -> Result<String> {
        let wanted = key.without_comment();
        match self.keys(token, "/user/ssh_signing_keys")? {
            Ok(keys) if keys.contains(&wanted) => return Ok(String::new()),
            Ok(_) => {}
            Err(_) => return Ok(" – als Signing-Key fehlt die Berechtigung „admin:ssh_signing_key“".to_string()),
        }
        let body = json!({ "title": title, "key": key.line() });
        let (status, resp) = self.post_json(&format!("{}/user/ssh_signing_keys", self.api_base), token, &body)?;
        Ok(match status {
            200 | 201 => ", auch als Signing-Key".to_string(),
            400 | 422 => format!(" – Signing-Key: {}", rejected("GitHub", &resp)),
            _ => " – als Signing-Key fehlt die Berechtigung „admin:ssh_signing_key“".to_string(),
        })
    }
}

/// Explains a rejected key, mostly "already in another account".
fn rejected(name: &str, body: &Value) -> String {
    let text = body.to_string().to_lowercase();
    if text.contains("already") || text.contains("taken") {
        format!(
            "Der Key ist schon in einem anderen {name}-Konto eingetragen – ein Key gehört immer zu genau einem Konto"
        )
    } else {
        format!("{name} lehnt den Key ab: {}", message_of(body))
    }
}

/// The human-readable part of an API error.
fn message_of(body: &Value) -> String {
    for key in ["error_description", "message", "error"] {
        match body.get(key) {
            Some(Value::String(s)) => return s.clone(),
            Some(v @ (Value::Object(_) | Value::Array(_))) => return v.to_string(),
            _ => {}
        }
    }
    let text = match body {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    text.chars().take(200).collect()
}

/// The title of the key in the account: its comment (who and where), or the
/// profile name.
pub fn key_title(key: &PublicKey, profile_name: &str) -> String {
    match &key.comment {
        Some(c) if !c.trim().is_empty() => format!("{c} (Keybund)"),
        _ => format!("Keybund – {profile_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::ApiConfig;

    fn env(base: Option<&str>, client: Option<&str>) -> Env {
        let mut env = Env::with_home("/home/t");
        env.api = ApiConfig {
            base_url: base.map(str::to_string),
            github_client_id: client.map(str::to_string),
            gitlab_client_id: client.map(str::to_string),
        };
        env
    }

    #[test]
    fn endpoints_and_device_flow_availability() {
        let e = env(None, Some("abc"));
        let gh = Api::new(&e, Provider::Github, "GitHub.com").unwrap();
        assert_eq!((gh.oauth_base.as_str(), gh.api_base.as_str()), ("https://github.com", "https://api.github.com"));
        assert!(gh.device_flow());
        let ghe = Api::new(&e, Provider::Github, "git.firma.de").unwrap();
        assert_eq!(ghe.api_base, "https://git.firma.de/api/v3");
        assert!(!ghe.device_flow());
        let gl = Api::new(&e, Provider::Gitlab, "gitlab.com").unwrap();
        assert_eq!(gl.api_base, "https://gitlab.com/api/v4");
        assert!(gl.device_flow());
        assert!(!Api::new(&e, Provider::Gitlab, "gitlab.kunde.de").unwrap().device_flow());
        assert!(Api::new(&e, Provider::Bitbucket, "bitbucket.org").is_none());
        assert!(Api::new(&e, Provider::Generic, "1.2.3.4").is_none());
        assert!(!Api::new(&env(None, None), Provider::Github, "github.com").unwrap().device_flow());
    }

    #[test]
    fn token_pages_preselect_scopes() {
        let e = env(None, None);
        assert_eq!(
            Api::new(&e, Provider::Github, "github.com").unwrap().token_url(true),
            "https://github.com/settings/tokens/new?scopes=write:public_key,admin:ssh_signing_key&description=Keybund"
        );
        assert_eq!(
            Api::new(&e, Provider::Gitlab, "gitlab.kunde.de").unwrap().token_url(false),
            "https://gitlab.kunde.de/-/user_settings/personal_access_tokens?name=Keybund&scopes=api"
        );
    }

    #[test]
    fn titles_and_messages() {
        let k = PublicKey::parse(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl t@zentrale",
        )
        .unwrap();
        assert_eq!(key_title(&k, "Privat"), "t@zentrale (Keybund)");
        assert_eq!(key_title(&PublicKey { comment: None, ..k }, "Privat"), "Keybund – Privat");
        assert_eq!(message_of(&json!({"message": "Bad credentials"})), "Bad credentials");
        assert_eq!(message_of(&json!({"message": {"key": ["is invalid"]}})), r#"{"key":["is invalid"]}"#);
        assert_eq!(message_of(&Value::String("x".repeat(300))).len(), 200);
        assert!(rejected("GitHub", &json!({"errors": [{"message": "key is already in use"}]}))
            .contains("anderen GitHub-Konto"));
        assert_eq!(rejected("GitLab", &json!({"message": "invalid"})), "GitLab lehnt den Key ab: invalid");
    }
}
