//! Everything the GUI can do, as plain functions over an [`Env`]. The Tauri
//! commands in `commands.rs` are thin wrappers around these, so the logic is
//! testable without a window.

use serde::{Deserialize, Serialize};
use specta::Type;

use keybund_core::apply::{self, ApplyReport, Plan};
use keybund_core::clone::{self, RepoUrl};
use keybund_core::keygen::{self, GenerateOptions, ImportCandidate, KeyInfo};
use keybund_core::model::{Config, KeyRef, Profile, Provider};
use keybund_core::{archive, provider, store, Env, Error};

/// Error as the frontend sees it: a stable kind plus a readable message.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CmdError {
    pub kind: String,
    pub message: String,
}

impl From<Error> for CmdError {
    fn from(e: Error) -> Self {
        CmdError { kind: e.kind().to_string(), message: e.to_string() }
    }
}

impl CmdError {
    pub fn other(message: impl Into<String>) -> Self {
        CmdError { kind: "other".into(), message: message.into() }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HostLink {
    pub host: String,
    pub provider: Provider,
    pub url: String,
}

/// Result of the last connection test, kept in memory for the session.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LastTest {
    pub success: bool,
    pub account: Option<String>,
    pub message: String,
    /// Milliseconds since the epoch.
    pub at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProfileView {
    pub profile: Profile,
    pub key: KeyInfo,
    pub is_default: bool,
    pub add_key_urls: Vec<HostLink>,
    pub last_test: Option<LastTest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProfilesView {
    pub default_profile: Option<String>,
    pub profiles: Vec<ProfileView>,
    /// Number of config files that differ from what Keybund would write.
    pub pending_changes: u32,
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
    }
}

pub fn load_profiles(env: &Env, last_test: impl Fn(&str) -> Option<LastTest>) -> CmdResult<ProfilesView> {
    let config = store::load(env)?;
    let pending_changes = apply::plan(env, &config).map(|p| p.changes.len() as u32).unwrap_or(0);
    Ok(ProfilesView {
        default_profile: config.default_profile.clone(),
        profiles: config.profiles.iter().map(|p| profile_view(env, &config, p, last_test(&p.id))).collect(),
        pending_changes,
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
    let key_path = keygen::default_key_path(&id);
    let comment = if hostname.is_empty() { format!("{user}-{id}") } else { format!("{user}@{hostname}-{id}") };
    Ok(ProfileDefaults {
        key_exists: env.expand(&key_path).exists(),
        id_taken: config.profiles.iter().any(|p| p.id == id),
        folder: format!("~/dev/{id}/"),
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
        h.hostname = h.hostname.trim().to_string();
        h.alias = h.alias.take().map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
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
    if project.make_default || config.default_profile.is_none() {
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
}

/// Creates the key (if requested), the project folders and the profile,
/// then writes all configs.
pub fn create_project(env: &Env, project: &NewProject) -> CmdResult<CreatedProject> {
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
    })
}

/// Saves changes to an existing profile (the id cannot change).
pub fn update_profile(env: &Env, profile: Profile) -> CmdResult<ProfileView> {
    let mut config = store::load(env)?;
    let profile = normalized(env, profile);
    let slot = config.profile_mut(&profile.id)?;
    *slot = profile.clone();
    store::save(env, &config)?;
    Ok(profile_view(env, &config, &profile, None))
}

pub fn set_default_profile(env: &Env, id: Option<String>) -> CmdResult<()> {
    let mut config = store::load(env)?;
    config.default_profile = id;
    store::save(env, &config)?;
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
/// user has seen.
pub fn apply_changes(env: &Env, expected_diffs: &[String]) -> CmdResult<ApplyReport> {
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
    let mut config = store::load(env)?;
    Ok(archive::archive(env, &mut config, id)?)
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
        assert!(created.profile.is_default, "first profile becomes default");
        assert_eq!(created.profile.add_key_urls[0].url, "https://github.com/settings/ssh/new");
        assert!(env.home.join("dev/c").is_dir());
        assert!(plan_changes(&env).unwrap().is_empty());

        let view = load_profiles(&env, |_| None).unwrap();
        assert_eq!(view.profiles.len(), 1);
        assert_eq!(view.pending_changes, 0);
        assert!(view.profiles[0].key.fingerprint.as_deref().unwrap().starts_with("SHA256:"));

        assert_eq!(create_project(&env, &p).unwrap_err().kind, "duplicate_id");
    }

    #[test]
    fn update_and_apply_with_stale_check() {
        let (_d, env) = env();
        create_project(&env, &project("a")).unwrap();
        let mut view = load_profiles(&env, |_| None).unwrap().profiles.remove(0);
        view.profile.git.as_mut().unwrap().sign = true;
        update_profile(&env, view.profile).unwrap();

        let plan = plan_changes(&env).unwrap();
        assert!(!plan.is_empty());
        let diffs: Vec<String> = plan.changes.iter().map(|c| c.diff.clone()).collect();
        assert_eq!(apply_changes(&env, &[]).unwrap_err().kind, "stale_plan");
        apply_changes(&env, &diffs).unwrap();
        assert!(std::fs::read_to_string(env.profile_gitconfig("a")).unwrap().contains("gpgsign = true"));
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
        assert!(plan_changes(&env).unwrap().changes.iter().any(|c| c.path.ends_with("b.gitconfig")));
        set_default_profile(&env, Some("a".into())).unwrap();
        assert_eq!(load_profiles(&env, |_| None).unwrap().default_profile.as_deref(), Some("a"));
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
