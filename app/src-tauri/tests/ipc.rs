//! Calls the commands through Tauri's IPC layer (mock runtime), exactly as
//! the frontend does: JSON in, JSON out.

use keybund_app_lib::commands::AppState;
use keybund_core::Env;
use serde_json::{json, Value};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

fn app() -> (tempfile::TempDir, tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let dir = tempfile::tempdir().unwrap();
    let env = Env::with_home(dir.path().canonicalize().unwrap());
    use keybund_app_lib::commands::*;
    let builder = tauri_specta::Builder::<MockRuntime>::new().commands(keybund_app_lib::keybund_commands!(MockRuntime));
    let app = mock_builder()
        .manage(AppState::new(env))
        .invoke_handler(builder.invoke_handler())
        .build(tauri::generate_context!())
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    (dir, app, webview)
}

fn invoke(webview: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().unwrap())
}

fn new_project(id: &str) -> Value {
    json!({
        "profile": {
            "id": id,
            "name": format!("Kunde {id}"),
            "color": "blue",
            "key": { "path": format!("~/.ssh/{id}_ed25519") },
            "git": { "name": "T", "email": format!("t@{id}.de"), "sign": false },
            "folders": [format!("~/dev/{id}")],
            "archived": false,
            "hosts": [{ "hostname": "gitlab.kunde.de", "provider": "gitlab", "alias": null, "user": null, "port": null }]
        },
        "key": { "type": "generate", "comment": format!("t@{id}"), "passphrase": null },
        "make_default": false
    })
}

#[test]
fn empty_profile_list() {
    let (_d, _app, webview) = app();
    let res = invoke(&webview, "load_profiles", json!({})).unwrap();
    assert_eq!(res["profiles"], json!([]));
    assert_eq!(res["default_profile"], Value::Null);
}

#[test]
fn wizard_flow_over_ipc() {
    if std::process::Command::new("ssh-keygen").arg("-?").output().is_err() {
        assert!(std::env::var_os("CI").is_none(), "ssh-keygen is required in CI");
        return;
    }
    let (_d, _app, webview) = app();

    let defaults = invoke(&webview, "profile_defaults", json!({ "name": "Kunde C" })).unwrap();
    assert_eq!(defaults["id"], "kunde-c");

    let plan = invoke(&webview, "preview_new_project", json!({ "project": new_project("c") })).unwrap();
    let paths: Vec<&str> = plan["changes"].as_array().unwrap().iter().map(|c| c["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"~/.config/keybund/git/c.gitconfig"));
    assert!(paths.contains(&"~/.ssh/config"));

    // create_project needs the clipboard plugin, so the app's api is used
    // for that step; everything around it goes over IPC.
    let state = webview.state::<AppState>();
    let project: keybund_app_lib::api::NewProject = serde_json::from_value(new_project("c")).unwrap();
    keybund_app_lib::api::create_project(&state.env, &project).unwrap();

    let res = invoke(&webview, "load_profiles", json!({})).unwrap();
    let p = &res["profiles"][0];
    assert_eq!(p["profile"]["id"], "c");
    assert_eq!(p["is_default"], false);
    assert_eq!(p["add_key_urls"][0]["url"], "https://gitlab.kunde.de/-/user_settings/ssh_keys");
    assert!(p["key"]["fingerprint"].as_str().unwrap().starts_with("SHA256:"));

    let plan = invoke(&webview, "plan_changes", json!({})).unwrap();
    assert_eq!(plan["changes"], json!([]));

    let report = invoke(&webview, "run_doctor", json!({})).unwrap();
    let include = report["checks"].as_array().unwrap().iter().find(|c| c["id"] == "ssh-include").unwrap();
    assert_eq!(include["status"], "ok");

    let which = invoke(&webview, "which", json!({ "query": "~/dev/c" })).unwrap();
    assert_eq!(which["profile_id"], "c");
    assert_eq!(which["mode"], "folder");

    let preview = invoke(&webview, "clone_preview", json!({ "url": "git@gitlab.kunde.de:team/app.git" })).unwrap();
    assert_eq!(preview["candidates"][0]["target"], "~/dev/c/app");

    let err = invoke(&webview, "clone_preview", json!({ "url": "nonsense" })).unwrap_err();
    assert_eq!(err["kind"], "invalid_url");
    // The frontend gets the message key and its arguments, not text.
    assert_eq!(err["message"], json!({ "key": "error_invalidUrl", "args": { "url": "nonsense" } }));

    let archived = invoke(&webview, "archive_profile", json!({ "id": "c" })).unwrap();
    assert_eq!(archived["key_path"], "~/.ssh/archive/c_ed25519");
}

#[test]
fn fix_is_tagged_like_the_typescript_type() {
    let (_d, _app, webview) = app();
    let report = invoke(&webview, "fix_doctor", json!({ "fix": { "type": "permissions" } })).unwrap();
    assert!(report["checks"].is_array());
}

#[test]
fn profile_commands_over_ipc() {
    if std::process::Command::new("ssh-keygen").arg("-?").output().is_err() {
        assert!(std::env::var_os("CI").is_none(), "ssh-keygen is required in CI");
        return;
    }
    let (_d, _app, webview) = app();
    let state = webview.state::<AppState>();
    let env = state.env.clone();
    for id in ["a", "b"] {
        let project: keybund_app_lib::api::NewProject = serde_json::from_value(new_project(id)).unwrap();
        keybund_app_lib::api::create_project(&env, &project).unwrap();
    }

    let normalized = invoke(&webview, "normalize_folder", json!({ "folder": "~/dev/x" })).unwrap();
    assert_eq!(normalized, "~/dev/x/");
    let paths = invoke(&webview, "paths", json!({})).unwrap();
    assert_eq!(paths["profiles_file"], "~/.config/keybund/profiles.toml");

    let files = invoke(&webview, "generated_config", json!({ "id": "b" })).unwrap();
    assert_eq!(files[0]["path"], "~/.config/keybund/git/b.gitconfig");

    let keys = invoke(&webview, "scan_keys", json!({})).unwrap();
    assert_eq!(keys.as_array().unwrap().len(), 2);

    // An edit is written right away.
    let mut profile = invoke(&webview, "load_profiles", json!({})).unwrap()["profiles"][1]["profile"].clone();
    profile["git"]["sign"] = json!(true);
    let saved = invoke(&webview, "update_profile", json!({ "profile": profile })).unwrap();
    assert_eq!(saved["profile"]["git"]["sign"], true);
    assert!(std::fs::read_to_string(env.profile_gitconfig("b")).unwrap().contains("gpgsign = true"));
    assert_eq!(invoke(&webview, "load_profiles", json!({})).unwrap()["pending_changes"], 0);

    // A file changed outside Keybund: plan → stale apply → apply.
    std::fs::write(env.profile_gitconfig("b"), "kaputt\n").unwrap();
    let plan = invoke(&webview, "plan_changes", json!({})).unwrap();
    let diffs: Vec<Value> = plan["changes"].as_array().unwrap().iter().map(|c| c["diff"].clone()).collect();
    assert!(!diffs.is_empty());
    let err = invoke(&webview, "apply_changes", json!({ "expectedDiffs": [] })).unwrap_err();
    assert_eq!(err["kind"], "stale_plan");
    let report = invoke(&webview, "apply_changes", json!({ "expectedDiffs": diffs })).unwrap();
    assert!(!report["written"].as_array().unwrap().is_empty());

    invoke(&webview, "set_default_profile", json!({ "id": "b" })).unwrap();
    assert_eq!(invoke(&webview, "load_profiles", json!({})).unwrap()["default_profile"], "b");

    assert_eq!(invoke(&webview, "last_test", json!({ "id": "a" })).unwrap(), Value::Null);
    let err = invoke(&webview, "clone_repo", json!({ "url": "git@x:y/z.git", "profileId": "nope", "target": null }))
        .unwrap_err();
    assert_eq!(err["kind"], "profile_not_found");
    let err = invoke(&webview, "open_url", json!({ "url": "file:///etc/passwd" })).unwrap_err();
    assert_eq!(err["kind"], "not_https");
    assert_eq!(err["message"]["key"], "error_notHttps");
}

#[test]
fn test_connection_without_host_is_an_error() {
    if std::process::Command::new("ssh-keygen").arg("-?").output().is_err() {
        return;
    }
    let (_d, _app, webview) = app();
    let env = webview.state::<AppState>().env.clone();
    let mut project = new_project("s");
    project["profile"]["hosts"] = json!([]);
    let project: keybund_app_lib::api::NewProject = serde_json::from_value(project).unwrap();
    keybund_app_lib::api::create_project(&env, &project).unwrap();
    let err = invoke(&webview, "test_connection", json!({ "id": "s" })).unwrap_err();
    assert_eq!(err["kind"], "no_host");
    let which = invoke(&webview, "which", json!({ "query": "~/dev/s/repo" })).unwrap();
    assert_eq!(which["profile_id"], "s");
}

#[test]
fn key_page_link_over_ipc() {
    let (d, _app, webview) = app();
    let config = d.path().canonicalize().unwrap().join(".config/keybund");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("profiles.toml"),
        "[[profile]]\nid = \"c\"\nname = \"C\"\nkey = { path = \"~/.ssh/c_ed25519\" }\nfolders = [\"~/dev/c/\"]\n\n[[profile.host]]\nhostname = \"gitlab.kunde.de\"\nprovider = \"gitlab\"\n",
    )
    .unwrap();
    // The page where the user pastes the pubkey, also for a self-hosted GitLab.
    let view = invoke(&webview, "load_profiles", json!({})).unwrap();
    assert_eq!(view["profiles"][0]["add_key_urls"][0]["url"], "https://gitlab.kunde.de/-/user_settings/ssh_keys");
}
