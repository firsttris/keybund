//! The way back: removes everything Keybund wrote. The user's keys are
//! never touched; `profiles.toml` stays unless `purge` is set.

use serde::{Deserialize, Serialize};

use crate::apply::{self, ApplyReport, Plan};
use crate::{fsutil, Env, Result};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UninstallOptions {
    /// Put back `~/.ssh/config` and `~/.gitconfig` as they were before
    /// Keybund's first write (from `*.keybund-backup`), instead of only
    /// removing Keybund's lines. Brings back entries removed by the import.
    pub restore_backups: bool,
    /// Also delete `profiles.toml`.
    pub purge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UninstallPreview {
    pub plan: Plan,
    /// Backups from before Keybund exist and could be restored.
    pub backups_available: bool,
}

/// `~/.ssh/config` without Keybund's `Include` line (and the blank line
/// Keybund put after it).
pub fn without_ssh_include(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut skip_blank = false;
    for line in lines {
        if apply::is_keybund_include(line) {
            // The user's other paths on the same line stay.
            if let Some(rest) = apply::without_keybund_path(line) {
                out.push(rest);
                skip_blank = false;
                continue;
            }
            skip_blank = true;
            continue;
        }
        if skip_blank && line.trim().is_empty() {
            skip_blank = false;
            continue;
        }
        skip_blank = false;
        out.push(line.to_string());
    }
    join(out.iter().map(String::as_str).collect(), apply::newline(content))
}

/// `~/.gitconfig` without the `[include]` of Keybund's index (and the
/// comment and blank line Keybund added with it).
pub fn without_git_include(content: &str, index_path: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut drop = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if apply::include_header(lines[i]) == Some(true) {
            // The section: up to the next header.
            let mut end = i + 1;
            while end < lines.len() && !lines[end].trim().starts_with('[') {
                end += 1;
            }
            let keys: Vec<&str> = lines[i + 1..end]
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
                .collect();
            let ours = |l: &&str| apply::is_index_entry(l, index_path);
            if keys.iter().any(ours) {
                if keys.iter().all(ours) {
                    // Only ours: drop the whole section with the comment and blank line before it.
                    let mut start = i;
                    if start > 0 && lines[start - 1].trim() == "# Added by Keybund" {
                        start -= 1;
                    }
                    if start > 0 && lines[start - 1].trim().is_empty() {
                        start -= 1;
                    }
                    let mut stop = end;
                    while stop > i + 1 && lines[stop - 1].trim().is_empty() {
                        stop -= 1;
                    }
                    drop[start..stop].iter_mut().for_each(|d| *d = true);
                } else {
                    for (j, line) in lines.iter().enumerate().take(end).skip(i + 1) {
                        if ours(&line.trim()) {
                            drop[j] = true;
                        }
                    }
                }
            }
            i = end;
            continue;
        }
        i += 1;
    }
    join(lines.iter().zip(&drop).filter(|(_, d)| !**d).map(|(l, _)| *l).collect(), apply::newline(content))
}

fn join(lines: Vec<&str>, nl: &str) -> String {
    let mut s = lines.join(nl);
    if !s.is_empty() {
        s.push_str(nl);
    }
    s
}

/// What uninstalling would change, without writing anything.
pub fn preview(env: &Env, opts: UninstallOptions) -> Result<UninstallPreview> {
    let ssh_backup = fsutil::backup_path(&env.ssh_config());
    let git_backup = fsutil::backup_path(&env.gitconfig());
    let backups_available = ssh_backup.exists() || git_backup.exists();
    let index = env.contract(&env.index_gitconfig());

    let mut files = Vec::new();
    for (path, backup) in [(env.ssh_config(), ssh_backup), (env.gitconfig(), git_backup)] {
        let Some(current) = fsutil::read_optional(&path)? else { continue };
        let new = if opts.restore_backups && backup.exists() {
            std::fs::read_to_string(&backup).map_err(|e| crate::Error::io(&backup, e))?
        } else if path == env.ssh_config() {
            without_ssh_include(&current)
        } else {
            without_git_include(&current, &index)
        };
        // Nothing left: the file only ever held Keybund's line, so it goes away.
        files.push((path, (!new.trim().is_empty()).then_some(new), true));
    }
    files.push((env.ssh_keybund_conf(), None, false));
    files.push((env.allowed_signers(), None, false));
    if let Ok(entries) = std::fs::read_dir(env.git_dir()) {
        let mut own: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("gitconfig"))
            .collect();
        own.sort();
        files.extend(own.into_iter().map(|p| (p, None, false)));
    }
    if opts.purge {
        files.push((env.profiles_file(), None, false));
        files.push((env.last_tests_file(), None, false));
        if let Ok(entries) = std::fs::read_dir(env.known_hosts_dir()) {
            let mut own: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
            own.sort();
            files.extend(own.into_iter().map(|p| (p, None, false)));
        }
    }
    Ok(UninstallPreview { plan: apply::plan_files(env, files)?, backups_available })
}

/// Applies a previewed uninstall and removes directories that became empty.
pub fn uninstall(env: &Env, plan: &Plan) -> Result<ApplyReport> {
    let report = apply::apply(env, plan)?;
    // A restored backup is used up: left behind, a later "restore" would
    // bring back a state that is no longer the one from before Keybund.
    for change in plan.changes.iter().filter(|c| c.user_file) {
        let backup = fsutil::backup_path(&change.abs_path);
        if std::fs::read_to_string(&backup).ok() == change.new {
            let _ = std::fs::remove_file(backup);
        }
    }
    for dir in [
        env.known_hosts_dir(),
        env.git_dir(),
        env.ssh_keybund_conf().parent().map(|p| p.to_path_buf()).unwrap_or_default(),
        env.config_dir.clone(),
    ] {
        let _ = std::fs::remove_dir(dir); // only succeeds when empty
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{ensure_git_include, ensure_ssh_include};

    #[test]
    fn removing_the_include_restores_the_original_ssh_config() {
        for original in ["Host a\n    User x\n", "", "# comment\nHost b\n", "Host a\n\nHost b\n"] {
            assert_eq!(
                without_ssh_include(&ensure_ssh_include(original, crate::render::SSH_INCLUDE_LINE)),
                original,
                "{original:?}"
            );
        }
    }

    /// A config written on Windows keeps its CRLF, and leading blank lines
    /// survive: add and remove the include and the file is as it was.
    #[test]
    fn crlf_and_leading_blank_lines_survive() {
        let include = crate::render::SSH_INCLUDE_LINE;
        for original in ["Host a\r\n    User x\r\n", "\r\n\r\nHost a\r\n", "\n\nHost a\n"] {
            let with = ensure_ssh_include(original, include);
            assert!(!with.replace("\r\n", "").contains('\n') || !original.contains('\r'), "{with:?}");
            assert_eq!(without_ssh_include(&with), original, "{original:?}");
        }
        assert_eq!(ensure_ssh_include("Host a\r\n", include), format!("{include}\r\n\r\nHost a\r\n"));
        let index = "~/.config/keybund/git/index.gitconfig";
        let git = "[user]\r\n\tname = T\r\n";
        let with = ensure_git_include(git, index);
        assert_eq!(
            with,
            format!("[user]\r\n\tname = T\r\n\r\n# Added by Keybund\r\n[include]\r\n\tpath = {index}\r\n")
        );
        assert_eq!(without_git_include(&with, index), git);
        // Removing taken-over blocks keeps the line ending too.
        let ssh = "Host a\r\n    IdentityFile ~/.ssh/a\r\n\r\nHost b\r\n    User x\r\n";
        let blocks = crate::import::parse_ssh_config(ssh);
        assert_eq!(crate::import::without_blocks(ssh, &[&blocks[0]]), "Host b\r\n    User x\r\n");
    }

    #[test]
    fn removing_the_include_restores_the_original_gitconfig() {
        let index = "~/.config/keybund/git/index.gitconfig";
        for original in ["[user]\n\tname = T\n", "", "[user]\n\tname = T\n[include]\n\tpath = ~/other.gitconfig\n"] {
            assert_eq!(without_git_include(&ensure_git_include(original, index), index), original, "{original:?}");
        }
        // A shared [include] section keeps the user's own path.
        let shared = format!("[include]\n\tpath = ~/other\n\tpath = {index}\n[core]\n\teditor = vim\n");
        assert_eq!(without_git_include(&shared, index), "[include]\n\tpath = ~/other\n[core]\n\teditor = vim\n");
    }
}
