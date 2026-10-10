//! Connection test: `ssh -T` with only the profile's key.

use serde::{Deserialize, Serialize};

use crate::model::{HostEntry, Profile, Provider};
use crate::process::{self, shell_quote};
use crate::{keygen, Env, Error, Result};

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
    /// The key the server accepted, if ssh reported it.
    pub used_key: Option<String>,
    /// `true` if the server accepted a different key than the profile's,
    /// e.g. one from an `IdentityFile` in `~/.ssh/config`.
    pub other_key: bool,
}

/// Arguments for `ssh` (without the program name).
///
/// No `BatchMode`: it would also suppress the passphrase prompt, so a new
/// key with a passphrase could never be tested before it is in the agent.
/// Instead only password and keyboard-interactive logins are switched off;
/// the passphrase is asked on the terminal or via `SSH_ASKPASS`
/// (ksshaskpass), and `AddKeysToAgent` keeps the key in the agent afterwards.
pub fn ssh_args(profile: &Profile, host: &HostEntry) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-o",
        "PasswordAuthentication=no",
        "-o",
        "KbdInteractiveAuthentication=no",
        "-o",
        "ConnectTimeout=8",
        "-o",
        // Host keys are confirmed beforehand (hostkeys::check), never silently.
        "StrictHostKeyChecking=yes",
        "-o",
        "IdentitiesOnly=yes",
        "-o",
        "AddKeysToAgent=yes",
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
    // -v makes ssh report which key the server accepted.
    args.insert(0, "-v".into());

    let mut cmd = env.command("ssh");
    cmd.args(&args);
    let out = process::output(cmd)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let output = format!("{stdout}{stderr}").trim().to_string();
    let expected = keygen::read_public(env, &profile.key).ok().map(|k| k.fingerprint());
    Ok(evaluate(env, host, &output, out.status.code(), format!("ssh {}", display_args.join(" ")), expected.as_deref()))
}

/// Interprets the output of `ssh -v` and checks that the server accepted
/// the profile's key (`expected` fingerprint). ssh also offers keys from
/// `IdentityFile` lines in `~/.ssh/config`; a login with one of those must
/// not count as success.
pub fn evaluate(
    env: &Env,
    host: &HostEntry,
    output: &str,
    code: Option<i32>,
    command: String,
    expected: Option<&str>,
) -> ProbeResult {
    let accepted = accepted_key(output);
    let mut result = interpret(host, &strip_debug(output), code, command);
    if let Some((path, fingerprint)) = &accepted {
        result.used_key = Some(env.contract(std::path::Path::new(path)));
        if result.success && expected.is_some_and(|e| e != fingerprint) {
            result.success = false;
            result.other_key = true;
            let who = result.account.as_deref().map(|a| format!(" als {a}")).unwrap_or_default();
            result.message = format!(
                "Angemeldet{who}, aber mit {} statt mit dem Key des Profils",
                result.used_key.as_deref().unwrap_or(path)
            );
        }
    }
    result
}

/// `(path, fingerprint)` from ssh's "Server accepts key" debug line.
pub fn accepted_key(output: &str) -> Option<(String, String)> {
    let line = output.lines().rev().find(|l| l.contains("Server accepts key: "))?;
    let rest = &line[line.find("Server accepts key: ")? + "Server accepts key: ".len()..];
    let mut parts = rest.split_whitespace();
    let path = parts.next()?.to_string();
    let fingerprint = parts.find(|p| p.starts_with("SHA256:"))?.to_string();
    Some((path, fingerprint))
}

/// Removes ssh's debug lines from the output shown to the user.
fn strip_debug(output: &str) -> String {
    output
        .lines()
        .filter(|l| !l.starts_with("debug") && !l.starts_with("OpenSSH_") && !l.starts_with("Authenticated to "))
        .collect::<Vec<_>>()
        .join("\n")
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
    } else if lower.contains("incorrect passphrase") || lower.contains("bad passphrase") {
        (false, "Passphrase falsch".to_string())
    } else if lower.contains("read_passphrase") || lower.contains("ssh_askpass") {
        (false, "Passphrase konnte nicht abgefragt werden – Key einmal mit ssh-add laden".to_string())
    } else if lower.contains("permission denied") {
        (false, format!("{} kennt diesen Key nicht", host.provider.display_name()))
    } else if lower.contains("could not resolve hostname") {
        (false, format!("Host {} nicht gefunden", host.hostname))
    } else if lower.contains("timed out") || lower.contains("connection refused") || lower.contains("no route to host")
    {
        (false, format!("{} ist nicht erreichbar", host.hostname))
    } else if lower.contains("remote host identification has changed") {
        (
            false,
            format!(
                "Der Host-Key von {} hat sich geändert – erst prüfen, dann mit ssh-keygen -R entfernen",
                host.hostname
            ),
        )
    } else if lower.contains("host key verification failed") {
        (false, format!("Host-Key von {} ist noch nicht bestätigt", host.hostname))
    } else if lower.contains("no such identity") || lower.contains("no such file") {
        (false, "Key-Datei nicht gefunden".to_string())
    } else {
        (false, "Verbindung fehlgeschlagen".to_string())
    };
    ProbeResult {
        success,
        host: host.target_name().to_string(),
        account,
        message,
        output: output.to_string(),
        command,
        used_key: None,
        other_key: false,
    }
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

    const VERBOSE_OK: &str = "OpenSSH_9.6p1 Ubuntu-3ubuntu13\n\
        debug1: Offering public key: /home/t/.ssh/test_ed25519 ED25519 SHA256:aaaa explicit\n\
        debug1: Offering public key: /home/t/.ssh/privat_ed25519 ED25519 SHA256:bbbb explicit\n\
        debug1: Server accepts key: /home/t/.ssh/privat_ed25519 ED25519 SHA256:bbbb explicit\n\
        Authenticated to github.com ([140.82.121.4]:22) using \"publickey\".\n\
        Hi firsttris! You've successfully authenticated, but GitHub does not provide shell access.";

    #[test]
    fn reads_the_accepted_key() {
        assert_eq!(
            accepted_key(VERBOSE_OK),
            Some(("/home/t/.ssh/privat_ed25519".to_string(), "SHA256:bbbb".to_string()))
        );
        assert_eq!(accepted_key("Hi x!"), None);
    }

    #[test]
    fn login_with_another_key_is_not_a_success() {
        let env = Env::with_home("/home/t");
        let gh = host(Provider::Github, "github.com");
        let r = evaluate(&env, &gh, VERBOSE_OK, Some(1), String::new(), Some("SHA256:aaaa"));
        assert!(!r.success);
        assert!(r.other_key);
        assert_eq!(r.account.as_deref(), Some("@firsttris"));
        assert_eq!(r.used_key.as_deref(), Some("~/.ssh/privat_ed25519"));
        assert_eq!(
            r.message,
            "Angemeldet als @firsttris, aber mit ~/.ssh/privat_ed25519 statt mit dem Key des Profils"
        );
        assert!(!r.output.contains("debug1"), "debug lines are hidden");
        assert!(r.output.contains("Hi firsttris!"));

        let r = evaluate(&env, &gh, VERBOSE_OK, Some(1), String::new(), Some("SHA256:bbbb"));
        assert!(r.success && !r.other_key);
        assert_eq!(r.message, "Verbunden als @firsttris");
    }

    #[test]
    fn passphrase_problems_are_not_reported_as_unknown_key() {
        let gh = host(Provider::Github, "github.com");
        let out = "read_passphrase: can't open /dev/tty: No such device or address\ngit@github.com: Permission denied (publickey).";
        let r = interpret(&gh, out, Some(255), String::new());
        assert!(!r.success);
        assert!(r.message.starts_with("Passphrase konnte nicht abgefragt werden"));
        let out = "Load key \"/home/t/.ssh/a\": incorrect passphrase supplied to decrypt private key\ngit@github.com: Permission denied (publickey).";
        assert_eq!(interpret(&gh, out, Some(255), String::new()).message, "Passphrase falsch");
    }

    #[test]
    fn args_for_alias_and_git_host() {
        let p = Profile { key: crate::model::KeyRef { path: "~/.ssh/a".into(), comment: None }, ..Default::default() };
        let gh = host(Provider::Github, "github.com");
        let args = ssh_args(&p, &gh);
        assert_eq!(args.last().unwrap(), "git@github.com");
        assert!(!args.iter().any(|a| a.starts_with("BatchMode")), "BatchMode would block the passphrase prompt");
        assert!(args.contains(&"AddKeysToAgent=yes".to_string()));
        assert!(args.contains(&"PasswordAuthentication=no".to_string()));
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
