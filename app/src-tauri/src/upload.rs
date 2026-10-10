//! Adding a profile's key to a GitHub/GitLab account from the GUI: device
//! flow (login in the browser) or a personal access token. The token stays
//! in the backend and is dropped after the upload, unless the user asks to
//! keep it in the keyring.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

use keybund_core::model::{HostEntry, Profile, Provider};
use keybund_core::upload::{self, Api, DeviceCode, Poll, UploadResult};
use keybund_core::{keygen, provider, store, tokens, Env};

use crate::api::{CmdError, CmdResult};

/// How the key of a profile gets into an account.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct UploadTarget {
    pub provider: Provider,
    pub hostname: String,
    /// Keybund can add the key via the API (GitHub, GitLab).
    pub supported: bool,
    /// Login in the browser is available (registered OAuth app).
    pub device_flow: bool,
    /// Page to create a token with the right scopes.
    pub token_url: Option<String>,
    pub scopes: Vec<String>,
    /// The page to add the key by hand.
    pub manual_url: Option<String>,
    /// A token for this profile and host is in the keyring.
    pub remembered: bool,
    /// Tokens can be kept (`secret-tool` is installed).
    pub keyring: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DeviceFlowStart {
    pub flow_id: String,
    pub user_code: String,
    /// Where to enter the code (with the code filled in where possible).
    pub verification_uri: String,
    pub expires_in: u32,
    pub interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeviceFlowStatus {
    /// Not confirmed yet; poll again after `interval` seconds.
    Pending {
        interval: u32,
    },
    Done {
        result: UploadResult,
    },
    Denied,
    Expired,
}

struct PendingFlow {
    profile_id: String,
    host: String,
    code: DeviceCode,
    interval: u64,
    deadline: Instant,
}

/// Device flows waiting for the user, by id.
#[derive(Default)]
pub struct Flows {
    next: AtomicU64,
    pending: Mutex<HashMap<String, PendingFlow>>,
}

struct Resolved {
    profile: Profile,
    host: HostEntry,
    api: Option<Api>,
}

impl Resolved {
    fn api(&self) -> CmdResult<&Api> {
        self.api.as_ref().ok_or_else(|| {
            CmdError::other(format!("{} kann Keybund nicht automatisch eintragen", self.host.provider.display_name()))
        })
    }

    fn sign(&self) -> bool {
        self.profile.git.as_ref().is_some_and(|g| g.sign)
    }

    fn label(&self) -> String {
        format!("Keybund: {}-Token für {}", self.host.provider.display_name(), self.profile.name)
    }
}

fn resolve(env: &Env, id: &str, host: &str) -> CmdResult<Resolved> {
    let config = store::load(env)?;
    let profile = config.profile(id)?.clone();
    let entry = profile
        .hosts
        .iter()
        .find(|h| h.provider.is_git_host() && (h.target_name() == host || h.hostname.eq_ignore_ascii_case(host)))
        .cloned()
        .ok_or_else(|| CmdError::other(format!("Key „{id}“ hat kein Git-Konto {host}")))?;
    let api = Api::new(env, entry.provider, &entry.hostname);
    Ok(Resolved { profile, host: entry, api })
}

pub fn upload_target(env: &Env, id: &str, host: &str) -> CmdResult<UploadTarget> {
    let r = resolve(env, id, host)?;
    let sign = r.sign();
    let keyring = tokens::available();
    Ok(UploadTarget {
        provider: r.host.provider,
        hostname: r.host.hostname.clone(),
        supported: r.api.is_some(),
        device_flow: r.api.as_ref().is_some_and(Api::device_flow),
        token_url: r.api.as_ref().map(|a| a.token_url(sign)),
        scopes: r.api.as_ref().map(|a| a.scopes(sign).iter().map(|s| s.to_string()).collect()).unwrap_or_default(),
        manual_url: provider::add_key_url(r.host.provider, &r.host.hostname),
        remembered: keyring && tokens::load(env, &r.profile.id, &r.host.hostname.to_ascii_lowercase()).is_some(),
        keyring,
    })
}

/// Uploads with `token`, or with the stored token if `None`.
pub fn upload_key(env: &Env, id: &str, host: &str, token: Option<String>, remember: bool) -> CmdResult<UploadResult> {
    let r = resolve(env, id, host)?;
    let api = r.api()?;
    let (token, fresh) = match token.filter(|t| !t.trim().is_empty()) {
        Some(t) => (t.trim().to_string(), true),
        None => (
            tokens::load(env, &r.profile.id, &api.hostname)
                .ok_or_else(|| CmdError::other("Kein gespeichertes Token – bitte anmelden oder ein Token eingeben"))?,
            false,
        ),
    };
    let mut result = upload(env, &r, api, &token)?;
    if result.auth_failed && !fresh {
        // An expired token is no use to anyone.
        tokens::forget(env, &r.profile.id, &api.hostname).ok();
    }
    if result.success && fresh && remember {
        remember_token(env, &r, api, &token, &mut result);
    }
    Ok(result)
}

fn upload(env: &Env, r: &Resolved, api: &Api, token: &str) -> CmdResult<UploadResult> {
    let public = keygen::read_public(env, &r.profile.key)?;
    let title = upload::key_title(&public, &r.profile.name);
    Ok(api.upload(token, &public, &title, r.sign())?)
}

fn remember_token(env: &Env, r: &Resolved, api: &Api, token: &str, result: &mut UploadResult) {
    if let Err(e) = tokens::store(env, &r.profile.id, &api.hostname, &r.label(), token) {
        result.message.push_str(&format!(" – Token nicht gespeichert: {e}"));
    }
}

pub fn forget_token(env: &Env, id: &str, host: &str) -> CmdResult<()> {
    let r = resolve(env, id, host)?;
    tokens::forget(env, &r.profile.id, &r.host.hostname.to_ascii_lowercase())?;
    Ok(())
}

pub fn start_device_flow(env: &Env, flows: &Flows, id: &str, host: &str) -> CmdResult<DeviceFlowStart> {
    let r = resolve(env, id, host)?;
    let code = r.api()?.start_device_flow(r.sign())?;
    let flow_id = format!("flow-{}", flows.next.fetch_add(1, Ordering::Relaxed));
    let start = DeviceFlowStart {
        flow_id: flow_id.clone(),
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri_complete.clone().unwrap_or_else(|| code.verification_uri.clone()),
        expires_in: code.expires_in.min(u32::MAX as u64) as u32,
        interval: code.interval.min(u32::MAX as u64) as u32,
    };
    let pending = PendingFlow {
        profile_id: id.to_string(),
        host: host.to_string(),
        interval: code.interval,
        deadline: Instant::now() + Duration::from_secs(code.expires_in),
        code,
    };
    flows.pending.lock().map_err(|_| CmdError::other("Interner Fehler"))?.insert(flow_id, pending);
    Ok(start)
}

/// Polls once; on success uploads the key right away. The token never
/// leaves the backend.
pub fn poll_device_flow(env: &Env, flows: &Flows, flow_id: &str, remember: bool) -> CmdResult<DeviceFlowStatus> {
    let (profile_id, host, device_code, deadline) = {
        let pending = flows.pending.lock().map_err(|_| CmdError::other("Interner Fehler"))?;
        let f = pending
            .get(flow_id)
            .ok_or_else(|| CmdError::other("Diese Anmeldung ist nicht mehr aktiv – bitte neu starten"))?;
        (f.profile_id.clone(), f.host.clone(), f.code.device_code.clone(), f.deadline)
    };
    let finish = |status: DeviceFlowStatus| {
        if let Ok(mut pending) = flows.pending.lock() {
            pending.remove(flow_id);
        }
        Ok(status)
    };
    if Instant::now() >= deadline {
        return finish(DeviceFlowStatus::Expired);
    }
    let r = resolve(env, &profile_id, &host)?;
    let api = r.api()?;
    match api.poll_device_flow(&device_code)? {
        Poll::Pending => Ok(DeviceFlowStatus::Pending { interval: interval(flows, flow_id, 0) }),
        Poll::SlowDown => Ok(DeviceFlowStatus::Pending { interval: interval(flows, flow_id, 5) }),
        Poll::Denied => finish(DeviceFlowStatus::Denied),
        Poll::Expired => finish(DeviceFlowStatus::Expired),
        Poll::Token(token) => {
            let mut result = upload(env, &r, api, &token)?;
            if result.success && remember {
                remember_token(env, &r, api, &token, &mut result);
            }
            finish(DeviceFlowStatus::Done { result })
        }
    }
}

/// The current poll interval, raised by `add` seconds.
fn interval(flows: &Flows, flow_id: &str, add: u64) -> u32 {
    let mut pending = match flows.pending.lock() {
        Ok(p) => p,
        Err(_) => return 5,
    };
    match pending.get_mut(flow_id) {
        Some(f) => {
            f.interval += add;
            f.interval.min(u32::MAX as u64) as u32
        }
        None => 5,
    }
}

pub fn cancel_device_flow(flows: &Flows, flow_id: &str) {
    if let Ok(mut pending) = flows.pending.lock() {
        pending.remove(flow_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{create_project, KeyChoice, NewProject};
    use keybund_core::env::ApiConfig;
    use keybund_core::model::{GitIdentity, KeyRef};
    use mockito::{Matcher, Server};

    fn setup(server: &Server, provider: Provider, hostname: &str) -> (tempfile::TempDir, Env) {
        let dir = tempfile::tempdir().unwrap();
        let mut env = Env::with_home(dir.path().canonicalize().unwrap());
        env.api =
            ApiConfig { base_url: Some(server.url()), github_client_id: Some("gh".into()), gitlab_client_id: None };
        let project = NewProject {
            profile: Profile {
                id: "kunde".into(),
                name: "Kunde".into(),
                color: None,
                key: KeyRef { path: "~/.ssh/kunde_ed25519".into(), comment: None },
                git: Some(GitIdentity { name: "T".into(), email: "t@k.de".into(), sign: false }),
                folders: vec!["~/dev/kunde".into()],
                archived: false,
                hosts: vec![
                    HostEntry { hostname: hostname.into(), provider, ..Default::default() },
                    HostEntry { hostname: "1.2.3.4".into(), provider: Provider::Generic, ..Default::default() },
                ],
            },
            key: KeyChoice::Generate { comment: "t@kunde".into(), passphrase: None },
            make_default: false,
        };
        create_project(&env, &project).unwrap();
        (dir, env)
    }

    fn github_accepts(server: &mut Server, token: &str) -> mockito::Mock {
        server
            .mock("GET", "/user")
            .match_header("authorization", format!("Bearer {token}").as_str())
            .with_body(r#"{"login":"octo"}"#)
            .create();
        server.mock("GET", "/user/keys").match_query(Matcher::Any).with_body("[]").create();
        server
            .mock("POST", "/user/keys")
            .match_body(Matcher::PartialJsonString(r#"{"title":"t@kunde (Keybund)"}"#.into()))
            .with_status(201)
            .with_body("{}")
            .create()
    }

    #[test]
    fn targets() {
        let server = Server::new();
        let (_d, env) = setup(&server, Provider::Github, "github.com");
        let t = upload_target(&env, "kunde", "github.com").unwrap();
        assert!(t.supported && t.device_flow);
        assert_eq!(t.scopes, vec!["write:public_key"]);
        assert_eq!(t.manual_url.as_deref(), Some("https://github.com/settings/ssh/new"));
        assert!(!t.remembered);
        assert!(upload_target(&env, "kunde", "1.2.3.4").unwrap_err().message.contains("kein Git-Konto 1.2.3.4"));

        let (_d, env) = setup(&server, Provider::Bitbucket, "bitbucket.org");
        let t = upload_target(&env, "kunde", "bitbucket.org").unwrap();
        assert!(!t.supported && !t.device_flow && t.token_url.is_none());
        let err = upload_key(&env, "kunde", "bitbucket.org", Some("x".into()), false).unwrap_err();
        assert_eq!(err.message, "Bitbucket kann Keybund nicht automatisch eintragen");
    }

    #[test]
    fn device_flow_uploads_once_confirmed() {
        let mut server = Server::new();
        let (_d, env) = setup(&server, Provider::Github, "github.com");
        server
            .mock("POST", "/login/device/code")
            .with_body(r#"{"device_code":"dc","user_code":"AB-12","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#)
            .create();
        let flows = Flows::default();
        let start = start_device_flow(&env, &flows, "kunde", "github.com").unwrap();
        assert_eq!(
            (start.user_code.as_str(), start.verification_uri.as_str(), start.interval),
            ("AB-12", "https://github.com/login/device", 5)
        );

        let pending = server
            .mock("POST", "/login/oauth/access_token")
            .with_body(r#"{"error":"authorization_pending"}"#)
            .expect(1)
            .create();
        assert!(matches!(
            poll_device_flow(&env, &flows, &start.flow_id, false).unwrap(),
            DeviceFlowStatus::Pending { interval: 5 }
        ));
        pending.remove();
        let slow = server.mock("POST", "/login/oauth/access_token").with_body(r#"{"error":"slow_down"}"#).create();
        assert!(matches!(
            poll_device_flow(&env, &flows, &start.flow_id, false).unwrap(),
            DeviceFlowStatus::Pending { interval: 10 }
        ));
        slow.remove();

        server.mock("POST", "/login/oauth/access_token").with_body(r#"{"access_token":"gho_x"}"#).create();
        let add = github_accepts(&mut server, "gho_x");
        match poll_device_flow(&env, &flows, &start.flow_id, false).unwrap() {
            DeviceFlowStatus::Done { result } => assert_eq!(result.message, "Key bei GitHub eingetragen (@octo)"),
            other => panic!("{other:?}"),
        }
        add.assert();
        let err = poll_device_flow(&env, &flows, &start.flow_id, false).unwrap_err();
        assert!(err.message.contains("nicht mehr aktiv"));
    }

    #[test]
    fn denied_and_cancelled_flows_end() {
        let mut server = Server::new();
        let (_d, env) = setup(&server, Provider::Github, "github.com");
        server
            .mock("POST", "/login/device/code")
            .with_body(r#"{"device_code":"dc","user_code":"AB-12","verification_uri":"https://github.com/login/device","expires_in":900}"#)
            .create();
        server.mock("POST", "/login/oauth/access_token").with_body(r#"{"error":"access_denied"}"#).create();
        let flows = Flows::default();
        let a = start_device_flow(&env, &flows, "kunde", "github.com").unwrap();
        assert!(matches!(poll_device_flow(&env, &flows, &a.flow_id, false).unwrap(), DeviceFlowStatus::Denied));
        let b = start_device_flow(&env, &flows, "kunde", "github.com").unwrap();
        assert_ne!(a.flow_id, b.flow_id);
        cancel_device_flow(&flows, &b.flow_id);
        assert!(poll_device_flow(&env, &flows, &b.flow_id, false).is_err());
    }

    #[test]
    fn token_upload() {
        let mut server = Server::new();
        let (_d, env) = setup(&server, Provider::Gitlab, "gitlab.kunde.de");
        assert!(!upload_target(&env, "kunde", "gitlab.kunde.de").unwrap().device_flow);
        server
            .mock("GET", "/api/v4/user")
            .match_header("authorization", "Bearer glpat")
            .with_body(r#"{"username":"t"}"#)
            .create();
        server.mock("GET", "/api/v4/user").with_status(401).with_body("{}").create();
        server.mock("GET", "/api/v4/user/keys").match_query(Matcher::Any).with_body("[]").create();
        server.mock("POST", "/api/v4/user/keys").with_status(201).with_body("{}").create();
        let r = upload_key(&env, "kunde", "gitlab.kunde.de", Some(" glpat ".into()), false).unwrap();
        assert_eq!(r.message, "Key bei GitLab eingetragen (@t)");
        let r = upload_key(&env, "kunde", "gitlab.kunde.de", Some("wrong".into()), false).unwrap();
        assert!(r.auth_failed);
        let err = upload_key(&env, "kunde", "gitlab.kunde.de", None, false).unwrap_err();
        assert!(err.message.contains("Kein gespeichertes Token"));
    }
}
