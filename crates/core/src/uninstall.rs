//! The way back: removes everything Keybund wrote. The user's keys are
//! never touched; `profiles.toml` stays unless `purge` is set.

use serde::{Deserialize, Serialize};

use crate::apply::{self, ApplyReport, Plan};
use crate::render::SSH_INCLUDE_LINE;
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
    let mut out: Vec<&str> = Vec::new();
    let mut skip_blank = false;
    for line in lines {
        let t = line.trim();
        let is_ours = t == SSH_INCLUDE_LINE
            || (t.to_ascii_lowercase().starts_with("include")
                && t.trim_end_matches('"').ends_with("config.d/keybund.conf"));
        if is_ours {
            skip_blank = true;
            continue;
        }
        if skip_blank && line.trim().is_empty() {
            skip_blank = false;
            continue;
        }
        skip_blank = false;
        out.push(line);
    }
    join(out)
}

/// `~/.gitconfig` without the `[include]` of Keybund's index (and the
/// comment and blank line Keybund added with it).
pub fn without_git_include(content: &str, index_path: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut drop = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        let header = lines[i].trim().trim_start_matches('[').trim_end_matches(']').trim().to_ascii_lowercase();
        if lines[i].trim().starts_with('[') && header == "include" {
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
            let ours = |l: &&str| {
                l.split_once('=').is_some_and(|(k, v)| {
                    k.trim().eq_ignore_ascii_case("path") && v.trim().trim_matches('"') == index_path
                })
            };
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
    join(lines.iter().zip(&drop).filter(|(_, d)| !**d).map(|(l, _)| *l).collect())
}

fn join(lines: Vec<&str>) -> String {
    let mut s = lines.join("\n");
    if !s.is_empty() {
        s.push('\n');
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
        files.push((path, Some(new), true));
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
    }
    Ok(UninstallPreview { plan: apply::plan_files(env, files)?, backups_available })
}

/// Applies a previewed uninstall and removes directories that became empty.
pub fn uninstall(env: &Env, plan: &Plan) -> Result<ApplyReport> {
    let report = apply::apply(env, plan)?;
    for dir in [
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
            assert_eq!(without_ssh_include(&ensure_ssh_include(original)), original, "{original:?}");
        }
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
