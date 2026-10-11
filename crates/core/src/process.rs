use std::process::{Command, Output};

use crate::{Error, Result};

/// Finds `program` on `PATH`.
pub fn find_program(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|p| p.is_file())
}

/// Runs a command and maps "not found" to [`Error::ToolMissing`].
pub fn output(mut cmd: Command) -> Result<Output> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ToolMissing(program.clone())
        } else {
            Error::ToolFailed { tool: program.clone(), message: e.to_string() }
        }
    })
}

/// Like [`output`], but kills the command after `timeout`: an ssh that
/// waits on a passphrase prompt nobody sees, a ProxyCommand that never
/// returns or a server that hangs in authentication must not hang the app.
pub fn output_timeout(mut cmd: Command, timeout: std::time::Duration) -> Result<Output> {
    use std::io::Read;
    let program = cmd.get_program().to_string_lossy().into_owned();
    let not_found = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ToolMissing(program.clone())
        } else {
            Error::ToolFailed { tool: program.clone(), message: e.to_string() }
        }
    };
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(not_found)?;
    // Read both pipes on threads so a chatty command cannot block on a full pipe.
    let reader = |pipe: Option<std::process::ChildStdout>, pipe_err: Option<std::process::ChildStderr>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut err = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut out);
            }
            if let Some(mut p) = pipe_err {
                let _ = p.read_to_end(&mut err);
            }
            (out, err)
        })
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = reader(stdout, None);
    let thread_err = reader(None, stderr);
    std::thread::spawn(move || {
        let out = thread.join().map(|(o, _)| o).unwrap_or_default();
        let err = thread_err.join().map(|(_, e)| e).unwrap_or_default();
        let _ = tx.send((out, err));
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::ToolFailed {
                    tool: program,
                    message: format!("keine Antwort nach {} s", timeout.as_secs()),
                });
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(e) => return Err(Error::ToolFailed { tool: program, message: e.to_string() }),
        }
    };
    let (stdout, stderr) = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default();
    Ok(Output { status, stdout, stderr })
}

/// Runs a command and returns its stdout, failing on a non-zero exit code.
pub fn stdout(cmd: Command) -> Result<String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = output(cmd)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::ToolFailed {
            tool: program,
            message: if stderr.is_empty() { format!("Exit-Code {}", out.status) } else { stderr },
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Quotes a word for `sh` if it contains anything but safe characters.
/// A leading `~/` stays unquoted so the shell still expands it.
pub fn shell_quote(word: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./@:+=,%".contains(c);
    if !word.is_empty() && word.chars().all(|c| safe(c) || c == '~') && !word[1..].contains('~') {
        return word.to_string();
    }
    if let Some(rest) = word.strip_prefix("~/") {
        if rest.chars().all(safe) {
            return word.to_string();
        }
        return format!("~/'{}'", rest.replace('\'', r"'\''"));
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::shell_quote;

    #[test]
    #[cfg(unix)]
    fn a_hanging_command_is_killed_after_the_timeout() {
        use std::time::Duration;
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 5"]);
        let err = super::output_timeout(cmd, Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), "tool_failed");
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2"]);
        let out = super::output_timeout(cmd, Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "out\n");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "err\n");
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("~/.ssh/id_ed25519"), "~/.ssh/id_ed25519");
        assert_eq!(shell_quote("~/my keys/a"), "~/'my keys/a'");
        assert_eq!(shell_quote("/opt/a b"), "'/opt/a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
