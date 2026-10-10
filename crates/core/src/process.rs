use std::process::{Command, Output};

use crate::{Error, Result};

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
    fn quoting() {
        assert_eq!(shell_quote("~/.ssh/id_ed25519"), "~/.ssh/id_ed25519");
        assert_eq!(shell_quote("~/my keys/a"), "~/'my keys/a'");
        assert_eq!(shell_quote("/opt/a b"), "'/opt/a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
