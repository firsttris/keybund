//! Integration tests: one command at a time against a temporary HOME.

mod common;

use common::{has_tools, Sandbox};
use predicates::prelude::*;

#[test]
fn help_lists_all_commands() {
    let sb = Sandbox::new();
    let out = sb.cmd().arg("--help").assert().success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for c in ["list", "new", "copy", "test", "which", "clone", "apply", "doctor", "keys", "archive"] {
        assert!(text.contains(c), "missing {c}");
    }
}

#[test]
fn list_without_profiles() {
    let sb = Sandbox::new();
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("Noch keine Profile"));
}

#[test]
fn new_requires_flags_without_terminal() {
    let sb = Sandbox::new();
    sb.cmd().args(["new", "--name", "Kunde C"]).assert().code(2).stderr(predicate::str::contains("--git-name fehlt"));
}

#[test]
fn new_generates_key_and_writes_configs() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "Kunde C",
        &["--provider", "github", "--folder", "~/dev/kunde-c", "--git-name", "T", "--git-email", "t@c.de"],
    );

    assert!(sb.path(".ssh/kunde-c_ed25519").exists());
    assert!(sb.path(".ssh/kunde-c_ed25519.pub").exists());
    assert!(sb.path("dev/kunde-c").is_dir());
    let toml = sb.read(".config/keybund/profiles.toml");
    assert!(toml.contains("id = \"kunde-c\""));
    assert!(toml.contains("default_profile = \"kunde-c\""), "first profile becomes default");
    assert!(sb.read(".ssh/config").starts_with("Include ~/.ssh/config.d/keybund.conf"));
    assert!(sb.read(".config/keybund/git/index.gitconfig").contains("[includeIf \"gitdir:~/dev/kunde-c/\"]"));
    assert!(sb.read(".config/keybund/git/kunde-c.gitconfig").contains("email = t@c.de"));
}

#[test]
fn new_rejects_duplicate_and_nested_folders() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    let args = ["--provider", "github", "--folder", "~/dev/a", "--git-name", "T", "--git-email", "t@a.de"];
    sb.new_profile("A", &args);
    sb.cmd()
        .args(["new", "--name", "A", "--yes"])
        .args(args)
        .assert()
        .failure()
        .stderr(predicate::str::contains("existiert bereits"));
    sb.cmd()
        .args([
            "new",
            "--name",
            "B",
            "--yes",
            "--provider",
            "github",
            "--folder",
            "~/dev/a/sub",
            "--git-name",
            "T",
            "--git-email",
            "t@b.de",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("überschneiden sich"));
}

#[test]
fn new_with_existing_key() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.path(".ssh")).unwrap();
    std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "old", "-f"])
        .arg(sb.path(".ssh/id_ed25519"))
        .status()
        .unwrap();
    sb.new_profile(
        "Server",
        &[
            "--provider",
            "generic",
            "--host",
            "1.2.3.4",
            "--alias",
            "meinserver",
            "--user",
            "t",
            "--key",
            "~/.ssh/id_ed25519",
        ],
    );
    let conf = sb.read(".ssh/config.d/keybund.conf");
    assert!(conf.contains("Host meinserver"));
    assert!(conf.contains("IdentityFile ~/.ssh/id_ed25519"));
    assert!(!sb.path(".ssh/server_ed25519").exists());
}

#[test]
fn copy_print_outputs_public_key() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "P",
        &[
            "--provider",
            "github",
            "--folder",
            "~/dev/p",
            "--git-name",
            "T",
            "--git-email",
            "t@p.de",
            "--comment",
            "me@test",
        ],
    );
    sb.cmd()
        .args(["copy", "p", "--print"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("ssh-ed25519 ").and(predicate::str::contains("me@test")));
    // No clipboard in the sandbox: clear error instead of a hang.
    sb.cmd().args(["copy", "p"]).assert().code(2).stderr(predicate::str::contains("--print"));
}

#[test]
fn unknown_profile_is_a_clear_error() {
    let sb = Sandbox::new();
    sb.cmd().args(["show", "nope"]).assert().code(2).stderr(predicate::str::contains("Profil „nope“ existiert nicht"));
}

#[test]
fn apply_dry_run_writes_nothing() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "P",
        &["--provider", "github", "--folder", "~/dev/p", "--git-name", "T", "--git-email", "t@p.de", "--no-apply"],
    );
    assert!(!sb.path(".ssh/config").exists());
    sb.cmd()
        .args(["apply", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("+Include ~/.ssh/config.d/keybund.conf"));
    assert!(!sb.path(".ssh/config").exists());
    sb.cmd().args(["apply", "--yes"]).assert().success().stdout(predicate::str::contains("geschrieben"));
    sb.cmd().args(["apply", "--dry-run"]).assert().success().stdout(predicate::str::contains("Configs sind aktuell"));
}

#[test]
fn test_command_uses_exit_code_from_output() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("P", &["--provider", "github", "--folder", "~/dev/p", "--git-name", "T", "--git-email", "t@p.de"]);

    #[cfg(unix)]
    {
        // GitHub exits with 1 on success.
        sb.fake_ssh("Hi octocat! You have successfully authenticated, but GitHub does not provide shell access.", 1);
        sb.cmd().args(["test", "p"]).assert().success().stdout(predicate::str::contains("Verbunden als @octocat"));
        let log = sb.ssh_log();
        assert!(log.contains("IdentitiesOnly=yes"));
        assert!(log.contains("BatchMode=yes"));
        assert!(log.contains("StrictHostKeyChecking=accept-new"));
        assert!(log.contains("git@github.com"));

        sb.fake_ssh("git@github.com: Permission denied (publickey).", 255);
        sb.cmd().args(["test", "p"]).assert().code(1).stderr(predicate::str::contains("GitHub kennt diesen Key nicht"));
    }
}

#[test]
fn default_and_keys() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    let common = ["--provider", "github", "--git-name", "T"];
    sb.new_profile("A", &[&common[..], &["--folder", "~/dev/a", "--git-email", "a@x.de"]].concat());
    sb.new_profile("B", &[&common[..], &["--folder", "~/dev/b", "--git-email", "b@x.de"]].concat());
    sb.cmd().args(["default", "b"]).assert().success();
    assert!(sb.read(".config/keybund/profiles.toml").contains("default_profile = \"b\""));
    sb.cmd()
        .arg("keys")
        .assert()
        .success()
        .stdout(predicate::str::contains("~/.ssh/a_ed25519").and(predicate::str::contains("[b]")));
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("B (b) default"));
}
