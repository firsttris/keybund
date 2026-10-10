//! Opening a URL in the user's browser from the CLI, the AppImage or a
//! development container (toolbox/distrobox).
//!
//! Plain `xdg-open` fails in two common setups: inside a toolbox there is
//! usually no `xdg-open` and no access to the host's browser, and an
//! AppImage passes its own library and data paths (`APPDIR`,
//! `XDG_DATA_DIRS`, GTK variables) on to the browser it starts.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Error, Result};

/// Variables an AppImage sets that must not reach other programs.
const APPIMAGE_VARS: &[&str] = &["APPDIR", "APPIMAGE", "ARGV0", "OWD", "APPIMAGE_SILENT_INSTALL"];

/// Environment for the opener: everything that points into the AppImage is
/// dropped, path lists lose their AppImage entries.
pub fn clean_env(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<(OsString, OsString)> {
    let vars: Vec<(OsString, OsString)> = vars.into_iter().collect();
    let appdir = vars.iter().find(|(k, _)| k == "APPDIR").map(|(_, v)| v.to_string_lossy().into_owned());
    vars.into_iter()
        .filter(|(k, _)| !APPIMAGE_VARS.iter().any(|a| k == OsStr::new(a)))
        .filter_map(|(k, v)| {
            let Some(appdir) = appdir.as_deref().filter(|a| !a.is_empty()) else { return Some((k, v)) };
            let value = v.to_string_lossy();
            if !value.contains(appdir) {
                return Some((k, v));
            }
            // A path list keeps its other entries, a single path is dropped.
            let rest: Vec<&str> = value.split(':').filter(|p| !p.is_empty() && !p.starts_with(appdir)).collect();
            (value.contains(':') && !rest.is_empty()).then(|| (k, OsString::from(rest.join(":"))))
        })
        .collect()
}

/// `true` inside a toolbox or distrobox container.
pub fn in_container(vars: &[(OsString, OsString)]) -> bool {
    let has = |name: &str| vars.iter().any(|(k, _)| k == name);
    has("TOOLBOX_PATH") || has("CONTAINER_ID") || Path::new("/run/.containerenv").exists()
}

/// The commands to try, in order.
pub fn opener_commands(url: &str, vars: Vec<(OsString, OsString)>, container: bool) -> Vec<Command> {
    let make = |program: &str, args: &[&str]| {
        let mut cmd = Command::new(program);
        cmd.args(args).arg(url).env_clear().envs(vars.iter().map(|(k, v)| (k, v)));
        cmd
    };
    let mut commands = Vec::new();
    if container {
        commands.push(make("flatpak-spawn", &["--host", "xdg-open"]));
        commands.push(make("distrobox-host-exec", &["xdg-open"]));
    }
    commands.push(make("xdg-open", &[]));
    commands.push(make("gio", &["open"]));
    commands
}

/// Opens an `https://` URL in the default browser.
pub fn open_url(url: &str) -> Result<()> {
    if !url.starts_with("https://") {
        return Err(Error::ToolFailed {
            tool: "Browser".into(),
            message: format!("Nur https-Links werden geöffnet: {url}"),
        });
    }
    let vars = clean_env(std::env::vars_os());
    let container = in_container(&vars);
    if cfg!(target_os = "macos") {
        return run(Command::new("open").arg(url), Duration::from_secs(5));
    }
    if cfg!(windows) {
        return run(Command::new("cmd").args(["/C", "start", ""]).arg(url), Duration::from_secs(5));
    }
    let mut last = None;
    for mut cmd in opener_commands(url, vars, container) {
        match run(&mut cmd, Duration::from_secs(5)) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::ToolMissing("xdg-open".into())))
}

/// Runs an opener. It counts as successful if it exits with 0 or is still
/// running after `grace` (some openers stay alive with the browser).
fn run(cmd: &mut Command, grace: Duration) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ToolMissing(program.clone())
        } else {
            Error::ToolFailed { tool: program.clone(), message: e.to_string() }
        }
    })?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(Error::ToolFailed { tool: program, message: format!("Exit-Code {status}") });
            }
            Ok(None) if start.elapsed() >= grace => return Ok(()),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(Error::ToolFailed { tool: program, message: e.to_string() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(list: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        list.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
    }

    fn get<'a>(vars: &'a [(OsString, OsString)], key: &str) -> Option<&'a str> {
        vars.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.to_str())
    }

    #[test]
    fn strips_appimage_environment() {
        let cleaned = clean_env(vars(&[
            ("APPDIR", "/tmp/.mount_Keybund"),
            ("APPIMAGE", "/home/t/Keybund.AppImage"),
            ("XDG_DATA_DIRS", "/tmp/.mount_Keybund/usr/share:/usr/local/share:/usr/share"),
            ("GTK_PATH", "/tmp/.mount_Keybund/usr/lib/gtk-3.0"),
            ("GIO_MODULE_DIR", "/tmp/.mount_Keybund/usr/lib/gio/modules"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_Keybund/usr/lib"),
            ("PATH", "/tmp/.mount_Keybund/usr/bin:/usr/bin"),
            ("HOME", "/home/t"),
            ("DISPLAY", ":0"),
        ]));
        assert_eq!(get(&cleaned, "XDG_DATA_DIRS"), Some("/usr/local/share:/usr/share"));
        assert_eq!(get(&cleaned, "PATH"), Some("/usr/bin"));
        assert_eq!(get(&cleaned, "HOME"), Some("/home/t"));
        assert_eq!(get(&cleaned, "DISPLAY"), Some(":0"));
        for gone in ["APPDIR", "APPIMAGE", "GTK_PATH", "GIO_MODULE_DIR", "LD_LIBRARY_PATH"] {
            assert_eq!(get(&cleaned, gone), None, "{gone}");
        }
    }

    #[test]
    fn leaves_a_normal_environment_alone() {
        let input = vars(&[("PATH", "/usr/bin"), ("XDG_DATA_DIRS", "/usr/share"), ("LD_LIBRARY_PATH", "/opt/x")]);
        assert_eq!(clean_env(input.clone()), input);
    }

    #[test]
    fn container_uses_the_host_first() {
        let names = |cmds: Vec<Command>| -> Vec<String> {
            cmds.iter()
                .map(|c| {
                    let mut parts = vec![c.get_program().to_string_lossy().into_owned()];
                    parts.extend(c.get_args().map(|a| a.to_string_lossy().into_owned()));
                    parts.join(" ")
                })
                .collect()
        };
        let url = "https://github.com/settings/ssh/new";
        assert_eq!(
            names(opener_commands(url, vec![], true)),
            [
                format!("flatpak-spawn --host xdg-open {url}"),
                format!("distrobox-host-exec xdg-open {url}"),
                format!("xdg-open {url}"),
                format!("gio open {url}"),
            ]
        );
        assert_eq!(names(opener_commands(url, vec![], false))[0], format!("xdg-open {url}"));
        assert!(in_container(&vars(&[("TOOLBOX_PATH", "/usr/bin/toolbox")])));
    }

    #[test]
    fn rejects_non_https() {
        assert!(open_url("file:///etc/passwd").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn falls_back_when_an_opener_fails() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let script = |name: &str, code: i32| {
            let path = dir.path().join(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\necho \"{name} $@ APPDIR=$APPDIR\" >> '{}'\nexit {code}\n", log.display()),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        script("xdg-open", 3);
        script("gio", 0);
        let path = format!("{}:/usr/bin:/bin", dir.path().display());
        let env = clean_env(vars(&[("PATH", &path), ("APPDIR", "/tmp/.mount_x")]));
        let mut ok = false;
        for mut cmd in opener_commands("https://example.org", env, false) {
            if run(&mut cmd, Duration::from_secs(5)).is_ok() {
                ok = true;
                break;
            }
        }
        assert!(ok);
        let log = std::fs::read_to_string(&log).unwrap();
        assert_eq!(log, "xdg-open https://example.org APPDIR=\ngio open https://example.org APPDIR=\n");
    }
}
