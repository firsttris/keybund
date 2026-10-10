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
    assert_eq!(r.via.as_deref(), Some("Standard-Key"));

    // A repo with its own core.sshCommand is reported as a mismatch.
    let repo = env.home.join("dev/kunde-a/special");
    fs::create_dir_all(&repo).unwrap();
    git(&env, &repo, &["init", "-q"]);
    git(&env, &repo, &["config", "core.sshCommand", "ssh -i ~/.ssh/kunde-b_ed25519"]);
    let r = which::which(&env, &config, &repo.display().to_string()).unwrap();
    assert_eq!(r.key_path.as_deref(), Some("~/.ssh/kunde-b_ed25519"));
    assert_eq!(r.identities_only, Some(false));
    assert!(r.notes.iter().any(|n| n.contains("anderen Key")), "{:?}", r.notes);

    // Without a default profile, a repo outside of all folders has no key.
    config.default_profile = None;
    store::save(&env, &config).unwrap();
    apply::apply_config(&env, &config).unwrap();
    let elsewhere = env.home.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    git(&env, &elsewhere, &["init", "-q"]);
    let r = which::which(&env, &config, &elsewhere.display().to_string()).unwrap();
    assert_eq!(r.key_path, None);
    assert!(r.notes.iter().any(|n| n.contains("kein core.sshCommand")));
    let r = which::which(&env, &config, &env.home.join("plain").display().to_string()).unwrap();
    assert!(r.notes.iter().any(|n| n.contains("keinen Standard-Key")));
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
    assert!(r.notes.iter().any(|n| n.contains("kunde-a, kunde-b")), "{:?}", r.notes);
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
    assert!(by_id("keys-missing").detail.as_deref().unwrap().contains("kunde-b_ed25519.pub"));
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
    let detail = check.detail.as_deref().unwrap();
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
    for key in ["~/.ssh/privat2_ed25519", "~/.ssh/pflegeabc/id_ed25519", "~/.ssh/s-communication_ed25519"] {
        keygen::generate(&env, &GenerateOptions { path: key.into(), comment: "x".into(), passphrase: None }).unwrap();
    }
    let user_config = "# Privater Account\nHost github.com\n    HostName github.com\n    User git\n    IdentityFile ~/.ssh/privat2_ed25519\n    IdentitiesOnly yes\n\n# Pflegeabc Account\nHost pflegeabc\n    HostName github.com\n    User git\n    IdentityFile ~/.ssh/pflegeabc/id_ed25519\n    IdentitiesOnly yes\n\n# S-Communication\nHost gitlab.sfp.berlin\n    HostName gitlab.sfp.berlin\n    User git\n    IdentityFile ~/.ssh/s-communication_ed25519\n    IdentitiesOnly yes\n\nHost *\n    ServerAliveInterval 30\n";
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
    assert_eq!(check.title, "Globaler Eintrag für github.com in ~/.ssh/config");
    assert!(check.fix.is_none(), "not imported yet, nothing to clean up");

    // Import all three.
    let proposals = import::proposals_for(&env).unwrap();
    assert_eq!(
        proposals.iter().map(|p| p.profile.id.as_str()).collect::<Vec<_>>(),
        ["privat2", "pflegeabc", "s-communication"]
    );
    assert!(proposals.iter().all(|p| p.key_exists));
    import::import(&env, &proposals).unwrap();
    apply::apply_config(&env, &store::load(&env).unwrap()).unwrap();
    assert!(import::proposals_for(&env).unwrap().is_empty(), "nothing left to import");
    let config = store::load(&env).unwrap();
    assert_eq!(config.default_profile.as_deref(), Some("privat2"));

    // Keybund now writes the alias blocks itself.
    let conf = fs::read_to_string(env.ssh_keybund_conf()).unwrap();
    assert!(conf.contains("Host pflegeabc\n    HostName github.com\n"));
    assert!(conf.contains("Host gitlab.sfp.berlin\n"));
    assert!(!conf.contains("Host github.com\n"), "github.com stays a shared host without Host block");

    // Doctor offers to remove the old entries; the fix removes exactly them.
    let report = doctor::run(&env);
    let check = report.checks.iter().find(|c| c.id == "ssh-config-github.com").unwrap();
    assert_eq!(check.fix, Some(doctor::Fix::CleanupSshConfig));
    assert!(report.checks.iter().any(|c| c.id == "ssh-config-pflegeabc"));
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
    assert_eq!(check.title, "GIT_SSH_COMMAND übersteuert Keybund");
    assert!(check.detail.as_deref().unwrap().contains("~/.config/fish/config.fish:2"), "{:?}", check.detail);

    let r = which::which(&env, &config, "~/dev/kunde-a").unwrap();
    assert_eq!(r.key_path.as_deref(), Some("~/.ssh/old_key"));
    assert_eq!(r.via.as_deref(), Some("GIT_SSH_COMMAND"));
    assert_eq!(r.source.as_deref(), Some("~/.config/fish/config.fish:2"));
    assert!(r.notes[0].starts_with("GIT_SSH_COMMAND ist gesetzt"));
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

    // Reinstall, then restore the originals and purge everything.
    apply::apply_config(&env, &config).unwrap();
    let opts = UninstallOptions { restore_backups: true, purge: true };
    let preview = uninstall::preview(&env, opts).unwrap();
    uninstall::uninstall(&env, &preview.plan).unwrap();
    assert_eq!(fs::read_to_string(env.ssh_config()).unwrap(), ssh_before);
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
