//! Computes the changes needed to bring all config files in line with
//! `profiles.toml`, shows them as diffs and writes them atomically.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Config;
use crate::render;
use crate::{fsutil, keygen, store, Env, Error, Result};

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
    pub(crate) abs_path: PathBuf,
    #[serde(skip)]
    pub(crate) old: Option<String>,
    #[serde(skip)]
    pub(crate) new: Option<String>,
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
    // A hand-edited profiles.toml is validated here too, not only on save:
    // nothing invalid reaches ssh or git (a `'` in a key path would stop ssh
    // for every host, an id with `..` would leave the config directory).
    store::validate_in(env, config)?;
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
    let include = render::ssh_include_line(env, &config.settings);
    let new_ssh = ensure_ssh_include(ssh_config.as_deref().unwrap_or(""), &include);
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

/// A plan that replaces the content of one of the user's own files (with a
/// one-time backup), e.g. to remove migrated blocks from `~/.ssh/config`.
pub fn plan_user_file(env: &Env, path: &Path, new_content: String) -> Result<Plan> {
    plan_files(env, vec![(path.to_path_buf(), Some(new_content), true)])
}

/// A plan for arbitrary files: `(path, new content or None to delete,
/// user_file)`. Unchanged files are left out.
pub fn plan_files(env: &Env, files: Vec<(PathBuf, Option<String>, bool)>) -> Result<Plan> {
    let mut changes = Vec::new();
    for (path, content, user_file) in files {
        if content.is_none() && !path.exists() {
            continue;
        }
        push_change(env, &mut changes, &path, content, None, user_file)?;
    }
    Ok(Plan { changes })
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
    if plan.changes.iter().any(|c| c.new.as_deref().is_some_and(|n| n.contains("known_hosts.d/"))) {
        // ssh cannot add host keys to a file in a missing folder.
        fsutil::ensure_dir(&env.known_hosts_dir(), Some(0o700))?;
    }

    let mut report = ApplyReport::default();
    for change in &plan.changes {
        // A write that takes Keybund out again (uninstall) is not a takeover:
        // the backup from before Keybund stays as it is, and a file Keybund
        // created itself gets none (it would only hold Keybund's own line).
        let keeps_keybund = change.new.as_deref().is_some_and(|n| keybund_in(env, &change.abs_path, n));
        if change.user_file && keeps_keybund {
            // The backup holds the file as it was before Keybund: taken anew
            // whenever Keybund is not (or no longer) in it, e.g. after an
            // uninstall, so "restore" never brings back an outdated state.
            let before_keybund = match &change.old {
                Some(old) => !keybund_in(env, &change.abs_path, old),
                None => true,
            };
            if !before_keybund {
                if let Some(b) = fsutil::backup_once(&change.abs_path)? {
                    report.backups.push(env.contract(&b));
                }
            } else if change.old.is_some() {
                report.backups.push(env.contract(&fsutil::backup(&change.abs_path)?));
            } else {
                // There was no file before Keybund: an old backup would be wrong.
                let _ = fs::remove_file(fsutil::backup_path(&change.abs_path));
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

/// `true` if the user's file already contains Keybund's include line.
fn keybund_in(env: &Env, path: &Path, content: &str) -> bool {
    if path == env.ssh_config() {
        ssh_include_present(content)
    } else if path == env.gitconfig() {
        git_include_present(content, &env.contract(&env.index_gitconfig()))
    } else {
        false
    }
}

/// Convenience: plan and apply in one go.
pub fn apply_config(env: &Env, config: &Config) -> Result<ApplyReport> {
    apply(env, &plan(env, config)?)
}

pub(crate) fn is_keybund_include(line: &str) -> bool {
    let t = line.trim();
    let mut parts = t.split_whitespace();
    matches!(parts.next(), Some(k) if k.eq_ignore_ascii_case("include"))
        && parts.any(|p| p.trim_matches('"').ends_with("config.d/keybund.conf"))
}

/// `true` if the line includes keybund.conf, also through a pattern such as
/// `Include ~/.ssh/config.d/*.conf` (ssh globs Include arguments; relative
/// ones are anchored at ~/.ssh). Such a line is the user's own and is never
/// removed – but it does make Keybund's own Include unnecessary.
pub(crate) fn includes_keybund_conf(line: &str) -> bool {
    let t = line.trim();
    let mut parts = t.split_whitespace();
    if !matches!(parts.next(), Some(k) if k.eq_ignore_ascii_case("include")) {
        return false;
    }
    parts.any(|p| {
        let p = p.trim_matches('"');
        let rel = p.rsplit_once("/.ssh/").map_or(p, |(_, rest)| rest);
        glob_match(rel, "config.d/keybund.conf")
    })
}

/// The user's own `Include` pattern that reaches keybund.conf.
fn is_glob_include(line: &str) -> bool {
    includes_keybund_conf(line) && line.contains(['*', '?'])
}

/// `*` and `?` as in ssh's Include globbing, `*` not crossing a `/`.
fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], n) || (n.first().is_some_and(|c| *c != b'/') && go(p, &n[1..])),
            (Some(b'?'), Some(c)) if *c != b'/' => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

/// The `Include` line without Keybund's path: `None` when the line held
/// nothing else, the remaining line when the user put other files on it
/// (`Include ~/.ssh/config.d/keybund.conf ~/.ssh/work.conf`).
pub fn without_keybund_path(line: &str) -> Option<String> {
    let indent = &line[..line.len() - line.trim_start().len()];
    let mut parts = line.split_whitespace();
    let keyword = parts.next()?;
    let rest: Vec<&str> = parts.filter(|p| !p.trim_matches('"').ends_with("config.d/keybund.conf")).collect();
    (!rest.is_empty()).then(|| format!("{indent}{keyword} {}", rest.join(" ")))
}

fn is_directive(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with('#')
}

/// `true` if Keybund's `Include` (or a pattern reaching it) is the first
/// directive of the SSH config.
pub fn ssh_include_on_top(content: &str) -> bool {
    content.lines().find(|l| is_directive(l)).is_some_and(includes_keybund_conf)
}

/// `true` if the SSH config includes keybund.conf anywhere.
pub fn ssh_include_present(content: &str) -> bool {
    content.lines().any(includes_keybund_conf)
}

/// Makes `include` (Keybund's `Include` line) the first directive of
/// `~/.ssh/config`. An `Include` below a `Host` block would only apply to
/// that host; another variant of the line (`~` or full path) is replaced.
pub fn ensure_ssh_include(content: &str, include: &str) -> String {
    // Already first: our own line, or the user's pattern that reaches it.
    if content.lines().find(|l| is_directive(l)).is_some_and(|l| l.trim() == include || is_glob_include(l)) {
        return content.to_string();
    }
    let rest: Vec<String> = content
        .lines()
        .filter_map(|l| if is_keybund_include(l) { without_keybund_path(l) } else { Some(l.to_string()) })
        .collect();
    let mut out = format!("{include}\n");
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

/// `true` if nothing is set after Keybund's include in `~/.gitconfig`: git
/// takes the last value, so a `[user]` appended later (e.g. by
/// `git config --global`) would override every key's settings.
pub fn git_include_last(content: &str, index_path: &str) -> bool {
    let mut seen = false;
    let mut in_include = false;
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        if t.starts_with('[') {
            // A section header alone sets nothing; its first setting decides.
            in_include = t.trim_start_matches('[').trim_end_matches(']').trim().eq_ignore_ascii_case("include");
            continue;
        }
        let is_ours = in_include
            && t.split_once('=').is_some_and(|(k, v)| {
                k.trim().eq_ignore_ascii_case("path") && v.trim().trim_matches('"') == index_path
            });
        if is_ours {
            seen = true;
        } else if seen {
            return false;
        }
    }
    seen
}

/// Makes sure `~/.gitconfig` includes Keybund's index as its last setting,
/// so the per-folder settings win over global ones. An include that is no
/// longer last is moved to the end.
pub fn ensure_git_include(content: &str, index_path: &str) -> String {
    if git_include_last(content, index_path) {
        return content.to_string();
    }
    let content = if git_include_present(content, index_path) {
        crate::uninstall::without_git_include(content, index_path)
    } else {
        content.to_string()
    };
    let mut out = content.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&format!("# Added by Keybund\n[include]\n\tpath = {}\n", crate::render::git_value(index_path)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::SSH_INCLUDE_LINE;

    #[test]
    fn git_include_stays_last() {
        let idx = "~/.config/keybund/git/index.gitconfig";
        let first = ensure_git_include("[user]\n\tname = T\n", idx);
        assert!(git_include_last(&first, idx));
        assert_eq!(ensure_git_include(&first, idx), first);
        // `git config --global user.email …` appends after it.
        let later = format!("{first}[user]\n\temail = me@global.com\n");
        assert!(git_include_present(&later, idx) && !git_include_last(&later, idx));
        let fixed = ensure_git_include(&later, idx);
        assert!(git_include_last(&fixed, idx), "{fixed}");
        assert_eq!(fixed.matches("keybund/git/index.gitconfig").count(), 1);
        assert!(fixed.contains("email = me@global.com"));
        assert!(fixed.find("me@global.com").unwrap() < fixed.find("index.gitconfig").unwrap());
    }

    #[test]
    fn an_include_line_shared_with_other_paths_keeps_them() {
        // ssh allows several paths on one Include; the user's other file must not vanish.
        let shared = "Include ~/.ssh/config.d/keybund.conf ~/.ssh/work.conf\n\nHost a\n";
        assert_eq!(
            without_keybund_path("Include ~/.ssh/config.d/keybund.conf ~/.ssh/work.conf"),
            Some("Include ~/.ssh/work.conf".into())
        );
        assert_eq!(without_keybund_path("  Include ~/.ssh/config.d/keybund.conf"), None);
        assert_eq!(
            ensure_ssh_include(shared, SSH_INCLUDE_LINE),
            "Include ~/.ssh/config.d/keybund.conf\n\nInclude ~/.ssh/work.conf\n\nHost a\n"
        );
        assert_eq!(crate::uninstall::without_ssh_include(shared), "Include ~/.ssh/work.conf\n\nHost a\n");
    }

    /// `Include ~/.ssh/config.d/*.conf` already reaches keybund.conf: no
    /// second line is added, the doctor sees the include, and an uninstall
    /// leaves the user's pattern alone.
    #[test]
    fn a_glob_include_counts_but_is_never_removed() {
        for content in [
            "Include ~/.ssh/config.d/*.conf\n\nHost a\n",
            "Include config.d/*\n\nHost a\n",
            "Include \"/home/t/.ssh/config.d/keybund.c?nf\"\n",
        ] {
            assert!(ssh_include_on_top(content), "{content:?}");
            assert!(ssh_include_present(content), "{content:?}");
            assert_eq!(ensure_ssh_include(content, SSH_INCLUDE_LINE), content);
            assert_eq!(crate::uninstall::without_ssh_include(content), content);
        }
        for content in
            ["Include ~/.ssh/other/*.conf\n", "Include ~/.ssh/config.d/work.conf\n", "Include ~/*/keybund.conf\n"]
        {
            assert!(!ssh_include_present(content), "{content:?}");
        }
        assert!(glob_match("config.d/*", "config.d/keybund.conf"));
        assert!(!glob_match("*", "config.d/keybund.conf"), "* does not cross a slash");
    }

    #[test]
    fn ssh_include_goes_first() {
        let user = "Host github.com\n    IdentityFile ~/.ssh/id_ed25519\n";
        let out = ensure_ssh_include(user, SSH_INCLUDE_LINE);
        assert!(out.starts_with(SSH_INCLUDE_LINE));
        assert!(out.ends_with(user));
        assert_eq!(ensure_ssh_include(&out, SSH_INCLUDE_LINE), out);
    }

    #[test]
    fn misplaced_ssh_include_is_moved() {
        let user = "Host a\n    User x\nInclude ~/.ssh/config.d/keybund.conf\n";
        assert!(ssh_include_present(user));
        assert!(!ssh_include_on_top(user));
        let out = ensure_ssh_include(user, SSH_INCLUDE_LINE);
        assert_eq!(out, format!("{SSH_INCLUDE_LINE}\n\nHost a\n    User x\n"));
    }

    #[test]
    fn leading_comments_are_fine() {
        let user = "# my config\nInclude ~/.ssh/config.d/keybund.conf\nHost a\n";
        assert!(ssh_include_on_top(user));
        assert_eq!(ensure_ssh_include(user, SSH_INCLUDE_LINE), user);
    }

    #[test]
    fn include_variant_is_replaced() {
        let user = "Include ~/.ssh/config.d/keybund.conf\n\nHost a\n";
        let out = ensure_ssh_include(user, "Include /home/t/.ssh/config.d/keybund.conf");
        assert_eq!(out, "Include /home/t/.ssh/config.d/keybund.conf\n\nHost a\n");
        assert_eq!(ensure_ssh_include(&out, SSH_INCLUDE_LINE), user);
    }

    #[test]
    fn empty_ssh_config() {
        assert_eq!(ensure_ssh_include("", SSH_INCLUDE_LINE), format!("{SSH_INCLUDE_LINE}\n"));
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
