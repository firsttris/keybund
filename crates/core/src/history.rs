//! The result of the last connection test per host, kept in
//! `~/.config/keybund/last-tests.json` so CLI and app share it and it
//! survives a restart. Entries are keyed `<profile>/<host>` (alias or
//! hostname); a key's summary – shown in the sidebar and on the start page –
//! is computed from its hosts: a failing host wins, else the latest test.
//! Files from before hold one entry per profile (`<profile>`), still read
//! as that profile's summary until a host is tested again.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::i18n::Msg;
use crate::model::Profile;
use crate::probe::ProbeResult;
use crate::{fsutil, Env, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LastTest {
    /// The host that was tested (alias or hostname); `None` in old files.
    #[serde(default)]
    pub host: Option<String>,
    pub success: bool,
    pub account: Option<String>,
    #[serde(deserialize_with = "message_or_text")]
    #[cfg_attr(feature = "specta", specta(type = Msg))]
    pub message: Msg,
    /// Milliseconds since the epoch.
    pub at: f64,
}

impl LastTest {
    pub fn from_probe(host: &str, result: &ProbeResult) -> Self {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        LastTest {
            host: Some(host.to_string()),
            success: result.success,
            account: result.account.clone(),
            message: result.message.clone(),
            at,
        }
    }
}

/// Files from before the messages were keys hold the German text: kept as it is.
fn message_or_text<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Msg, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Msg(Msg),
        Text(String),
    }
    Ok(match Stored::deserialize(d)? {
        Stored::Msg(m) => m,
        Stored::Text(text) => crate::msg!("common_text", text = text),
    })
}

/// All stored results (`<profile>/<host>`, old files also `<profile>`). A
/// damaged file counts as empty: it is only a convenience.
pub fn load(env: &Env) -> BTreeMap<String, LastTest> {
    fsutil::read_optional(&env.last_tests_file())
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn host_key(id: &str, host: &str) -> String {
    format!("{id}/{host}")
}

/// The last test of each of the profile's current hosts (removed hosts are
/// left out), in the profile's host order.
pub fn host_tests(all: &BTreeMap<String, LastTest>, profile: &Profile) -> Vec<LastTest> {
    profile.hosts.iter().filter_map(|h| all.get(&host_key(&profile.id, h.target_name())).cloned()).collect()
}

/// The profile's summary: the latest failing host if any failed, else the
/// latest test – so a broken server is not hidden by a later success on
/// GitHub. Falls back to an entry from an old file.
pub fn summary(all: &BTreeMap<String, LastTest>, profile: &Profile) -> Option<LastTest> {
    let tests = host_tests(all, profile);
    let latest = |it: &mut dyn Iterator<Item = &LastTest>| it.max_by(|a, b| a.at.total_cmp(&b.at)).cloned();
    latest(&mut tests.iter().filter(|t| !t.success))
        .or_else(|| latest(&mut tests.iter()))
        .or_else(|| all.get(&profile.id).cloned())
}

/// The summary of one profile, read from disk.
pub fn get(env: &Env, profile: &Profile) -> Option<LastTest> {
    summary(&load(env), profile)
}

/// Stores the result of a test of one host of a profile.
pub fn record(env: &Env, id: &str, host: &str, result: &ProbeResult) -> Result<LastTest> {
    let test = LastTest::from_probe(host, result);
    let mut all = load(env);
    // The old per-profile entry is replaced by per-host ones.
    all.remove(id);
    all.insert(host_key(id, host), test.clone());
    save(env, &all)?;
    Ok(test)
}

/// Drops every result of a profile (archived or removed).
pub fn forget(env: &Env, id: &str) -> Result<()> {
    let mut all = load(env);
    let prefix = format!("{id}/");
    let before = all.len();
    all.retain(|k, _| k != id && !k.starts_with(&prefix));
    if all.len() != before {
        save(env, &all)?;
    }
    Ok(())
}

fn save(env: &Env, all: &BTreeMap<String, LastTest>) -> Result<()> {
    let text = serde_json::to_string_pretty(all).expect("plain data");
    fsutil::ensure_dir(&env.config_dir, Some(0o700))?;
    fsutil::atomic_write(&env.last_tests_file(), &text, Some(0o600))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(success: bool) -> ProbeResult {
        ProbeResult {
            success,
            host: "github.com".into(),
            account: success.then(|| "@t".to_string()),
            message: if success {
                crate::msg!("probe_connected", account = "@t")
            } else {
                crate::msg!("probe_keyUnknown", provider = "GitHub")
            },
            output: String::new(),
            command: String::new(),
            used_key: None,
            other_key: false,
            key_rejected: !success,
            steps: Vec::new(),
            log: String::new(),
        }
    }

    fn profile(id: &str, hosts: &[&str]) -> Profile {
        Profile {
            id: id.into(),
            hosts: hosts
                .iter()
                .map(|h| crate::model::HostEntry { hostname: h.to_string(), ..Default::default() })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn records_survive_and_can_be_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        let a = profile("a", &["github.com"]);
        let b = profile("b", &["github.com"]);
        assert!(get(&env, &a).is_none());
        record(&env, "a", "github.com", &probe(true)).unwrap();
        record(&env, "b", "github.com", &probe(false)).unwrap();
        let last = get(&env, &a).unwrap();
        assert!(last.success && last.at > 0.0);
        assert_eq!(last.account.as_deref(), Some("@t"));
        assert_eq!(last.host.as_deref(), Some("github.com"));
        assert_eq!(load(&env).len(), 2);
        forget(&env, "a").unwrap();
        assert!(get(&env, &a).is_none());
        assert_eq!(get(&env, &b).unwrap().message.to_string(), "GitHub kennt diesen Key nicht");

        std::fs::write(env.last_tests_file(), "kaputt").unwrap();
        assert!(load(&env).is_empty());
    }

    /// Each host keeps its own result; the key's summary shows a failing
    /// host even when another host was tested later and worked.
    #[test]
    fn results_are_kept_per_host() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        let p = profile("p", &["nas", "github.com"]);
        record(&env, "p", "nas", &probe(false)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        record(&env, "p", "github.com", &probe(true)).unwrap();
        let all = load(&env);
        let hosts: Vec<(Option<String>, bool)> =
            host_tests(&all, &p).into_iter().map(|t| (t.host, t.success)).collect();
        assert_eq!(hosts, [(Some("nas".into()), false), (Some("github.com".into()), true)]);
        let s = summary(&all, &p).unwrap();
        assert_eq!((s.host.as_deref(), s.success), (Some("nas"), false));
        // The server works again: now the latest test is the summary.
        record(&env, "p", "nas", &probe(true)).unwrap();
        assert_eq!(get(&env, &p).unwrap().host.as_deref(), Some("nas"));
        assert!(get(&env, &p).unwrap().success);
        // A removed host no longer counts.
        record(&env, "p", "nas", &probe(false)).unwrap();
        assert!(get(&env, &profile("p", &["github.com"])).unwrap().success);
    }

    /// A file from before: one entry per profile, read as its summary until
    /// a host is tested.
    #[test]
    fn old_files_still_count() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        std::fs::create_dir_all(&env.config_dir).unwrap();
        std::fs::write(
            env.last_tests_file(),
            r#"{"p": {"success": false, "account": null, "message": "Server kennt diesen Key nicht", "at": 1.0}}"#,
        )
        .unwrap();
        let p = profile("p", &["nas"]);
        let old = get(&env, &p).unwrap();
        assert!(!old.success && old.host.is_none());
        record(&env, "p", "nas", &probe(true)).unwrap();
        assert!(get(&env, &p).unwrap().success);
        assert!(!load(&env).contains_key("p"));
    }
}
