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
        .stderr(predicate::str::contains("Mehrere Profile nutzen github.com"));
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
