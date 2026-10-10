//! Environment variables that silently switch Keybund off for git:
//! `GIT_SSH_COMMAND` wins over every `core.sshCommand`, and
//! `GIT_CONFIG_GLOBAL` makes git skip `~/.gitconfig` with Keybund's include.
//!
//! They are usually set in a shell config, which a GUI started from the
//! desktop never reads. So besides the own environment, the login shell is
//! asked and the usual shell config files are searched, to name the line
//! that sets them.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::Env;

pub const WATCHED: [&str; 2] = ["GIT_SSH_COMMAND", "GIT_CONFIG_GLOBAL"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EnvOverride {
    pub name: String,
    pub value: String,
    /// Where it is set, e.g. `~/.config/fish/config.fish:12` or `Login-Shell`.
    pub origin: String,
}

/// Shell config files that commonly set environment variables.
pub fn config_files(env: &Env) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> =
        [".bashrc", ".bash_profile", ".profile", ".zshrc", ".zprofile", ".zshenv", ".config/fish/config.fish"]
            .iter()
            .map(|f| env.home.join(f))
            .collect();
    for (dir, ext) in [(".config/fish/conf.d", "fish"), (".config/environment.d", "conf")] {
        if let Ok(entries) = std::fs::read_dir(env.home.join(dir)) {
            let mut found: Vec<PathBuf> =
                entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect();
            found.sort();
            files.extend(found);
        }
    }
    files
}

/// If `line` sets one of the watched variables, its name and value.
/// Understands `NAME=…`, `export NAME=…`, fish's `set -gx NAME …` and
/// csh's `setenv NAME …`; comments are skipped.
pub fn parse_assignment(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.starts_with('#') {
        return None;
    }
    let unquote = |v: &str| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    let t2 = t.strip_prefix("export ").map(str::trim_start).unwrap_or(t);
    for name in WATCHED {
        if let Some(value) = t2.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return Some((name.to_string(), unquote(value)));
        }
    }
    let mut words = t.split_whitespace();
    match words.next()? {
        "set" => {
            let mut exported = false;
            for w in words.by_ref() {
                if w.starts_with('-') {
                    exported |= w == "--export" || (!w.starts_with("--") && w.contains('x'));
                    continue;
                }
                let name = WATCHED.iter().find(|n| **n == w)?;
                let rest = t[t.find(name)? + name.len()..].trim();
                return exported.then(|| (name.to_string(), unquote(rest)));
            }
            None
        }
        "setenv" => {
            let name = WATCHED.iter().find(|n| words.next() == Some(**n))?;
            let rest = t[t.find(name)? + name.len()..].trim();
            Some((name.to_string(), unquote(rest)))
        }
        _ => None,
    }
}

/// Assignments in the shell config files, with `file:line` as origin.
pub fn scan_files(env: &Env) -> Vec<EnvOverride> {
    let mut out = Vec::new();
    for file in config_files(env) {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        for (i, line) in text.lines().enumerate() {
            if let Some((name, value)) = parse_assignment(line) {
                out.push(EnvOverride { name, value, origin: format!("{}:{}", env.contract(&file), i + 1) });
            }
        }
    }
    out
}

/// Environment of an interactive login shell (`$SHELL -l -i -c env`), with
/// a timeout in case a shell config starts something long-running.
pub fn shell_vars(shell: &Path, home: &Path, timeout: Duration) -> Option<Vec<(String, String)>> {
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", "env"])
        .env("HOME", home)
        .env("KEYBUND_PROBE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Read stdout on a thread: a large environment would otherwise fill the
    // pipe, and a background process started by the shell config keeps the
    // pipe open after the shell exits – neither may block us past the timeout.
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        let _ = tx.send(buf);
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    // `env` is done when the shell exits; give lingering pipe holders only the rest of the timeout.
    let out = rx.recv_timeout(timeout.saturating_sub(start.elapsed()).max(Duration::from_millis(200))).ok()?;
    Some(
        String::from_utf8_lossy(&out)
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
            .collect(),
    )
}

/// Combines the own environment, the login shell and the config files.
/// `GIT_CONFIG_GLOBAL` pointing at `~/.gitconfig` itself is harmless and
/// left out.
pub fn detect_with(env: &Env, process: &[(String, String)], shell: Option<&[(String, String)]>) -> Vec<EnvOverride> {
    let files = scan_files(env);
    let mut out: Vec<EnvOverride> = Vec::new();
    for name in WATCHED {
        let value = process
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| (v.clone(), "Umgebung von Keybund"))
            .or_else(|| shell?.iter().find(|(k, _)| k == name).map(|(_, v)| (v.clone(), "Login-Shell")));
        let in_file = files.iter().find(|f| f.name == name);
        let found = match (value, in_file) {
            (Some((value, _)), Some(f)) => Some(EnvOverride { name: name.into(), value, origin: f.origin.clone() }),
            (Some((value, origin)), None) => Some(EnvOverride { name: name.into(), value, origin: origin.into() }),
            // Only in a file the login shell did not confirm (or could not be
            // asked): still worth reporting, the file is the cause.
            (None, Some(f)) if shell.is_none() => Some(f.clone()),
            (None, _) => None,
        };
        if let Some(o) = found.filter(|o| !o.value.is_empty()) {
            if o.name == "GIT_CONFIG_GLOBAL" && env.expand(&o.value) == env.gitconfig() {
                continue;
            }
            out.push(o);
        }
    }
    out
}

/// Detects overrides for the current user.
pub fn detect(env: &Env) -> Vec<EnvOverride> {
    let process: Vec<(String, String)> = std::env::vars().collect();
    let shell = env.login_shell.as_deref().and_then(|s| shell_vars(s, &env.home, Duration::from_secs(4)));
    detect_with(env, &process, shell.as_deref())
}

/// The identity from the user's own `~/.gitconfig`: what git uses outside
/// Keybund's folders and where a key leaves name or e-mail empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GlobalIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Reads `user.name` and `user.email` with `git config --global`, which does
/// not follow includes – so Keybund's own files never count.
pub fn global_identity(env: &Env) -> GlobalIdentity {
    let mut cmd = env.command("git");
    cmd.args(["config", "--global", "--get-regexp", r"^user\.(name|email)$"]);
    // Exit code 1: neither is set.
    let out = crate::process::stdout(cmd).unwrap_or_default();
    let mut id = GlobalIdentity::default();
    for line in out.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        let value = Some(value.trim().to_string()).filter(|v| !v.is_empty());
        match key {
            "user.name" => id.name = value.or(id.name),
            "user.email" => id.email = value.or(id.email),
            _ => {}
        }
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_global_identity_without_includes() {
        // These point git at another global config than the temporary HOME.
        if ["GIT_CONFIG_GLOBAL", "XDG_CONFIG_HOME"].iter().any(|v| std::env::var_os(v).is_some()) {
            return;
        }
        if Command::new("git").arg("--version").output().is_err() {
            assert!(std::env::var_os("CI").is_none(), "git is required in CI");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        assert_eq!(global_identity(&env), GlobalIdentity::default());
        std::fs::write(dir.path().join("other.gitconfig"), "[user]\n\temail = included@x.de\n").unwrap();
        std::fs::write(
            dir.path().join(".gitconfig"),
            "[user]\n\tname = Tristan Teufel\n[include]\n\tpath = ~/other.gitconfig\n",
        )
        .unwrap();
        let id = global_identity(&env);
        assert_eq!(id.name.as_deref(), Some("Tristan Teufel"));
        assert_eq!(id.email, None, "includes (Keybund's files) do not count");
    }

    fn kv(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parses_shell_syntax() {
        let p = |l: &str| parse_assignment(l).map(|(n, v)| format!("{n}={v}"));
        assert_eq!(p("export GIT_SSH_COMMAND=\"ssh -i ~/.ssh/a\"").as_deref(), Some("GIT_SSH_COMMAND=ssh -i ~/.ssh/a"));
        assert_eq!(p("GIT_SSH_COMMAND='ssh -i ~/.ssh/b'").as_deref(), Some("GIT_SSH_COMMAND=ssh -i ~/.ssh/b"));
        assert_eq!(
            p("set -gx GIT_SSH_COMMAND \"ssh -i ~/.ssh/c\"").as_deref(),
            Some("GIT_SSH_COMMAND=ssh -i ~/.ssh/c")
        );
        assert_eq!(
            p("set --export GIT_CONFIG_GLOBAL ~/work.gitconfig").as_deref(),
            Some("GIT_CONFIG_GLOBAL=~/work.gitconfig")
        );
        assert_eq!(p("setenv GIT_SSH_COMMAND 'ssh -i x'").as_deref(), Some("GIT_SSH_COMMAND=ssh -i x"));
        assert_eq!(p("# export GIT_SSH_COMMAND=ssh"), None);
        assert_eq!(p("set -g GIT_SSH_COMMAND ssh"), None, "not exported");
        assert_eq!(p("echo $GIT_SSH_COMMAND"), None);
        assert_eq!(p("export GIT_SSH_COMMANDS=x"), None);
    }

    #[test]
    fn finds_the_line_in_a_fish_config() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        std::fs::create_dir_all(env.home.join(".config/fish/conf.d")).unwrap();
        std::fs::write(
            env.home.join(".config/fish/conf.d/git.fish"),
            "# git\nset -gx GIT_SSH_COMMAND 'ssh -i ~/.ssh/old'\n",
        )
        .unwrap();

        // Confirmed by the login shell: reported with the file as origin.
        let shell = kv(&[("GIT_SSH_COMMAND", "ssh -i ~/.ssh/old"), ("PATH", "/usr/bin")]);
        let found = detect_with(&env, &[], Some(&shell));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].origin, "~/.config/fish/conf.d/git.fish:2");
        assert_eq!(found[0].value, "ssh -i ~/.ssh/old");

        // The login shell says it is not set (line inside an if, removed meanwhile ...): trust the shell.
        assert!(detect_with(&env, &[], Some(&kv(&[("PATH", "/usr/bin")]))).is_empty());
        // No shell to ask: the file alone counts.
        assert_eq!(detect_with(&env, &[], None).len(), 1);
    }

    #[test]
    fn own_environment_and_harmless_global_config() {
        let env = Env::with_home("/home/t");
        let found = detect_with(&env, &kv(&[("GIT_SSH_COMMAND", "ssh -v")]), None);
        assert_eq!(found[0].origin, "Umgebung von Keybund");
        assert!(detect_with(&env, &kv(&[("GIT_CONFIG_GLOBAL", "/home/t/.gitconfig")]), None).is_empty());
        assert_eq!(
            detect_with(&env, &kv(&[("GIT_CONFIG_GLOBAL", "/home/t/other")]), None)[0].name,
            "GIT_CONFIG_GLOBAL"
        );
        assert!(detect_with(&env, &kv(&[("GIT_SSH_COMMAND", "")]), None).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn asks_the_login_shell_with_a_timeout() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("fakeshell");
        std::fs::write(&shell, "#!/bin/sh\necho GIT_SSH_COMMAND=ssh -i /x\necho HOME=$HOME\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let vars = shell_vars(&shell, Path::new("/home/t"), Duration::from_secs(4)).unwrap();
        assert!(vars.contains(&("GIT_SSH_COMMAND".into(), "ssh -i /x".into())));
        assert!(vars.contains(&("HOME".into(), "/home/t".into())));

        let slow = dir.path().join("slowshell");
        std::fs::write(&slow, "#!/bin/sh\nsleep 5\n").unwrap();
        std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = Instant::now();
        assert!(shell_vars(&slow, Path::new("/"), Duration::from_millis(300)).is_none());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
