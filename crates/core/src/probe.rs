//! Connection test: `ssh -T` with only the profile's key.

use serde::{Deserialize, Serialize};

use crate::model::{HostEntry, Profile, Provider};
use crate::process::{self, shell_quote};
use crate::{Env, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ProbeResult {
    pub success: bool,
    pub host: String,
    /// Account the provider associated the key with.
    pub account: Option<String>,
    /// Short explanation for the user.
    pub message: String,
    /// Raw output of ssh.
    pub output: String,
    /// The command that was run, for display.
    pub command: String,
}

/// Arguments for `ssh` (without the program name).
pub fn ssh_args(profile: &Profile, host: &HostEntry) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "IdentitiesOnly=yes",
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(profile.key.path.clone());
    if host.alias.is_none() {
        if let Some(port) = host.port {
            args.push("-p".into());
            args.push(port.to_string());
        }
    }
    let user = host.user.as_deref().or(host.provider.git_user());
    let target = match (host.alias.as_deref(), user) {
        (Some(alias), _) => alias.to_string(),
        (None, Some(u)) => format!("{u}@{}", host.hostname),
        (None, None) => host.hostname.clone(),
    };
    args.push(target);
    if host.provider == Provider::Generic {
        args.push("true".into());
    }
    args
}

/// Runs the connection test against one host of a profile.
pub fn probe(env: &Env, profile: &Profile, host: &HostEntry) -> Result<ProbeResult> {
    let mut args = ssh_args(profile, host);
    let key_idx = args.iter().position(|a| a == "-i").map(|i| i + 1).unwrap_or(0);
    let display_args: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
    args[key_idx] = env.expand(&profile.key.path).display().to_string();

    let mut cmd = env.command("ssh");
    cmd.args(&args);
    let out = process::output(cmd)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let output = format!("{stdout}{stderr}").trim().to_string();
    let code = out.status.code();
    Ok(interpret(host, &output, code, format!("ssh {}", display_args.join(" "))))
}

/// Runs the test against the first host of a profile.
pub fn probe_profile(env: &Env, profile: &Profile) -> Result<ProbeResult> {
    let host = profile.hosts.first().ok_or_else(|| Error::NoHost(profile.id.clone()))?;
    probe(env, profile, host)
}

/// Interprets ssh's output. GitHub exits with 1 even on success, so success
/// is detected from the greeting, not from the exit code.
pub fn interpret(host: &HostEntry, output: &str, code: Option<i32>, command: String) -> ProbeResult {
    let account = parse_account(host.provider, output);
    let lower = output.to_lowercase();
    let (success, message) = if let Some(acc) = &account {
        (true, format!("Verbunden als {acc}"))
    } else if (host.provider == Provider::Generic && code == Some(0))
        || (host.provider == Provider::Bitbucket && lower.contains("authenticated via ssh key"))
    {
        (true, "Anmeldung erfolgreich".to_string())
    } else if lower.contains("permission denied") {
        (false, format!("{} kennt diesen Key nicht", host.provider.display_name()))
    } else if lower.contains("could not resolve hostname") {
        (false, format!("Host {} nicht gefunden", host.hostname))
    } else if lower.contains("timed out") || lower.contains("connection refused") || lower.contains("no route to host")
    {
        (false, format!("{} ist nicht erreichbar", host.hostname))
    } else if lower.contains("host key verification failed") || lower.contains("remote host identification has changed")
    {
        (false, "Host-Key stimmt nicht mit known_hosts überein".to_string())
    } else if lower.contains("no such identity") || lower.contains("no such file") {
        (false, "Key-Datei nicht gefunden".to_string())
    } else {
        (false, "Verbindung fehlgeschlagen".to_string())
    };
    ProbeResult { success, host: host.target_name().to_string(), account, message, output: output.to_string(), command }
}

/// Extracts the account name from a provider greeting.
pub fn parse_account(provider: Provider, output: &str) -> Option<String> {
    let between = |start: &str, end: &[char]| -> Option<String> {
        let i = output.find(start)? + start.len();
        let rest = &output[i..];
        let j = rest.find(end)?;
        let name = rest[..j].trim();
        (!name.is_empty()).then(|| name.to_string())
    };
    match provider {
        // "Hi octocat! You've successfully authenticated, …"
        Provider::Github => between("Hi ", &['!']).map(|n| format!("@{n}")),
        // "Welcome to GitLab, @user!"
        Provider::Gitlab => between("Welcome to GitLab, ", &['!']),
        // "authenticated via ssh key. … logged in as user."
        Provider::Bitbucket => between("logged in as ", &['.', '\n']).map(|n| format!("@{n}")),
        Provider::Generic => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(provider: Provider, hostname: &str) -> HostEntry {
        HostEntry { hostname: hostname.into(), provider, ..Default::default() }
    }

    #[test]
    fn github_success_with_exit_code_1() {
        let r = interpret(
            &host(Provider::Github, "github.com"),
            "Hi octocat! You've successfully authenticated, but GitHub does not provide shell access.",
            Some(1),
            String::new(),
        );
        assert!(r.success);
        assert_eq!(r.account.as_deref(), Some("@octocat"));
    }

    #[test]
    fn gitlab_success() {
        let r = interpret(
            &host(Provider::Gitlab, "gitlab.kunde.de"),
            "Welcome to GitLab, @tristan!",
            Some(0),
            String::new(),
        );
        assert!(r.success);
        assert_eq!(r.account.as_deref(), Some("@tristan"));
    }

    #[test]
    fn bitbucket_success() {
        let out = "authenticated via ssh key.\n\nYou can use git to connect to Bitbucket. Shell access is disabled\nlogged in as tristan.";
        let r = interpret(&host(Provider::Bitbucket, "bitbucket.org"), out, Some(0), String::new());
        assert!(r.success);
        assert_eq!(r.account.as_deref(), Some("@tristan"));
    }

    #[test]
    fn denied() {
        let r = interpret(
            &host(Provider::Github, "github.com"),
            "git@github.com: Permission denied (publickey).",
            Some(255),
            String::new(),
        );
        assert!(!r.success);
        assert_eq!(r.message, "GitHub kennt diesen Key nicht");
    }

    #[test]
    fn args_for_alias_and_git_host() {
        let p = Profile { key: crate::model::KeyRef { path: "~/.ssh/a".into(), comment: None }, ..Default::default() };
        let gh = host(Provider::Github, "github.com");
        let args = ssh_args(&p, &gh);
        assert_eq!(args.last().unwrap(), "git@github.com");
        let server = HostEntry {
            alias: Some("meinserver".into()),
            hostname: "1.2.3.4".into(),
            port: Some(2222),
            ..Default::default()
        };
        let args = ssh_args(&p, &server);
        assert_eq!(&args[args.len() - 2..], ["meinserver", "true"]);
        assert!(!args.contains(&"-p".to_string()));
    }
}
