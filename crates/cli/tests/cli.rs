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
        // The result is remembered for `show` and the app.
        sb.cmd().args(["show", "p"]).assert().success().stdout(predicate::str::contains("Letzter Test"));
        assert!(sb.read(".config/keybund/last-tests.json").contains("Verbunden als @octocat"));
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
        sb.cmd()
            .args(["test", "server", "--host", "1.2.3.4"])
            .assert()
            .success()
            .stdout(predicate::str::contains("srv: Anmeldung erfolgreich"));
        sb.cmd()
            .args(["test", "server", "--host", "nas"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("Profil „server“ hat keinen Host nas"));
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

/// A GitHub API that knows the user "octo" and accepts one new key.
fn github_api(server: &mut mockito::Server, token: &str) -> mockito::Mock {
    let auth = format!("Bearer {token}");
    server.mock("GET", "/user").match_header("authorization", auth.as_str()).with_body(r#"{"login":"octo"}"#).create();
    server
        .mock("GET", "/user")
        .match_header("authorization", "Bearer old")
        .with_status(401)
        .with_body(r#"{"message":"Bad credentials"}"#)
        .create();
    server.mock("GET", "/user/keys").match_query(mockito::Matcher::Any).with_body("[]").create();
    server
        .mock("POST", "/user/keys")
        .match_header("authorization", auth.as_str())
        .match_body(mockito::Matcher::PartialJsonString(r#"{"title":"t@g (Keybund)"}"#.into()))
        .with_status(201)
        .with_body("{}")
        .create()
}

#[test]
fn install_key_uploads_to_github_with_a_token() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "Git",
        &[
            "--provider",
            "github",
            "--folder",
            "~/dev/g",
            "--git-name",
            "T",
            "--git-email",
            "t@g.de",
            "--comment",
            "t@g",
        ],
    );
    let mut server = mockito::Server::new();
    let add = github_api(&mut server, "ghp_secret");
    #[cfg(unix)]
    {
        let store = sb.fake_secret_tool();
        sb.cmd()
            .env("KEYBUND_API_URL", server.url())
            .args(["install-key", "git", "--token-stdin", "--remember"])
            .write_stdin("ghp_secret\n")
            .assert()
            .success()
            .stdout(predicate::str::contains("Key bei GitHub eingetragen (@octo)"));
        add.assert();
        assert_eq!(std::fs::read_to_string(&store).unwrap(), "ghp_secret");
        let log = std::fs::read_to_string(store.with_extension("log")).unwrap();
        assert!(
            log.contains("store --label=Keybund: GitHub-Token für Git application keybund profile git host github.com"),
            "{log}"
        );
        assert!(!log.contains("ghp_secret"), "the token never goes on the command line");

        // Next time the stored token is used.
        sb.cmd().env("KEYBUND_API_URL", server.url()).args(["install-key", "git"]).assert().success();

        // An expired token is forgotten; without a browser login a new token is needed.
        std::fs::write(&store, "old").unwrap();
        sb.cmd()
            .env("KEYBUND_API_URL", server.url())
            .args(["install-key", "git"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("Das gespeicherte Token gilt nicht mehr"))
            .stdout(predicate::str::contains("settings/tokens/new?scopes=write:public_key"));
        assert!(!store.exists());
    }
}

#[test]
fn install_key_logs_in_with_the_device_flow() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile(
        "Git",
        &[
            "--provider",
            "github",
            "--folder",
            "~/dev/g",
            "--git-name",
            "T",
            "--git-email",
            "t@g.de",
            "--comment",
            "t@g",
        ],
    );
    let mut server = mockito::Server::new();
    server
        .mock("POST", "/login/device/code")
        .match_body(mockito::Matcher::UrlEncoded("client_id".into(), "keybund-test".into()))
        .with_body(r#"{"device_code":"dc","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":60,"interval":0}"#)
        .create();
    server.mock("POST", "/login/oauth/access_token").with_body(r#"{"access_token":"gho_device"}"#).create();
    let add = github_api(&mut server, "gho_device");
    sb.cmd()
        .env("KEYBUND_API_URL", server.url())
        .env("KEYBUND_GITHUB_CLIENT_ID", "keybund-test")
        .args(["install-key", "git"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Code eingeben: WDJB-MJHT"))
        .stdout(predicate::str::contains("mit dem Konto für „Git“"))
        .stdout(predicate::str::contains("Key bei GitHub eingetragen (@octo)"));
    add.assert();
}

#[test]
fn install_key_explains_hosts_without_api_login() {
    if !has_tools() {
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
        ],
    );
    sb.cmd()
        .args(["install-key", "firma"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Für gitlab.firma.de gibt es keine Anmeldung per Browser"))
        .stderr(predicate::str::contains(
            "https://gitlab.firma.de/-/user_settings/personal_access_tokens?name=Keybund&scopes=api",
        ));
    sb.cmd()
        .args(["install-key", "firma", "--token-stdin"])
        .write_stdin("\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Kein Token"));

    sb.new_profile(
        "Bb",
        &["--provider", "bitbucket", "--folder", "~/dev/b", "--git-name", "T", "--git-email", "t@b.de"],
    );
    sb.cmd()
        .args(["install-key", "bb"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Bitbucket kann Keybund nicht automatisch eintragen"))
        .stdout(predicate::str::contains("bitbucket.org/account/settings/ssh-keys"));
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
            .code(2)
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
