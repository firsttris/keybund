use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Error, Result};

/// The file system locations Keybund works with.
///
/// Everything is derived from a home directory and Keybund's own config
/// directory, so tests can run against a temporary `HOME`.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    /// `~/.config/keybund` (respects `XDG_CONFIG_HOME`).
    pub config_dir: PathBuf,
    /// The user's login shell (`$SHELL`), asked for variables like
    /// `GIT_SSH_COMMAND` that a GUI process does not inherit.
    pub login_shell: Option<PathBuf>,
    /// The system-wide known_hosts (`/etc/ssh/ssh_known_hosts`); none in tests.
    pub global_known_hosts: Option<PathBuf>,
    /// Tests only: the config `ssh -G` reads (`-F`). ssh takes `~/.ssh/config`
    /// and `~` from the passwd entry, not from `HOME`, so a test home needs
    /// this to be asked at all; paths ssh prints below the real home are
    /// mapped back into the test home.
    pub ssh_config_override: Option<PathBuf>,
}

impl Env {
    /// Locations of the current user, from `HOME` and `XDG_CONFIG_HOME`.
    pub fn from_system() -> Result<Self> {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from).ok_or(Error::NoHome)?;
        let config_base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let login_shell = std::env::var_os("SHELL").filter(|s| !s.is_empty()).map(PathBuf::from);
        Ok(Env {
            home,
            config_dir: config_base.join("keybund"),
            login_shell,
            global_known_hosts: Some(PathBuf::from("/etc/ssh/ssh_known_hosts")),
            ssh_config_override: None,
        })
    }

    /// Locations below an arbitrary home directory (used by tests).
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let config_dir = home.join(".config").join("keybund");
        let ssh_config_override = Some(home.join(".ssh").join("config"));
        Env { home, config_dir, login_shell: None, global_known_hosts: None, ssh_config_override }
    }

    /// Expands a leading `~` to the home directory.
    pub fn expand(&self, path: &str) -> PathBuf {
        if path == "~" {
            self.home.clone()
        } else if let Some(rest) = path.strip_prefix("~/") {
            self.home.join(rest)
        } else {
            PathBuf::from(path)
        }
    }

    /// Replaces the home directory prefix with `~`, for display and for
    /// paths written into generated configs.
    pub fn contract(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }

    /// `ssh -G <host>`: what ssh resolves for a host from its config (the
    /// test config with `-F` when [`Env::ssh_config_override`] is set).
    pub fn ssh_g(&self, host: &str) -> Option<String> {
        let mut cmd = self.command("ssh");
        if let Some(config) = &self.ssh_config_override {
            cmd.arg("-F").arg(config);
        }
        cmd.args(["-G", "--", host]);
        crate::process::stdout(cmd).ok()
    }

    /// A path from `ssh -G` as a path of this environment: in a test home,
    /// what ssh expanded below the real home is moved into the test home.
    pub fn from_ssh(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if self.ssh_config_override.is_none() {
            return path;
        }
        match std::env::home_dir().and_then(|real| path.strip_prefix(real).ok().map(|r| self.home.join(r))) {
            Some(mapped) => mapped,
            None => path,
        }
    }

    pub fn ssh_dir(&self) -> PathBuf {
        self.home.join(".ssh")
    }
    pub fn ssh_config(&self) -> PathBuf {
        self.ssh_dir().join("config")
    }
    pub fn ssh_keybund_conf(&self) -> PathBuf {
        self.ssh_dir().join("config.d").join("keybund.conf")
    }
    pub fn known_hosts(&self) -> PathBuf {
        self.ssh_dir().join("known_hosts")
    }
    pub fn known_hosts_dir(&self) -> PathBuf {
        self.ssh_dir().join("known_hosts.d")
    }
    pub fn ssh_archive_dir(&self) -> PathBuf {
        self.ssh_dir().join("archive")
    }
    pub fn gitconfig(&self) -> PathBuf {
        self.home.join(".gitconfig")
    }
    /// `~/.config/git/config` (or under `XDG_CONFIG_HOME`): git reads it
    /// before `~/.gitconfig`, so the latter wins.
    pub fn xdg_gitconfig(&self) -> PathBuf {
        self.config_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| self.home.join(".config")).join("git/config")
    }
    pub fn profiles_file(&self) -> PathBuf {
        self.config_dir.join("profiles.toml")
    }
    pub fn git_dir(&self) -> PathBuf {
        self.config_dir.join("git")
    }
    pub fn index_gitconfig(&self) -> PathBuf {
        self.git_dir().join("index.gitconfig")
    }
    pub fn profile_gitconfig(&self, id: &str) -> PathBuf {
        self.git_dir().join(format!("{id}.gitconfig"))
    }
    pub fn last_tests_file(&self) -> PathBuf {
        self.config_dir.join("last-tests.json")
    }
    pub fn allowed_signers(&self) -> PathBuf {
        self.config_dir.join("allowed_signers")
    }

    /// A command that sees this environment's home directory.
    pub fn command(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.env("HOME", &self.home);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_and_contract() {
        let env = Env::with_home("/home/t");
        assert_eq!(env.expand("~/.ssh/a"), PathBuf::from("/home/t/.ssh/a"));
        assert_eq!(env.expand("/etc/x"), PathBuf::from("/etc/x"));
        assert_eq!(env.contract(Path::new("/home/t/dev/x")), "~/dev/x");
        assert_eq!(env.contract(Path::new("/srv/x")), "/srv/x");
        assert_eq!(env.contract(Path::new("/home/t")), "~");
    }
}
