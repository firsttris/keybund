//! Key upload against a local mock of the GitHub and GitLab APIs.

use keybund_core::env::ApiConfig;
use keybund_core::keygen::PublicKey;
use keybund_core::upload::{Api, Poll};
use keybund_core::{Env, Error, Provider};
use mockito::{Matcher, Server};
use serde_json::json;

const KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl t@zentrale";
const OTHER: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBcdj0E0jnGqmQ7Ui7aXzYuNOOjZpZ8Yy4o3D1vHjTkn x@y";

fn api(server: &Server, provider: Provider, hostname: &str) -> Api {
    let mut env = Env::with_home("/nonexistent");
    env.api = ApiConfig {
        base_url: Some(server.url()),
        github_client_id: Some("gh-client".into()),
        gitlab_client_id: Some("gl-client".into()),
    };
    Api::new(&env, provider, hostname).unwrap()
}

fn key() -> PublicKey {
    PublicKey::parse(KEY).unwrap()
}

fn bearer() -> Matcher {
    Matcher::Exact("Bearer tok".into())
}

#[test]
fn github_device_flow() {
    let mut server = Server::new();
    let start = server
        .mock("POST", "/login/device/code")
        .match_header("accept", "application/json")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("client_id".into(), "gh-client".into()),
            Matcher::UrlEncoded("scope".into(), "write:public_key admin:ssh_signing_key".into()),
        ]))
        .with_body(
            json!({"device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device", "expires_in": 900, "interval": 5})
                .to_string(),
        )
        .create();
    let gh = api(&server, Provider::Github, "github.com");
    let code = gh.start_device_flow(true).unwrap();
    start.assert();
    assert_eq!((code.user_code.as_str(), code.device_code.as_str(), code.interval), ("ABCD-1234", "dc", 5));
    assert_eq!(code.verification_uri_complete, None);

    let mut poll = |answer: serde_json::Value| {
        let m = server
            .mock("POST", "/login/oauth/access_token")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("device_code".into(), "dc".into()),
                Matcher::UrlEncoded("grant_type".into(), "urn:ietf:params:oauth:grant-type:device_code".into()),
            ]))
            .with_body(answer.to_string())
            .create();
        let r = gh.poll_device_flow("dc");
        m.remove();
        r
    };
    assert_eq!(poll(json!({"error": "authorization_pending"})).unwrap(), Poll::Pending);
    assert_eq!(poll(json!({"error": "slow_down", "interval": 10})).unwrap(), Poll::SlowDown);
    assert_eq!(poll(json!({"error": "access_denied"})).unwrap(), Poll::Denied);
    assert_eq!(poll(json!({"error": "expired_token"})).unwrap(), Poll::Expired);
    assert_eq!(poll(json!({"access_token": "tok", "token_type": "bearer"})).unwrap(), Poll::Token("tok".into()));
    let err = poll(json!({"error": "device_flow_disabled", "error_description": "Device Flow must be explicitly enabled for this App"}))
        .unwrap_err();
    assert!(err.to_string().contains("Device Flow must be explicitly enabled"), "{err}");
}

#[test]
fn gitlab_device_flow_answers_with_http_400_while_pending() {
    let mut server = Server::new();
    server
        .mock("POST", "/oauth/authorize_device")
        .match_body(Matcher::UrlEncoded("scope".into(), "api".into()))
        .with_body(
            json!({"device_code": "dc", "user_code": "XYZ", "verification_uri": "https://gitlab.com/oauth/device",
                   "verification_uri_complete": "https://gitlab.com/oauth/device?user_code=XYZ", "expires_in": 300, "interval": 5})
            .to_string(),
        )
        .create();
    server
        .mock("POST", "/oauth/token")
        .with_status(400)
        .with_body(json!({"error": "authorization_pending"}).to_string())
        .create();
    let gl = api(&server, Provider::Gitlab, "gitlab.com");
    let code = gl.start_device_flow(false).unwrap();
    assert_eq!(code.verification_uri_complete.as_deref(), Some("https://gitlab.com/oauth/device?user_code=XYZ"));
    assert_eq!(gl.poll_device_flow("dc").unwrap(), Poll::Pending);
}

#[test]
fn device_flow_needs_a_registered_app() {
    let server = Server::new();
    let mut env = Env::with_home("/nonexistent");
    env.api.base_url = Some(server.url());
    let gh = Api::new(&env, Provider::Github, "github.com").unwrap();
    let err = gh.start_device_flow(false).unwrap_err();
    assert!(err.to_string().contains("bitte ein Token verwenden"), "{err}");

    let mut server = Server::new();
    server.mock("POST", "/oauth/authorize_device").with_status(401).with_body(r#"{"error":"invalid_client"}"#).create();
    let err = api(&server, Provider::Gitlab, "gitlab.com").start_device_flow(false).unwrap_err();
    assert_eq!(err.to_string(), format!("{} antwortet mit HTTP 401: invalid_client", "gitlab.com"));
}

#[test]
fn github_upload_adds_auth_and_signing_key() {
    let mut server = Server::new();
    server.mock("GET", "/user").match_header("authorization", bearer()).with_body(r#"{"login":"octo"}"#).create();
    server
        .mock("GET", "/user/keys")
        .match_query(Matcher::Any)
        .with_body(json!([{"id": 1, "key": OTHER}]).to_string())
        .create();
    let add = server
        .mock("POST", "/user/keys")
        .match_header("authorization", bearer())
        .match_body(Matcher::Json(json!({"title": "t@zentrale (Keybund)", "key": KEY})))
        .with_status(201)
        .with_body("{}")
        .create();
    server.mock("GET", "/user/ssh_signing_keys").match_query(Matcher::Any).with_body("[]").create();
    let sign = server.mock("POST", "/user/ssh_signing_keys").with_status(201).with_body("{}").create();

    let r = api(&server, Provider::Github, "github.com").upload("tok", &key(), "t@zentrale (Keybund)", true).unwrap();
    add.assert();
    sign.assert();
    assert!(r.success && !r.already_present && !r.auth_failed);
    assert_eq!(r.account.as_deref(), Some("octo"));
    assert_eq!(r.message, "Key bei GitHub eingetragen (@octo), auch als Signing-Key");
}

#[test]
fn github_upload_skips_a_key_that_is_already_there() {
    let mut server = Server::new();
    server.mock("GET", "/user").with_body(r#"{"login":"octo"}"#).create();
    // GitHub lists keys without their comment.
    server
        .mock("GET", "/user/keys")
        .match_query(Matcher::Any)
        .with_body(
            json!([{"key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"}])
                .to_string(),
        )
        .create();
    let add = server.mock("POST", "/user/keys").expect(0).create();
    let r = api(&server, Provider::Github, "github.com").upload("tok", &key(), "x", false).unwrap();
    add.assert();
    assert!(r.success && r.already_present);
    assert_eq!(r.message, "Der Key war bei GitHub schon eingetragen (@octo)");
}

#[test]
fn github_upload_explains_failures() {
    // A key can only belong to one account.
    let mut server = Server::new();
    server.mock("GET", "/user").with_body(r#"{"login":"octo"}"#).create();
    server.mock("GET", "/user/keys").match_query(Matcher::Any).with_body("[]").create();
    server
        .mock("POST", "/user/keys")
        .with_status(422)
        .with_body(r#"{"message":"Validation Failed","errors":[{"resource":"PublicKey","code":"custom","field":"key","message":"key is already in use"}]}"#)
        .create();
    let r = api(&server, Provider::Github, "github.com").upload("tok", &key(), "x", false).unwrap();
    assert!(!r.success && !r.auth_failed);
    assert!(r.message.contains("schon in einem anderen GitHub-Konto"), "{}", r.message);

    // Revoked token.
    let mut server = Server::new();
    server.mock("GET", "/user").with_status(401).with_body(r#"{"message":"Bad credentials"}"#).create();
    let r = api(&server, Provider::Github, "github.com").upload("tok", &key(), "x", false).unwrap();
    assert!(!r.success && r.auth_failed && r.account.is_none());

    // Token without the scope for keys.
    let mut server = Server::new();
    server.mock("GET", "/user").with_body(r#"{"login":"octo"}"#).create();
    server.mock("GET", "/user/keys").match_query(Matcher::Any).with_status(404).with_body("{}").create();
    let r = api(&server, Provider::Github, "github.com").upload("tok", &key(), "x", false).unwrap();
    assert!(r.auth_failed);
    assert_eq!(r.message, "Dem Token fehlt die Berechtigung „write:public_key“ – bitte neu anmelden");

    // Signing key scope missing: the auth key still counts.
    let mut server = Server::new();
    server.mock("GET", "/user").with_body(r#"{"login":"octo"}"#).create();
    server.mock("GET", "/user/keys").match_query(Matcher::Any).with_body(json!([{"key": KEY}]).to_string()).create();
    server.mock("GET", "/user/ssh_signing_keys").match_query(Matcher::Any).with_status(404).with_body("{}").create();
    let r = api(&server, Provider::Github, "github.com").upload("tok", &key(), "x", true).unwrap();
    assert!(r.success);
    assert!(r.message.ends_with("als Signing-Key fehlt die Berechtigung „admin:ssh_signing_key“"), "{}", r.message);

    // Server error.
    let mut server = Server::new();
    server.mock("GET", "/user").with_status(500).with_body("oops").create();
    let err = api(&server, Provider::Github, "github.com").upload("tok", &key(), "x", false).unwrap_err();
    assert!(matches!(err, Error::Api(_)), "{err}");
}

#[test]
fn gitlab_upload_sets_the_usage_type() {
    let mut server = Server::new();
    server.mock("GET", "/api/v4/user").with_body(r#"{"username":"tanuki"}"#).create();
    server.mock("GET", "/api/v4/user/keys").match_query(Matcher::Any).with_body("[]").create();
    let add = server
        .mock("POST", "/api/v4/user/keys")
        .match_body(Matcher::Json(json!({"title": "t", "key": KEY, "usage_type": "auth_and_signing"})))
        .with_status(201)
        .with_body("{}")
        .create();
    let r = api(&server, Provider::Gitlab, "gitlab.kunde.de").upload("tok", &key(), "t", true).unwrap();
    add.assert();
    assert_eq!(r.message, "Key bei GitLab eingetragen (@tanuki)");

    let mut server = Server::new();
    server.mock("GET", "/api/v4/user").with_body(r#"{"username":"tanuki"}"#).create();
    server.mock("GET", "/api/v4/user/keys").match_query(Matcher::Any).with_body("[]").create();
    server
        .mock("POST", "/api/v4/user/keys")
        .with_status(400)
        .with_body(r#"{"message":{"fingerprint_sha256":["has already been taken"]}}"#)
        .create();
    let r = api(&server, Provider::Gitlab, "gitlab.kunde.de").upload("tok", &key(), "t", false).unwrap();
    assert!(r.message.contains("anderen GitLab-Konto"), "{}", r.message);
}

#[test]
fn unreachable_host_is_a_network_error() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}");
    let mut env = Env::with_home("/nonexistent");
    env.api.base_url = Some(url);
    let err = Api::new(&env, Provider::Github, "github.com").unwrap().account("tok").unwrap_err();
    assert!(matches!(err, Error::Network { .. }), "{err}");
    assert!(err.to_string().starts_with("Keine Verbindung zu github.com"), "{err}");
}
