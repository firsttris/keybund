//! The result of the last connection test per profile, kept in
//! `~/.config/keybund/last-tests.json` so CLI and app share it and it
//! survives a restart.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::i18n::Msg;
use crate::probe::ProbeResult;
use crate::{fsutil, Env, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LastTest {
    pub success: bool,
    pub account: Option<String>,
    #[serde(deserialize_with = "message_or_text")]
    #[cfg_attr(feature = "specta", specta(type = Msg))]
    pub message: Msg,
    /// Milliseconds since the epoch.
    pub at: f64,
}

impl LastTest {
    pub fn from_probe(result: &ProbeResult) -> Self {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        LastTest { success: result.success, account: result.account.clone(), message: result.message.clone(), at }
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

/// All stored results by profile id. A damaged file counts as empty: it is
/// only a convenience.
pub fn load(env: &Env) -> BTreeMap<String, LastTest> {
    fsutil::read_optional(&env.last_tests_file())
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn get(env: &Env, id: &str) -> Option<LastTest> {
    load(env).remove(id)
}

/// Stores the result of a profile's connection test.
pub fn record(env: &Env, id: &str, result: &ProbeResult) -> Result<LastTest> {
    let test = LastTest::from_probe(result);
    let mut all = load(env);
    all.insert(id.to_string(), test.clone());
    save(env, &all)?;
    Ok(test)
}

/// Drops the result of a profile (archived or removed).
pub fn forget(env: &Env, id: &str) -> Result<()> {
    let mut all = load(env);
    if all.remove(id).is_some() {
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

    #[test]
    fn records_survive_and_can_be_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::with_home(dir.path());
        assert!(get(&env, "a").is_none());
        record(&env, "a", &probe(true)).unwrap();
        record(&env, "b", &probe(false)).unwrap();
        let a = get(&env, "a").unwrap();
        assert!(a.success && a.at > 0.0);
        assert_eq!(a.account.as_deref(), Some("@t"));
        assert_eq!(load(&env).len(), 2);
        forget(&env, "a").unwrap();
        assert!(get(&env, "a").is_none());
        assert_eq!(get(&env, "b").unwrap().message.to_string(), "GitHub kennt diesen Key nicht");

        std::fs::write(env.last_tests_file(), "kaputt").unwrap();
        assert!(load(&env).is_empty());
    }
}
