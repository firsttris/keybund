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
    assert!(!sb.path("work/kunde/sub").exists(), "nothing created for a rejected folder");
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
        assert!(log.contains("AddKeysToAgent=yes"));
        assert!(!log.contains("BatchMode"));
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

#[test]
fn show_prints_details_and_public_key() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "P",
        &[
            "--provider",
            "gitlab",
            "--host",
            "gitlab.kunde.de",
            "--folder",
            "~/dev/p",
            "--git-name",
            "T",
            "--git-email",
            "t@p.de",
            "--sign",
        ],
    );
    sb.cmd().args(["show", "p"]).assert().success().stdout(
        predicate::str::contains("ssh-ed25519")
            .and(predicate::str::contains("SHA256:"))
            .and(predicate::str::contains("T <t@p.de>"))
            .and(predicate::str::contains("Signieren      ja"))
            .and(predicate::str::contains("gitlab.kunde.de (GitLab)")),
    );
}

#[test]
fn which_host_uses_ssh_g() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("Server", &["--provider", "generic", "--host", "1.2.3.4", "--alias", "meinserver", "--user", "t"]);
    #[cfg(unix)]
    {
        let key = sb.path(".ssh/server_ed25519");
        sb.fake_ssh_g(&format!(
            "hostname 1.2.3.4\nuser t\nport 22\nidentitiesonly yes\nidentityfile ~/.ssh/id_rsa\nidentityfile {}",
            key.display()
        ));
        sb.cmd().args(["which", "meinserver"]).assert().success().stdout(
            predicate::str::contains("~/.ssh/server_ed25519")
                .and(predicate::str::contains("Profil         Server"))
                .and(predicate::str::contains("t@1.2.3.4:22"))
                .and(predicate::str::contains("keybund.conf · via Host meinserver"))
                .and(predicate::str::contains("ja, nur dieser Key wird angeboten")),
        );

        // Unknown host: ssh tries several default keys.
        std::fs::write(sb.path(".ssh/id_rsa"), "x").unwrap();
        std::fs::write(sb.path(".ssh/id_ecdsa"), "x").unwrap();
        sb.fake_ssh_g(&format!(
            "hostname example.org\nport 22\nidentitiesonly no\nidentityfile {}\nidentityfile {}",
            sb.path(".ssh/id_rsa").display(),
            sb.path(".ssh/id_ecdsa").display()
        ));
        sb.cmd().args(["which", "example.org"]).assert().success().stdout(
            predicate::str::contains("ssh probiert 2 Keys nacheinander durch")
                .and(predicate::str::contains("~/.ssh/id_rsa")),
        );
    }
}

#[test]
fn test_generic_server_and_missing_host() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("Server", &["--provider", "generic", "--host", "1.2.3.4", "--alias", "srv"]);
    #[cfg(unix)]
    {
        sb.fake_ssh("", 0);
        sb.cmd()
            .args(["test", "server"])
            .assert()
            .success()
            .stdout(predicate::str::contains("srv: Anmeldung erfolgreich"));
        assert!(sb.ssh_log().contains("srv true"));
    }
}

#[test]
fn clone_errors_are_explained() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("P", &["--provider", "github", "--folder", "~/dev/p", "--git-name", "T", "--git-email", "t@p.de"]);
    sb.cmd()
        .args(["clone", "git@codeberg.org:a/b.git"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Kein Profil hat den Host codeberg.org"));
    sb.cmd().args(["clone", "kaputt"]).assert().code(2).stderr(predicate::str::contains("Ungültige Repo-URL"));
    std::fs::create_dir_all(sb.path("dev/p/b")).unwrap();
    sb.cmd()
        .args(["clone", "git@github.com:a/b.git"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Zielordner existiert bereits: ~/dev/p/b"));
}

#[test]
fn keys_and_list_without_data() {
    let sb = Sandbox::new();
    sb.cmd().arg("keys").assert().success().stdout(predicate::str::contains("Keine Keys"));
    sb.cmd().args(["doctor"]).assert().code(1).stdout(predicate::str::contains("Include fehlt in ~/.ssh/config"));
}

#[test]
fn invalid_id_and_existing_key_errors() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    let args = ["--provider", "github", "--folder", "~/dev/x", "--git-name", "T", "--git-email", "t@x.de"];
    sb.cmd()
        .args(["new", "--name", "!!!", "--yes"])
        .args(args)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Ungültige Profil-ID"));
    sb.cmd()
        .args(["new", "--name", "X", "--key", "~/.ssh/missing", "--yes"])
        .args(args)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Key fehlt"));
    std::fs::create_dir_all(sb.path(".ssh")).unwrap();
    std::fs::write(sb.path(".ssh/x_ed25519"), "x").unwrap();
    sb.cmd()
        .args(["new", "--name", "X", "--yes"])
        .args(args)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Key existiert bereits"));
}

#[test]
fn import_takes_over_an_existing_ssh_config() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.path(".ssh/pflegeabc")).unwrap();
    for key in [".ssh/privat_ed25519", ".ssh/pflegeabc/id_ed25519"] {
        std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(sb.path(key))
            .status()
            .unwrap();
    }
    std::fs::write(
        sb.path(".ssh/config"),
        "# Privater Account\nHost github.com\n    User git\n    IdentityFile ~/.ssh/privat_ed25519\n\n# Pflegeabc Account\nHost pflegeabc\n    HostName github.com\n    IdentityFile ~/.ssh/pflegeabc/id_ed25519\n",
    )
    .unwrap();

    // Without --all it only lists.
    sb.cmd().arg("import").assert().success().stdout(
        predicate::str::contains("Privater Account (privat) Default")
            .and(predicate::str::contains("pflegeabc → github.com"))
            .and(predicate::str::contains("keybund import --all")),
    );
    assert!(!sb.path(".config/keybund/profiles.toml").exists());

    sb.cmd().args(["import", "--all", "--yes"]).assert().success().stdout(
        predicate::str::contains("2 Profil(e) angelegt: privat, pflegeabc")
            .and(predicate::str::contains("Übernommene Einträge aus ~/.ssh/config entfernen"))
            .and(
                predicate::str::contains("-IdentityFile ~/.ssh/privat_ed25519")
                    .or(predicate::str::contains("-    IdentityFile ~/.ssh/privat_ed25519")),
            ),
    );
    assert_eq!(sb.read(".ssh/config"), "Include ~/.ssh/config.d/keybund.conf\n");
    assert!(sb.read(".ssh/config.keybund-backup").contains("Privater Account"));
    assert!(sb.read(".ssh/config.d/keybund.conf").contains("Host pflegeabc"));
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("Privater Account (privat) default"));
    sb.cmd().arg("import").assert().success().stdout(predicate::str::contains("Nichts zu übernehmen"));
}

#[test]
fn uninstall_and_reinstall() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.path(".ssh")).unwrap();
    std::fs::write(sb.path(".ssh/config"), "Host nas\n    User admin\n").unwrap();
    sb.new_profile("P", &["--provider", "github", "--folder", "~/dev/p", "--git-name", "T", "--git-email", "t@p.de"]);
    assert!(sb.read(".ssh/config").starts_with("Include"));

    sb.cmd().arg("uninstall").assert().code(2).stderr(predicate::str::contains("--yes"));
    sb.cmd().args(["uninstall", "--yes"]).assert().success().stdout(
        predicate::str::contains("-Include ~/.ssh/config.d/keybund.conf")
            .and(predicate::str::contains("Keybund entfernt")),
    );
    assert_eq!(sb.read(".ssh/config"), "Host nas\n    User admin\n");
    assert!(!sb.path(".ssh/config.d/keybund.conf").exists());
    assert!(sb.path(".ssh/p_ed25519").exists());
    sb.cmd().args(["uninstall", "--yes"]).assert().success().stdout(predicate::str::contains("Nichts zu entfernen"));

    // The profiles survived: apply sets everything up again.
    sb.cmd().args(["apply", "--yes"]).assert().success();
    assert!(sb.read(".ssh/config").starts_with("Include"));
}

#[test]
fn edit_changes_folders_hosts_identity_and_key() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "Kunde",
        &["--provider", "github", "--folder", "~/dev/kunde", "--git-name", "T", "--git-email", "t@k.de"],
    );

    // Without options: shows the profile.
    sb.cmd().args(["edit", "kunde"]).assert().success().stdout(predicate::str::contains("Nichts geändert"));

    sb.cmd()
        .args([
            "edit",
            "kunde",
            "--add-folder",
            "~/work/kunde",
            "--remove-folder",
            "~/dev/kunde",
            "--git-email",
            "neu@k.de",
            "--sign",
            "--name",
            "Kunde GmbH",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("+ Ordner ~/work/kunde/").and(predicate::str::contains("geschrieben")));
    let toml = sb.read(".config/keybund/profiles.toml");
    assert!(toml.contains("name = \"Kunde GmbH\""));
    assert!(toml.contains("\"~/work/kunde/\"") && !toml.contains("~/dev/kunde/"));
    let index = sb.read(".config/keybund/git/index.gitconfig");
    assert!(index.contains("gitdir:~/work/kunde/") && !index.contains("gitdir:~/dev/kunde/"));
    let gitconfig = sb.read(".config/keybund/git/kunde.gitconfig");
    assert!(gitconfig.contains("email = neu@k.de") && gitconfig.contains("gpgsign = true"));

    // Server host with alias, then remove it again.
    sb.cmd()
        .args(["edit", "kunde", "--add-host", "10.0.0.5", "--alias", "kundenserver", "--user", "admin", "--yes"])
        .assert()
        .success();
    assert!(sb.read(".ssh/config.d/keybund.conf").contains("Host kundenserver 10.0.0.5\n    HostName 10.0.0.5\n    User admin"));
    sb.cmd().args(["edit", "kunde", "--remove-host", "kundenserver", "--yes"]).assert().success();
    assert!(!sb.read(".ssh/config.d/keybund.conf").contains("kundenserver"));

    // Errors are explained, nothing is saved.
    sb.cmd()
        .args(["edit", "kunde", "--remove-folder", "~/nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("hat den Ordner ~/nope nicht"));
    sb.cmd().args(["edit", "kunde", "--alias", "x"]).assert().code(2).stderr(predicate::str::contains("--add-host"));
    sb.cmd()
        .args(["edit", "kunde", "--key", "~/.ssh/missing"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Key fehlt"));
    sb.new_profile(
        "Andere",
        &["--provider", "github", "--folder", "~/dev/andere", "--git-name", "T", "--git-email", "t@a.de"],
    );
    sb.cmd()
        .args(["edit", "andere", "--add-folder", "~/work/kunde/sub", "--yes"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("überschneiden sich"));

    // Switch to an existing key.
    sb.cmd().args(["edit", "andere", "--key", "~/.ssh/kunde_ed25519", "--yes"]).assert().success();
    assert!(sb.read(".config/keybund/git/andere.gitconfig").contains("ssh -i ~/.ssh/kunde_ed25519"));
}
