//! ssh-agent detection. Lives in its own test binary because it changes
//! `SSH_AUTH_SOCK` for the whole process.

use std::fs;
use std::path::Path;
use std::process::Command;

use keybund_core::keygen::{self, GenerateOptions};
use keybund_core::model::{Config, GitIdentity, HostEntry, KeyRef, Profile, Provider};
use keybund_core::{apply, doctor, store, which, Env};

/// Missing tools skip a test locally, but never in CI.
fn has(tool: &str) -> bool {
    let ok = Command::new(tool).arg("-V").output().is_ok() || Command::new(tool).arg("--version").output().is_ok();
    assert!(ok || std::env::var_os("CI").is_none(), "{tool} is required in CI");
    ok
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

/// A real ssh-agent on a private socket, killed on drop.
struct Agent {
    child: std::process::Child,
    sock: std::path::PathBuf,
}

impl Agent {
    fn start(dir: &Path) -> Agent {
        let sock = dir.join("agent.sock");
        let child = Command::new("ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&sock)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        for _ in 0..50 {
            if sock.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Agent { child, sock }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn agent_state_is_reported() {
    if !has("ssh-agent") || !has("ssh-keygen") {
        return;
    }
    let (dir, env, config) = setup();
    apply::apply_config(&env, &config).unwrap();
    let agent = Agent::start(dir.path());
    // Env::command passes the environment on; point SSH_AUTH_SOCK at our agent.
    std::env::set_var("SSH_AUTH_SOCK", &agent.sock);

    assert_eq!(which::agent_fingerprints(&env), Some(vec![]));
    let added = Command::new("ssh-add")
        .arg(env.expand("~/.ssh/kunde-a_ed25519"))
        .env("SSH_AUTH_SOCK", &agent.sock)
        .output()
        .unwrap();
    assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));

    let r = which::which(&env, &config, "~/dev/kunde-a").unwrap();
    assert_eq!(r.agent_loaded, Some(true));
    let r = which::which(&env, &config, "~/dev/kunde-b").unwrap();
    assert_eq!(r.agent_loaded, Some(false));

    let report = doctor::run(&env);
    let agent_check = report.checks.iter().find(|c| c.id == "agent").unwrap();
    assert_eq!(agent_check.status, doctor::Status::Ok);
    assert_eq!(agent_check.detail.as_deref(), Some("1 Keys geladen"));

    std::env::set_var("SSH_AUTH_SOCK", dir.path().join("nope.sock"));
    assert_eq!(which::agent_fingerprints(&env), None);
    let r = which::which(&env, &config, "~/dev/kunde-a").unwrap();
    assert!(r.notes.iter().any(|n| n.contains("Kein ssh-agent")));
    std::env::remove_var("SSH_AUTH_SOCK");
}
