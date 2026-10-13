#![allow(dead_code)]

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::Duration;

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
        // No desktop either, on macOS too: no clipboard, and `open` never
        // starts the runner's real browser.
        #[cfg(target_os = "macos")]
        {
            sb.script("pbcopy", "#!/bin/sh\nexit 1\n");
            sb.script("open", "#!/bin/sh\nexit 0\n");
        }
        sb
    }

    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("keybund").unwrap();
        self.isolate(&mut cmd);
        cmd
    }

    /// The binary as a plain `std` command (for tests that spawn several at once).
    pub fn spawnable(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin("keybund"));
        self.isolate_std(&mut cmd);
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
            // Config given through the environment (`git -c`, GIT_CONFIG_COUNT) – a
            // CI machine may rewrite git@github.com: to https there.
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_SSH_COMMAND")
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("KEYBUND_PASSPHRASE");
    }

    /// Nothing of the real user leaks in: a HOME of its own, no XDG or
    /// git overrides, no agent, no display.
    fn isolate_std(&self, cmd: &mut std::process::Command) {
        let path = format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap_or_default());
        cmd.env("HOME", &self.home)
            .env("PATH", path)
            .env("NO_COLOR", "1")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("GIT_CONFIG_GLOBAL")
            // Config given through the environment (`git -c`, GIT_CONFIG_COUNT) – a
            // CI machine may rewrite git@github.com: to https there.
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_SSH_COMMAND")
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("KEYBUND_PASSPHRASE");
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
        let script = self.bin.join("ssh");
        let log = self.dir.path().join("ssh-args.log");
        write_executable(
            &script,
            &format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\necho '{}' >&2\nexit {code}\n",
                log.display(),
                output.replace('\'', "")
            ),
        );
    }

    /// A fake `ssh` that answers `ssh -G` with the given lines on stdout.
    #[cfg(unix)]
    pub fn fake_ssh_g(&self, lines: &str) {
        let script = self.bin.join("ssh");
        write_executable(&script, &format!("#!/bin/sh\ncat <<'EOF'\n{lines}\nEOF\n"));
    }

    pub fn ssh_log(&self) -> String {
        fs::read_to_string(self.dir.path().join("ssh-args.log")).unwrap_or_default()
    }

    /// Creates a profile non-interactively.
    /// A fake `ssh-keyscan` printing `output` (known_hosts lines).
    #[cfg(unix)]
    pub fn fake_keyscan(&self, output: &str) {
        let script = self.bin.join("ssh-keyscan");
        write_executable(&script, &format!("#!/bin/sh\ncat <<'EOF'\n{output}\nEOF\n"));
    }

    /// A fake browser opener (`xdg-open` on Linux, `open` on macOS) that
    /// writes the opened URL to a file; returns the file.
    #[cfg(unix)]
    pub fn fake_xdg_open(&self) -> PathBuf {
        let log = self.dir.path().join("opened");
        let body = format!("#!/bin/sh\necho \"$1\" >> '{}'\n", log.display());
        self.script("xdg-open", &body);
        self.script("open", &body);
        log
    }

    /// An executable script in the sandbox's `bin`, which comes first on PATH.
    #[cfg(unix)]
    pub fn script(&self, name: &str, body: &str) {
        let path = self.bin.join(name);
        write_executable(&path, body);
    }

    pub fn new_profile(&self, name: &str, extra: &[&str]) {
        self.cmd().arg("new").arg("--name").arg(name).args(extra).arg("--yes").assert().success();
    }

    /// Removes the fake `ssh-keyscan`: the test talks to a real server.
    pub fn real_keyscan(&self) {
        let _ = fs::remove_file(self.bin.join("ssh-keyscan"));
    }

    /// An `ssh` that reads the sandbox's configuration. ssh takes `~/.ssh/config`
    /// from the passwd entry, not from `HOME`, so without this every `keybund
    /// test` in a sandbox would read the developer's own config. The shim hands
    /// the real ssh the generated files with `~` expanded – the Include first,
    /// then the user's config, in the order ssh itself reads them.
    #[cfg(unix)]
    pub fn ssh_shim(&self) {
        let real = real_tool("ssh").expect("ssh in PATH");
        let script = self.bin.join("ssh");
        write_executable(
            &script,
            &format!(
                "#!/bin/sh\numask 077\ncfg=\"$HOME/.ssh/.test-config\"\n\
                 sed \"s|~/|$HOME/|g\" \"$HOME/.ssh/config.d/keybund.conf\" \"$HOME/.ssh/config\" 2>/dev/null \
                 | grep -v '^Include ' > \"$cfg\"\nexec '{}' -F \"$cfg\" \"$@\"\n",
                real.display()
            ),
        );
    }

    /// `ssh -G <host>` through the shim: what ssh resolves from the generated files.
    pub fn ssh_g(&self, host: &str) -> String {
        let mut cmd = Command::new(self.bin.join("ssh"));
        self.isolate(&mut cmd);
        let out = cmd.args(["-G", host]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// The first `name` in PATH (the test's own PATH, without the sandbox).
pub fn real_tool(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|f| f.is_file()))
}

/// The binary writes the files for the system it runs on
/// (`Platform::current()`): macOS gets `gitdir/i:` and the Keychain options.
pub fn platform() -> keybund_core::env::Platform {
    keybund_core::env::Platform::current()
}

/// `gitdir:<folder>` as the binary writes it on this system.
pub fn gitdir(folder: &str) -> String {
    keybund_core::render::gitdir_condition(folder, platform())
}

/// `core.sshCommand` as the binary writes it on this system.
pub fn ssh_command(key: &str) -> String {
    keybund_core::render::ssh_command(key, platform())
}

/// Tests that need git and ssh-keygen are skipped locally when they are
/// missing, but never in CI.
pub fn has_tools() -> bool {
    let ok = ["git", "ssh-keygen"].iter().all(|t| std::process::Command::new(t).arg("--help").output().is_ok());
    assert!(ok || std::env::var_os("CI").is_none(), "git and ssh-keygen are required in CI");
    ok
}

/// `sshd`, for tests against a real server; skipped locally when missing, never in CI.
pub fn sshd_path() -> Option<PathBuf> {
    let found = ["/usr/sbin/sshd", "/usr/bin/sshd"].iter().map(PathBuf::from).find(|p| p.exists());
    assert!(found.is_some() || std::env::var_os("CI").is_none(), "sshd is required in CI");
    found
}

/// A child process that dies with the test.
pub struct Killed(pub Child);
impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn wait_for(cond: impl Fn() -> bool) {
    for _ in 0..100 {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out");
}

/// A throw-away `sshd` on localhost that accepts the keys in `authorized`
/// (a file the test may change later). Logins land in the current user's
/// account; the key install writes to that same file instead of
/// `~/.ssh/authorized_keys` (the `KEYBUND_AUTHORIZED_KEYS` test hook).
pub fn start_sshd(sshd: &Path, dir: &Path, authorized: &Path) -> (Killed, u16) {
    start_sshd_with(sshd, dir, authorized, "")
}

/// `start_sshd` with more `sshd_config` lines (`MaxAuthTries`, another `ListenAddress`).
pub fn start_sshd_with(sshd: &Path, dir: &Path, authorized: &Path, extra: &str) -> (Killed, u16) {
    let host_key = dir.join("host_ed25519");
    assert!(std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&host_key)
        .status()
        .unwrap()
        .success());
    let port = free_port();
    let config = dir.join("sshd_config");
    fs::write(
        &config,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPidFile none\n\
             PasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\n\
             UsePAM no\nStrictModes no\nPermitRootLogin prohibit-password\n\
             SetEnv KEYBUND_AUTHORIZED_KEYS={}\n{extra}",
            host_key.display(),
            authorized.display(),
            authorized.display()
        ),
    )
    .unwrap();
    // Privilege separation directory, only needed when tests run as root.
    if std::process::Command::new("id").arg("-u").output().map(|o| o.stdout.starts_with(b"0")).unwrap_or(false) {
        let _ = fs::create_dir_all("/run/sshd");
    }
    let child = std::process::Command::new(sshd)
        .arg("-D")
        .arg("-e")
        .arg("-f")
        .arg(&config)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(|| TcpStream::connect(("127.0.0.1", port)).is_ok());
    (Killed(child), port)
}

/// The user sshd logs in as.
pub fn user() -> String {
    String::from_utf8(std::process::Command::new("id").arg("-un").output().unwrap().stdout).unwrap().trim().to_string()
}

/// Writes an executable stand-in script. Written in the test process, the file would be open for
/// writing while another test thread forks, the child would inherit that handle, and Linux refuses
/// to execute a file that is open for writing (ETXTBSY, `execve(2)`). A short `sh` child writes
/// it instead, so no write handle ever lives in the test process.
#[cfg(unix)]
pub fn write_executable(path: &Path, content: &str) {
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
