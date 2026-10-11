//! Tauri commands. Each one is a thin wrapper: it takes the environment
//! from the app state, runs the blocking work off the main thread and
//! delegates to `api` or the core.

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use keybund_core::apply::{ApplyReport, Plan};
use keybund_core::archive::ArchiveResult;
use keybund_core::doctor::{self, Fix, Report};
use keybund_core::keygen::ImportCandidate;
use keybund_core::model::Profile;
use keybund_core::probe::{self, ProbeResult};
use keybund_core::which::{self, WhichResult};
use keybund_core::{history, store, Env};

use crate::api::{self, *};

pub struct AppState {
    pub env: Env,
}

impl AppState {
    pub fn new(env: Env) -> Self {
        AppState { env }
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn load_profiles(state: State<'_, AppState>) -> CmdResult<ProfilesView> {
    let env = state.env.clone();
    blocking(move || api::load_profiles(&env)).await
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
    let mut created = blocking(move || api::create_project(&env, &project)).await?;
    let _ = app.clipboard().write_text(created.public_key.clone());
    if open_provider_page {
        if let Some(link) = created.profile.add_key_urls.first().map(|l| l.url.clone()) {
            created.open_error =
                blocking(move || Ok(keybund_core::desktop::open_url(&link).err().map(|e| e.msg()))).await?;
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
pub async fn preview_add_host(
    state: State<'_, AppState>,
    id: String,
    host: keybund_core::model::HostEntry,
) -> CmdResult<Plan> {
    let env = state.env.clone();
    blocking(move || api::preview_add_host(&env, &id, host)).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_host(
    state: State<'_, AppState>,
    id: String,
    host: keybund_core::model::HostEntry,
) -> CmdResult<ProfileView> {
    let env = state.env.clone();
    blocking(move || api::add_host(&env, &id, host)).await
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
pub async fn settings(state: State<'_, AppState>) -> CmdResult<keybund_core::model::Settings> {
    let env = state.env.clone();
    blocking(move || api::settings(&env)).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: keybund_core::model::Settings,
) -> CmdResult<keybund_core::model::Settings> {
    let env = state.env.clone();
    blocking(move || api::update_settings(&env, settings)).await
}

/// Opens an https link in the browser, also from a toolbox or an AppImage.
#[tauri::command]
#[specta::specta]
pub async fn open_url(url: String) -> CmdResult<()> {
    blocking(move || Ok(keybund_core::desktop::open_url(&url)?)).await
}

/// Checks the host key before connecting (pinned keys are added).
#[tauri::command]
#[specta::specta]
pub async fn host_key_check(
    state: State<'_, AppState>,
    id: String,
    host: Option<String>,
) -> CmdResult<keybund_core::hostkeys::HostKeyCheck> {
    let env = state.env.clone();
    blocking(move || api::host_key_check(&env, &id, host.as_deref())).await
}

/// Stores the host keys whose fingerprints the user confirmed.
#[tauri::command]
#[specta::specta]
pub async fn trust_host_key(
    state: State<'_, AppState>,
    id: String,
    host: Option<String>,
    fingerprints: Vec<String>,
) -> CmdResult<()> {
    let env = state.env.clone();
    blocking(move || api::trust_host_key(&env, &id, host.as_deref(), &fingerprints)).await
}

/// Tests the connection to `host` (alias or hostname) or the profile's first host.
#[tauri::command]
#[specta::specta]
pub async fn test_connection(state: State<'_, AppState>, id: String, host: Option<String>) -> CmdResult<ProbeResult> {
    let env = state.env.clone();
    blocking(move || {
        let config = store::load(&env)?;
        let profile = config.profile(&id)?;
        match host {
            Some(name) => {
                let h = profile
                    .hosts
                    .iter()
                    .find(|h| h.target_name() == name || h.hostname == name)
                    .ok_or_else(|| CmdError::no_such_host(&id, &name))?;
                let result = probe::probe_detached(&env, profile, h)?;
                // Remembered per host (its row) and summed up per key (sidebar, start page).
                history::record(&env, &id, h.target_name(), &result)?;
                Ok(result)
            }
            None => {
                let h = profile.hosts.first().ok_or_else(|| keybund_core::Error::NoHost(profile.id.clone()))?;
                let result = probe::probe_detached(&env, profile, h)?;
                history::record(&env, &id, h.target_name(), &result)?;
                Ok(result)
            }
        }
    })
    .await
}

/// Puts the profile's key on a server (`host` = alias or hostname).
#[tauri::command]
#[specta::specta]
pub async fn install_key(
    state: State<'_, AppState>,
    id: String,
    host: String,
) -> CmdResult<keybund_core::keyinstall::InstallResult> {
    let env = state.env.clone();
    blocking(move || {
        let config = store::load(&env)?;
        let profile = config.profile(&id)?;
        let server = profile
            .hosts
            .iter()
            .find(|h| h.target_name() == host || h.hostname == host)
            .ok_or_else(|| CmdError::no_such_host(&id, &host))?;
        Ok(keybund_core::keyinstall::install_detached(&env, profile, server)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn last_test(state: State<'_, AppState>, id: String) -> CmdResult<Option<LastTest>> {
    let config = store::load(&state.env)?;
    Ok(history::get(&state.env, config.profile(&id)?))
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
        {
            let _write = api::writing(&env)?;
            doctor::fix(&env, &fix)?;
        }
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
pub async fn import_proposals(state: State<'_, AppState>) -> CmdResult<Vec<keybund_core::import::ImportProposal>> {
    let env = state.env.clone();
    blocking(move || api::import_proposals(&env)).await
}

#[tauri::command]
#[specta::specta]
pub async fn import_profiles(
    state: State<'_, AppState>,
    chosen: Vec<keybund_core::import::ImportProposal>,
    remove_old: bool,
) -> CmdResult<api::ImportDone> {
    let env = state.env.clone();
    blocking(move || api::import_profiles(&env, &chosen, remove_old)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cleanup_preview(state: State<'_, AppState>) -> CmdResult<Plan> {
    let env = state.env.clone();
    blocking(move || api::cleanup_preview(&env)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cleanup_apply(state: State<'_, AppState>, expected_diffs: Vec<String>) -> CmdResult<ApplyReport> {
    let env = state.env.clone();
    blocking(move || api::cleanup_apply(&env, &expected_diffs)).await
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_preview(
    state: State<'_, AppState>,
    opts: keybund_core::uninstall::UninstallOptions,
) -> CmdResult<keybund_core::uninstall::UninstallPreview> {
    let env = state.env.clone();
    blocking(move || api::uninstall_preview(&env, opts)).await
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_apply(
    state: State<'_, AppState>,
    opts: keybund_core::uninstall::UninstallOptions,
    expected_diffs: Vec<String>,
) -> CmdResult<ApplyReport> {
    let env = state.env.clone();
    blocking(move || api::uninstall_apply(&env, opts, &expected_diffs)).await
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
