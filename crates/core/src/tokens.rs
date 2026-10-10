//! Optional storage of API tokens in the desktop keyring (Secret Service:
//! GNOME Keyring, KWallet) via `secret-tool`. Nothing is stored unless the
//! user asks for it; one token per profile and host, because each profile
//! usually is a different account.

use std::io::Write;
use std::process::Stdio;

use crate::{process, Env, Error, Result};

const TOOL: &str = "secret-tool";

fn attributes<'a>(profile_id: &'a str, hostname: &'a str) -> [&'a str; 6] {
    ["application", "keybund", "profile", profile_id, "host", hostname]
}

/// Whether tokens can be kept (`secret-tool` is installed).
pub fn available() -> bool {
    process::find_program(TOOL).is_some()
}

/// The stored token, if any. Errors (no keyring running) count as none.
pub fn load(env: &Env, profile_id: &str, hostname: &str) -> Option<String> {
    let mut cmd = env.command(TOOL);
    cmd.arg("lookup").args(attributes(profile_id, hostname)).stdin(Stdio::null());
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Keeps `token` for this profile and host. The token goes via stdin, never
/// on the command line.
pub fn store(env: &Env, profile_id: &str, hostname: &str, label: &str, token: &str) -> Result<()> {
    let mut cmd = env.command(TOOL);
    cmd.arg("store")
        .arg(format!("--label={label}"))
        .args(attributes(profile_id, hostname))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| missing(e, TOOL))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(token.as_bytes())
        .map_err(|e| Error::ToolFailed { tool: TOOL.into(), message: e.to_string() })?;
    let out = child.wait_with_output().map_err(|e| Error::ToolFailed { tool: TOOL.into(), message: e.to_string() })?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::ToolFailed {
            tool: TOOL.into(),
            message: if message.is_empty() { "Schlüsselbund nicht erreichbar".into() } else { message },
        });
    }
    Ok(())
}

/// Removes the stored token (no error if there is none).
pub fn forget(env: &Env, profile_id: &str, hostname: &str) -> Result<()> {
    let mut cmd = env.command(TOOL);
    cmd.arg("clear").args(attributes(profile_id, hostname));
    process::output(cmd)?;
    Ok(())
}

fn missing(e: std::io::Error, tool: &str) -> Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        Error::ToolMissing(tool.into())
    } else {
        Error::ToolFailed { tool: tool.into(), message: e.to_string() }
    }
}
