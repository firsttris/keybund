//! End-to-end: one person, five keys – a private GitHub account (the
//! default), two companies on GitHub with a folder each, and two servers –
//! with every connection real. A small GitHub stand-in decides the account by
//! the key, like GitHub does, and an ssh-agent holds all five keys in the
//! worst order. Without Keybund that agent logs in as the wrong account; the
//! test shows that first, then that every folder, account and server gets its
//! own key.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use common::{has_tools, Killed, Sandbox};
use predicates::prelude::*;

/// What the sandbox's processes see, plus the test's ssh-agent.
fn run(sb: &Sandbox, sock: &Path, program: &str, dir: &Path, args: &[&str]) -> Output {
    let path = format!("{}:{}", sb.bin.display(), std::env::var("PATH").unwrap_or_default());
    Command::new(program)
        .current_dir(dir)
        .args(args)
        .env("HOME", &sb.home)
        .env("PATH", path)
        .env("SSH_AUTH_SOCK", sock)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("NO_COLOR", "1")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("GIT_CONFIG_GLOBAL")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("SSH_ASKPASS")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[track_caller]
fn ok(out: Output) -> String {
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[track_caller]
fn failed(out: Output) -> String {
    assert!(!out.status.success(), "should have failed: {}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// GitHub's rule in a few lines: the key decides the account
/// (`command="gate <account>"` in authorized_keys), an account sees only its
/// own repositories, and a login without a command gets the greeting.
fn gate_script(root: &Path) -> String {
    format!(
        r#"#!/bin/sh
account=$1
root='{root}'
echo "$account" >> "$root/logins"
case "$SSH_ORIGINAL_COMMAND" in
  git-upload-pack\ *|git-receive-pack\ *)
    service=${{SSH_ORIGINAL_COMMAND%% *}}
    repo=$(printf '%s' "${{SSH_ORIGINAL_COMMAND#* }}" | tr -d "'" | sed 's|^/||')
    case "$repo" in "$account"/*) ;; *) echo 'ERROR: Repository not found.' >&2; exit 1;; esac
    [ -d "$root/$repo" ] || {{ echo 'ERROR: Repository not found.' >&2; exit 1; }}
    exec "$service" "$root/$repo" ;;
  *)
    echo "Hi $account! You've successfully authenticated, but GitHub does not provide shell access." >&2
    exit 1 ;;
esac
"#,
        root = root.display()
    )
}

fn host_key(dir: &Path) -> String {
    let line = fs::read_to_string(dir.join("host_ed25519.pub")).unwrap();
    line.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

fn start_agent(dir: &Path) -> (Killed, PathBuf) {
    let sock = dir.join("agent.sock");
    let child = Command::new("ssh-agent").arg("-D").arg("-a").arg(&sock).stdout(Stdio::null()).spawn().unwrap();
    for _ in 0..100 {
        if sock.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(sock.exists(), "ssh-agent did not start");
    (Killed(child), sock)
}

#[test]
#[cfg(unix)]
fn two_companies_on_github_a_private_account_and_two_servers() {
    use common::{sshd_path, start_sshd_with, user};
    if !has_tools() {
        return;
    }
    let Some(sshd) = sshd_path() else { return };
    let sb = Sandbox::new();
    sb.ssh_shim();
    sb.real_keyscan();
    let me = user();
    let world = sb.home.parent().unwrap().join("world");
    // The stand-in runs as this machine's user: GitHub's "git" does not exist
    // here. Only that name changes on the way to ssh, never a key or option.
    fs::rename(sb.bin.join("ssh"), sb.bin.join("ssh-sandbox")).unwrap();
    sb.script(
        "ssh",
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do shift; [ \"$a\" = git@github.com ] && a={me}@github.com; set -- \"$@\" \"$a\"; done\n\
             exec '{}' \"$@\"\n",
            sb.bin.join("ssh-sandbox").display()
        ),
    );

    // Three ssh servers on this machine. GitHub allows one key per login try,
    // the servers two: a key that is offered first and refused costs a try.
    let github = world.join("github");
    let web = world.join("web");
    let nas = world.join("nas");
    for dir in [&github, &web, &nas] {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("authorized_keys"), "").unwrap();
    }
    let (_github_sshd, github_port) =
        start_sshd_with(&sshd, &github, &github.join("authorized_keys"), "MaxAuthTries 1\n");
    let (_web_sshd, web_port) = start_sshd_with(&sshd, &web, &web.join("authorized_keys"), "MaxAuthTries 2\n");
    // The NAS answers as "localhost" – a second server needs a name of its own.
    let (_nas_sshd, nas_port) =
        start_sshd_with(&sshd, &nas, &nas.join("authorized_keys"), "MaxAuthTries 2\nListenAddress ::1\n");

    // The user's own files: a global identity, and github.com pointing to the
    // stand-in (the only line here that a real setup does not have).
    fs::create_dir_all(sb.path(".ssh")).unwrap();
    fs::write(sb.path(".gitconfig"), "[user]\n\tname = Tris\n\temail = tris@example.org\n").unwrap();
    fs::write(
        sb.path(".ssh/config"),
        format!("Host github.com\n    HostName 127.0.0.1\n    Port {github_port}\n    UserKnownHostsFile ~/.ssh/known_hosts\n"),
    )
    .unwrap();
    fs::write(sb.path(".ssh/known_hosts"), format!("[127.0.0.1]:{github_port} {}\n", host_key(&github))).unwrap();

    // 1. Five keys, set up the way a user would.
    let github_key = |name: &str, folder: &str, mail: &str, extra: &[&str]| {
        let mut args = vec!["--provider", "github", "--folder", folder, "--git-name", "Tris", "--git-email", mail];
        args.extend(extra);
        sb.new_profile(name, &args);
    };
    github_key("Privat", "~/dev/privat", "tris@privat.de", &["--sign", "--default"]);
    github_key("Firma A", "~/dev/firma-a", "tris@firma-a.de", &[]);
    github_key("Firma B", "~/dev/firma-b", "tris@firma-b.de", &[]);
    let (web_port_s, nas_port_s) = (web_port.to_string(), nas_port.to_string());
    sb.new_profile(
        "Webserver",
        &["--provider", "generic", "--host", "127.0.0.1", "--alias", "web", "--user", &me, "--port", &web_port_s],
    );
    sb.new_profile(
        "NAS",
        &["--provider", "generic", "--host", "localhost", "--alias", "nas", "--user", &me, "--port", &nas_port_s],
    );
    let public = |id: &str| sb.read(&format!(".ssh/{id}_ed25519.pub")).trim().to_string();

    // Each account and server knows its own key, as after "Add key".
    common::write_executable(&github.join("gate"), &gate_script(&github));
    let gate_line = |account: &str| {
        format!(
            "command=\"{} {account}\",no-pty,no-port-forwarding {}\n",
            github.join("gate").display(),
            public(account)
        )
    };
    fs::write(github.join("authorized_keys"), ["privat", "firma-a", "firma-b"].map(gate_line).concat()).unwrap();
    fs::write(web.join("authorized_keys"), format!("{}\n", public("webserver"))).unwrap();
    fs::write(nas.join("authorized_keys"), format!("{}\n", public("nas"))).unwrap();
    for repo in ["privat/dotfiles.git", "firma-a/app.git", "firma-b/shop.git"] {
        sb.git(&github, &["init", "-q", "--bare", repo]);
    }
    let logins = || {
        let file = github.join("logins");
        let seen = fs::read_to_string(&file).unwrap_or_default();
        let _ = fs::remove_file(&file);
        seen.lines().map(String::from).collect::<Vec<_>>()
    };

    // 2. The dangerous agent: it holds the other keys – another GitHub account
    //    first – but not the one needed, like the morning after a restart.
    //    ssh offers a key from `-i` first only once the agent has it, so with
    //    every key loaded a missing IdentitiesOnly would go unnoticed.
    let (_agent, sock) = start_agent(&world);
    let others_only = |id: &str| {
        ok(run(&sb, &sock, "ssh-add", &sb.home, &["-q", "-D"]));
        for other in ["firma-b", "firma-a", "nas", "privat", "webserver"].into_iter().filter(|o| *o != id) {
            let key = sb.path(&format!(".ssh/{other}_ed25519"));
            if key.exists() {
                ok(run(&sb, &sock, "ssh-add", &sb.home, &["-q", &key.display().to_string()]));
            }
        }
    };
    // AddKeysToAgent: after its first use the key is in the agent, no second prompt.
    let in_agent = |id: &str| ok(run(&sb, &sock, "ssh-add", &sb.home, &["-L"])).contains(&public(id));

    // 3. Without Keybund: plain ssh takes the agent's first key, GitHub logs in
    //    as Firma B, and Firma A's repository "does not exist"; the web server
    //    gives up after two wrong keys. The test setup catches the mix-up.
    let dev_a = sb.path("dev/firma-a");
    fs::create_dir_all(&dev_a).unwrap();
    others_only("firma-a");
    let err = failed(run(
        &sb,
        &sock,
        "git",
        &dev_a,
        &["-c", "core.sshCommand=ssh", "ls-remote", "git@github.com:firma-a/app.git"],
    ));
    assert!(err.contains("Repository not found"), "{err}");
    assert_eq!(logins(), ["firma-b"]);
    let err = failed(run(
        &sb,
        &sock,
        &common::real_tool("ssh").unwrap().display().to_string(),
        &sb.home,
        &[
            "-F",
            "/dev/null",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-p",
            &web_port_s,
            &format!("{me}@127.0.0.1"),
            "true",
        ],
    ));
    assert!(err.contains("Too many authentication failures") || err.contains("Permission denied"), "{err}");

    // 4. The connection test: each GitHub key reaches its own account.
    for (id, account) in [("privat", "@privat"), ("firma-a", "@firma-a"), ("firma-b", "@firma-b")] {
        others_only(id);
        sb.cmd()
            .env("SSH_AUTH_SOCK", &sock)
            .args(["test", id])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("Verbunden als {account}")));
        let seen = logins();
        assert!(!seen.is_empty() && seen.iter().all(|a| format!("@{a}") == account), "{id}: {seen:?}");
        assert!(in_agent(id), "{id} was not added to the agent");
    }

    // 5. Cloning: a plain `git clone` into a company folder already uses the
    //    folder's key – git creates the repository and reads its config,
    //    includeIf "gitdir:" included, before it connects (builtin/clone.c).
    //    Cloned anywhere else it is the default key; `keybund clone` picks the
    //    folder by the profile.
    others_only("firma-a");
    ok(run(&sb, &sock, "git", &dev_a, &["clone", "-q", "git@github.com:firma-a/app.git"]));
    assert_eq!(logins(), ["firma-a"]);
    let downloads = sb.path("Downloads");
    fs::create_dir_all(&downloads).unwrap();
    others_only("privat");
    let err = failed(run(&sb, &sock, "git", &downloads, &["clone", "-q", "git@github.com:firma-a/app.git"]));
    assert!(err.contains("Repository not found"), "{err}");
    assert_eq!(logins(), ["privat"]);
    for (id, url) in [("firma-b", "git@github.com:firma-b/shop.git"), ("privat", "git@github.com:privat/dotfiles.git")]
    {
        others_only(id);
        sb.cmd().env("SSH_AUTH_SOCK", &sock).args(["clone", url, "--profile", id]).assert().success();
        assert_eq!(logins(), [id], "clone {url}");
    }

    // 6. Daily work: commit and push in every folder – each push logs in as
    //    the folder's account and the commit carries the folder's e-mail.
    for (id, repo, mail) in [
        ("firma-a", "firma-a/app", "tris@firma-a.de"),
        ("firma-b", "firma-b/shop", "tris@firma-b.de"),
        ("privat", "privat/dotfiles", "tris@privat.de"),
    ] {
        let name = repo.split('/').nth(1).unwrap();
        let checkout = sb.path(&format!("dev/{id}/{name}"));
        fs::write(checkout.join("README"), id).unwrap();
        ok(run(&sb, &sock, "git", &checkout, &["add", "README"]));
        ok(run(&sb, &sock, "git", &checkout, &["commit", "-q", "-m", "first"]));
        others_only(id);
        ok(run(&sb, &sock, "git", &checkout, &["push", "-q", "origin", "HEAD"]));
        assert_eq!(logins(), [id], "push in ~/dev/{id}");
        assert!(in_agent(id), "{id} was not added to the agent");
        let bare = github.join(format!("{repo}.git"));
        assert_eq!(sb.git(&bare, &["log", "--all", "-1", "--format=%ae"]), mail);
        others_only(id);
        ok(run(&sb, &sock, "git", &checkout, &["pull", "-q", "--ff-only", "origin", "HEAD"]));
        assert_eq!(logins(), [id], "pull in ~/dev/{id}");
    }
    // The private key signs its commits, and git verifies them with Keybund's allowed signers.
    let dotfiles = sb.path("dev/privat/dotfiles");
    ok(run(&sb, &sock, "git", &dotfiles, &["verify-commit", "HEAD"]));
    let unsigned = run(&sb, &sock, "git", &sb.path("dev/firma-a/app"), &["verify-commit", "HEAD"]);
    assert!(!unsigned.status.success(), "Firma A does not sign");

    // 7. A company folder never reaches another account's repositories, even
    //    when the agent holds the other company's key and not its own.
    others_only("firma-b");
    let err =
        failed(run(&sb, &sock, "git", &sb.path("dev/firma-b/shop"), &["ls-remote", "git@github.com:firma-a/app.git"]));
    assert!(err.contains("Repository not found"), "{err}");
    assert_eq!(logins(), ["firma-b"]);
    // Outside every folder the default key applies: the private account.
    others_only("privat");
    ok(run(&sb, &sock, "git", &downloads, &["ls-remote", "git@github.com:privat/dotfiles.git"]));
    assert_eq!(logins(), ["privat"]);

    // 8. Both servers: host key confirmed once, then test, ssh and git with
    //    their own key – the agent holding only the four others.
    for id in ["webserver", "nas"] {
        sb.cmd().args(["trust", id, "--yes"]).assert().success();
    }
    for (id, alias) in [("webserver", "web"), ("nas", "nas")] {
        others_only(id);
        sb.cmd()
            .env("SSH_AUTH_SOCK", &sock)
            .args(["test", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("Anmeldung erfolgreich"));
        others_only(id);
        assert_eq!(ok(run(&sb, &sock, "ssh", &sb.home, &[alias, "echo", alias])), alias);
    }
    // A deploy from inside a company folder: the folder key goes first and is
    // refused, the server's own key follows (Host block) – one try of two.
    let deploy = nas.join("deploy.git");
    sb.git(&nas, &["init", "-q", "--bare", "deploy.git"]);
    let app = sb.path("dev/firma-a/app");
    others_only("nas");
    ok(run(
        &sb,
        &sock,
        "git",
        &app,
        &["push", "-q", &format!("ssh://nas{}", deploy.display()), "HEAD:refs/heads/live"],
    ));
    assert_eq!(sb.git(&deploy, &["log", "-1", "--format=%ae", "live"]), "tris@firma-a.de");
    assert!(logins().is_empty(), "the deploy never went to GitHub");

    // 9. Offboarding Firma B: its key is gone, its folder falls back to the
    //    default, the others keep working.
    sb.cmd().args(["archive", "firma-b", "--yes"]).assert().success();
    others_only("privat");
    let err = failed(run(&sb, &sock, "git", &sb.path("dev/firma-b/shop"), &["ls-remote", "origin"]));
    assert!(err.contains("Repository not found"), "{err}");
    assert_eq!(logins(), ["privat"]);
    others_only("firma-a");
    ok(run(&sb, &sock, "git", &app, &["ls-remote", "origin"]));
    assert_eq!(logins(), ["firma-a"]);
    sb.cmd().env("SSH_AUTH_SOCK", &sock).args(["test", "nas"]).assert().success();
}
