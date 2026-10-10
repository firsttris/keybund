use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Writes `content` to `path` atomically (temp file in the same directory,
/// then rename) and applies `mode` on Unix.
pub fn atomic_write(path: &Path, content: &str, mode: Option<u32>) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{file_name}.keybund-tmp-{}", std::process::id()));
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
