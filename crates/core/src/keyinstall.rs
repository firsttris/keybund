//! Putting a profile's public key into `~/.ssh/authorized_keys` on a server,
//! like `ssh-copy-id`. The login for that uses whatever already works for
//! the user (another key in the agent, the password); the password is asked
//! on the terminal or via `SSH_ASKPASS` (ksshaskpass) in the app.

use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::i18n::Msg;
use crate::model::{HostEntry, Profile};
use crate::msg;
use crate::{keygen, Env, Error, Result};

/// POSIX sh, run via `sh -c` so it works whatever the login shell is (fish).
/// Reads the key from stdin, appends it once, keeps the file's last line
/// intact. `KEYBUND_AUTHORIZED_KEYS` only exists for tests.
const REMOTE_SCRIPT: &str = r#"umask 077; f=${KEYBUND_AUTHORIZED_KEYS:-$HOME/.ssh/authorized_keys}; mkdir -p "$(dirname "$f")" && touch "$f" && k=$(cat) && b=$(printf "%s" "$k" | cut -d" " -f1,2) && if grep -qF -- "$b" "$f"; then echo KEYBUND_PRESENT; else if [ -s "$f" ] && [ -n "$(tail -c1 "$f")" ]; then echo >> "$f"; fi; printf "%s\n" "$k" >> "$f" && echo KEYBUND_ADDED; fi; if command -v restorecon >/dev/null 2>&1; then restorecon -F "$(dirname "$f")" "$f" 2>/dev/null || :; fi"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InstallResult {
    pub success: bool,
    /// The key was already in `authorized_keys`.
    pub already_present: bool,
    pub message: Msg,
    pub output: String,
}

/// Known password dialogs, for the app (no terminal to ask on).
const ASKPASS: [&str; 4] =
    ["/usr/bin/ksshaskpass", "/usr/libexec/openssh/ssh-askpass", "/usr/lib/ssh/ssh-askpass", "/usr/bin/ssh-askpass"];

pub fn askpass_program() -> Option<String> {
    std::env::var("SSH_ASKPASS")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| ASKPASS.iter().find(|p| Path::new(p).exists()).map(|p| p.to_string()))
}

/// ssh's default keys: with Keybund's Host block setting an IdentityFile,
/// ssh no longer tries them on its own.
const DEFAULT_KEYS: [&str; 6] = ["id_rsa", "id_ecdsa", "id_ecdsa_sk", "id_ed25519", "id_ed25519_sk", "id_dsa"];

/// `true` if the user's own `~/.ssh/config` has a `Host` block for `alias`.
fn user_has_block(env: &Env, alias: &str) -> bool {
    std::fs::read_to_string(env.ssh_config())
        .map(|c| crate::import::parse_ssh_config(&c).iter().any(|b| b.pattern.eq_ignore_ascii_case(alias)))
        .unwrap_or(false)
}

/// Arguments for `ssh` (without the program name).
pub fn ssh_args(env: &Env, host: &HostEntry) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        // Keybund's own Host block would only offer the new key, which the
        // server does not know yet; allow every way in that already works.
        "-o",
        "IdentitiesOnly=no",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=yes",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Every login that already works: also ssh's default keys (agent keys come anyway).
    for name in DEFAULT_KEYS {
        let path = env.ssh_dir().join(name);
        if path.exists() {
            args.push("-i".into());
            args.push(path.display().to_string());
        }
    }
    // By hostname, so the user's existing `Host <ip>` settings apply – unless
    // the user has an own block for the alias (e.g. with ProxyJump): then by
    // alias, with HostName and port given in case the configs are not written yet.
    if let Some(port) = host.port {
        args.push("-p".into());
        args.push(port.to_string());
    }
    let name = match &host.alias {
        Some(alias) if user_has_block(env, alias) => {
            args.push("-o".into());
            args.push(format!("HostName={}", host.hostname));
            alias.clone()
        }
        _ => host.hostname.clone(),
    };
    let target = match &host.user {
        Some(u) => format!("{u}@{name}"),
        None => name,
    };
    // `--`: a host can never be read as an ssh option.
    args.push("--".into());
    args.push(target);
    args.push(format!("sh -c '{REMOTE_SCRIPT}'"));
    args
}

/// Installs the profile's public key on `host`. `extra_args` go before the
/// target (tests use them for an isolated ssh config).
pub fn install_with(env: &Env, profile: &Profile, host: &HostEntry, extra_args: &[String]) -> Result<InstallResult> {
    install_prompting(env, profile, host, extra_args, crate::probe::Prompt::Terminal)
}

/// Like [`install`], for the app: a password or passphrase goes through the
/// askpass dialog, never through a terminal the app may have been started from.
pub fn install_detached(env: &Env, profile: &Profile, host: &HostEntry) -> Result<InstallResult> {
    install_prompting(env, profile, host, &[], crate::probe::Prompt::Detached)
}

fn install_prompting(
    env: &Env,
    profile: &Profile,
    host: &HostEntry,
    extra_args: &[String],
    prompt: crate::probe::Prompt,
) -> Result<InstallResult> {
    let public = keygen::read_public(env, &profile.key)?.line();
    let mut cmd = env.command("ssh");
    cmd.args(extra_args).args(ssh_args(env, host)).stdout(Stdio::piped()).stderr(Stdio::piped());
    if prompt == crate::probe::Prompt::Detached {
        crate::probe::detach(&mut cmd);
    }
    // The key goes in through stdin after the login (also after a password prompt).
    cmd.stdin(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ToolMissing("ssh".into())
        } else {
            Error::ToolFailed { tool: "ssh".into(), message: e.to_string() }
        }
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(public.as_bytes());
    }
    let out = wait_with_timeout(child, Duration::from_secs(60))?;
    let output =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    Ok(interpret(&output, out.status.success(), host))
}

/// `wait_with_output` with a deadline: an ssh waiting on a password prompt
/// nobody answers must not hang the app.
fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<std::process::Output> {
    use std::io::Read;
    let tool =
        || Error::ToolFailed { tool: "ssh".into(), message: format!("keine Antwort nach {} s", timeout.as_secs()) };
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        if let Some(p) = stdout.as_mut() {
            let _ = p.read_to_end(&mut out);
        }
        if let Some(p) = stderr.as_mut() {
            let _ = p.read_to_end(&mut err);
        }
        (out, err)
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(tool());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(Error::ToolFailed { tool: "ssh".into(), message: e.to_string() }),
        }
    };
    let (stdout, stderr) = reader.join().unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr })
}

pub fn install(env: &Env, profile: &Profile, host: &HostEntry) -> Result<InstallResult> {
    install_with(env, profile, host, &[])
}

pub fn interpret(output: &str, ok: bool, host: &HostEntry) -> InstallResult {
    let lower = output.to_lowercase();
    let name = host.target_name();
    let (success, already_present, message) = if output.contains("KEYBUND_PRESENT") {
        (true, true, msg!("install_present", host = name))
    } else if ok && output.contains("KEYBUND_ADDED") {
        (true, false, msg!("install_added", host = name))
    } else if lower.contains("permission denied") {
        (false, false, msg!("install_denied", host = name))
    } else if lower.contains("could not resolve hostname") {
        (false, false, msg!("probe_hostNotFound", host = &host.hostname))
    } else if lower.contains("timed out") || lower.contains("connection refused") || lower.contains("no route to host")
    {
        (false, false, msg!("probe_unreachable", host = name))
    } else if lower.contains("read_passphrase") || lower.contains("ssh_askpass") {
        (false, false, msg!("install_noAskpass"))
    } else {
        (false, false, msg!("install_failed", host = name))
    };
    let output = output.lines().filter(|l| !l.starts_with("KEYBUND_")).collect::<Vec<_>>().join("\n");
    InstallResult { success, already_present, message, output }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Provider;

    fn server() -> HostEntry {
        HostEntry {
            alias: Some("nas".into()),
            hostname: "192.168.1.5".into(),
            user: Some("admin".into()),
            port: Some(2222),
            provider: Provider::Generic,
        }
    }

    #[test]
    fn connects_by_alias_or_hostname_with_user_and_port() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        std::fs::create_dir_all(env.ssh_dir()).unwrap();
        std::fs::write(env.ssh_dir().join("id_ed25519"), "x").unwrap();
        let args = ssh_args(&env, &server());
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
        assert_eq!(args[args.len() - 2], "admin@192.168.1.5");
        assert!(args.windows(2).any(|w| w[0] == "-i" && w[1].ends_with(".ssh/id_ed25519")), "{args:?}");
        assert!(args.contains(&"IdentitiesOnly=no".to_string()));
        // The user's own block for the alias (ProxyJump …) must apply: by alias.
        std::fs::write(env.ssh_config(), "Host nas\n    ProxyJump bastion\n").unwrap();
        let args = ssh_args(&env, &server());
        assert!(args.contains(&"HostName=192.168.1.5".to_string()));
        assert_eq!(args[args.len() - 2], "admin@nas");
        assert!(args.last().unwrap().starts_with("sh -c '"));
        assert!(!REMOTE_SCRIPT.contains('\''), "the script is wrapped in single quotes");
    }

    /// The remote script, run locally: a key that is already there with
    /// another comment counts as present, a new one is appended once.
    #[test]
    #[cfg(unix)]
    fn remote_script_matches_keys_not_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("authorized_keys");
        std::fs::write(&file, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAA old-comment").unwrap();
        let run = |key: &str| {
            use std::io::Write;
            let mut child = std::process::Command::new("sh")
                .args(["-c", REMOTE_SCRIPT])
                .env("KEYBUND_AUTHORIZED_KEYS", &file)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(key.as_bytes()).unwrap();
            String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
        };
        assert_eq!(run("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAA new-comment").trim(), "KEYBUND_PRESENT");
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 1);
        assert_eq!(run("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBBB nas").trim(), "KEYBUND_ADDED");
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.ends_with("AAAAIBBB nas\n"));
    }

    #[test]
    fn interprets_results() {
        assert!(interpret("KEYBUND_ADDED", true, &server()).success);
        let r = interpret("KEYBUND_PRESENT", true, &server());
        assert!(r.success && r.already_present);
        assert_eq!(r.message.to_string(), "Der Key war auf nas schon eingetragen");
        let r = interpret("admin@192.168.1.5: Permission denied (publickey,password).", false, &server());
        assert!(!r.success);
        assert!(r.message.to_string().starts_with("Anmeldung an nas fehlgeschlagen"));
    }
}
