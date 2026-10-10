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
    let config = Config { default_profile: Some("privat".into()), profiles };
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
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("ssh -i {key} -o IdentitiesOnly=yes"));
        let out = git(&env, &repo, &["config", "--get", "user.email"]);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("{id}@example.org"));
    }

    // Outside of all profile folders the default profile's key applies,
    // while the global identity stays untouched.
    let other = env.home.join("elsewhere");
    fs::create_dir_all(&other).unwrap();
    assert!(git(&env, &other, &["init", "-q"]).status.success());
    let out = git(&env, &other, &["config", "--get", "core.sshCommand"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ssh -i ~/.ssh/privat_ed25519 -o IdentitiesOnly=yes");
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
