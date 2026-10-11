//! End-to-end: the complete life cycle of two customer projects on the
//! same host, driven only through the `keybund` binary and checked with the
//! real git.

mod common;

use std::fs;

use common::{has_tools, Sandbox};
use predicates::prelude::*;

#[test]
fn two_customers_on_github_full_lifecycle() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    fs::create_dir_all(sb.path(".ssh")).unwrap();
    fs::write(sb.path(".ssh/config"), "Host github.com\n    IdentityFile ~/.ssh/id_rsa\n").unwrap();
    fs::write(sb.path(".gitconfig"), "[user]\n\tname = Global\n\temail = global@example.org\n").unwrap();

    // 1. Two projects on github.com, a private default and a server.
    sb.new_profile(
        "Privat",
        &[
            "--provider",
            "github",
            "--folder",
            "~/dev/privat",
            "--git-name",
            "T",
            "--git-email",
            "t@privat.de",
            "--sign",
            "--default",
        ],
    );
    sb.new_profile(
        "Kunde A",
        &["--provider", "github", "--folder", "~/dev/kunde-a", "--git-name", "T", "--git-email", "t@kunde-a.de"],
    );
    sb.new_profile(
        "Kunde B",
        &["--provider", "github", "--folder", "~/dev/kunde-b", "--git-name", "T", "--git-email", "t@kunde-b.de"],
    );
    sb.new_profile(
        "Server",
        &["--provider", "generic", "--host", "10.0.0.5", "--alias", "meinserver", "--user", "admin", "--port", "2222"],
    );

    sb.cmd().arg("list").assert().success().stdout(
        predicate::str::contains("Kunde A (kunde-a)")
            .and(predicate::str::contains("meinserver → 10.0.0.5"))
            .and(predicate::str::contains("SHA256:")),
    );

    // 2. The user's own files got exactly one include line each, plus a backup.
    let ssh = sb.read(".ssh/config");
    assert!(ssh.starts_with("Include ~/.ssh/config.d/keybund.conf\n"));
    assert!(ssh.contains("Host github.com\n    IdentityFile ~/.ssh/id_rsa"));
    assert!(sb.path(".ssh/config.keybund-backup").exists());
    assert!(sb.read(".gitconfig").starts_with("[user]\n\tname = Global"));

    // 3. Git picks the right key and identity per folder.
    for (id, mail) in [("kunde-a", "t@kunde-a.de"), ("kunde-b", "t@kunde-b.de"), ("privat", "t@privat.de")] {
        let repo = sb.path(&format!("dev/{id}/repo"));
        fs::create_dir_all(&repo).unwrap();
        sb.git(&repo, &["init", "-q"]);
        assert_eq!(
            sb.git(&repo, &["config", "core.sshCommand"]),
            format!("ssh -i ~/.ssh/{id}_ed25519 -o IdentitiesOnly=yes -o AddKeysToAgent=yes")
        );
        assert_eq!(sb.git(&repo, &["config", "user.email"]), mail);
    }
    assert_eq!(sb.git(&sb.path("dev/privat/repo"), &["config", "commit.gpgsign"]), "true");

    // 4. which explains the resolution.
    sb.cmd().args(["which", "~/dev/kunde-b/repo"]).assert().success().stdout(
        predicate::str::contains("~/.ssh/kunde-b_ed25519")
            .and(predicate::str::contains("includeIf \"gitdir:~/dev/kunde-b/\""))
            .and(predicate::str::contains("t@kunde-b.de")),
    );

    // 5. doctor finds the pre-existing global github.com entry.
    sb.cmd().arg("doctor").assert().code(1).stdout(predicate::str::contains("Globaler Eintrag für github.com"));

    // 6. Clone into the right profile (git's insteadOf redirects to a local remote).
    let remote = sb.dir.path().join("remote");
    fs::create_dir_all(&remote).unwrap();
    sb.git(&remote, &["init", "-q", "--bare", "portal.git"]);
    let global = sb.read(".gitconfig");
    fs::write(
        sb.path(".gitconfig"),
        format!("{global}[url \"{}/\"]\n\tinsteadOf = git@github.com:team/\n", remote.display()),
    )
    .unwrap();
    sb.cmd()
        .args(["clone", "git@github.com:team/portal.git"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Mehrere Keys nutzen github.com"));
    sb.cmd().args(["clone", "git@github.com:team/portal.git", "--profile", "kunde-a"]).assert().success();
    let cloned = sb.path("dev/kunde-a/portal");
    assert!(cloned.join(".git").is_dir());
    assert_eq!(sb.git(&cloned, &["config", "user.email"]), "t@kunde-a.de");

    // 7. Offboarding Kunde B.
    sb.cmd()
        .args(["archive", "kunde-b", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("https://github.com/settings/keys"));
    assert!(sb.path(".ssh/archive/kunde-b_ed25519").exists());
    assert!(!sb.path(".config/keybund/git/kunde-b.gitconfig").exists());
    assert!(!sb.read(".config/keybund/git/index.gitconfig").contains("kunde-b"));
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("Kunde B").not());
    sb.cmd().args(["list", "--all"]).assert().success().stdout(predicate::str::contains("archiviert"));

    // 8. Everything is in sync; a second apply changes nothing.
    sb.cmd().args(["apply", "--dry-run"]).assert().success().stdout(predicate::str::contains("Configs sind aktuell"));
}

#[test]
fn doctor_fix_repairs_permissions_and_include() {
    if !has_tools() || cfg!(not(unix)) {
        return;
    }
    let sb = Sandbox::new();
    sb.new_profile("P", &["--provider", "github", "--folder", "~/dev/p", "--git-name", "T", "--git-email", "t@p.de"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(sb.path(".ssh/p_ed25519"), fs::Permissions::from_mode(0o644)).unwrap();
        let ssh = sb.read(".ssh/config").replace("Include ~/.ssh/config.d/keybund.conf\n", "");
        fs::write(sb.path(".ssh/config"), format!("Host x\n    User y\n{ssh}Include ~/.ssh/config.d/keybund.conf\n"))
            .unwrap();

        sb.cmd().arg("doctor").assert().code(1).stdout(
            predicate::str::contains("Include steht nicht ganz oben")
                .and(predicate::str::contains("zu offenen Rechten")),
        );
        // The sandbox has no ssh-agent, so doctor still reports that one hint.
        sb.cmd().args(["doctor", "--fix"]).assert().stdout(
            predicate::str::contains("✓ Include steht ganz oben")
                .and(predicate::str::contains("✓ Private Keys haben Rechte 600"))
                .and(predicate::str::contains("✓ Configs sind aktuell")),
        );
        assert_eq!(fs::metadata(sb.path(".ssh/p_ed25519")).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(sb.read(".ssh/config").starts_with("Include ~/.ssh/config.d/keybund.conf\n"));
    }
}

/// A real user with a NAS: from the first contact (host key) over putting the
/// key on the server to the connection test and git over ssh – all through
/// the binary, against a throw-away `sshd`, with the generated `Host` block
/// doing the work. Plus the folder rules a real checkout tree exercises:
/// symlinks, a sibling folder with the same prefix, repos outside every folder.
#[test]
#[cfg(unix)]
fn a_server_from_first_contact_to_git_over_ssh() {
    use common::{sshd_path, start_sshd, user};
    if !has_tools() {
        return;
    }
    let Some(sshd) = sshd_path() else { return };
    let sb = Sandbox::new();
    sb.ssh_shim();
    sb.real_keyscan();
    fs::create_dir_all(sb.path(".ssh")).unwrap();
    fs::write(sb.path(".gitconfig"), "[user]\n\tname = Global\n\temail = global@example.org\n").unwrap();

    // The user already reaches the server with their old default key.
    let old_key = sb.path(".ssh/id_ed25519");
    assert!(std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "old", "-f"])
        .arg(&old_key)
        .status()
        .unwrap()
        .success());
    let server = sb.dir.path().join("server");
    fs::create_dir_all(&server).unwrap();
    let authorized = server.join("authorized_keys");
    fs::write(&authorized, sb.read(".ssh/id_ed25519.pub")).unwrap();
    let (_sshd, port) = start_sshd(&sshd, &server, &authorized);
    let port_s = port.to_string();
    let me = user();

    // 1. A key for the NAS, a private default with a folder.
    sb.new_profile(
        "Privat",
        &["--provider", "github", "--folder", "~/dev/privat", "--git-email", "t@privat.de", "--default"],
    );
    sb.new_profile(
        "NAS",
        &["--provider", "generic", "--host", "127.0.0.1", "--alias", "nas", "--user", &me, "--port", &port_s],
    );
    let nas_pub = sb.read(".ssh/nas_ed25519.pub");
    assert!(nas_pub.starts_with("ssh-ed25519 "));

    // 2. ssh resolves the alias and the IP to the same key, user and port.
    for target in ["nas", "127.0.0.1", &format!("{me}@127.0.0.1")] {
        let g = sb.ssh_g(target);
        assert!(g.contains(&format!("\nuser {me}\n")), "{target}: {g}");
        assert!(g.contains("\nhostname 127.0.0.1\n"), "{target}: {g}");
        assert!(g.contains(&format!("\nport {port}\n")), "{target}: {g}");
        assert!(g.contains("\nidentitiesonly yes\n"), "{target}: {g}");
        assert!(g.contains(&format!("\nidentityfile {}\n", sb.path(".ssh/nas_ed25519").display())), "{target}: {g}");
        // The server's host key goes to the key's own file; the global one still counts.
        assert!(
            g.contains(&format!(
                "\nuserknownhostsfile {} {}\n",
                sb.path(".ssh/known_hosts.d/nas").display(),
                sb.path(".ssh/known_hosts").display()
            )),
            "{target}: {g}"
        );
    }
    // Other hosts keep ssh's defaults: no Keybund key leaks there.
    assert!(sb.ssh_g("example.org").contains("\nidentitiesonly no\n"));

    // 3. First contact: without a terminal the host key must be confirmed explicitly.
    sb.cmd().args(["test", "nas"]).assert().code(1).stderr(predicate::str::contains("keybund trust nas"));
    assert!(!sb.path(".ssh/known_hosts.d/nas").exists());
    sb.cmd()
        .args(["trust", "nas", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA256:").and(predicate::str::contains("known_hosts.d/nas")));
    let known = sb.read(".ssh/known_hosts.d/nas");
    assert!(known.starts_with(&format!("[127.0.0.1]:{port} ssh-ed25519 ")), "{known}");

    // 4. The server does not know the new key yet: the test says so and points to install-key.
    sb.cmd().args(["test", "nas"]).assert().code(1).stdout(
        predicate::str::contains("Server kennt diesen Key nicht").and(predicate::str::contains("install-key nas")),
    );

    // 5. install-key logs in with what already works and appends the key once.
    sb.cmd().args(["install-key", "nas"]).assert().success().stdout(predicate::str::contains("eingetragen"));
    let lines: Vec<String> = fs::read_to_string(&authorized).unwrap().lines().map(String::from).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[1], nas_pub.trim());
    sb.cmd().args(["install-key", "nas"]).assert().success().stdout(predicate::str::contains("schon eingetragen"));
    assert_eq!(fs::read_to_string(&authorized).unwrap().lines().count(), 2);

    // 6. Now the connection test proves this key logs in, and remembers it.
    sb.cmd().args(["test", "nas"]).assert().success().stdout(predicate::str::contains("Anmeldung erfolgreich"));
    let last = sb.read(".config/keybund/last-tests.json");
    assert!(last.contains("\"nas\"") && last.contains("\"success\": true"), "{last}");
    sb.cmd().arg("list").assert().success().stdout(predicate::str::contains("nas → 127.0.0.1"));

    // 7. git over ssh through the alias: the generated Host block carries key, user and port.
    let remote = server.join("remote.git");
    sb.git(&server, &["init", "-q", "--bare", "remote.git"]);
    sb.git(&sb.home, &["ls-remote", &format!("ssh://nas{}", remote.display())]);

    // 8. Access revoked on the server: the test fails again instead of pretending.
    fs::write(&authorized, sb.read(".ssh/id_ed25519.pub")).unwrap();
    sb.cmd().args(["test", "nas"]).assert().code(1).stdout(predicate::str::contains("Server kennt diesen Key nicht"));
    assert!(sb.read(".config/keybund/last-tests.json").contains("\"success\": false"));

    // 9. Folder rules on a real checkout tree.
    let repo = |rel: &str| {
        let dir = sb.path(rel);
        fs::create_dir_all(&dir).unwrap();
        sb.git(&dir, &["init", "-q"]);
        dir
    };
    let privat = repo("dev/privat/app");
    let sibling = repo("dev/privat-old/app"); // same prefix, not inside the folder
    let outside = repo("Downloads/quick");
    std::os::unix::fs::symlink(sb.path("dev/privat"), sb.path("work")).unwrap();
    sb.git(&privat, &["commit", "-q", "--allow-empty", "-m", "init"]);
    sb.git(&privat, &["worktree", "add", "-q", &sb.path("dev/privat/app-wt").display().to_string()]);
    let email = |dir: &std::path::Path| sb.git(dir, &["config", "user.email"]);
    assert_eq!(email(&privat), "t@privat.de");
    assert_eq!(email(&sb.path("work/app")), "t@privat.de", "through a symlink");
    assert_eq!(email(&sb.path("dev/privat/app-wt")), "t@privat.de", "in a linked worktree");
    assert_eq!(email(&sibling), "global@example.org", "a sibling with the same prefix");
    assert_eq!(email(&outside), "global@example.org");
    // The default key applies to git everywhere, the folder key inside the folder.
    assert!(sb.git(&outside, &["config", "core.sshCommand"]).contains("privat_ed25519"));
    assert!(sb.git(&sibling, &["config", "core.sshCommand"]).contains("privat_ed25519"));
    sb.cmd()
        .args(["which", "~/work/app"])
        .assert()
        .success()
        .stdout(predicate::str::contains("includeIf \"gitdir:~/dev/privat/\""));
    sb.cmd().args(["which", "nas"]).assert().success().stdout(
        predicate::str::contains("nas_ed25519").and(predicate::str::contains(format!("{me}@127.0.0.1:{port}"))),
    );
}

/// The app and the CLI, or a setup script and the user, writing at the same
/// time: every profile survives and every config is written (a file lock
/// serializes load → change → save → apply across processes).
#[test]
fn parallel_runs_do_not_lose_profiles() {
    if !has_tools() {
        return;
    }
    let sb = Sandbox::new();
    let children: Vec<_> = (0..6)
        .map(|n| {
            let name = format!("Kunde {n}");
            let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin("keybund"));
            cmd.args(["new", "--name", &name, "--provider", "github", "--host", "github.com", "--yes", "--no-browser"]);
            cmd.env("HOME", &sb.home)
                .env("PATH", format!("{}:{}", sb.bin.display(), std::env::var("PATH").unwrap_or_default()))
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env_remove("SSH_AUTH_SOCK")
                .env_remove("DISPLAY");
            cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).spawn().unwrap()
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    let profiles = sb.read(".config/keybund/profiles.toml");
    assert_eq!(profiles.matches("[[profile]]").count(), 6, "{profiles}");
    for n in 0..6 {
        assert!(sb.path(&format!(".config/keybund/git/kunde-{n}.gitconfig")).exists(), "kunde-{n}");
        assert!(sb.path(&format!(".ssh/kunde-{n}_ed25519")).exists(), "key {n}");
    }
    sb.cmd().args(["apply", "--dry-run"]).assert().success().stdout(predicate::str::contains("Configs sind aktuell"));
}
