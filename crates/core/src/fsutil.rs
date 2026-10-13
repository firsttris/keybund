use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Writes `content` to `path` atomically (temp file in the same directory,
/// then rename). `mode` is applied on Unix; without one an existing file
/// keeps its permissions (e.g. 0600 of `~/.ssh/config`, which ssh insists
/// on). A symlink (dotfile managers) is written through, not replaced.
pub fn atomic_write(path: &Path, content: &str, mode: Option<u32>) -> Result<()> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let target = resolve_link(path)?;
    let path = target.as_path();
    let mode = mode.or_else(|| self::mode(path));
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // Unique per write, so two threads never share a temp file.
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{file_name}.keybund-tmp-{}-{n}", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        if let Some(mode) = mode {
            set_mode(&tmp, mode)?;
        }
        f.write_all(content.as_bytes()).map_err(|e| Error::io(&tmp, e))?;
        f.sync_all().map_err(|e| Error::io(&tmp, e))?;
        fs::rename(&tmp, path).map_err(|e| Error::io(path, e))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Follows symlinks by hand, so a dangling link (a dotfiles repository
/// not checked out yet) still names the file to create.
fn resolve_link(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..40 {
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                let target = fs::read_link(&current).map_err(|e| Error::io(&current, e))?;
                current =
                    if target.is_absolute() { target } else { current.parent().unwrap_or(Path::new(".")).join(target) };
            }
            _ => return Ok(current),
        }
    }
    Err(Error::io(path, std::io::Error::other("symlink loop")))
}

/// Copies `path` to `<path>.keybund-backup`, replacing an older backup.
pub fn backup(path: &Path) -> Result<PathBuf> {
    let backup = backup_path(path);
    fs::copy(path, &backup).map_err(|e| Error::io(&backup, e))?;
    Ok(backup)
}

/// Copies `path` to `<path>.keybund-backup` unless such a backup already
/// exists. Returns the backup path if one was created.
pub fn backup_once(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = backup_path(path);
    if backup.exists() {
        return Ok(None);
    }
    fs::copy(path, &backup).map_err(|e| Error::io(&backup, e))?;
    Ok(Some(backup))
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".keybund-backup");
    path.with_file_name(name)
}

/// Creates a directory (and parents) and applies `mode` to the leaf.
pub fn ensure_dir(path: &Path, mode: Option<u32>) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
        if let Some(mode) = mode {
            set_mode(path, mode)?;
        }
    }
    Ok(())
}

pub fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

#[cfg(unix)]
pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| Error::io(path, e))
}

#[cfg(not(unix))]
pub fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Permission bits of a file, `None` if it does not exist or on non-Unix.
#[cfg(unix)]
pub fn mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).ok().map(|m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
pub fn mode(_path: &Path) -> Option<u32> {
    None
}

/// Tests only: writes an executable stand-in script. Written in this process, the file would be
/// open for writing while another test thread forks, the child would inherit that handle, and
/// Linux refuses to execute a file that is open for writing (ETXTBSY, `execve(2)`); under
/// `cargo llvm-cov` that failed CI. A short `sh` child writes it instead, so no write handle ever
/// lives in the test process.
#[cfg(all(test, unix))]
pub fn write_test_script(path: &Path, content: &str) {
    use std::io::Write;
    let mut child = std::process::Command::new("sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(content.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success(), "could not write {}", path.display());
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn keeps_permissions_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        fs::write(&config, "Host a\n").unwrap();
        set_mode(&config, 0o600).unwrap();
        atomic_write(&config, "Include x\nHost a\n", None).unwrap();
        assert_eq!(mode(&config), Some(0o600), "ssh refuses a group-writable config");
        assert_eq!(fs::read_to_string(&config).unwrap(), "Include x\nHost a\n");

        // ~/.ssh/config -> dotfiles/ssh_config stays a link.
        let real = dir.path().join("ssh_config");
        fs::write(&real, "old\n").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        atomic_write(&link, "new\n", None).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new\n");

        // An explicit mode still wins.
        atomic_write(&config, "x\n", Some(0o644)).unwrap();
        assert_eq!(mode(&config), Some(0o644));
    }

    /// A dotfiles link whose repository is not checked out yet: the file is
    /// created where the link points, the link stays.
    #[test]
    fn writes_through_a_dangling_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join(".gitconfig");
        std::os::unix::fs::symlink("dotfiles/git/config", &link).unwrap();
        atomic_write(&link, "[user]\n", None).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(dir.path().join("dotfiles/git/config")).unwrap(), "[user]\n");
        assert_eq!(fs::read_to_string(&link).unwrap(), "[user]\n");
    }
}
