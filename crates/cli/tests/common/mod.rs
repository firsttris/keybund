#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// A temporary HOME with nothing leaking in from the real user.
pub struct Sandbox {
    pub dir: tempfile::TempDir,
    pub home: PathBuf,
    pub bin: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap().join("home");
        let bin = dir.path().canonicalize().unwrap().join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let sb = Sandbox { dir, home, bin };
        // No network in tests: hosts answer no keys unless a test says so.
        #[cfg(unix)]
        sb.fake_keyscan("");
        sb
    }

    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("keybund").unwrap();
        self.isolate(&mut cmd);
        cmd
    }

    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let mut cmd = Command::new("git");
        self.isolate(&mut cmd);
        let out = cmd.arg("-C").arg(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn isolate(&self, cmd: &mut Command) {
        let path = format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap_or_default());
        cmd.env("HOME", &self.home)
            .env("PATH", path)
            .env("NO_COLOR", "1")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("KEYBUND_PASSPHRASE")
            .env_remove("KEYBUND_API_URL")
            .env_remove("KEYBUND_GITHUB_CLIENT_ID")
            .env_remove("KEYBUND_GITLAB_CLIENT_ID");
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap_or_default()
    }

    /// Puts a fake `ssh` first in PATH that prints `output` and exits with `code`.
    #[cfg(unix)]
    pub fn fake_ssh(&self, output: &str, code: i32) {
        use std::os::unix::fs::PermissionsExt;
        let script = self.bin.join("ssh");
        let log = self.dir.path().join("ssh-args.log");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\necho '{}' >&2\nexit {code}\n",
                log.display(),
                output.replace('\'', "")
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A fake `ssh` that answers `ssh -G` with the given lines on stdout.
    #[cfg(unix)]
    pub fn fake_ssh_g(&self, lines: &str) {
        use std::os::unix::fs::PermissionsExt;
        let script = self.bin.join("ssh");
        fs::write(&script, format!("#!/bin/sh\ncat <<'EOF'\n{lines}\nEOF\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    pub fn ssh_log(&self) -> String {
        fs::read_to_string(self.dir.path().join("ssh-args.log")).unwrap_or_default()
    }

    /// Creates a profile non-interactively.
    /// A fake `ssh-keyscan` printing `output` (known_hosts lines).
    #[cfg(unix)]
    pub fn fake_keyscan(&self, output: &str) {
        use std::os::unix::fs::PermissionsExt;
        let script = self.bin.join("ssh-keyscan");
        fs::write(&script, format!("#!/bin/sh\ncat <<'EOF'\n{output}\nEOF\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A fake `secret-tool` keeping one secret in a file; returns the file.
    /// Every call is logged next to it (`secrets.log`).
    #[cfg(unix)]
    pub fn fake_secret_tool(&self) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let store = self.dir.path().join("secrets");
        let script = self.bin.join("secret-tool");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nf='{0}'\necho \"$@\" >> \"$f.log\"\ncase \"$1\" in\n  store) cat > \"$f\" ;;\n  lookup) [ -f \"$f\" ] && cat \"$f\" || exit 1 ;;\n  clear) rm -f \"$f\" ;;\nesac\n",
                store.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        store
    }

    pub fn new_profile(&self, name: &str, extra: &[&str]) {
        self.cmd().arg("new").arg("--name").arg(name).args(extra).arg("--yes").assert().success();
    }
}

/// Tests that need git and ssh-keygen are skipped locally when they are
/// missing, but never in CI.
pub fn has_tools() -> bool {
    let ok = ["git", "ssh-keygen"].iter().all(|t| std::process::Command::new(t).arg("--help").output().is_ok());
    assert!(ok || std::env::var_os("CI").is_none(), "git and ssh-keygen are required in CI");
    ok
}
