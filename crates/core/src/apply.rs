//! Computes the changes needed to bring all config files in line with
//! `profiles.toml`, shows them as diffs and writes them atomically.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Config;
use crate::render::{self, SSH_INCLUDE_LINE};
use crate::{fsutil, keygen, Env, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
}

/// A pending change to one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileChange {
    /// Path for display, relative to `~` where possible.
    pub path: String,
    pub kind: ChangeKind,
    /// Unified diff of the change.
    pub diff: String,
    pub added: u32,
    pub removed: u32,
    /// `true` for files that belong to the user (only an include line is
    /// added); they get a one-time backup before the first write.
    pub user_file: bool,
    #[serde(skip)]
    abs_path: PathBuf,
    #[serde(skip)]
    old: Option<String>,
    #[serde(skip)]
    new: Option<String>,
    #[serde(skip)]
    mode: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Plan {
    pub changes: Vec<FileChange>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
    pub fn added(&self) -> u32 {
        self.changes.iter().map(|c| c.added).sum()
    }
    pub fn removed(&self) -> u32 {
        self.changes.iter().map(|c| c.removed).sum()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ApplyReport {
    pub written: Vec<String>,
    pub deleted: Vec<String>,
    pub backups: Vec<String>,
}

/// Public keys of all signing profiles, for `allowed_signers`.
pub fn signing_pubkeys(env: &Env, config: &Config) -> BTreeMap<String, String> {
    config
        .active_profiles()
        .filter(|p| p.git.as_ref().is_some_and(|g| g.sign))
        .filter_map(|p| keygen::read_public(env, &p.key).ok().map(|k| (p.id.clone(), k.without_comment())))
        .collect()
}

/// Computes every change without writing anything.
pub fn plan(env: &Env, config: &Config) -> Result<Plan> {
    let mut changes = Vec::new();
    let rendered = render::render_all(config, env, &signing_pubkeys(env, config));
    let owned: Vec<PathBuf> = rendered.iter().map(|f| f.path.clone()).collect();

    for file in rendered {
        let mode = if file.path.starts_with(env.ssh_dir()) { Some(0o600) } else { Some(0o644) };
        push_change(env, &mut changes, &file.path, Some(file.content), mode, false)?;
    }

    // Profile gitconfigs of deleted or archived profiles.
    if let Ok(entries) = fs::read_dir(env.git_dir()) {
        let mut stale: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("gitconfig"))
            .filter(|p| !owned.contains(p))
            .collect();
        stale.sort();
        for path in stale {
            push_change(env, &mut changes, &path, None, None, false)?;
        }
    }
    if !owned.contains(&env.allowed_signers()) && env.allowed_signers().exists() {
        push_change(env, &mut changes, &env.allowed_signers(), None, None, false)?;
    }

    // The user's own files only get one include line each.
    let ssh_config = fsutil::read_optional(&env.ssh_config())?;
    let new_ssh = ensure_ssh_include(ssh_config.as_deref().unwrap_or(""));
    push_change(env, &mut changes, &env.ssh_config(), Some(new_ssh), Some(0o600), true)?;

    let gitconfig = fsutil::read_optional(&env.gitconfig())?;
    let index = env.contract(&env.index_gitconfig());
    let new_git = ensure_git_include(gitconfig.as_deref().unwrap_or(""), &index);
    push_change(env, &mut changes, &env.gitconfig(), Some(new_git), None, true)?;

    Ok(Plan { changes })
}

fn push_change(
    env: &Env,
    changes: &mut Vec<FileChange>,
    path: &Path,
    new: Option<String>,
    mode: Option<u32>,
    user_file: bool,
) -> Result<()> {
    let old = fsutil::read_optional(path)?;
    if old == new {
        return Ok(());
    }
    let kind = match (&old, &new) {
        (None, _) => ChangeKind::Create,
        (Some(_), None) => ChangeKind::Delete,
        (Some(_), Some(_)) => ChangeKind::Modify,
    };
    let display = env.contract(path);
    let (diff, added, removed) = diff(&display, old.as_deref().unwrap_or(""), new.as_deref().unwrap_or(""));
    changes.push(FileChange {
        path: display,
        kind,
        diff,
        added,
        removed,
        user_file,
        abs_path: path.to_path_buf(),
        old,
        new,
        mode,
    });
    Ok(())
}

fn diff(name: &str, old: &str, new: &str) -> (String, u32, u32) {
    let d = similar::TextDiff::from_lines(old, new);
    let mut added = 0;
    let mut removed = 0;
    for change in d.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => removed += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    let text = d.unified_diff().context_radius(3).header(name, name).to_string();
    (text, added, removed)
}

/// Writes a plan. Each file is checked against the content the plan was
/// computed from, so a concurrent edit is never overwritten silently.
pub fn apply(env: &Env, plan: &Plan) -> Result<ApplyReport> {
    for change in &plan.changes {
        if fsutil::read_optional(&change.abs_path)? != change.old {
            return Err(Error::StalePlan(change.path.clone()));
        }
    }
    fsutil::ensure_dir(&env.ssh_dir(), Some(0o700))?;
    fsutil::ensure_dir(&env.ssh_dir().join("config.d"), Some(0o700))?;
    fsutil::ensure_dir(&env.config_dir, Some(0o700))?;

    let mut report = ApplyReport::default();
    for change in &plan.changes {
        if change.user_file {
            if let Some(b) = fsutil::backup_once(&change.abs_path)? {
                report.backups.push(env.contract(&b));
            }
        }
        match &change.new {
            Some(content) => {
                // Keep the permissions of existing user files.
                let mode = if change.old.is_some() && change.user_file { None } else { change.mode };
                fsutil::atomic_write(&change.abs_path, content, mode)?;
                report.written.push(change.path.clone());
            }
            None => {
                fs::remove_file(&change.abs_path).map_err(|e| Error::io(&change.abs_path, e))?;
                report.deleted.push(change.path.clone());
            }
        }
    }
    Ok(report)
}

/// Convenience: plan and apply in one go.
pub fn apply_config(env: &Env, config: &Config) -> Result<ApplyReport> {
    apply(env, &plan(env, config)?)
}

fn is_keybund_include(line: &str) -> bool {
    let t = line.trim();
    let mut parts = t.split_whitespace();
    matches!(parts.next(), Some(k) if k.eq_ignore_ascii_case("include"))
        && parts.any(|p| p.trim_matches('"').ends_with("config.d/keybund.conf"))
}

fn is_directive(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with('#')
}

/// `true` if Keybund's `Include` is the first directive of the SSH config.
pub fn ssh_include_on_top(content: &str) -> bool {
    content.lines().find(|l| is_directive(l)).is_some_and(is_keybund_include)
}

/// `true` if the SSH config contains Keybund's `Include` anywhere.
pub fn ssh_include_present(content: &str) -> bool {
    content.lines().any(is_keybund_include)
}

/// Makes Keybund's `Include` the first line of `~/.ssh/config`. An
/// `Include` below a `Host` block would only apply to that host.
pub fn ensure_ssh_include(content: &str) -> String {
    if ssh_include_on_top(content) {
        return content.to_string();
    }
    let rest: Vec<&str> = content.lines().filter(|l| !is_keybund_include(l)).collect();
    let mut out = format!("{SSH_INCLUDE_LINE}\n");
    if rest.iter().any(|l| !l.trim().is_empty()) {
        out.push('\n');
        let joined = rest.join("\n");
        out.push_str(joined.trim_start_matches('\n'));
        out.push('\n');
    }
    out
}

/// `true` if `~/.gitconfig` includes Keybund's index file.
pub fn git_include_present(content: &str, index_path: &str) -> bool {
    let mut in_include = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_include = t.trim_start_matches('[').trim_end_matches(']').trim().eq_ignore_ascii_case("include");
            continue;
        }
        if in_include {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case("path") && v.trim().trim_matches('"') == index_path {
                    return true;
                }
            }
        }
    }
    false
}

/// Appends an `[include]` for Keybund's index at the end of `~/.gitconfig`,
/// so the per-folder settings win over global ones.
pub fn ensure_git_include(content: &str, index_path: &str) -> String {
    if git_include_present(content, index_path) {
        return content.to_string();
    }
    let mut out = content.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("# Added by Keybund\n[include]\n\tpath = {index_path}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_include_goes_first() {
        let user = "Host github.com\n    IdentityFile ~/.ssh/id_ed25519\n";
        let out = ensure_ssh_include(user);
        assert!(out.starts_with(SSH_INCLUDE_LINE));
        assert!(out.ends_with(user));
        assert_eq!(ensure_ssh_include(&out), out);
    }

    #[test]
    fn misplaced_ssh_include_is_moved() {
        let user = "Host a\n    User x\nInclude ~/.ssh/config.d/keybund.conf\n";
        assert!(ssh_include_present(user));
        assert!(!ssh_include_on_top(user));
        let out = ensure_ssh_include(user);
        assert_eq!(out, format!("{SSH_INCLUDE_LINE}\n\nHost a\n    User x\n"));
    }

    #[test]
    fn leading_comments_are_fine() {
        let user = "# my config\nInclude ~/.ssh/config.d/keybund.conf\nHost a\n";
        assert!(ssh_include_on_top(user));
        assert_eq!(ensure_ssh_include(user), user);
    }

    #[test]
    fn empty_ssh_config() {
        assert_eq!(ensure_ssh_include(""), format!("{SSH_INCLUDE_LINE}\n"));
    }

    #[test]
    fn git_include_is_appended_once() {
        let user = "[user]\n\tname = T\n";
        let out = ensure_git_include(user, "~/.config/keybund/git/index.gitconfig");
        assert!(out.starts_with(user));
        assert!(out.contains("[include]\n\tpath = ~/.config/keybund/git/index.gitconfig\n"));
        assert_eq!(ensure_git_include(&out, "~/.config/keybund/git/index.gitconfig"), out);
    }
}
