//! Talks to a real, throw-away `sshd` on localhost to prove the agent
//! behavior: a key with a passphrase is asked for once and then kept by
//! ssh-agent, both for the connection test and for git.

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use keybund_core::hostkeys::{self, HostKeyCheck};
use keybund_core::keygen::{self, GenerateOptions};
use keybund_core::model::{HostEntry, KeyRef, Profile, Provider, Settings};
use keybund_core::{keyinstall, probe, render, Env};

const PASSPHRASE: &str = "geheim";

fn sshd_path() -> Option<PathBuf> {
    let found = ["/usr/sbin/sshd", "/usr/bin/sshd"].iter().map(PathBuf::from).find(|p| p.exists());
    assert!(found.is_some() || std::env::var_os("CI").is_none(), "sshd is required in CI");
    found
}

struct Killed(Child);
impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn wait_for(cond: impl Fn() -> bool) {
    for _ in 0..100 {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out");
}

fn start_sshd(sshd: &Path, dir: &Path, authorized: &str) -> (Killed, u16) {
    let host_key = dir.join("host_ed25519");
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&host_key)
        .status()
        .unwrap()
        .success());
    fs::write(dir.join("authorized_keys"), authorized).unwrap();
    let port = free_port();
    let config = dir.join("sshd_config");
    fs::write(
        &config,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPidFile none\n\
             PasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\n\
             UsePAM no\nStrictModes no\nPermitRootLogin prohibit-password\n\
             SetEnv KEYBUND_AUTHORIZED_KEYS={}\n",
            host_key.display(),
            dir.join("authorized_keys").display(),
            dir.join("authorized_keys").display()
        ),
    )
    .unwrap();
    // Privilege separation directory, only needed when tests run as root.
    if Command::new("id").arg("-u").output().map(|o| o.stdout.starts_with(b"0")).unwrap_or(false) {
        let _ = fs::create_dir_all("/run/sshd");
    }
    let child = Command::new(sshd).arg("-D").arg("-e").arg("-f").arg(&config).stderr(Stdio::null()).spawn().unwrap();
    wait_for(|| TcpStream::connect(("127.0.0.1", port)).is_ok());
    (Killed(child), port)
}

fn start_agent(dir: &Path, name: &str) -> (Killed, PathBuf) {
    let sock = dir.join(name);
    let child = Command::new("ssh-agent").arg("-D").arg("-a").arg(&sock).stdout(Stdio::null()).spawn().unwrap();
    wait_for(|| sock.exists());
    (Killed(child), sock)
}

fn loaded(sock: &Path) -> String {
    let out = Command::new("ssh-add").arg("-l").env("SSH_AUTH_SOCK", sock).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn askpass(dir: &Path, answer: Option<&str>) -> PathBuf {
    let script = dir.join(if answer.is_some() { "askpass-ok" } else { "askpass-fail" });
    let body = match answer {
        Some(a) => format!("#!/bin/sh\necho '{a}'\n"),
        None => "#!/bin/sh\nexit 1\n".to_string(),
    };
    write_executable(&script, &body);
    script
}

/// Writes an executable script through a short `sh` child: written in this process, the file
/// would be open for writing while another test thread forks, and Linux refuses to execute a file
/// that is open for writing (ETXTBSY, `execve(2)`).
fn write_executable(path: &Path, content: &str) {
    use std::io::Write;
    let mut child = Command::new("sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(content.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success(), "could not write {}", path.display());
}

/// Runs a command with a private agent and askpass, isolated from the
/// user's own ssh config and known_hosts.
fn isolated(mut cmd: Command, sock: &Path, askpass: &Path) -> Output {
    cmd.env("SSH_AUTH_SOCK", sock)
        .env("SSH_ASKPASS", askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env_remove("DISPLAY")
        .stdin(Stdio::null());
    cmd.output().unwrap()
}

fn user() -> String {
    String::from_utf8(Command::new("id").arg("-un").output().unwrap().stdout).unwrap().trim().to_string()
}

#[test]
fn passphrase_is_asked_once_then_the_agent_holds_the_key() {
    let Some(sshd) = sshd_path() else { return };
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().canonicalize().unwrap();
    let env = Env::with_home(dir.join("home"));
    let key = keygen::generate(
        &env,
        &GenerateOptions {
            path: "~/.ssh/srv_ed25519".into(),
            comment: "t".into(),
            passphrase: Some(PASSPHRASE.into()),
        },
    )
    .unwrap();
    let fingerprint = key.fingerprint.clone().unwrap();
    let (_sshd, port) = start_sshd(&sshd, &dir, key.public_key.as_deref().unwrap());
    let known_hosts = format!("UserKnownHostsFile={}", dir.join("known_hosts").display());

    let host = HostEntry {
        alias: None,
        hostname: "127.0.0.1".into(),
        user: Some(user()),
        port: Some(port),
        provider: Provider::Generic,
    };
    let profile = Profile {
        id: "srv".into(),
        name: "Srv".into(),
        key: KeyRef { path: "~/.ssh/srv_ed25519".into(), comment: None },
        hosts: vec![host.clone()],
        ..Default::default()
    };
    let probe_cmd = |args: &[String]| {
        let mut cmd = Command::new("ssh");
        cmd.args(["-F", "/dev/null", "-o", &known_hosts, "-o", "StrictHostKeyChecking=accept-new"]);
        cmd.args(
            args.iter().map(|a| if a.starts_with("~/") { env.expand(a).display().to_string() } else { a.clone() }),
        );
        cmd
    };

    // Before the fix: BatchMode blocks the passphrase prompt, nothing is
    // added to the agent and the test fails although the key is valid.
    let (_old_agent, old_sock) = start_agent(&dir, "old.sock");
    let old_args: Vec<String> = probe::ssh_args(&profile, &host)
        .into_iter()
        .filter(|a| a != "AddKeysToAgent=yes")
        .map(|a| if a == "PasswordAuthentication=no" { "BatchMode=yes".into() } else { a })
        .collect();
    let out = isolated(probe_cmd(&old_args), &old_sock, &askpass(&dir, Some(PASSPHRASE)));
    assert!(!out.status.success());
    assert!(!loaded(&old_sock).contains(&fingerprint));

    // Connection test: asks once via askpass and leaves the key in the agent.
    let (_agent, sock) = start_agent(&dir, "agent.sock");
    let out = isolated(probe_cmd(&probe::ssh_args(&profile, &host)), &sock, &askpass(&dir, Some(PASSPHRASE)));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let result = probe::interpret(&host, &text, out.status.code(), String::new());
    assert!(result.success, "{text}");
    assert!(loaded(&sock).contains(&fingerprint), "key not in agent after the test");

    // Second connection: askpass would fail, the agent answers instead.
    let out = isolated(probe_cmd(&probe::ssh_args(&profile, &host)), &sock, &askpass(&dir, None));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn git_with_generated_ssh_command_keeps_the_key_in_the_agent() {
    let Some(sshd) = sshd_path() else { return };
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().canonicalize().unwrap();
    let env = Env::with_home(dir.join("home"));
    let key = keygen::generate(
        &env,
        &GenerateOptions {
            path: "~/.ssh/kunde_ed25519".into(),
            comment: "t".into(),
            passphrase: Some(PASSPHRASE.into()),
        },
    )
    .unwrap();
    let fingerprint = key.fingerprint.clone().unwrap();
    let (_sshd, port) = start_sshd(&sshd, &dir, key.public_key.as_deref().unwrap());
    let remote = dir.join("remote.git");
    assert!(Command::new("git").args(["init", "-q", "--bare"]).arg(&remote).status().unwrap().success());

    // Exactly the command Keybund writes into <id>.gitconfig, plus test isolation.
    let ssh_command = format!(
        "{} -F /dev/null -o UserKnownHostsFile={} -o StrictHostKeyChecking=accept-new",
        render::ssh_command("~/.ssh/kunde_ed25519", env.platform),
        dir.join("known_hosts").display()
    );
    let url = format!("ssh://{}@127.0.0.1:{port}{}", user(), remote.display());
    let ls_remote = || {
        let mut cmd = env.command("git");
        cmd.args(["ls-remote", &url]).env("GIT_SSH_COMMAND", &ssh_command).env("GIT_CONFIG_NOSYSTEM", "1");
        cmd
    };

    let (_agent, sock) = start_agent(&dir, "agent.sock");
    let out = isolated(ls_remote(), &sock, &askpass(&dir, Some(PASSPHRASE)));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(loaded(&sock).contains(&fingerprint), "git did not add the key to the agent");

    let out = isolated(ls_remote(), &sock, &askpass(&dir, None));
    assert!(out.status.success(), "second git call asked again: {}", String::from_utf8_lossy(&out.stderr));
}

/// The user's own `~/.ssh/config` has an `IdentityFile` for the host. ssh
/// offers it after the profile key; if the server only knows that one, the
/// login succeeds with the wrong key. The test must say so.
#[test]
fn connection_test_detects_a_login_with_another_key() {
    let Some(sshd) = sshd_path() else { return };
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().canonicalize().unwrap();
    let env = Env::with_home(dir.join("home"));
    let gen = |name: &str| {
        keygen::generate(
            &env,
            &GenerateOptions { path: format!("~/.ssh/{name}_ed25519"), comment: name.into(), passphrase: None },
        )
        .unwrap()
    };
    let profile_key = gen("test");
    let old_key = gen("privat");
    // The server only knows the old key.
    let (_sshd, port) = start_sshd(&sshd, &dir, old_key.public_key.as_deref().unwrap());
    let user_config = dir.join("user_config");
    fs::write(
        &user_config,
        format!(
            "Host 127.0.0.1\n    IdentityFile {}\n    IdentitiesOnly yes\n",
            env.expand("~/.ssh/privat_ed25519").display()
        ),
    )
    .unwrap();

    let host = HostEntry {
        alias: None,
        hostname: "127.0.0.1".into(),
        user: Some(user()),
        port: Some(port),
        provider: Provider::Generic,
    };
    let profile = Profile {
        id: "test".into(),
        name: "Test".into(),
        key: KeyRef { path: "~/.ssh/test_ed25519".into(), comment: None },
        hosts: vec![host.clone()],
        ..Default::default()
    };
    let run = |authorized: &str| {
        fs::write(dir.join("authorized_keys"), authorized).unwrap();
        let mut cmd = Command::new("ssh");
        cmd.arg("-v").args(["-F"]).arg(&user_config);
        cmd.args(["-o", &format!("UserKnownHostsFile={}", dir.join("known_hosts").display())]);
        cmd.args(["-o", "StrictHostKeyChecking=accept-new"]);
        cmd.args(probe::ssh_args(&profile, &host).into_iter().map(|a| {
            if a.starts_with("~/") {
                env.expand(&a).display().to_string()
            } else {
                a
            }
        }));
        let (_agent, sock) = start_agent(&dir, &format!("a{}.sock", authorized.len()));
        let out = isolated(cmd, &sock, &askpass(&dir, None));
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        probe::evaluate(&env, &host, &text, out.status.code(), String::new(), profile_key.fingerprint.as_deref())
    };

    // Before: plain exit code 0 would have said "erfolgreich".
    let r = run(old_key.public_key.as_deref().unwrap());
    assert!(!r.success, "{}", r.output);
    assert!(r.other_key);
    assert_eq!(r.used_key.as_deref(), Some("~/.ssh/privat_ed25519"));
    // The steps tell the story in plain words: the profile key was refused,
    // the other one from the user's config got in.
    let texts: Vec<String> = r.steps.iter().map(|s| s.text.to_string()).collect();
    assert!(texts.iter().any(|t| t.starts_with("Verbinde mit 127.0.0.1")), "{texts:#?}");
    assert!(texts.iter().any(|t| t.starts_with("Anmelden als Benutzer „")), "{texts:#?}");
    assert!(texts.iter().any(|t| t == "Biete Key an: ~/.ssh/test_ed25519"), "{texts:#?}");
    assert!(
        r.steps.iter().any(|s| s.ok == Some(false) && s.text.to_string() == "Server kennt diesen Key nicht"),
        "{texts:#?}"
    );
    assert!(texts.iter().any(|t| t == "Server akzeptiert ~/.ssh/privat_ed25519"), "{texts:#?}");
    assert!(r.log.contains("debug1:"));

    // Once the server knows the profile key, it is used first and succeeds.
    let r = run(&format!("{}\n{}\n", old_key.public_key.unwrap(), profile_key.public_key.unwrap()));
    assert!(r.success, "{}", r.output);
    assert!(!r.other_key);
    assert_eq!(r.used_key.as_deref(), Some("~/.ssh/test_ed25519"));
}

/// "Key auf Server eintragen": the server only knows the user's existing
/// key; Keybund logs in with it and appends the new profile key once.
#[test]
fn installs_the_profile_key_on_a_server() {
    let Some(sshd) = sshd_path() else { return };
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().canonicalize().unwrap();
    let env = Env::with_home(dir.join("home"));
    let gen = |name: &str| {
        keygen::generate(
            &env,
            &GenerateOptions { path: format!("~/.ssh/{name}_ed25519"), comment: name.into(), passphrase: None },
        )
        .unwrap()
    };
    let existing = gen("privat");
    let new_key = gen("nas");
    // authorized_keys without a trailing newline: the new key must start on its own line.
    let (_sshd, port) = start_sshd(&sshd, &dir, existing.public_key.as_deref().unwrap());
    let user_config = dir.join("user_config");
    fs::write(
        &user_config,
        format!("Host 127.0.0.1\n    IdentityFile {}\n", env.expand("~/.ssh/privat_ed25519").display()),
    )
    .unwrap();
    let host = HostEntry {
        alias: Some("nas".into()),
        hostname: "127.0.0.1".into(),
        user: Some(user()),
        port: Some(port),
        provider: Provider::Generic,
    };
    let profile = Profile {
        id: "nas".into(),
        name: "NAS".into(),
        key: KeyRef { path: "~/.ssh/nas_ed25519".into(), comment: None },
        hosts: vec![host.clone()],
        ..Default::default()
    };

    // First contact: the user confirms the fingerprint, the key goes into
    // the profile's own known_hosts file. ssh then checks strictly.
    let settings = Settings::default();
    let HostKeyCheck::Unknown { host: name, file, keys } = hostkeys::check(&env, &settings, &profile, &host).unwrap()
    else {
        panic!("expected an unknown host key")
    };
    assert_eq!(name, format!("[127.0.0.1]:{port}"));
    assert_eq!(file, "~/.ssh/known_hosts.d/nas");
    assert!(keys.iter().all(|k| k.fingerprint.starts_with("SHA256:")));
    let err = hostkeys::trust(&env, &settings, &profile, &host, &["SHA256:falsch".into()]).unwrap_err();
    assert_eq!(err.kind(), "host_key_changed");
    let fingerprints: Vec<String> = keys.iter().map(|k| k.fingerprint.clone()).collect();
    assert_eq!(hostkeys::trust(&env, &settings, &profile, &host, &fingerprints).unwrap(), keys.len());
    assert_eq!(hostkeys::check(&env, &settings, &profile, &host).unwrap(), HostKeyCheck::Known);
    let known_hosts = env.expand("~/.ssh/known_hosts.d/nas");
    assert!(fs::read_to_string(&known_hosts).unwrap().starts_with(&format!("[127.0.0.1]:{port} ")));

    let extra: Vec<String> = vec![
        "-F".into(),
        user_config.display().to_string(),
        "-o".into(),
        format!("UserKnownHostsFile={}", known_hosts.display()),
    ];

    let r = keyinstall::install_with(&env, &profile, &host, &extra).unwrap();
    assert!(r.success && !r.already_present, "{} / {}", r.message, r.output);
    let authorized = fs::read_to_string(dir.join("authorized_keys")).unwrap();
    assert_eq!(authorized.lines().count(), 2, "{authorized}");
    assert!(authorized.lines().any(|l| l == new_key.public_key.as_deref().unwrap()));

    // Twice is fine and changes nothing.
    let r = keyinstall::install_with(&env, &profile, &host, &extra).unwrap();
    assert!(r.success && r.already_present);
    assert_eq!(fs::read_to_string(dir.join("authorized_keys")).unwrap(), authorized);

    // Now the profile key alone gets in.
    let mut cmd = Command::new("ssh");
    cmd.arg("-v").args(["-F", "/dev/null", "-o", &format!("UserKnownHostsFile={}", known_hosts.display())]);
    cmd.args(probe::ssh_args(&profile, &HostEntry { alias: None, ..host.clone() }).into_iter().map(|a| {
        if a.starts_with("~/") {
            env.expand(&a).display().to_string()
        } else {
            a
        }
    }));
    let (_agent, sock) = start_agent(&dir, "agent.sock");
    let out = isolated(cmd, &sock, &askpass(&dir, None));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let r = probe::evaluate(&env, &host, &text, out.status.code(), String::new(), new_key.fingerprint.as_deref());
    assert!(r.success, "{}", r.output);
    assert_eq!(r.used_key.as_deref(), Some("~/.ssh/nas_ed25519"));
}
