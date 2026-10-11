//! Integration tests: one command at a time against a temporary HOME.

mod common;

use std::fs;

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
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("Noch keine Keys"));
}

#[test]
fn new_requires_flags_without_terminal() {
    let sb = Sandbox::new();
    sb.cmd().args(["new"]).assert().code(2).stderr(predicate::str::contains("--name fehlt"));
}

#[test]
fn new_follows_the_global_git_identity() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    std::fs::write(sb.path(".gitconfig"), "[user]\n\tname = Tristan\n\temail = t@privat.de\n").unwrap();
    // Same as ~/.gitconfig: nothing stored, the key follows it.
    sb.new_profile("Privat", &["--provider", "github", "--folder", "~/dev/privat"]);
    assert!(!sb.read(".config/keybund/profiles.toml").contains("[profile.git]"));
    assert!(!sb.read(".config/keybund/git/privat.gitconfig").contains("[user]"));
    // Another e-mail for a customer: only that is different.
    sb.new_profile("Kunde", &["--provider", "github", "--folder", "~/dev/kunde", "--git-email", "t@kunde.de"]);
    let kunde = sb.read(".config/keybund/git/kunde.gitconfig");
    assert!(kunde.contains("name = Tristan") && kunde.contains("email = t@kunde.de"), "{kunde}");
    sb.cmd()
        .args(["show", "privat"])
        .assert()
        .success()
        .stdout(predicate::str::contains("t@privat.de (aus ~/.gitconfig)"));
    sb.cmd()
        .args(["edit", "kunde", "--sign", "--git-email", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("soll Commits signieren, hat aber keine eigene E-Mail"));
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
    assert!(!toml.contains("default_profile"), "the standard key is only set on request");
    sb.cmd().args(["default", "kunde-c"]).assert().success();
    assert!(sb.read(".config/keybund/profiles.toml").contains("default_profile = \"kunde-c\""));
    assert!(sb.read(".config/keybund/git/index.gitconfig").contains("# Default key: "), "written right away");
    sb.cmd().args(["default", "--unset"]).assert().success();
    assert!(!sb.read(".config/keybund/profiles.toml").contains("default_profile"));
    assert!(!sb.read(".config/keybund/git/index.gitconfig").contains("Default key"));
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
        .stderr(predicate::str::contains("gibt es schon"));
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
    assert!(!sb.path("dev/a/sub").exists(), "nothing created for a rejected folder");
    assert!(!sb.path(".ssh/b_ed25519").exists(), "no key for a rejected entry");
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
    sb.cmd().args(["show", "nope"]).assert().code(2).stderr(predicate::str::contains("Key „nope“ existiert nicht"));
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
        // The result is remembered for `show` and the app.
        sb.cmd().args(["show", "p"]).assert().success().stdout(predicate::str::contains("Letzter Test"));
        // Stored as a message key, so the app shows it in its own language.
        let stored = sb.read(".config/keybund/last-tests.json");
        assert!(stored.contains("probe_connected") && stored.contains("@octocat"), "{stored}");
        sb.cmd().args(["show", "p"]).assert().success().stdout(predicate::str::contains("Verbunden als @octocat"));
        let log = sb.ssh_log();
        assert!(log.contains("IdentitiesOnly=yes"));
        assert!(log.contains("AddKeysToAgent=yes"));
        assert!(!log.contains("BatchMode"));
        assert!(log.contains("StrictHostKeyChecking=yes"));
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
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("B (b) Standard"));
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
            .and(predicate::str::contains("Git-Name       T\n"))
            .and(predicate::str::contains("Git-E-Mail     t@p.de"))
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
                .and(predicate::str::contains("Key            Server"))
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
        sb.cmd()
            .args(["test", "server", "--host", "1.2.3.4"])
            .assert()
            .success()
            .stdout(predicate::str::contains("srv: Anmeldung erfolgreich"));
        sb.cmd()
            .args(["test", "server", "--host", "nas"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("Key „server“ hat keinen Host nas"));
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
        .stderr(predicate::str::contains("Kein Key hat den Host codeberg.org"));
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
        .stderr(predicate::str::contains("Ungültige Key-ID"));
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
fn without_terminal_nothing_is_removed_or_archived_without_yes() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.path(".ssh")).unwrap();
    std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(sb.path(".ssh/work_ed25519"))
        .status()
        .unwrap();
    std::fs::write(sb.path(".ssh/config"), "Host work\n    HostName 10.0.0.1\n    IdentityFile ~/.ssh/work_ed25519\n")
        .unwrap();
    sb.cmd().args(["import", "--all", "--keep-old", "--yes"]).assert().success();
    // A later plain import must not delete the user's block.
    sb.cmd().arg("import").assert().success().stdout(predicate::str::contains("Ohne Terminal nur mit --yes"));
    assert!(sb.read(".ssh/config").contains("Host work"));
    sb.cmd()
        .args(["archive", "work"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Ohne Terminal nur mit --yes"));
    assert!(sb.path(".ssh/work_ed25519").exists());
}

#[test]
fn import_takes_over_an_existing_ssh_config() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.path(".ssh/nebenjob")).unwrap();
    for key in [".ssh/privat_ed25519", ".ssh/nebenjob/id_ed25519"] {
        std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(sb.path(key))
            .status()
            .unwrap();
    }
    std::fs::write(
        sb.path(".ssh/config"),
        "# Privater Account\nHost github.com\n    User git\n    IdentityFile ~/.ssh/privat_ed25519\n\n# Nebenjob Account\nHost nebenjob\n    HostName github.com\n    IdentityFile ~/.ssh/nebenjob/id_ed25519\n",
    )
    .unwrap();

    // Without --all it only lists.
    sb.cmd().arg("import").assert().success().stdout(
        predicate::str::contains("Privater Account (privat) Default")
            .and(predicate::str::contains("nebenjob → github.com"))
            .and(predicate::str::contains("keybund import --all")),
    );
    assert!(!sb.path(".config/keybund/profiles.toml").exists());

    sb.cmd().args(["import", "--all", "--yes"]).assert().success().stdout(
        predicate::str::contains("2 Key(s) übernommen: privat, nebenjob")
            .and(predicate::str::contains("Übernommene Einträge aus ~/.ssh/config entfernen"))
            .and(
                predicate::str::contains("-IdentityFile ~/.ssh/privat_ed25519")
                    .or(predicate::str::contains("-    IdentityFile ~/.ssh/privat_ed25519")),
            ),
    );
    assert_eq!(sb.read(".ssh/config"), "Include ~/.ssh/config.d/keybund.conf\n");
    assert!(sb.read(".ssh/config.keybund-backup").contains("Privater Account"));
    assert!(sb.read(".ssh/config.d/keybund.conf").contains("Host nebenjob"));
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("Privater Account (privat) Standard"));
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
    assert!(sb
        .read(".ssh/config.d/keybund.conf")
        .contains("Host kundenserver 10.0.0.5\n    HostName 10.0.0.5\n    User admin"));
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

#[test]
fn install_key_runs_ssh_with_the_key_on_stdin() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("NAS", &["--provider", "generic", "--host", "192.168.1.5", "--alias", "nas", "--user", "admin"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Fake ssh: records its arguments and stdin, answers like the remote script.
        let log = sb.dir.path().join("install.log");
        let script = sb.bin.join("ssh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho \"$@\" > '{0}'\ncat >> '{0}'\necho KEYBUND_ADDED\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        sb.cmd()
            .args(["install-key", "nas"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Key auf nas eingetragen"));
        let logged = std::fs::read_to_string(&log).unwrap();
        assert!(logged.contains("IdentitiesOnly=no"));
        assert!(logged.contains("admin@192.168.1.5"));
        assert!(logged.contains(sb.read(".ssh/nas_ed25519.pub").trim()), "public key goes over stdin");

        sb.cmd()
            .args(["install-key", "nas", "--host", "other"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("keinen Host other"));
    }
}

#[test]
fn install_key_opens_the_page_for_git_hosts() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "Firma",
        &[
            "--provider",
            "gitlab",
            "--host",
            "gitlab.firma.de",
            "--folder",
            "~/dev/f",
            "--git-name",
            "T",
            "--git-email",
            "t@f.de",
            "--comment",
            "t@f",
        ],
    );
    #[cfg(unix)]
    {
        let opened = sb.fake_xdg_open();
        // Without a desktop there is no clipboard: the pubkey is printed instead.
        sb.cmd()
            .args(["install-key", "firma"])
            .assert()
            .success()
            .stdout(predicate::str::contains("ssh-ed25519 "))
            .stdout(predicate::str::contains(
                "Bei GitLab als neuen SSH-Key einfügen: https://gitlab.firma.de/-/user_settings/ssh_keys",
            ))
            .stdout(predicate::str::contains("keybund test firma --host gitlab.firma.de"));
        assert_eq!(std::fs::read_to_string(opened).unwrap().trim(), "https://gitlab.firma.de/-/user_settings/ssh_keys");
    }
}

#[test]
fn config_changes_the_defaults_for_new_profiles() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.cmd()
        .args(["config"])
        .assert()
        .success()
        .stdout(predicate::str::contains("projects-dir").and(predicate::str::contains("~/dev")));
    sb.cmd().args(["config", "projects-dir", "~/work/"]).assert().success();
    sb.cmd().args(["config", "keys-dir", "~/.ssh/keybund"]).assert().success();
    sb.cmd()
        .args(["config", "known-hosts-per-profile", "nein"])
        .assert()
        .success()
        .stdout(predicate::str::contains("keybund apply"));
    sb.cmd()
        .args(["config", "projects-dir", "relativ"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Ungültiger Ordner"));
    sb.cmd()
        .args(["config", "absolute-ssh-include", "vielleicht"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("kein Ja/Nein-Wert"));
    sb.cmd().args(["config", "farbe", "x"]).assert().code(2).stderr(predicate::str::contains("Unbekannte Einstellung"));
    sb.cmd().args(["config", "keys-dir"]).assert().code(2).stderr(predicate::str::contains("Wert fehlt"));
    let toml = sb.read(".config/keybund/profiles.toml");
    assert!(toml.contains("projects_dir = \"~/work\""), "{toml}");
    assert!(toml.contains("known_hosts_per_profile = false"), "{toml}");

    sb.new_profile(
        "Kunde X",
        &["--provider", "github", "--git-name", "T", "--git-email", "t@x.de", "--folder", "~/work/kunde-x"],
    );
    assert!(sb.path(".ssh/keybund/kunde-x_ed25519").exists());
}

#[test]
fn unknown_host_keys_are_confirmed_first() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("Server", &["--provider", "generic", "--host", "10.1.2.3", "--alias", "srv", "--port", "2222"]);
    #[cfg(unix)]
    {
        sb.fake_ssh("", 0);
        sb.fake_keyscan(
            "[10.1.2.3]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl",
        );
        sb.cmd()
            .args(["test", "server"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("Erste Verbindung zu [10.1.2.3]:2222"))
            .stdout(predicate::str::contains("SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"))
            .stderr(predicate::str::contains("keybund trust server --host srv"));
        assert!(!sb.ssh_log().contains(" true"), "no connection before the key is confirmed");

        sb.cmd()
            .args(["trust", "server", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("in ~/.ssh/known_hosts.d/server gespeichert"));
        assert!(sb.read(".ssh/known_hosts.d/server").starts_with("[10.1.2.3]:2222 ssh-ed25519 AAAA"));
        sb.cmd().args(["trust", "server"]).assert().success().stdout(predicate::str::contains("Host-Key ist bekannt"));
        sb.cmd().args(["test", "server"]).assert().success();
        assert!(sb.ssh_log().contains("StrictHostKeyChecking=yes"));

        // The Host block keeps new keys of this server in the profile's file.
        assert!(sb
            .read(".ssh/config.d/keybund.conf")
            .contains("UserKnownHostsFile ~/.ssh/known_hosts.d/server ~/.ssh/known_hosts"));

        // Archiving takes the file along.
        sb.cmd().args(["archive", "server", "--yes"]).assert().success();
        assert!(!sb.path(".ssh/known_hosts.d/server").exists());
        assert!(sb.path(".ssh/archive/known_hosts-server").exists());
    }
}

/// A script or a pipe cannot answer a question: without --yes nothing of the
/// user's files is written, as the documentation promises.
#[test]
fn without_a_terminal_the_users_files_need_yes() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.cmd()
        .args(["new", "--name", "Foo", "--provider", "github", "--host", "github.com", "--no-browser"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ohne Terminal nur mit --yes"));
    assert!(sb.path(".ssh/foo_ed25519").exists(), "the key and the profile are created");
    assert!(sb.read(".config/keybund/profiles.toml").contains("id = \"foo\""));
    assert!(!sb.read(".ssh/config").contains("keybund"), "but nothing of the user's files is touched");
    assert!(!sb.path(".gitconfig").exists());
    sb.cmd().args(["edit", "foo", "--add-folder", "~/dev/foo"]).assert().success();
    assert!(!sb.path(".gitconfig").exists());
    sb.cmd().args(["apply", "--yes"]).assert().success();
    assert!(sb.read(".ssh/config").starts_with("Include ~/.ssh/config.d/keybund.conf\n"));
}

/// A refused key leaves nothing behind – no folder, no profile.
#[test]
fn a_refused_key_creates_no_folders() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    fs::create_dir_all(sb.path(".ssh")).unwrap();
    fs::write(sb.path(".ssh/foo_ed25519"), "not really a key").unwrap();
    sb.cmd()
        .args([
            "new",
            "--name",
            "Foo",
            "--provider",
            "github",
            "--host",
            "github.com",
            "--folder",
            "~/dev/foo",
            "--yes",
            "--no-browser",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("existiert bereits"));
    assert!(!sb.path("dev/foo").exists());
    assert!(!sb.path(".config/keybund/profiles.toml").exists());
}

/// `import --all` takes what it can: a block whose key file is gone is
/// skipped with a word, the others become keys.
#[test]
fn import_all_skips_blocks_without_a_key_file() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    fs::create_dir_all(sb.path(".ssh")).unwrap();
    assert!(std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(sb.path(".ssh/work_ed25519"))
        .status()
        .unwrap()
        .success());
    fs::write(
        sb.path(".ssh/config"),
        "Host work\n    HostName gitlab.example.com\n    User git\n    IdentityFile ~/.ssh/work_ed25519\n\nHost old\n    HostName old.example.com\n    IdentityFile ~/.ssh/gone_ed25519\n",
    )
    .unwrap();
    sb.cmd()
        .args(["import", "--all", "--yes", "--keep-old"])
        .assert()
        .success()
        .stdout(predicate::str::contains("übersprungen").and(predicate::str::contains("gone_ed25519")))
        .stdout(predicate::str::contains("1 Key(s) übernommen"));
    assert!(sb.read(".config/keybund/profiles.toml").contains("work_ed25519"));
}
