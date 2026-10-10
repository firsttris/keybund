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
    assert_eq!(p["is_default"], true);
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
    assert!(err["message"].as_str().unwrap().contains("Ungültige Repo-URL"));

    let archived = invoke(&webview, "archive_profile", json!({ "id": "c" })).unwrap();
    assert_eq!(archived["key_path"], "~/.ssh/archive/c_ed25519");
}

#[test]
fn fix_is_tagged_like_the_typescript_type() {
    let (_d, _app, webview) = app();
    let report = invoke(&webview, "fix_doctor", json!({ "fix": { "type": "permissions" } })).unwrap();
    assert!(report["checks"].is_array());
}
