//! Tauri commands. Each one is a thin wrapper: it takes the environment
//! from the app state, runs the blocking work off the main thread and
//! delegates to `api` or the core.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use keybund_core::apply::{ApplyReport, Plan};
use keybund_core::archive::ArchiveResult;
use keybund_core::doctor::{self, Fix, Report};
use keybund_core::keygen::ImportCandidate;
use keybund_core::model::Profile;
use keybund_core::probe::{self, ProbeResult};
use keybund_core::which::{self, WhichResult};
use keybund_core::{store, Env};

use crate::api::{self, *};

pub struct AppState {
    pub env: Env,
    pub last_tests: Mutex<HashMap<String, LastTest>>,
}

impl AppState {
    pub fn new(env: Env) -> Self {
        AppState { env, last_tests: Mutex::new(HashMap::new()) }
    }

    fn last_test(&self, id: &str) -> Option<LastTest> {
        self.last_tests.lock().ok()?.get(id).cloned()
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn load_profiles(state: State<'_, AppState>) -> CmdResult<ProfilesView> {
    let env = state.env.clone();
    let tests = state.last_tests.lock().map(|t| t.clone()).unwrap_or_default();
    blocking(move || api::load_profiles(&env, |id| tests.get(id).cloned())).await
}

#[tauri::command]
#[specta::specta]
pub fn profile_defaults(state: State<'_, AppState>, name: String) -> CmdResult<ProfileDefaults> {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let host = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
    api::profile_defaults(&state.env, &name, &user, &host)
}

#[tauri::command]
#[specta::specta]
pub fn normalize_folder(state: State<'_, AppState>, folder: String) -> String {
    store::normalize_folder(&state.env, &folder)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_new_project(state: State<'_, AppState>, project: NewProject) -> CmdResult<Plan> {
    let env = state.env.clone();
    blocking(move || api::preview_new_project(&env, &project)).await
}

/// Creates the project, writes the configs, copies the public key and
/// opens the provider's "add key" page.
#[tauri::command]
#[specta::specta]
pub async fn create_project<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    project: NewProject,
    open_provider_page: bool,
) -> CmdResult<CreatedProject> {
    let env = state.env.clone();
    let created = blocking(move || api::create_project(&env, &project)).await?;
    let _ = app.clipboard().write_text(created.public_key.clone());
    if open_provider_page {
        if let Some(link) = created.profile.add_key_urls.first() {
            let _ = app.opener().open_url(&link.url, None::<&str>);
        }
    }
    Ok(created)
}

#[tauri::command]
#[specta::specta]
pub async fn update_profile(state: State<'_, AppState>, profile: Profile) -> CmdResult<ProfileView> {
    let env = state.env.clone();
    blocking(move || api::update_profile(&env, profile)).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_default_profile(state: State<'_, AppState>, id: Option<String>) -> CmdResult<()> {
    let env = state.env.clone();
    blocking(move || api::set_default_profile(&env, id)).await
}

#[tauri::command]
#[specta::specta]
pub fn copy_public_key<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>, id: String) -> CmdResult<String> {
    let key = api::public_key(&state.env, &id)?;
    app.clipboard().write_text(key.clone()).map_err(|e| CmdError::other(e.to_string()))?;
    Ok(key)
}

#[tauri::command]
#[specta::specta]
pub fn open_url<R: Runtime>(app: AppHandle<R>, url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err(CmdError::other("Nur https-Links werden geöffnet"));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| CmdError::other(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn test_connection(state: State<'_, AppState>, id: String) -> CmdResult<ProbeResult> {
    let env = state.env.clone();
    let pid = id.clone();
    let result = blocking(move || {
        let config = store::load(&env)?;
        Ok(probe::probe_profile(&env, config.profile(&pid)?)?)
    })
    .await?;
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
    if let Ok(mut tests) = state.last_tests.lock() {
        tests.insert(
            id,
            LastTest { success: result.success, account: result.account.clone(), message: result.message.clone(), at },
        );
    }
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub async fn last_test(state: State<'_, AppState>, id: String) -> CmdResult<Option<LastTest>> {
    Ok(state.last_test(&id))
}

#[tauri::command]
#[specta::specta]
pub async fn plan_changes(state: State<'_, AppState>) -> CmdResult<Plan> {
    let env = state.env.clone();
    blocking(move || api::plan_changes(&env)).await
}

#[tauri::command]
#[specta::specta]
pub async fn apply_changes(state: State<'_, AppState>, expected_diffs: Vec<String>) -> CmdResult<ApplyReport> {
    let env = state.env.clone();
    blocking(move || api::apply_changes(&env, &expected_diffs)).await
}

#[tauri::command]
#[specta::specta]
pub async fn which(state: State<'_, AppState>, query: String) -> CmdResult<WhichResult> {
    let env = state.env.clone();
    blocking(move || {
        let config = store::load(&env)?;
        Ok(which::which(&env, &config, &query)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn run_doctor(state: State<'_, AppState>) -> CmdResult<Report> {
    let env = state.env.clone();
    blocking(move || Ok(doctor::run(&env))).await
}

#[tauri::command]
#[specta::specta]
pub async fn fix_doctor(state: State<'_, AppState>, fix: Fix) -> CmdResult<Report> {
    let env = state.env.clone();
    blocking(move || {
        doctor::fix(&env, &fix)?;
        Ok(doctor::run(&env))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn scan_keys(state: State<'_, AppState>) -> CmdResult<Vec<ImportCandidate>> {
    let env = state.env.clone();
    blocking(move || api::scan_keys(&env)).await
}

#[tauri::command]
#[specta::specta]
pub async fn clone_preview(state: State<'_, AppState>, url: String) -> CmdResult<ClonePreview> {
    let env = state.env.clone();
    blocking(move || api::clone_preview(&env, &url)).await
}

#[tauri::command]
#[specta::specta]
pub async fn clone_repo(
    state: State<'_, AppState>,
    url: String,
    profile_id: String,
    target: Option<String>,
) -> CmdResult<String> {
    let env = state.env.clone();
    blocking(move || api::clone_repo(&env, &url, &profile_id, target)).await
}

#[tauri::command]
#[specta::specta]
pub async fn archive_profile(state: State<'_, AppState>, id: String) -> CmdResult<ArchiveResult> {
    let env = state.env.clone();
    blocking(move || api::archive_profile(&env, &id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn generated_config(state: State<'_, AppState>, id: String) -> CmdResult<Vec<GeneratedFile>> {
    let env = state.env.clone();
    blocking(move || api::generated_config(&env, &id)).await
}

#[tauri::command]
#[specta::specta]
pub fn paths(state: State<'_, AppState>) -> Paths {
    api::paths(&state.env)
}
