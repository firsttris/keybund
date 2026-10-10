//! Pure functions that turn profiles into config file contents.
//! No I/O happens here, which keeps the output trivially testable.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;

use crate::model::{Config, Profile, Provider, Settings};
use crate::process::shell_quote;
use crate::Env;

const MANAGED: &str = "# Managed by Keybund – manual changes will be overwritten.";

/// The `Include` line Keybund keeps at the very top of `~/.ssh/config`.
pub const SSH_INCLUDE_LINE: &str = "Include ~/.ssh/config.d/keybund.conf";

/// The `Include` line for these settings: with `~`, or with the full path
/// for old libssh (see [`Settings::absolute_ssh_include`]).
pub fn ssh_include_line(env: &Env, settings: &Settings) -> String {
    if !settings.absolute_ssh_include {
        return SSH_INCLUDE_LINE.to_string();
    }
    let path = env.ssh_keybund_conf().display().to_string();
    if path.contains(char::is_whitespace) {
        format!("Include \"{path}\"")
    } else {
        format!("Include {path}")
    }
}

/// One generated file.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedFile {
    pub path: PathBuf,
    pub content: String,
}

/// `core.sshCommand` value for a key. `AddKeysToAgent` makes ssh hand a
/// key to the running agent after its passphrase was entered once, so git
/// does not ask again on every push.
pub fn ssh_command(key_path: &str) -> String {
    format!("ssh -i {} -o IdentitiesOnly=yes -o AddKeysToAgent=yes", shell_quote(key_path))
}

/// `true` if an ssh command line enables `AddKeysToAgent`.
pub fn adds_keys_to_agent(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    ["addkeystoagent=yes", "addkeystoagent=confirm", "addkeystoagent yes"].iter().any(|o| lower.contains(o))
}

/// `~/.ssh/config.d/keybund.conf`: a `Host` block for every server (by
/// alias and hostname/IP) and every aliased git host, each pinned to the
/// profile's key.
pub fn ssh_config(config: &Config) -> String {
    let mut out = format!("{MANAGED}\n# Included from the top of ~/.ssh/config.\n");
    for p in config.active_profiles() {
        for h in p.hosts.iter() {
            let patterns = h.ssh_patterns();
            if patterns.is_empty() {
                continue;
            }
            let _ = writeln!(out, "\n# Key: {}", p.name);
            let _ = writeln!(out, "Host {}", patterns.join(" "));
            let _ = writeln!(out, "    HostName {}", h.hostname);
            // `ssh -T github-kunde` must log in as git, not as the local user.
            if let Some(user) = h.user.as_deref().or(h.provider.git_user()) {
                let _ = writeln!(out, "    User {user}");
            }
            if let Some(port) = h.port {
                let _ = writeln!(out, "    Port {port}");
            }
            let _ = writeln!(out, "    IdentityFile {}", ssh_path(&p.key.path));
            let _ = writeln!(out, "    IdentitiesOnly yes");
            let _ = writeln!(out, "    AddKeysToAgent yes");
            if config.settings.known_hosts_per_profile && h.provider == Provider::Generic {
                // New host keys go into the profile's own file, known ones
                // from the shared file still count.
                let _ = writeln!(out, "    UserKnownHostsFile {} ~/.ssh/known_hosts", profile_known_hosts(&p.id));
            }
        }
    }
    // Leave the parser in a global context again so options that follow
    // the Include line in ~/.ssh/config keep applying to every host.
    out.push_str("\nHost *\n");
    out
}

/// `~/.config/keybund/git/index.gitconfig`: default key, allowed signers
/// and one `includeIf "gitdir:…"` per profile folder.
pub fn index_gitconfig(config: &Config, env: &Env) -> String {
    let mut out = format!("{MANAGED}\n# Included from ~/.gitconfig.\n");
    if let Some(def) = config.default_entry() {
        let _ = writeln!(out, "\n# Standard-Key: {}", def.name);
        let _ = writeln!(out, "[core]\n\tsshCommand = {}", ssh_command(&def.key.path));
    }
    if config.active_profiles().any(signs) {
        let _ = writeln!(out, "\n[gpg \"ssh\"]\n\tallowedSignersFile = {}", env.contract(&env.allowed_signers()));
    }
    for p in config.active_profiles() {
        out.push('\n');
        out.push_str(&include_if_blocks(p, env));
    }
    out
}

/// The `includeIf` sections of one profile in `index.gitconfig`.
pub fn include_if_blocks(profile: &Profile, env: &Env) -> String {
    let path = env.contract(&env.profile_gitconfig(&profile.id));
    profile
        .folders
        .iter()
        .map(|f| format!("[includeIf \"gitdir:{f}\"]\n\tpath = {path}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `~/.ssh/known_hosts.d/<id>`: host keys of a profile's servers.
pub fn profile_known_hosts(id: &str) -> String {
    format!("~/.ssh/known_hosts.d/{id}")
}

/// The `Host` blocks of one profile in `keybund.conf` (empty without aliases).
pub fn ssh_host_blocks(settings: &Settings, profile: &Profile) -> String {
    let single = Config {
        profiles: vec![Profile { archived: false, ..profile.clone() }],
        settings: settings.clone(),
        ..Default::default()
    };
    let full = ssh_config(&single);
    let start = full.find("\n# Key:").map(|i| i + 1).unwrap_or(full.len());
    let end = full.rfind("\nHost *").unwrap_or(full.len());
    if start >= end {
        String::new()
    } else {
        full[start..end].trim_end().to_string() + "\n"
    }
}

/// `~/.config/keybund/git/<id>.gitconfig`.
pub fn profile_gitconfig(profile: &Profile) -> String {
    let mut out = format!("{MANAGED}\n# Key: {}\n", profile.name);
    if let Some(git) = &profile.git {
        let _ = writeln!(out, "[user]\n\tname = {}\n\temail = {}", git_value(&git.name), git_value(&git.email));
        if git.sign {
            let _ = writeln!(out, "\tsigningkey = {}", git_value(&profile.key.pub_path()));
        }
    }
    let _ = writeln!(out, "[core]\n\tsshCommand = {}", ssh_command(&profile.key.path));
    if signs(profile) {
        out.push_str("[gpg]\n\tformat = ssh\n[commit]\n\tgpgsign = true\n[tag]\n\tgpgsign = true\n");
    }
    out
}

/// `~/.config/keybund/allowed_signers`: one line per signing profile.
/// `pubkeys` maps profile id → public key (`type blob`).
pub fn allowed_signers(config: &Config, pubkeys: &BTreeMap<String, String>) -> String {
    let mut out = format!("{MANAGED}\n");
    for p in config.active_profiles().filter(|p| signs(p)) {
        let (Some(git), Some(key)) = (&p.git, pubkeys.get(&p.id)) else { continue };
        let _ = writeln!(out, "{} namespaces=\"git\" {}", git.email, key);
    }
    out
}

/// All files Keybund owns, in a stable order.
pub fn render_all(config: &Config, env: &Env, pubkeys: &BTreeMap<String, String>) -> Vec<RenderedFile> {
    let mut files = vec![
        RenderedFile { path: env.ssh_keybund_conf(), content: ssh_config(config) },
        RenderedFile { path: env.index_gitconfig(), content: index_gitconfig(config, env) },
    ];
    for p in config.active_profiles() {
        files.push(RenderedFile { path: env.profile_gitconfig(&p.id), content: profile_gitconfig(p) });
    }
    if config.active_profiles().any(signs) {
        files.push(RenderedFile { path: env.allowed_signers(), content: allowed_signers(config, pubkeys) });
    }
    files
}

fn signs(p: &Profile) -> bool {
    p.git.as_ref().is_some_and(|g| g.sign)
}

/// ssh_config takes paths with spaces only when quoted.
fn ssh_path(path: &str) -> String {
    if path.contains(char::is_whitespace) {
        format!("\"{path}\"")
    } else {
        path.to_string()
    }
}

/// Quotes a gitconfig value if needed.
fn git_value(v: &str) -> String {
    if v.starts_with(char::is_whitespace) || v.ends_with(char::is_whitespace) || v.contains(['#', ';', '"', '\\']) {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GitIdentity, HostEntry, KeyRef, Provider};

    fn profile(id: &str, name: &str, folder: &str, sign: bool) -> Profile {
        Profile {
            id: id.into(),
            name: name.into(),
            color: None,
            key: KeyRef { path: format!("~/.ssh/{id}_ed25519"), comment: None },
            git: Some(GitIdentity { name: "Tristan".into(), email: format!("tristan@{id}.de"), sign }),
            folders: vec![folder.into()],
            archived: false,
            hosts: vec![],
        }
    }

    /// Server with alias, two customers on github.com, signing on/off.
    fn sample() -> Config {
        let mut privat = profile("privat", "Privat", "~/dev/privat/", true);
        privat.hosts.push(HostEntry {
            alias: Some("meinserver".into()),
            hostname: "1.2.3.4".into(),
            user: Some("tristan".into()),
            port: Some(22),
            provider: Provider::Generic,
        });
        let mut a = profile("kunde-a", "Kunde A", "~/dev/kunde-a/", false);
        a.hosts.push(HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() });
        let mut b = profile("kunde-b", "Kunde B", "~/dev/kunde-b/", false);
        b.hosts.push(HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() });
        let mut old = profile("alt", "Alt", "~/dev/alt/", false);
        old.archived = true;
        Config { default_profile: Some("privat".into()), profiles: vec![privat, a, b, old], ..Default::default() }
    }

    #[test]
    fn snapshot_ssh_config() {
        insta::assert_snapshot!(ssh_config(&sample()));
    }

    #[test]
    fn snapshot_index_gitconfig() {
        insta::assert_snapshot!(index_gitconfig(&sample(), &Env::with_home("/home/t")));
    }

    #[test]
    fn snapshot_profile_gitconfigs() {
        let cfg = sample();
        insta::assert_snapshot!("profile_signing", profile_gitconfig(&cfg.profiles[0]));
        insta::assert_snapshot!("profile_plain", profile_gitconfig(&cfg.profiles[1]));
    }

    #[test]
    fn snapshot_allowed_signers() {
        let mut keys = BTreeMap::new();
        keys.insert(
            "privat".to_string(),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPCiQ4JKZ4ytCkt2jcR5wRuDme4A+7To8EOX6+uTnRyy".to_string(),
        );
        insta::assert_snapshot!(allowed_signers(&sample(), &keys));
    }

    #[test]
    fn servers_match_by_alias_and_ip_git_hosts_only_by_alias() {
        let mut p = profile("p", "P", "~/dev/p/", false);
        p.hosts = vec![
            HostEntry { alias: Some("nas".into()), hostname: "192.168.1.5".into(), ..Default::default() },
            HostEntry { hostname: "10.0.0.7".into(), user: Some("root".into()), ..Default::default() },
            HostEntry {
                alias: Some("pflegeabc".into()),
                hostname: "github.com".into(),
                provider: Provider::Github,
                ..Default::default()
            },
            HostEntry { hostname: "github.com".into(), provider: Provider::Github, ..Default::default() },
        ];
        let conf = ssh_config(&Config { profiles: vec![p], ..Default::default() });
        assert!(conf.contains("Host nas 192.168.1.5\n"));
        assert!(conf.contains("Host 10.0.0.7\n    HostName 10.0.0.7\n    User root\n"));
        assert!(conf.contains("Host pflegeabc\n    HostName github.com\n"));
        assert!(!conf.contains("Host github.com"));
        assert!(!conf.contains("Host pflegeabc github.com"));
    }

    #[test]
    fn archived_profiles_are_skipped() {
        let files = render_all(&sample(), &Env::with_home("/home/t"), &BTreeMap::new());
        assert!(files.iter().all(|f| !f.path.ends_with("alt.gitconfig")));
        assert!(files.iter().all(|f| !f.content.contains("~/dev/alt/")));
    }

    #[test]
    fn no_default_profile_means_no_core_section() {
        let mut cfg = sample();
        cfg.default_profile = None;
        assert!(!index_gitconfig(&cfg, &Env::with_home("/home/t")).contains("[core]"));
    }

    #[test]
    fn ssh_command_adds_keys_to_agent() {
        assert!(adds_keys_to_agent(&ssh_command("~/.ssh/a")));
        assert!(!adds_keys_to_agent("ssh -i ~/.ssh/a -o IdentitiesOnly=yes"));
        assert!(adds_keys_to_agent("ssh -o addkeystoagent=confirm"));
    }

    #[test]
    fn per_profile_excerpts() {
        let cfg = sample();
        let env = Env::with_home("/home/t");
        assert_eq!(
            include_if_blocks(&cfg.profiles[1], &env),
            "[includeIf \"gitdir:~/dev/kunde-a/\"]\n\tpath = ~/.config/keybund/git/kunde-a.gitconfig\n"
        );
        let blocks = ssh_host_blocks(&cfg.settings, &cfg.profiles[0]);
        assert!(blocks.starts_with("# Key: Privat\nHost meinserver 1.2.3.4\n"));
        assert!(!blocks.contains("Host *"));
        assert_eq!(ssh_host_blocks(&cfg.settings, &cfg.profiles[1]), "");
    }

    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(ssh_command("~/my keys/a"), "ssh -i ~/'my keys/a' -o IdentitiesOnly=yes -o AddKeysToAgent=yes");
        assert_eq!(git_value("a # b"), "\"a # b\"");
    }

    #[test]
    fn git_alias_block_sets_the_git_user() {
        let mut p = profile("pflege", "Pflege", "~/dev/pflege/", false);
        p.hosts = vec![HostEntry {
            alias: Some("pflegeabc".into()),
            hostname: "github.com".into(),
            provider: Provider::Github,
            ..Default::default()
        }];
        let conf = ssh_config(&Config { profiles: vec![p], ..Default::default() });
        assert!(conf.contains("Host pflegeabc\n    HostName github.com\n    User git\n"), "{conf}");
    }
}
