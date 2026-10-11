//! Everything the GUI can do, as plain functions over an [`Env`]. The Tauri
//! commands in `commands.rs` are thin wrappers around these, so the logic is
//! testable without a window.

use serde::{Deserialize, Serialize};
use specta::Type;

use keybund_core::apply::{self, ApplyReport, Plan};
use keybund_core::clone::{self, RepoUrl};
use keybund_core::gitenv::GlobalIdentity;
use keybund_core::hostkeys::{self, HostKeyCheck};
use keybund_core::import::{self, ImportProposal};
use keybund_core::keygen::{self, GenerateOptions, ImportCandidate, KeyInfo};
use keybund_core::model::{Config, HostEntry, KeyRef, Profile, Provider, Settings};
use keybund_core::uninstall::{self, UninstallOptions, UninstallPreview};
use keybund_core::{archive, msg, provider, store, Env, Error, Msg};

/// Error as the frontend sees it: a stable kind plus the message key the
/// frontend translates.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CmdError {
    pub kind: String,
    pub message: Msg,
}

impl From<Error> for CmdError {
    fn from(e: Error) -> Self {
        CmdError { kind: e.kind().to_string(), message: e.msg() }
    }
}

impl CmdError {
    /// An unexpected failure with a technical detail (a crashed thread, an OS error).
    pub fn other(detail: impl Into<String>) -> Self {
        CmdError { kind: "other".into(), message: msg!("error_other", detail = detail.into()) }
    }

    /// The key has no host or server with that name.
    pub fn no_such_host(id: &str, host: &str) -> Self {
        CmdError { kind: "no_host".into(), message: msg!("error_noSuchHost", id = id, host = host) }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

/// GUI commands run on parallel threads; changes to `profiles.toml` and the
/// configs must not interleave (load → change → save → apply). The file
/// lock also keeps the CLI out while the app writes, and vice versa.
static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub struct WriteGuard {
    _mem: std::sync::MutexGuard<'static, ()>,
    _file: store::WriteLock,
}

pub fn writing(env: &Env) -> CmdResult<WriteGuard> {
    let mem = WRITES.lock().unwrap_or_else(|e| e.into_inner());
    Ok(WriteGuard { _mem: mem, _file: store::lock(env)? })
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HostLink {
    pub host: String,
    pub provider: Provider,
    pub url: String,
}

pub use keybund_core::history::LastTest;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProfileView {
    pub profile: Profile,
    pub key: KeyInfo,
    pub is_default: bool,
    pub add_key_urls: Vec<HostLink>,
    pub last_test: Option<LastTest>,
    /// The key is loaded in ssh-agent (`None`: no agent running).
    pub in_agent: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProfilesView {
    pub default_profile: Option<String>,
    pub profiles: Vec<ProfileView>,
    /// Number of config files that differ from what Keybund would write.
    pub pending_changes: u32,
    /// Profiles that could be taken over from `~/.ssh/config`.
    pub import_available: u32,
    /// When Keybund last wrote its configs (milliseconds since the epoch).
    pub last_applied: Option<f64>,
    /// Name and e-mail from `~/.gitconfig`, used where a key sets none.
    pub global_identity: GlobalIdentity,
}

pub fn profile_view(env: &Env, config: &Config, p: &Profile, last_test: Option<LastTest>) -> ProfileView {
    ProfileView {
        profile: p.clone(),
        key: keygen::key_info(env, &p.key),
        is_default: config.default_profile.as_deref() == Some(p.id.as_str()),
        add_key_urls: p
            .hosts
            .iter()
            .filter_map(|h| {
                provider::add_key_url(h.provider, &h.hostname).map(|url| HostLink {
                    host: h.hostname.clone(),
                    provider: h.provider,
                    url,
                })
            })
            .collect(),
        last_test,
        in_agent: None,
    }
}

/// Modification time of Keybund's newest generated file.
fn last_applied(env: &Env) -> Option<f64> {
    [env.ssh_keybund_conf(), env.index_gitconfig()]
        .iter()
        .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .max()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
}

pub fn load_profiles(env: &Env) -> CmdResult<ProfilesView> {
    let tests = keybund_core::history::load(env);
    let last_test = |id: &str| tests.get(id).cloned();
    let config = store::load(env)?;
    let pending_changes = apply::plan(env, &config).map(|p| p.changes.len() as u32).unwrap_or(0);
    let import_available = import::proposals_for(env).map(|p| p.len() as u32).unwrap_or(0);
    let agent = keybund_core::which::agent_fingerprints(env);
    Ok(ProfilesView {
        default_profile: config.default_profile.clone(),
        profiles: config
            .profiles
            .iter()
            .map(|p| {
                let mut view = profile_view(env, &config, p, last_test(&p.id));
                view.in_agent = match (&agent, &view.key.fingerprint) {
                    (Some(loaded), Some(fp)) => Some(loaded.contains(fp)),
                    _ => None,
                };
                view
            })
            .collect(),
        pending_changes,
        import_available,
        last_applied: last_applied(env),
        global_identity: keybund_core::gitenv::global_identity(env),
    })
}

/// Suggestions for a new project, derived from its name.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProfileDefaults {
    pub id: String,
    pub key_path: String,
    pub folder: String,
    pub key_comment: String,
    pub key_exists: bool,
    pub id_taken: bool,
}

pub fn profile_defaults(env: &Env, name: &str, user: &str, hostname: &str) -> CmdResult<ProfileDefaults> {
    let config = store::load(env)?;
    let id = store::slugify(name);
    let key_path = config.settings.key_path(&id);
    let comment = if hostname.is_empty() { format!("{user}-{id}") } else { format!("{user}@{hostname}-{id}") };
    Ok(ProfileDefaults {
        key_exists: env.expand(&key_path).exists(),
        id_taken: config.profiles.iter().any(|p| p.id == id),
        folder: config.settings.project_folder(&id),
        key_comment: comment,
        key_path,
        id,
    })
}

/// How the key of a new project is obtained.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyChoice {
    Generate { comment: String, passphrase: Option<String> },
    Existing { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NewProject {
    pub profile: Profile,
    pub key: KeyChoice,
    pub make_default: bool,
}

fn normalized(env: &Env, mut profile: Profile) -> Profile {
    profile.id = profile.id.trim().to_string();
    profile.name = profile.name.trim().to_string();
    profile.folders =
        profile.folders.iter().filter(|f| !f.trim().is_empty()).map(|f| store::normalize_folder(env, f)).collect();
    for h in &mut profile.hosts {
        // ssh matches `Host` patterns case-sensitively against what was typed;
        // names are stored in lower case so `ssh nas` and `ssh NAS.local` agree.
        h.hostname = h.hostname.trim().to_lowercase();
        h.alias = h.alias.take().map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty());
        h.user = h.user.take().map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    }
    profile
}

fn with_new_profile(env: &Env, project: &NewProject) -> CmdResult<Config> {
    let mut config = store::load(env)?;
    if config.profiles.iter().any(|p| p.id == project.profile.id) {
        return Err(Error::DuplicateId(project.profile.id.clone()).into());
    }
    let mut profile = normalized(env, project.profile.clone());
    if let KeyChoice::Existing { path } = &project.key {
        profile.key = KeyRef { path: env.contract(&env.expand(path)), comment: None };
    }
    config.profiles.push(profile);
    // Only on request: a standard key replaces ssh's own key choice (and an own
    // core.sshCommand) for every repo outside the git folders.
    if project.make_default {
        config.default_profile = Some(project.profile.id.clone());
    }
    store::validate(&config)?;
    Ok(config)
}

/// The config changes a new project would cause, without writing anything.
pub fn preview_new_project(env: &Env, project: &NewProject) -> CmdResult<Plan> {
    let config = with_new_profile(env, project)?;
    Ok(apply::plan(env, &config)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CreatedProject {
    pub profile: ProfileView,
    pub report: ApplyReport,
    pub public_key: String,
    /// Why the provider page could not be opened, if it was requested.
    pub open_error: Option<Msg>,
}

/// Creates the key (if requested), the project folders and the profile,
/// then writes all configs.
pub fn create_project(env: &Env, project: &NewProject) -> CmdResult<CreatedProject> {
    let _write = writing(env)?;
    let mut config = with_new_profile(env, project)?;
    let id = project.profile.id.clone();
    if let KeyChoice::Generate { comment, passphrase } = &project.key {
        let path = config.profile(&id)?.key.path.clone();
        keygen::generate(env, &GenerateOptions { path, comment: comment.clone(), passphrase: passphrase.clone() })?;
        config.profile_mut(&id)?.key.comment = Some(comment.clone());
    }
    for folder in &config.profile(&id)?.folders {
        let path = env.expand(folder);
        std::fs::create_dir_all(&path).map_err(|e| CmdError::other(format!("{folder}: {e}")))?;
    }
    store::save(env, &config)?;
    let report = apply::apply_config(env, &config)?;
    let profile = config.profile(&id)?;
    Ok(CreatedProject {
        profile: profile_view(env, &config, profile, None),
        report,
        public_key: keygen::read_public(env, &profile.key)?.line(),
        open_error: None,
    })
}

/// Saves `profiles.toml` and writes the configs right away, so a change in
/// the GUI takes effect without an extra step (user files get a one-time
/// backup before the first write).
fn save_and_apply(env: &Env, config: &Config) -> CmdResult<ApplyReport> {
    store::save(env, config)?;
    Ok(apply::apply_config(env, config)?)
}

/// Saves changes to an existing key (the id cannot change) and applies them.
pub fn update_profile(env: &Env, profile: Profile) -> CmdResult<ProfileView> {
    let _write = writing(env)?;
    let mut config = store::load(env)?;
    let profile = normalized(env, profile);
    let slot = config.profile_mut(&profile.id)?;
    *slot = profile.clone();
    save_and_apply(env, &config)?;
    Ok(profile_view(env, &config, &profile, None))
}

fn with_host(env: &Env, id: &str, host: HostEntry) -> CmdResult<Config> {
    let mut config = store::load(env)?;
    let profile = config.profile_mut(id)?;
    let host = normalized(env, Profile { hosts: vec![host], ..Default::default() }).hosts.remove(0);
    profile.hosts.push(host);
    store::validate(&config)?;
    Ok(config)
}

/// The config changes from adding a server to an existing profile (e.g.
/// "this server with my private key"), without writing anything.
pub fn preview_add_host(env: &Env, id: &str, host: HostEntry) -> CmdResult<Plan> {
    Ok(apply::plan(env, &with_host(env, id, host)?)?)
}

/// Adds the server to the profile and writes the configs.
pub fn add_host(env: &Env, id: &str, host: HostEntry) -> CmdResult<ProfileView> {
    let _write = writing(env)?;
    let config = with_host(env, id, host)?;
    save_and_apply(env, &config)?;
    let profile = config.profile(id)?;
    Ok(profile_view(env, &config, profile, None))
}

fn host_of<'a>(profile: &'a Profile, host: Option<&str>) -> CmdResult<&'a HostEntry> {
    match host {
        Some(name) => profile
            .hosts
            .iter()
            .find(|h| h.target_name() == name || h.hostname == name)
            .ok_or_else(|| CmdError::no_such_host(&profile.id, name)),
        None => Ok(profile.hosts.first().ok_or_else(|| Error::NoHost(profile.id.clone()))?),
    }
}

/// Before a connection: is the host key known? `host` `None` = first host.
pub fn host_key_check(env: &Env, id: &str, host: Option<&str>) -> CmdResult<HostKeyCheck> {
    let config = store::load(env)?;
    let profile = config.profile(id)?;
    Ok(hostkeys::check(env, &config.settings, profile, host_of(profile, host)?)?)
}

/// Stores the host keys the user confirmed.
pub fn trust_host_key(env: &Env, id: &str, host: Option<&str>, fingerprints: &[String]) -> CmdResult<()> {
    let _write = writing(env)?;
    let config = store::load(env)?;
    let profile = config.profile(id)?;
    hostkeys::trust(env, &config.settings, profile, host_of(profile, host)?, fingerprints)?;
    Ok(())
}

pub fn settings(env: &Env) -> CmdResult<Settings> {
    Ok(store::load(env)?.settings)
}

/// Saves the settings and rewrites the configs they affect.
pub fn update_settings(env: &Env, settings: Settings) -> CmdResult<Settings> {
    let _write = writing(env)?;
    let mut config = store::load(env)?;
    config.settings = Settings {
        projects_dir: store::normalize_dir(env, &settings.projects_dir)?,
        keys_dir: store::normalize_dir(env, &settings.keys_dir)?,
        ..settings
    };
    save_and_apply(env, &config)?;
    Ok(config.settings)
}

pub fn set_default_profile(env: &Env, id: Option<String>) -> CmdResult<()> {
    let _write = writing(env)?;
    let mut config = store::load(env)?;
    config.default_profile = id;
    save_and_apply(env, &config)?;
    Ok(())
}

pub fn public_key(env: &Env, id: &str) -> CmdResult<String> {
    let config = store::load(env)?;
    Ok(keygen::read_public(env, &config.profile(id)?.key)?.line())
}

pub fn plan_changes(env: &Env) -> CmdResult<Plan> {
    Ok(apply::plan(env, &store::load(env)?)?)
}

/// Applies the current plan, but only if it still matches the diffs the
/// user has seen. Changes made in the GUI are applied right away, so this
/// is for files that were changed outside Keybund.
pub fn apply_changes(env: &Env, expected_diffs: &[String]) -> CmdResult<ApplyReport> {
    let _write = writing(env)?;
    let plan = plan_changes(env)?;
    let current: Vec<&str> = plan.changes.iter().map(|c| c.diff.as_str()).collect();
    if current != expected_diffs.iter().map(String::as_str).collect::<Vec<_>>() {
        return Err(Error::StalePlan("Bitte Vorschau neu laden".into()).into());
    }
    Ok(apply::apply(env, &plan)?)
}

pub fn scan_keys(env: &Env) -> CmdResult<Vec<ImportCandidate>> {
    Ok(keygen::scan(env, &store::load(env)?)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CloneTarget {
    pub profile_id: String,
    pub profile_name: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ClonePreview {
    pub repo: RepoUrl,
    /// `true` if `candidates` were matched by host; otherwise all profiles.
    pub matched: bool,
    pub candidates: Vec<CloneTarget>,
}

pub fn clone_preview(env: &Env, url: &str) -> CmdResult<ClonePreview> {
    let config = store::load(env)?;
    let repo = clone::parse_url(url)?;
    let matching = clone::matching_profiles(&config, &repo);
    let matched = !matching.is_empty();
    let profiles: Vec<&Profile> = if matched { matching } else { config.active_profiles().collect() };
    let candidates = profiles
        .into_iter()
        .map(|p| CloneTarget {
            profile_id: p.id.clone(),
            profile_name: p.name.clone(),
            target: clone::target_dir(env, p, &repo).ok().map(|t| env.contract(&t)),
        })
        .collect();
    Ok(ClonePreview { repo, matched, candidates })
}

pub fn clone_repo(env: &Env, url: &str, profile_id: &str, target: Option<String>) -> CmdResult<String> {
    let config = store::load(env)?;
    let profile = config.profile(profile_id)?;
    let target = target.filter(|t| !t.trim().is_empty()).map(|t| env.expand(t.trim()));
    let path = clone::clone(env, profile, url, target)?;
    Ok(env.contract(&path))
}

pub fn archive_profile(env: &Env, id: &str) -> CmdResult<archive::ArchiveResult> {
    let _write = writing(env)?;
    let mut config = store::load(env)?;
    let result = archive::archive(env, &mut config, id)?;
    apply::apply_config(env, &config)?;
    Ok(result)
}

pub fn import_proposals(env: &Env) -> CmdResult<Vec<ImportProposal>> {
    Ok(import::proposals_for(env)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ImportDone {
    pub ids: Vec<String>,
    /// Everything written, including the backup of `~/.ssh/config`.
    pub report: ApplyReport,
}

/// Takes over the chosen proposals in one go: saves them as keys, writes
/// the configs and (with `remove_old`) removes the old blocks from
/// `~/.ssh/config`, which gets a backup first.
pub fn import_profiles(env: &Env, chosen: &[ImportProposal], remove_old: bool) -> CmdResult<ImportDone> {
    let _write = writing(env)?;
    let ids = import::import(env, chosen)?;
    let mut report = apply::apply_config(env, &store::load(env)?)?;
    if remove_old {
        let cleanup = apply::apply(env, &import::plan_cleanup(env)?)?;
        report.written.extend(cleanup.written);
        report.backups.extend(cleanup.backups);
    }
    Ok(ImportDone { ids, report })
}

/// Removing the blocks from `~/.ssh/config` that profiles now cover.
pub fn cleanup_preview(env: &Env) -> CmdResult<Plan> {
    Ok(import::plan_cleanup(env)?)
}

pub fn cleanup_apply(env: &Env, expected_diffs: &[String]) -> CmdResult<ApplyReport> {
    let _write = writing(env)?;
    let plan = import::plan_cleanup(env)?;
    let current: Vec<&str> = plan.changes.iter().map(|c| c.diff.as_str()).collect();
    if current != expected_diffs.iter().map(String::as_str).collect::<Vec<_>>() {
        return Err(Error::StalePlan("Bitte Vorschau neu laden".into()).into());
    }
    Ok(apply::apply(env, &plan)?)
}

pub fn uninstall_preview(env: &Env, opts: UninstallOptions) -> CmdResult<UninstallPreview> {
    Ok(uninstall::preview(env, opts)?)
}

/// Uninstalls, but only if the plan still matches the previewed diffs.
pub fn uninstall_apply(env: &Env, opts: UninstallOptions, expected_diffs: &[String]) -> CmdResult<ApplyReport> {
    let _write = writing(env)?;
    let plan = uninstall::preview(env, opts)?.plan;
    let current: Vec<&str> = plan.changes.iter().map(|c| c.diff.as_str()).collect();
    if current != expected_diffs.iter().map(String::as_str).collect::<Vec<_>>() {
        return Err(Error::StalePlan("Bitte Vorschau neu laden".into()).into());
    }
    Ok(uninstall::uninstall(env, &plan)?)
}

/// A file (or the part of a file) Keybund generates for a profile.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    /// `true` if `content` is only this profile's part of a shared file.
    pub excerpt: bool,
}

pub fn generated_config(env: &Env, id: &str) -> CmdResult<Vec<GeneratedFile>> {
    let config = store::load(env)?;
    let p = config.profile(id)?;
    let mut files = vec![GeneratedFile {
        path: env.contract(&env.profile_gitconfig(id)),
        content: keybund_core::render::profile_gitconfig(p, env),
        excerpt: false,
    }];
    if !p.folders.is_empty() {
        files.push(GeneratedFile {
            path: env.contract(&env.index_gitconfig()),
            content: keybund_core::render::include_if_blocks(p, env),
            excerpt: true,
        });
    }
    let ssh = keybund_core::render::ssh_host_blocks(&config.settings, p);
    if !ssh.is_empty() {
        files.push(GeneratedFile { path: env.contract(&env.ssh_keybund_conf()), content: ssh, excerpt: true });
    }
    Ok(files)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Paths {
    pub profiles_file: String,
    pub ssh_config: String,
    pub ssh_keybund_conf: String,
    pub gitconfig: String,
    pub index_gitconfig: String,
}

pub fn paths(env: &Env) -> Paths {
    Paths {
        profiles_file: env.contract(&env.profiles_file()),
        ssh_config: env.contract(&env.ssh_config()),
        ssh_keybund_conf: env.contract(&env.ssh_keybund_conf()),
        gitconfig: env.contract(&env.gitconfig()),
        index_gitconfig: env.contract(&env.index_gitconfig()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keybund_core::model::{GitIdentity, HostEntry};

    fn env() -> (tempfile::TempDir, Env) {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path().canonicalize().unwrap());
        (dir, env)
    }

    fn project(id: &str) -> NewProject {
        NewProject {
            profile: Profile {
                id: id.into(),
                name: format!("Kunde {id}"),
                color: Some("blue".into()),
                key: KeyRef { path: keygen::default_key_path(id), comment: None },
                git: Some(GitIdentity { name: "T".into(), email: format!("t@{id}.de"), sign: false }),
                folders: vec![format!("~/dev/{id}")],
                archived: false,
                hosts: vec![HostEntry {
                    hostname: "github.com".into(),
                    provider: Provider::Github,
                    ..Default::default()
                }],
            },
            key: KeyChoice::Generate { comment: format!("t@{id}"), passphrase: None },
            make_default: false,
        }
    }

    #[test]
    fn host_keys_of_git_hosts_are_pinned() {
        let (_d, env) = env();
        create_project(&env, &project("c")).unwrap();
        assert_eq!(host_key_check(&env, "c", None).unwrap(), HostKeyCheck::Pinned);
        assert_eq!(host_key_check(&env, "c", Some("github.com")).unwrap(), HostKeyCheck::Known);
        assert!(host_key_check(&env, "c", Some("nope")).unwrap_err().message.to_string().contains("keinen Host nope"));
    }

    #[test]
    fn settings_change_the_defaults() {
        let (_d, env) = env();
        let mut s = settings(&env).unwrap();
        assert_eq!(s.projects_dir, "~/dev");
        s.projects_dir = format!("{}/work/", env.home.display());
        s.keys_dir = "~/.ssh/k/".into();
        let saved = update_settings(&env, s.clone()).unwrap();
        assert_eq!((saved.projects_dir.as_str(), saved.keys_dir.as_str()), ("~/work", "~/.ssh/k"));
        let d = profile_defaults(&env, "Kunde C", "t", "h").unwrap();
        assert_eq!((d.folder.as_str(), d.key_path.as_str()), ("~/work/kunde-c/", "~/.ssh/k/kunde-c_ed25519"));
        s.keys_dir = "relativ".into();
        assert_eq!(update_settings(&env, s).unwrap_err().kind, "invalid_path");
    }

    #[test]
    fn defaults_from_name() {
        let (_d, env) = env();
        let d = profile_defaults(&env, "Kunde C", "tristan", "zentrale").unwrap();
        assert_eq!(d.id, "kunde-c");
        assert_eq!(d.key_path, "~/.ssh/kunde-c_ed25519");
        assert_eq!(d.folder, "~/dev/kunde-c/");
        assert_eq!(d.key_comment, "tristan@zentrale-kunde-c");
        assert!(!d.key_exists && !d.id_taken);
    }

    #[test]
    fn preview_writes_nothing_and_create_writes_everything() {
        let (_d, env) = env();
        let p = project("c");
        let plan = preview_new_project(&env, &p).unwrap();
        assert!(plan.changes.iter().any(|c| c.path == "~/.config/keybund/git/c.gitconfig"));
        assert!(plan.changes.iter().any(|c| c.diff.contains("+[includeIf \"gitdir:~/dev/c/\"]")));
        assert!(!env.profiles_file().exists());
        assert!(!env.home.join("dev/c").exists());

        let created = create_project(&env, &p).unwrap();
        assert!(created.public_key.starts_with("ssh-ed25519 "));
        assert!(!created.profile.is_default, "the standard key is only set on request");
        assert_eq!(created.profile.add_key_urls[0].url, "https://github.com/settings/ssh/new");
        assert!(env.home.join("dev/c").is_dir());
        assert!(plan_changes(&env).unwrap().is_empty());

        let view = load_profiles(&env).unwrap();
        assert_eq!(view.profiles.len(), 1);
        assert_eq!(view.pending_changes, 0);
        assert!(view.profiles[0].key.fingerprint.as_deref().unwrap().starts_with("SHA256:"));

        assert_eq!(create_project(&env, &p).unwrap_err().kind, "duplicate_id");
    }

    #[test]
    fn changes_are_applied_right_away() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        let mut view = load_profiles(&env).unwrap().profiles.remove(0);
        view.profile.git.as_mut().unwrap().sign = true;
        update_profile(&env, view.profile).unwrap();
        assert!(std::fs::read_to_string(env.profile_gitconfig("a")).unwrap().contains("gpgsign = true"));
        assert!(plan_changes(&env).unwrap().is_empty());

        let mut s = settings(&env).unwrap();
        s.known_hosts_per_profile = !s.known_hosts_per_profile;
        update_settings(&env, s).unwrap();
        set_default_profile(&env, None).unwrap();
        assert!(!std::fs::read_to_string(env.index_gitconfig()).unwrap().contains("sshCommand"));
        assert_eq!(load_profiles(&env).unwrap().pending_changes, 0);
    }

    #[test]
    fn files_changed_outside_are_restored_with_stale_check() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        std::fs::write(env.profile_gitconfig("a"), "kaputt\n").unwrap();

        let plan = plan_changes(&env).unwrap();
        assert!(!plan.is_empty());
        let diffs: Vec<String> = plan.changes.iter().map(|c| c.diff.clone()).collect();
        assert_eq!(apply_changes(&env, &[]).unwrap_err().kind, "stale_plan");
        apply_changes(&env, &diffs).unwrap();
        assert!(std::fs::read_to_string(env.profile_gitconfig("a")).unwrap().contains("sshCommand"));
    }

    #[test]
    fn clone_preview_matches_by_host() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        create_project(&env, &project("b")).unwrap();
        let preview = clone_preview(&env, "git@github.com:team/portal.git").unwrap();
        assert!(preview.matched);
        assert_eq!(preview.candidates.len(), 2);
        assert_eq!(preview.candidates[0].target.as_deref(), Some("~/dev/a/portal"));
        let preview = clone_preview(&env, "git@gitlab.example.org:x/y.git").unwrap();
        assert!(!preview.matched);
        assert_eq!(clone_preview(&env, "nonsense").unwrap_err().kind, "invalid_url");
    }

    #[test]
    fn archive_then_plan_removes_configs() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        create_project(&env, &project("b")).unwrap();
        let res = archive_profile(&env, "b").unwrap();
        assert_eq!(res.key_path, "~/.ssh/archive/b_ed25519");
        assert!(!env.profile_gitconfig("b").exists());
        assert!(plan_changes(&env).unwrap().is_empty());
        set_default_profile(&env, Some("a".into())).unwrap();
        assert_eq!(load_profiles(&env).unwrap().default_profile.as_deref(), Some("a"));
    }

    #[test]
    fn generated_config_for_profile() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        let files = generated_config(&env, "a").unwrap();
        assert_eq!(files[0].path, "~/.config/keybund/git/a.gitconfig");
        assert!(files[0].content.contains("sshCommand = ssh -i ~/.ssh/a_ed25519 -o IdentitiesOnly=yes"));
        assert!(files[1].excerpt && files[1].content.contains("gitdir:~/dev/a/"));
        assert_eq!(files.len(), 2, "no ssh block without alias");
    }

    #[test]
    fn import_then_cleanup() {
        let (_d, env) = env();
        std::fs::create_dir_all(env.ssh_dir()).unwrap();
        keygen::generate(
            &env,
            &GenerateOptions { path: "~/.ssh/privat_ed25519".into(), comment: "x".into(), passphrase: None },
        )
        .unwrap();
        std::fs::write(env.ssh_config(), "# Privat\nHost github.com\n  IdentityFile ~/.ssh/privat_ed25519\n").unwrap();
        assert_eq!(load_profiles(&env).unwrap().import_available, 1);

        let proposals = import_proposals(&env).unwrap();
        assert_eq!(import_profiles(&env, &proposals, false).unwrap().ids, ["privat"]);
        let view = load_profiles(&env).unwrap();
        assert_eq!(view.import_available, 0);
        assert_eq!(view.default_profile.as_deref(), Some("privat"));
        assert_eq!(view.pending_changes, 0);

        let cleanup = cleanup_preview(&env).unwrap();
        assert_eq!(cleanup.changes.len(), 1);
        assert_eq!(cleanup_apply(&env, &[]).unwrap_err().kind, "stale_plan");
        let diffs: Vec<String> = cleanup.changes.iter().map(|c| c.diff.clone()).collect();
        cleanup_apply(&env, &diffs).unwrap();
        assert_eq!(std::fs::read_to_string(env.ssh_config()).unwrap(), "Include ~/.ssh/config.d/keybund.conf\n");
        assert!(cleanup_preview(&env).unwrap().is_empty());
    }

    #[test]
    fn import_in_one_step() {
        let (_d, env) = env();
        std::fs::create_dir_all(env.ssh_dir()).unwrap();
        keygen::generate(
            &env,
            &GenerateOptions { path: "~/.ssh/kunde_ed25519".into(), comment: "x".into(), passphrase: None },
        )
        .unwrap();
        std::fs::write(
            env.ssh_config(),
            "Host kunde\n  HostName gitlab.kunde.de\n  IdentityFile ~/.ssh/kunde_ed25519\n",
        )
        .unwrap();
        let done = import_profiles(&env, &import_proposals(&env).unwrap(), true).unwrap();
        assert_eq!(done.ids, ["kunde"]);
        assert!(done.report.written.contains(&"~/.ssh/config".to_string()));
        assert_eq!(done.report.backups.len(), 1, "{:?}", done.report);
        assert!(std::fs::read_to_string(env.ssh_keybund_conf()).unwrap().contains("Host kunde"));
        assert_eq!(std::fs::read_to_string(env.ssh_config()).unwrap(), "Include ~/.ssh/config.d/keybund.conf\n");
        let view = load_profiles(&env).unwrap();
        assert_eq!((view.pending_changes, view.import_available), (0, 0));
        assert!(cleanup_preview(&env).unwrap().is_empty());
    }

    #[test]
    fn add_a_server_to_an_existing_profile() {
        let (_d, env) = env();
        create_project(&env, &project("privat")).unwrap();
        let nas = HostEntry {
            alias: Some(" nas ".into()),
            hostname: "192.168.1.5".into(),
            user: Some("".into()),
            ..Default::default()
        };
        let plan = preview_add_host(&env, "privat", nas.clone()).unwrap();
        assert!(plan.changes.iter().any(|c| c.diff.contains("+Host nas 192.168.1.5")));
        let view = add_host(&env, "privat", nas.clone()).unwrap();
        assert_eq!(view.profile.hosts[1].alias.as_deref(), Some("nas"));
        assert_eq!(view.profile.hosts[1].user, None);
        assert!(std::fs::read_to_string(env.ssh_keybund_conf()).unwrap().contains("Host nas 192.168.1.5"));
        assert!(plan_changes(&env).unwrap().is_empty());
        // The same server cannot go to a second profile.
        create_project(&env, &project("other")).unwrap();
        assert_eq!(add_host(&env, "other", nas).unwrap_err().kind, "duplicate_alias");
    }

    #[test]
    fn uninstall_with_preview() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        let opts = UninstallOptions::default();
        let preview = uninstall_preview(&env, opts).unwrap();
        assert!(preview.plan.changes.iter().any(|c| c.path == "~/.ssh/config.d/keybund.conf"));
        assert_eq!(uninstall_apply(&env, opts, &[]).unwrap_err().kind, "stale_plan");
        let diffs: Vec<String> = preview.plan.changes.iter().map(|c| c.diff.clone()).collect();
        uninstall_apply(&env, opts, &diffs).unwrap();
        assert!(!env.ssh_keybund_conf().exists());
        assert_eq!(load_profiles(&env).unwrap().profiles.len(), 1);
    }

    #[test]
    fn existing_key_is_used_as_is() {
        let (_d, env) = env();
        std::fs::create_dir_all(env.ssh_dir()).unwrap();
        keygen::generate(
            &env,
            &GenerateOptions { path: "~/.ssh/id_ed25519".into(), comment: "x".into(), passphrase: None },
        )
        .unwrap();
        let mut p = project("srv");
        p.key = KeyChoice::Existing { path: "~/.ssh/id_ed25519".into() };
        let created = create_project(&env, &p).unwrap();
        assert_eq!(created.profile.profile.key.path, "~/.ssh/id_ed25519");
        assert!(!env.home.join(".ssh/srv_ed25519").exists());
        assert_eq!(scan_keys(&env).unwrap()[0].used_by.as_deref(), Some("srv"));
    }
}
