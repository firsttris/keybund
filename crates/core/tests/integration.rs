//! Runs apply against a temporary HOME and checks the result with the real
//! `git` and `ssh` binaries, not against our own assumptions.

use std::fs;
use std::path::Path;
use std::process::Command;

use keybund_core::keygen::{self, GenerateOptions};
use keybund_core::model::{Config, GitIdentity, HostEntry, KeyRef, Profile, Provider};
use keybund_core::{apply, archive, doctor, store, which, Env};

/// Missing tools skip a test locally, but never in CI.
fn has(tool: &str) -> bool {
    let ok = Command::new(tool).arg("-V").output().is_ok() || Command::new(tool).arg("--version").output().is_ok();
    assert!(ok || std::env::var_os("CI").is_none(), "{tool} is required in CI");
    ok
}

fn git(env: &Env, dir: &Path, args: &[&str]) -> std::process::Output {
    env.command("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("GIT_CONFIG_GLOBAL")
        .output()
        .unwrap()
}

/// Temp HOME with two customers on github.com plus a server alias.
fn setup() -> (tempfile::TempDir, Env, Config) {
    let dir = tempfile::tempdir().unwrap();
    let env = Env::with_home(dir.path().canonicalize().unwrap());
    fs::create_dir_all(env.ssh_dir()).unwrap();
    fs::write(env.ssh_config(), "Host github.com\n    User git\n").unwrap();
    fs::write(env.gitconfig(), "[user]\n\tname = Global\n\temail = global@example.org\n").unwrap();

    let mut profiles = Vec::new();
    for (id, sign) in [("privat", true), ("kunde-a", false), ("kunde-b", false)] {
        let path = keygen::default_key_path(id);
        keygen::generate(&env, &GenerateOptions { path: path.clone(), comment: format!("t@{id}"), passphrase: None })
            .unwrap();
        fs::create_dir_all(env.home.join("dev").join(id)).unwrap();
        profiles.push(Profile {
            id: id.into(),
            name: id.into(),
            color: None,
            key: KeyRef { path, comment: None },
            git: Some(GitIdentity { name: format!("Name {id}"), email: format!("{id}@example.org"), sign }),
            folders: vec![store::normalize_folder(&env, &format!("~/dev/{id}"))],
            archived: false,
            hosts: vec![HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() }],
        });
    }
    profiles[0].hosts = vec![HostEntry {
        alias: Some("meinserver".into()),
        hostname: "1.2.3.4".into(),
        user: Some("tristan".into()),
        port: Some(2222),
        provider: Provider::Generic,
    }];
    let config = Config { default_profile: Some("privat".into()), profiles, ..Default::default() };
    store::save(&env, &config).unwrap();
    (dir, env, config)
}

#[test]
fn generated_key_matches_ssh_keygen_fingerprint() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    let key = &config.profiles[0].key;
    let info = keygen::key_info(&env, key);
    let out = Command::new("ssh-keygen").arg("-lf").arg(env.expand(&key.pub_path())).output().unwrap();
    let expected = String::from_utf8_lossy(&out.stdout).split_whitespace().nth(1).unwrap().to_string();
    assert_eq!(info.fingerprint.as_deref(), Some(expected.as_str()));
    assert_eq!(info.key_type.as_deref(), Some("ssh-ed25519"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(env.expand(&key.path)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let found = keygen::scan(&env, &config).unwrap();
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|c| c.used_by.is_some()));
}

#[test]
fn apply_is_idempotent_and_keeps_user_files() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    let plan = apply::plan(&env, &config).unwrap();
    assert!(plan.changes.iter().any(|c| c.path == "~/.ssh/config" && c.user_file));
    let report = apply::apply(&env, &plan).unwrap();
    assert_eq!(report.backups.len(), 2);

    let ssh = fs::read_to_string(env.ssh_config()).unwrap();
    assert!(ssh.starts_with("Include ~/.ssh/config.d/keybund.conf\n"));
    assert!(ssh.contains("Host github.com\n    User git\n"));
    let gitconfig = fs::read_to_string(env.gitconfig()).unwrap();
    assert!(gitconfig.starts_with("[user]\n\tname = Global\n"));
    assert!(gitconfig.contains("path = ~/.config/keybund/git/index.gitconfig"));
    assert!(env.home.join(".ssh/config.keybund-backup").exists());
    assert!(env.profile_gitconfig("kunde-a").exists());
    assert!(fs::read_to_string(env.allowed_signers())
        .unwrap()
        .contains("privat@example.org namespaces=\"git\" ssh-ed25519 "));

    // Second run changes nothing.
    assert!(apply::plan(&env, &config).unwrap().is_empty());

    // A changed file between plan and apply is detected.
    let mut changed = config.clone();
    changed.profiles[1].git.as_mut().unwrap().email = "neu@example.org".into();
    let plan = apply::plan(&env, &changed).unwrap();
    fs::write(env.profile_gitconfig("kunde-a"), "manual edit").unwrap();
    assert!(apply::apply(&env, &plan).is_err());
}

#[test]
fn git_resolves_profile_per_folder() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();

    for (id, key) in [("kunde-a", "~/.ssh/kunde-a_ed25519"), ("kunde-b", "~/.ssh/kunde-b_ed25519")] {
        let repo = env.home.join("dev").join(id).join("repo");
        fs::create_dir_all(&repo).unwrap();
        assert!(git(&env, &repo, &["init", "-q"]).status.success());
        let out = git(&env, &repo, &["config", "--get", "core.sshCommand"]);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("ssh -i {key} -o IdentitiesOnly=yes -o AddKeysToAgent=yes")
        );
        let out = git(&env, &repo, &["config", "--get", "user.email"]);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("{id}@example.org"));
    }

    // Outside of all profile folders the default profile's key applies,
    // while the global identity stays untouched.
    let other = env.home.join("elsewhere");
    fs::create_dir_all(&other).unwrap();
    assert!(git(&env, &other, &["init", "-q"]).status.success());
    let out = git(&env, &other, &["config", "--get", "core.sshCommand"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "ssh -i ~/.ssh/privat_ed25519 -o IdentitiesOnly=yes -o AddKeysToAgent=yes"
    );
    let out = git(&env, &other, &["config", "--get", "user.email"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "global@example.org");

    // Signing settings for the signing profile.
    let repo = env.home.join("dev/privat/repo");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    let out = git(&env, &repo, &["config", "--get", "gpg.format"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ssh");
}

#[test]
fn ssh_resolves_server_alias() {
    if !has("ssh") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    // ssh expands `~` via the password database, not $HOME, so the
    // generated file is checked directly with -F.
    let out = Command::new("ssh").arg("-G").arg("-F").arg(env.ssh_keybund_conf()).arg("meinserver").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|l| l == "hostname 1.2.3.4"), "{text}");
    assert!(text.lines().any(|l| l == "port 2222"));
    assert!(text.lines().any(|l| l == "identitiesonly yes"));
    assert!(text.lines().any(|l| l.starts_with("identityfile ") && l.ends_with("/.ssh/privat_ed25519")));

    // The same server by its IP uses the same key and settings.
    let out = Command::new("ssh").arg("-G").arg("-F").arg(env.ssh_keybund_conf()).arg("1.2.3.4").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|l| l == "port 2222"), "{text}");
    assert!(text.lines().any(|l| l.starts_with("identityfile ") && l.ends_with("/.ssh/privat_ed25519")));
}

#[test]
fn which_and_doctor() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();

    // Plain folder (no repo): resolved via the profile folder.
    let r = which::which(&env, &config, "~/dev/kunde-a").unwrap();
    assert_eq!(r.profile_id.as_deref(), Some("kunde-a"));
    assert_eq!(r.key_path.as_deref(), Some("~/.ssh/kunde-a_ed25519"));

    // Inside a repo: asks git.
    let repo = env.home.join("dev/kunde-b/x");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    let r = which::which(&env, &config, &repo.display().to_string()).unwrap();
    assert_eq!(r.profile_id.as_deref(), Some("kunde-b"));
    assert_eq!(r.source.as_deref(), Some("~/.config/keybund/git/kunde-b.gitconfig"));
    assert_eq!(r.git_email.as_deref(), Some("kunde-b@example.org"));

    let report = doctor::run(&env);
    let get = |id: &str| report.checks.iter().find(|c| c.id == id).unwrap().status;
    assert_eq!(get("ssh-include"), doctor::Status::Ok);
    assert_eq!(get("git-include"), doctor::Status::Ok);
    assert_eq!(get("configs"), doctor::Status::Ok);
    assert_eq!(get("key-perms"), doctor::Status::Ok);

    // Misplaced include is detected and fixed.
    let ssh = fs::read_to_string(env.ssh_config()).unwrap();
    let moved =
        ssh.replacen("Include ~/.ssh/config.d/keybund.conf\n", "", 1) + "Include ~/.ssh/config.d/keybund.conf\n";
    fs::write(env.ssh_config(), moved).unwrap();
    let report = doctor::run(&env);
    let check = report.checks.iter().find(|c| c.id == "ssh-include").unwrap();
    assert_eq!(check.status, doctor::Status::Error);
    doctor::fix(&env, check.fix.as_ref().unwrap()).unwrap();
    assert!(apply::ssh_include_on_top(&fs::read_to_string(env.ssh_config()).unwrap()));
}

#[test]
fn archive_moves_key_and_drops_configs() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, mut config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let result = archive::archive(&env, &mut config, "kunde-a").unwrap();
    assert_eq!(result.key_path, "~/.ssh/archive/kunde-a_ed25519");
    assert_eq!(result.remove_key_urls, vec!["https://github.com/settings/keys".to_string()]);
    assert!(env.home.join(".ssh/archive/kunde-a_ed25519.pub").exists());
    assert!(!env.home.join(".ssh/kunde-a_ed25519").exists());
    assert!(store::load(&env).unwrap().profile("kunde-a").unwrap().archived);

    let plan = apply::plan(&env, &config).unwrap();
    assert!(plan.changes.iter().any(|c| c.path.ends_with("kunde-a.gitconfig") && c.kind == apply::ChangeKind::Delete));
    apply::apply(&env, &plan).unwrap();
    assert!(!env.profile_gitconfig("kunde-a").exists());
}

#[test]
fn which_outside_profiles_and_with_local_override() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, mut config) = setup();
    apply::apply_config(&env, &config).unwrap();

    // A folder outside of all profiles falls back to the default profile.
    let r = which::which(&env, &config, &env.home.join("tmp").display().to_string()).unwrap();
    assert_eq!(r.profile_id.as_deref(), Some("privat"));
    assert_eq!(r.via.as_ref().map(|v| v.to_string()).as_deref(), Some("Standard-Key"));

    // A repo with its own core.sshCommand is reported as a mismatch.
    let repo = env.home.join("dev/kunde-a/special");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    git(&env, &repo, &["config", "core.sshCommand", "ssh -i ~/.ssh/kunde-b_ed25519"]);
    let r = which::which(&env, &config, &repo.display().to_string()).unwrap();
    assert_eq!(r.key_path.as_deref(), Some("~/.ssh/kunde-b_ed25519"));
    assert_eq!(r.identities_only, Some(false));
    assert!(r.notes.iter().any(|n| n.to_string().contains("anderen Key")), "{:?}", r.notes);

    // Without a default profile, a repo outside of all folders has no key.
    config.default_profile = None;
    store::save(&env, &config).unwrap();
    apply::apply_config(&env, &config).unwrap();
    let elsewhere = env.home.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    git(&env, &elsewhere, &["init", "-q"]);
    let r = which::which(&env, &config, &elsewhere.display().to_string()).unwrap();
    assert_eq!(r.key_path, None);
    assert!(r.notes.iter().any(|n| n.to_string().contains("kein core.sshCommand")));
    let r = which::which(&env, &config, &env.home.join("plain").display().to_string()).unwrap();
    assert!(r.notes.iter().any(|n| n.to_string().contains("keinen Standard-Key")));
}

#[test]
fn which_host_lists_profiles_sharing_it() {
    if !has("ssh") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    let r = which::which(&env, &config, "github.com").unwrap();
    assert_eq!(r.mode, which::WhichMode::Host);
    assert_eq!(r.hostname.as_deref(), Some("github.com"));
    assert_eq!(r.source.as_deref(), Some("ssh -G"));
    assert!(r.notes.iter().any(|n| n.to_string().contains("kunde-a, kunde-b")), "{:?}", r.notes);
}

#[test]
fn doctor_reports_missing_keys_bad_permissions_and_old_folders() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    fs::remove_file(env.expand("~/.ssh/kunde-b_ed25519.pub")).unwrap();
    fs::remove_dir_all(env.home.join("dev/kunde-a")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(env.ssh_dir(), fs::Permissions::from_mode(0o775)).unwrap();
    }
    fs::write(
        env.ssh_config(),
        fs::read_to_string(env.ssh_config()).unwrap() + "Host github.com\n  IdentityFile ~/.ssh/id_rsa\n",
    )
    .unwrap();

    let report = doctor::run(&env);
    let by_id = |id: &str| report.checks.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no check {id}"));
    assert_eq!(by_id("keys-missing").status, doctor::Status::Error);
    assert!(by_id("keys-missing").detail.as_ref().unwrap().to_string().contains("kunde-b_ed25519.pub"));
    assert_eq!(by_id("folders").status, doctor::Status::Warn);
    assert_eq!(by_id("ssh-config-github.com").status, doctor::Status::Warn);
    #[cfg(unix)]
    {
        assert_eq!(by_id("ssh-dir-perms").status, doctor::Status::Error);
        doctor::fix(&env, &doctor::Fix::Permissions).unwrap();
        let report = doctor::run(&env);
        assert_eq!(report.checks.iter().find(|c| c.id == "ssh-dir-perms").unwrap().status, doctor::Status::Ok);
    }
    assert!(report.problems() > 0);
    assert!(report.has_errors());

    // A broken profiles.toml is reported, not a crash.
    fs::write(env.profiles_file(), "this is = = not toml").unwrap();
    let report = doctor::run(&env);
    assert_eq!(report.checks[0].status, doctor::Status::Error);
}

#[test]
fn configs_from_older_versions_get_add_keys_to_agent() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let ok = doctor::run(&env);
    assert_eq!(ok.checks.iter().find(|c| c.id == "add-keys").unwrap().status, doctor::Status::Ok);

    // Simulate files written by a version without AddKeysToAgent.
    for id in ["kunde-a", "privat"] {
        let path = env.profile_gitconfig(id);
        let old = fs::read_to_string(&path).unwrap().replace(" -o AddKeysToAgent=yes", "");
        fs::write(&path, old).unwrap();
    }
    let conf = fs::read_to_string(env.ssh_keybund_conf()).unwrap().replace("    AddKeysToAgent yes\n", "");
    fs::write(env.ssh_keybund_conf(), conf).unwrap();

    let report = doctor::run(&env);
    let check = report.checks.iter().find(|c| c.id == "add-keys").unwrap();
    assert_eq!(check.status, doctor::Status::Warn);
    let detail = check.detail.as_ref().unwrap().to_string();
    assert!(detail.contains("kunde-a") && detail.contains("privat") && detail.contains("keybund.conf"), "{detail}");
    assert!(!detail.contains("kunde-b"));

    doctor::fix(&env, check.fix.as_ref().unwrap()).unwrap();
    let report = doctor::run(&env);
    assert_eq!(report.checks.iter().find(|c| c.id == "add-keys").unwrap().status, doctor::Status::Ok);

    // And git really passes the option to ssh.
    let repo = env.home.join("dev/kunde-a/repo");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    let out = git(&env, &repo, &["config", "--get", "core.sshCommand"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("-o AddKeysToAgent=yes"));
}

/// The situation from the issue report: a hand-written ~/.ssh/config with a
/// private account for github.com, a second GitHub account behind an alias
/// and a company GitLab, plus a new Keybund profile on github.com.
#[test]
fn import_existing_ssh_config_and_clean_up() {
    use keybund_core::import;
    if !has("ssh-keygen") || !has("git") {
        return;
    }
    let (_dir, env, _) = setup();
    // Start without Keybund profiles, only the hand-made setup.
    fs::remove_file(env.profiles_file()).unwrap();
    for key in ["~/.ssh/privat2_ed25519", "~/.ssh/nebenjob/id_ed25519", "~/.ssh/nordlicht-media_ed25519"] {
        keygen::generate(&env, &GenerateOptions { path: key.into(), comment: "x".into(), passphrase: None }).unwrap();
    }
    let user_config = "# Privater Account\nHost github.com\n    HostName github.com\n    User git\n    IdentityFile ~/.ssh/privat2_ed25519\n    IdentitiesOnly yes\n\n# Nebenjob Account\nHost nebenjob\n    HostName github.com\n    User git\n    IdentityFile ~/.ssh/nebenjob/id_ed25519\n    IdentitiesOnly yes\n\n# Nordlicht-Media\nHost gitlab.nordlicht.de\n    HostName gitlab.nordlicht.de\n    User git\n    IdentityFile ~/.ssh/nordlicht-media_ed25519\n    IdentitiesOnly yes\n\nHost *\n    ServerAliveInterval 30\n";
    fs::write(env.ssh_config(), user_config).unwrap();

    // A Keybund profile on github.com next to the old global entry: doctor
    // warns that the old key is offered as well.
    let test = Profile {
        id: "test".into(),
        name: "Test".into(),
        key: KeyRef { path: "~/.ssh/kunde-a_ed25519".into(), comment: None },
        folders: vec![store::normalize_folder(&env, "~/dev/kunde-a")],
        hosts: vec![HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() }],
        ..Default::default()
    };
    store::save(&env, &Config { profiles: vec![test], ..Default::default() }).unwrap();
    apply::apply_config(&env, &store::load(&env).unwrap()).unwrap();
    let report = doctor::run(&env);
    let check = report.checks.iter().find(|c| c.id == "ssh-config-github.com").unwrap();
    assert_eq!(check.title.to_string(), "Globaler Eintrag für github.com in ~/.ssh/config");
    assert!(check.fix.is_none(), "not imported yet, nothing to clean up");

    // Import all three.
    let proposals = import::proposals_for(&env).unwrap();
    assert_eq!(
        proposals.iter().map(|p| p.profile.id.as_str()).collect::<Vec<_>>(),
        ["privat2", "nebenjob", "nordlicht-media"]
    );
    assert!(proposals.iter().all(|p| p.key_exists));
    import::import(&env, &proposals).unwrap();
    apply::apply_config(&env, &store::load(&env).unwrap()).unwrap();
    assert!(import::proposals_for(&env).unwrap().is_empty(), "nothing left to import");
    let config = store::load(&env).unwrap();
    assert_eq!(config.default_profile.as_deref(), Some("privat2"));

    // Keybund now writes the alias blocks itself.
    let conf = fs::read_to_string(env.ssh_keybund_conf()).unwrap();
    assert!(conf.contains("Host nebenjob\n    HostName github.com\n"));
    assert!(conf.contains("Host gitlab.nordlicht.de\n"));
    assert!(!conf.contains("Host github.com\n"), "github.com stays a shared host without Host block");

    // Doctor offers to remove the old entries; the fix removes exactly them.
    let report = doctor::run(&env);
    let check = report.checks.iter().find(|c| c.id == "ssh-config-github.com").unwrap();
    assert_eq!(check.fix, Some(doctor::Fix::CleanupSshConfig));
    assert!(report.checks.iter().any(|c| c.id == "ssh-config-nebenjob"));
    let plan = import::plan_cleanup(&env).unwrap();
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.changes[0].user_file);
    doctor::fix(&env, check.fix.as_ref().unwrap()).unwrap();

    let cleaned = fs::read_to_string(env.ssh_config()).unwrap();
    assert_eq!(cleaned, "Include ~/.ssh/config.d/keybund.conf\n\nHost *\n    ServerAliveInterval 30\n");
    let backup = fs::read_to_string(env.home.join(".ssh/config.keybund-backup")).unwrap();
    assert!(backup.contains("privat2_ed25519"), "the original is kept as backup");
    let report = doctor::run(&env);
    assert!(report.checks.iter().all(|c| !c.id.starts_with("ssh-config-")), "{:?}", report.checks);
    assert!(import::plan_cleanup(&env).unwrap().is_empty());

    // Git: the profile folder uses only its key, everything else the private one.
    let repo = env.home.join("dev/kunde-a/repo");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    let out = git(&env, &repo, &["config", "--get", "core.sshCommand"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("kunde-a_ed25519"));
    let other = env.home.join("elsewhere");
    fs::create_dir_all(&other).unwrap();
    git(&env, &other, &["init", "-q"]);
    let out = git(&env, &other, &["config", "--get", "core.sshCommand"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("privat2_ed25519"));
}

#[test]
fn git_ssh_command_in_the_shell_config_is_reported() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let report = doctor::run(&env);
    assert_eq!(report.checks.iter().find(|c| c.id == "git-env").unwrap().status, doctor::Status::Ok);

    fs::create_dir_all(env.home.join(".config/fish")).unwrap();
    fs::write(
        env.home.join(".config/fish/config.fish"),
        "fish_add_path ~/bin\nset -gx GIT_SSH_COMMAND \"ssh -i ~/.ssh/old_key\"\n",
    )
    .unwrap();

    let report = doctor::run(&env);
    let check = report.checks.iter().find(|c| c.id == "git-env").unwrap();
    assert_eq!(check.status, doctor::Status::Error);
    assert_eq!(check.title.to_string(), "GIT_SSH_COMMAND übersteuert Keybund");
    assert!(check.detail.as_ref().unwrap().to_string().contains("~/.config/fish/config.fish:2"), "{:?}", check.detail);

    let r = which::which(&env, &config, "~/dev/kunde-a").unwrap();
    assert_eq!(r.key_path.as_deref(), Some("~/.ssh/old_key"));
    assert_eq!(r.via.as_ref().map(|v| v.to_string()).as_deref(), Some("GIT_SSH_COMMAND"));
    assert_eq!(r.source.as_deref(), Some("~/.config/fish/config.fish:2"));
    assert!(r.notes[0].to_string().starts_with("GIT_SSH_COMMAND ist gesetzt"));
}

#[test]
fn uninstall_removes_only_keybund_and_can_restore_the_originals() {
    use keybund_core::uninstall::{self, UninstallOptions};
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    let ssh_before = fs::read_to_string(env.ssh_config()).unwrap();
    let git_before = fs::read_to_string(env.gitconfig()).unwrap();
    apply::apply_config(&env, &config).unwrap();
    // The user keeps editing their own config after installing Keybund.
    let edited = fs::read_to_string(env.ssh_config()).unwrap() + "\nHost nas\n    User admin\n";
    fs::write(env.ssh_config(), &edited).unwrap();

    // Default: only Keybund's lines and files go, later edits stay.
    let preview = uninstall::preview(&env, UninstallOptions::default()).unwrap();
    assert!(preview.backups_available);
    let paths: Vec<&str> = preview.plan.changes.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"~/.ssh/config.d/keybund.conf"));
    assert!(paths.contains(&"~/.config/keybund/git/privat.gitconfig"));
    assert!(!paths.contains(&"~/.config/keybund/profiles.toml"));
    uninstall::uninstall(&env, &preview.plan).unwrap();
    assert_eq!(fs::read_to_string(env.ssh_config()).unwrap(), format!("{ssh_before}\nHost nas\n    User admin\n"));
    assert_eq!(fs::read_to_string(env.gitconfig()).unwrap(), git_before);
    assert!(!env.ssh_keybund_conf().exists());
    assert!(!env.git_dir().exists());
    assert!(env.profiles_file().exists(), "profiles stay for a later reinstall");
    assert!(env.expand("~/.ssh/privat_ed25519").exists(), "keys are never touched");
    assert!(uninstall::preview(&env, UninstallOptions::default()).unwrap().plan.is_empty());

    // Reinstall: the backup is taken anew, so restoring keeps the edits
    // made in between instead of going back to the very first state.
    apply::apply_config(&env, &config).unwrap();
    let opts = UninstallOptions { restore_backups: true, purge: true };
    let preview = uninstall::preview(&env, opts).unwrap();
    uninstall::uninstall(&env, &preview.plan).unwrap();
    assert_eq!(fs::read_to_string(env.ssh_config()).unwrap(), format!("{ssh_before}\nHost nas\n    User admin\n"));
    assert!(!env.config_dir.exists());
}

/// Dolphin with an old libssh: the doctor's fix writes the Include with the
/// full path, which OpenSSH follows just as well; the Host block keeps new
/// host keys in the profile's own known_hosts file.
#[test]
fn absolute_include_and_known_hosts_per_profile() {
    if !has("ssh") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let ssh_config = fs::read_to_string(env.ssh_config()).unwrap();
    assert!(ssh_config.starts_with("Include ~/.ssh/config.d/keybund.conf\n"), "{ssh_config}");
    assert!(env.known_hosts_dir().is_dir(), "ssh needs the folder to add host keys");

    doctor::fix(&env, &doctor::Fix::AbsoluteSshInclude).unwrap();
    let ssh_config = fs::read_to_string(env.ssh_config()).unwrap();
    let line = format!("Include {}\n", env.ssh_keybund_conf().display());
    assert!(ssh_config.starts_with(&line), "{ssh_config}");
    assert_eq!(ssh_config.matches("keybund.conf").count(), 1, "{ssh_config}");
    assert!(store::load(&env).unwrap().settings.absolute_ssh_include);

    let out = Command::new("ssh").arg("-G").arg("-F").arg(env.ssh_config()).arg("meinserver").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("identityfile ~/.ssh/privat_ed25519"), "{text}");
    assert!(text.lines().any(|l| l.starts_with("userknownhostsfile") && l.contains("known_hosts.d/privat")), "{text}");
    // Nothing left to do after the fix.
    assert!(apply::plan(&env, &store::load(&env).unwrap()).unwrap().changes.is_empty());
}

/// No ~/.gitconfig before Keybund: an old backup from an earlier install
/// must not count as "the original".
#[test]
fn stale_backup_goes_when_there_was_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let env = Env::with_home(dir.path());
    fs::write(dir.path().join(".gitconfig.keybund-backup"), "[user]\n\tname = alt\n").unwrap();
    apply::apply_config(&env, &keybund_core::model::Config::default()).unwrap();
    assert!(env.gitconfig().exists());
    assert!(!dir.path().join(".gitconfig.keybund-backup").exists());
}

/// A linked worktree belongs where its main repository is: git matches
/// `gitdir:` against the main repo's git directory, and so does `which`.
#[test]
fn which_follows_worktrees_like_git() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let main = env.home.join("dev/kunde-a/main");
    fs::create_dir_all(&main).unwrap();
    git(&env, &main, &["init", "-q"]);
    git(&env, &main, &["-c", "user.name=T", "-c", "user.email=t@x", "commit", "-q", "--allow-empty", "-m", "x"]);
    let tree = env.home.join("dev/kunde-b/tree");
    git(&env, &main, &["worktree", "add", "-q", &tree.display().to_string()]);
    let r = which::which(&env, &config, &tree.display().to_string()).unwrap();
    assert_eq!(r.profile_id.as_deref(), Some("kunde-a"), "{r:?}");
    assert_eq!(r.git_email.as_deref(), Some("kunde-a@example.org"));
    assert!(!r.notes.iter().any(|n| n.to_string().contains("anderen Key")), "{:?}", r.notes);
}

/// A `'` in a key path: ssh treats it as an opening quote, and an unterminated
/// one makes it refuse the whole config – for every host. The renderer
/// quotes such a path; ssh must still resolve it.
#[test]
fn a_key_path_with_an_apostrophe_does_not_break_ssh() {
    if !has("ssh") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, mut config) = setup();
    let path = "~/.ssh/o'brien_ed25519".to_string();
    keygen::generate(&env, &GenerateOptions { path: path.clone(), comment: "t".into(), passphrase: None }).unwrap();
    config.profiles[0].key.path = path;
    store::save(&env, &config).unwrap();
    apply::apply_config(&env, &config).unwrap();
    for host in ["meinserver", "github.com"] {
        let out = Command::new("ssh").arg("-G").arg("-F").arg(env.ssh_keybund_conf()).arg(host).output().unwrap();
        assert!(out.status.success(), "{host}: {}", String::from_utf8_lossy(&out.stderr));
    }
    let out = Command::new("ssh").arg("-G").arg("-F").arg(env.ssh_keybund_conf()).arg("meinserver").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|l| l.starts_with("identityfile ") && l.ends_with("/.ssh/o'brien_ed25519")), "{text}");
}

/// profiles.toml edited by hand: whatever would break ssh or git is refused
/// before anything is written, not only on save.
#[test]
fn apply_refuses_an_invalid_profiles_file() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let before = fs::read_to_string(env.ssh_keybund_conf()).unwrap();
    let text = fs::read_to_string(env.profiles_file()).unwrap();
    for (from, to) in [("alias = \"meinserver\"", "alias = \"mein server\""), ("id = \"privat\"", "id = \"../evil\"")] {
        fs::write(env.profiles_file(), text.replace(from, to)).unwrap();
        let config = store::load(&env).unwrap();
        assert!(apply::plan(&env, &config).is_err(), "{to}");
        assert!(apply::apply_config(&env, &config).is_err(), "{to}");
        assert_eq!(fs::read_to_string(env.ssh_keybund_conf()).unwrap(), before);
    }
}

/// `<file>.keybund-backup`, next to the file.
fn backup_of(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap().to_os_string();
    name.push(".keybund-backup");
    path.with_file_name(name)
}

/// A fresh machine: no ~/.ssh/config, no ~/.gitconfig before Keybund. An
/// uninstall takes both files away again, leaves no backup that would only
/// hold Keybund's own line, and a later "restore backups" has nothing to do.
#[test]
fn uninstall_after_a_fresh_install_leaves_nothing_behind() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    fs::remove_file(env.ssh_config()).unwrap();
    fs::remove_file(env.gitconfig()).unwrap();
    apply::apply_config(&env, &config).unwrap();
    assert!(env.ssh_config().exists() && env.gitconfig().exists());
    assert!(!backup_of(&env.ssh_config()).exists());

    let opts = keybund_core::uninstall::UninstallOptions { restore_backups: false, purge: true };
    let preview = keybund_core::uninstall::preview(&env, opts).unwrap();
    assert!(!preview.backups_available);
    keybund_core::uninstall::uninstall(&env, &preview.plan).unwrap();
    assert!(!env.ssh_config().exists(), "an empty file is not 'as before'");
    assert!(!env.gitconfig().exists());
    assert!(!backup_of(&env.ssh_config()).exists());
    assert!(!backup_of(&env.gitconfig()).exists());
    assert!(!env.last_tests_file().exists());

    let again = keybund_core::uninstall::UninstallOptions { restore_backups: true, purge: false };
    let preview = keybund_core::uninstall::preview(&env, again).unwrap();
    assert!(!preview.backups_available);
    assert!(preview.plan.changes.is_empty(), "{:?}", preview.plan.changes);
}

/// A restored backup is used up: "restore" twice does not bring Keybund back.
#[test]
fn a_restored_backup_is_not_restored_twice() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let opts = keybund_core::uninstall::UninstallOptions { restore_backups: true, purge: false };
    let preview = keybund_core::uninstall::preview(&env, opts).unwrap();
    assert!(preview.backups_available);
    keybund_core::uninstall::uninstall(&env, &preview.plan).unwrap();
    assert_eq!(fs::read_to_string(env.ssh_config()).unwrap(), "Host github.com\n    User git\n");
    assert!(!backup_of(&env.ssh_config()).exists());
    let preview = keybund_core::uninstall::preview(&env, opts).unwrap();
    assert!(!preview.backups_available);
    assert!(preview.plan.changes.is_empty());
}

/// The archive keeps what is there: a stale `.pub` in it blocks the move
/// as much as a stale private key.
#[test]
fn archive_refuses_to_overwrite_a_public_key_in_the_archive() {
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, mut config) = setup();
    fs::create_dir_all(env.ssh_archive_dir()).unwrap();
    fs::write(env.ssh_archive_dir().join("kunde-b_ed25519.pub"), "old").unwrap();
    let err = archive::archive(&env, &mut config, "kunde-b").unwrap_err();
    assert_eq!(err.kind(), "key_exists");
    assert!(env.expand("~/.ssh/kunde-b_ed25519").exists(), "nothing moved");
    assert!(!config.profile("kunde-b").unwrap().archived);
}

#[test]
fn a_key_comment_cannot_add_lines_to_the_pub_file() {
    let dir = tempfile::tempdir().unwrap();
    let env = Env::with_home(dir.path());
    let opts = GenerateOptions { path: "~/.ssh/x".into(), comment: "a\nHost evil".into(), passphrase: None };
    let err = keygen::generate(&env, &opts).unwrap_err();
    assert_eq!(err.kind(), "invalid_value");
    assert!(!env.expand("~/.ssh/x").exists());
}

/// ssh refuses a config file others may write to ("Bad owner or
/// permissions") – then no Host block applies, Keybund's included. The
/// doctor sees it and the fix takes only the group/other write bits away;
/// a 0400 key stays 0400.
#[test]
#[cfg(unix)]
fn doctor_checks_the_config_files_ssh_refuses() {
    use std::os::unix::fs::PermissionsExt;
    if !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let by_id = |report: &doctor::Report, id: &str| report.checks.iter().find(|c| c.id == id).map(|c| c.status);
    assert_eq!(by_id(&doctor::run(&env), "config-perms"), Some(doctor::Status::Ok));

    fs::set_permissions(env.ssh_config(), fs::Permissions::from_mode(0o664)).unwrap();
    fs::set_permissions(env.ssh_keybund_conf(), fs::Permissions::from_mode(0o602)).unwrap();
    let key = env.expand("~/.ssh/privat_ed25519");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o400)).unwrap();
    let report = doctor::run(&env);
    assert_eq!(by_id(&report, "config-perms"), Some(doctor::Status::Error));
    assert_eq!(by_id(&report, "key-perms"), Some(doctor::Status::Ok), "0400 is fine for ssh");

    doctor::fix(&env, &doctor::Fix::Permissions).unwrap();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&env.ssh_config()), 0o644);
    assert_eq!(mode(&env.ssh_keybund_conf()), 0o600);
    assert_eq!(mode(&key), 0o400, "the fix only touches what is wrong");
    assert_eq!(by_id(&doctor::run(&env), "config-perms"), Some(doctor::Status::Ok));
}

/// `git config --show-origin` C-quotes a path with non-ASCII bytes; `which`
/// must still show `~/…`, also for a user named jörg.
#[test]
fn which_shows_the_origin_in_a_non_ascii_home() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let outer = tempfile::tempdir().unwrap();
    let home = outer.path().canonicalize().unwrap().join("jörg");
    fs::create_dir_all(&home).unwrap();
    let env = Env::with_home(&home);
    fs::create_dir_all(env.ssh_dir()).unwrap();
    let path = keygen::default_key_path("acme");
    keygen::generate(&env, &GenerateOptions { path: path.clone(), comment: "t".into(), passphrase: None }).unwrap();
    fs::create_dir_all(home.join("dev/acme/app")).unwrap();
    let config = Config {
        profiles: vec![Profile {
            id: "acme".into(),
            name: "Acme".into(),
            key: KeyRef { path, comment: None },
            folders: vec![store::normalize_folder(&env, "~/dev/acme")],
            ..Default::default()
        }],
        ..Default::default()
    };
    store::save(&env, &config).unwrap();
    apply::apply_config(&env, &config).unwrap();
    git(&env, &home.join("dev/acme/app"), &["init", "-q"]);
    let r = which::which(&env, &config, "~/dev/acme/app").unwrap();
    assert_eq!(r.source.as_deref(), Some("~/.config/keybund/git/acme.gitconfig"), "{r:?}");
    assert_eq!(r.profile_id.as_deref(), Some("acme"));
    // A repository's own setting: the origin names the repository, not `.git/config`.
    git(&env, &home.join("dev/acme/app"), &["config", "core.sshCommand", "ssh -i ~/.ssh/other"]);
    let r = which::which(&env, &config, "~/dev/acme/app").unwrap();
    assert_eq!(r.source.as_deref(), Some("~/dev/acme/app/.git/config"), "{r:?}");
}

/// The user's own `UserKnownHostsFile` under `Host *`: ssh reads and writes
/// only that file, so Keybund must store and look up host keys there – a key
/// in `~/.ssh/known_hosts` would never be read and the test would stay
/// "host key not confirmed" forever.
#[test]
fn host_keys_go_where_ssh_looks_for_them() {
    use keybund_core::hostkeys::{self, HostKeyCheck};
    use keybund_core::model::Settings;
    if !has("ssh") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    fs::write(env.ssh_config(), "Host *\n    UserKnownHostsFile ~/.ssh/kh2\n").unwrap();
    let profile = config.profile("kunde-a").unwrap();
    let github = &profile.hosts[0];
    let settings = Settings::default();

    assert_eq!(hostkeys::target_file(&env, &settings, profile, github), env.home.join(".ssh/kh2"));
    assert!(!hostkeys::is_known(&env, &settings, profile, github));
    assert_eq!(hostkeys::check(&env, &settings, profile, github).unwrap(), HostKeyCheck::Pinned);
    assert!(fs::read_to_string(env.home.join(".ssh/kh2")).unwrap().starts_with("github.com ssh-ed25519 "));
    assert!(!env.known_hosts().exists(), "nothing in a file ssh does not read");
    assert!(hostkeys::is_known(&env, &settings, profile, github));

    // A key only in ~/.ssh/known_hosts does not count while ssh reads kh2.
    fs::remove_file(env.home.join(".ssh/kh2")).unwrap();
    fs::write(env.known_hosts(), format!("github.com {}\n", hostkeys::PINNED[0].1)).unwrap();
    assert!(!hostkeys::is_known(&env, &settings, profile, github));
}

/// Several keys for a host and no IdentitiesOnly: ssh tries them all, so
/// `which` must say so even when the first one belongs to a key – that key
/// is not necessarily the one that logs in.
#[test]
fn which_says_when_ssh_tries_several_keys() {
    if !has("ssh") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    keygen::generate(
        &env,
        &GenerateOptions { path: "~/.ssh/id_ed25519".into(), comment: "x".into(), passphrase: None },
    )
    .unwrap();
    fs::write(env.ssh_config(), "Host *\n    IdentityFile ~/.ssh/privat_ed25519\n    IdentityFile ~/.ssh/id_ed25519\n")
        .unwrap();
    let r = which::which(&env, &config, "example.org").unwrap();
    assert_eq!(r.key_path.as_deref(), Some("~/.ssh/privat_ed25519"));
    assert_eq!(r.profile_id.as_deref(), Some("privat"));
    assert!(r.notes.iter().any(|n| n.key == "which_triesKeys"), "{:?}", r.notes);
}

/// What docs/generated-files.md says about linked worktrees, checked with
/// git: `gitdir:` sees the main repository's `.git`, so a worktree follows
/// its main repository – out of a folder into it gets no key, from a folder
/// out of it keeps the key.
#[test]
fn worktrees_follow_their_main_repository() {
    if !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let repo = |rel: &str| {
        let dir = env.home.join(rel);
        fs::create_dir_all(&dir).unwrap();
        git(&env, &dir, &["init", "-q"]);
        git(&env, &dir, &["-c", "user.email=x@y", "-c", "user.name=x", "commit", "-q", "--allow-empty", "-m", "i"]);
        dir
    };
    let email =
        |dir: &Path| String::from_utf8_lossy(&git(&env, dir, &["config", "user.email"]).stdout).trim().to_string();
    let outside = repo("elsewhere/main");
    let inside_wt = env.home.join("dev/privat/wt");
    git(&env, &outside, &["worktree", "add", "-q", &inside_wt.display().to_string()]);
    assert_eq!(email(&inside_wt), "global@example.org", "a worktree inside the folder, main outside: no key");

    let inside = repo("dev/privat/main");
    let outside_wt = env.home.join("elsewhere/wt");
    git(&env, &inside, &["worktree", "add", "-q", &outside_wt.display().to_string()]);
    assert_eq!(email(&outside_wt), "privat@example.org", "a worktree outside, main inside: the key");
}

/// The macOS configs, asked of a real ssh and git here on Linux: ssh does
/// not know `UseKeychain` (like Homebrew's ssh on a Mac) and still accepts
/// the files and the command line because of `IgnoreUnknown`; git applies
/// `gitdir/i:` to a checkout whose folder differs in case.
#[test]
fn macos_configs_work_with_an_ssh_without_keychain_support() {
    use keybund_core::env::Platform;
    if !has("ssh") || !has("git") || !has("ssh-keygen") {
        return;
    }
    let (_dir, env, config) = setup();
    let env = Env { platform: Platform::MacOs, ..env };
    apply::apply_config(&env, &config).unwrap();
    let conf = fs::read_to_string(env.ssh_keybund_conf()).unwrap();
    assert!(conf.contains("UseKeychain yes"));
    let out = Command::new("ssh").arg("-G").arg("-F").arg(env.ssh_keybund_conf()).arg("meinserver").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let cmd = keybund_core::render::ssh_command("~/.ssh/privat_ed25519", Platform::MacOs);
    let args: Vec<&str> = cmd.split_whitespace().skip(1).collect();
    let out = Command::new("ssh").args(&args).arg("-G").arg("example.org").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    // gitdir/i: the folder is ~/dev/kunde-a/, the checkout sits in ~/DEV/Kunde-A/app.
    let repo = env.home.join("DEV/Kunde-A/app");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    // git compares a gitdir/i: pattern without case, also on Linux.
    let out = git(&env, &repo, &["config", "user.email"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "kunde-a@example.org");
}

/// macOS: no Dolphin check (there is no Dolphin), and the agent hint talks
/// about launchd instead of KDE and GNOME.
#[test]
fn the_doctor_speaks_macos_on_a_mac() {
    use keybund_core::env::Platform;
    if !has("ssh-keygen") || std::env::var_os("SSH_AUTH_SOCK").is_some() {
        return;
    }
    let (_dir, env, _config) = setup();
    let mac = Env { platform: Platform::MacOs, ..env.clone() };
    let report = doctor::run(&mac);
    assert!(report.checks.iter().all(|c| !c.id.starts_with("dolphin")), "{:?}", report.checks);
    let agent = report.checks.iter().find(|c| c.id == "agent").unwrap();
    assert_eq!(agent.detail.as_ref().unwrap().key, "doctor_agent_missing_detail_macos");
    let agent = doctor::run(&env).checks.into_iter().find(|c| c.id == "agent").unwrap();
    assert_eq!(agent.detail.unwrap().key, "doctor_agent_missing_detail");
}
