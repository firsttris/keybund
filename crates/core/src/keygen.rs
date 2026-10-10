//! Generating keys with `ssh-keygen`, reading public keys and fingerprints,
//! and finding existing keys to import. Private keys are never read.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{Config, KeyRef};
use crate::{fsutil, process, Env, Error, Result};

/// Everything Keybund knows about a key, derived from its `.pub` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct KeyInfo {
    pub path: String,
    pub pub_path: String,
    pub private_exists: bool,
    pub public_exists: bool,
    /// e.g. `ssh-ed25519`.
    pub key_type: Option<String>,
    /// `SHA256:…`, as printed by `ssh-keygen -lf`.
    pub fingerprint: Option<String>,
    pub comment: Option<String>,
    /// The full public key line.
    pub public_key: Option<String>,
    /// Creation date of the `.pub` file as `YYYY-MM-DD`.
    pub created: Option<String>,
    pub age_days: Option<u32>,
}

/// A parsed public key line.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicKey {
    pub key_type: String,
    pub blob: String,
    pub comment: Option<String>,
}

impl PublicKey {
    pub fn parse(line: &str) -> Option<PublicKey> {
        let line = line.lines().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))?;
        let mut parts = line.split_whitespace();
        let key_type = parts.next()?.to_string();
        let blob = parts.next()?.to_string();
        if !(key_type.starts_with("ssh-") || key_type.starts_with("ecdsa-") || key_type.starts_with("sk-")) {
            return None;
        }
        base64::engine::general_purpose::STANDARD.decode(&blob).ok()?;
        let comment: Vec<&str> = parts.collect();
        let comment = if comment.is_empty() { None } else { Some(comment.join(" ")) };
        Some(PublicKey { key_type, blob, comment })
    }

    /// SHA256 fingerprint in OpenSSH notation.
    pub fn fingerprint(&self) -> String {
        let raw = base64::engine::general_purpose::STANDARD.decode(&self.blob).unwrap_or_default();
        let digest = Sha256::digest(&raw);
        format!("SHA256:{}", base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest))
    }

    /// `type blob` without the comment (used for `allowed_signers`).
    pub fn without_comment(&self) -> String {
        format!("{} {}", self.key_type, self.blob)
    }

    pub fn line(&self) -> String {
        match &self.comment {
            Some(c) => format!("{} {} {}", self.key_type, self.blob, c),
            None => self.without_comment(),
        }
    }
}

/// Reads the public half of `key` (never the private key).
pub fn read_public(env: &Env, key: &KeyRef) -> Result<PublicKey> {
    let pub_path = env.expand(&key.pub_path());
    let text = fs::read_to_string(&pub_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::KeyMissing(key.pub_path())
        } else {
            Error::io(&pub_path, e)
        }
    })?;
    PublicKey::parse(&text).ok_or_else(|| Error::InvalidPublicKey(key.pub_path()))
}

pub fn key_info(env: &Env, key: &KeyRef) -> KeyInfo {
    let private = env.expand(&key.path);
    let public = env.expand(&key.pub_path());
    let parsed = read_public(env, key).ok();
    let created = file_created(&public);
    KeyInfo {
        path: key.path.clone(),
        pub_path: key.pub_path(),
        private_exists: private.exists(),
        public_exists: public.exists(),
        key_type: parsed.as_ref().map(|k| k.key_type.clone()),
        fingerprint: parsed.as_ref().map(PublicKey::fingerprint),
        comment: parsed.as_ref().and_then(|k| k.comment.clone()).or_else(|| key.comment.clone()),
        public_key: parsed.as_ref().map(PublicKey::line),
        created: created.map(format_date),
        age_days: created.map(|secs| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(secs);
            (now.saturating_sub(secs) / 86_400) as u32
        }),
    }
}

/// Options for [`generate`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GenerateOptions {
    /// Target path of the private key, e.g. `~/.ssh/kunde-c_ed25519`.
    pub path: String,
    pub comment: String,
    /// Empty or `None` creates a key without passphrase.
    pub passphrase: Option<String>,
}

/// Creates a new ed25519 key with `ssh-keygen`.
pub fn generate(env: &Env, opts: &GenerateOptions) -> Result<KeyInfo> {
    let path = env.expand(&opts.path);
    if path.exists() || Path::new(&format!("{}.pub", path.display())).exists() {
        return Err(Error::KeyExists(opts.path.clone()));
    }
    if let Some(dir) = path.parent() {
        let mode = if dir.starts_with(env.ssh_dir()) { Some(0o700) } else { None };
        fsutil::ensure_dir(dir, mode)?;
    }
    let mut cmd = env.command("ssh-keygen");
    cmd.args(["-q", "-t", "ed25519", "-C", &opts.comment, "-f"])
        .arg(&path)
        .args(["-N", opts.passphrase.as_deref().unwrap_or("")]);
    process::stdout(cmd)?;
    fsutil::set_mode(&path, 0o600)?;
    Ok(key_info(env, &KeyRef { path: opts.path.clone(), comment: Some(opts.comment.clone()) }))
}

/// Default file name for a profile key: `~/.ssh/<id>_ed25519`.
pub fn default_key_path(id: &str) -> String {
    crate::model::Settings::default().key_path(id)
}

/// A key pair found in `~/.ssh` that could be imported.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportCandidate {
    pub key: KeyInfo,
    /// Profile that already uses this key.
    pub used_by: Option<String>,
}

/// Lists key pairs in `~/.ssh` (`*.pub` with a matching private key).
pub fn scan(env: &Env, config: &Config) -> Result<Vec<ImportCandidate>> {
    let dir = env.ssh_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io(&dir, e)),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pub") {
            continue;
        }
        let private = path.with_extension("");
        if !private.is_file() {
            continue;
        }
        let key_ref = KeyRef { path: env.contract(&private), comment: None };
        let info = key_info(env, &key_ref);
        if info.fingerprint.is_none() {
            continue;
        }
        let used_by = config.profiles.iter().find(|p| env.expand(&p.key.path) == private).map(|p| p.id.clone());
        out.push(ImportCandidate { key: info, used_by });
    }
    out.sort_by(|a, b| a.key.path.cmp(&b.key.path));
    Ok(out)
}

fn file_created(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let time = meta.created().or_else(|_| meta.modified()).ok()?;
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// Formats unix seconds as `YYYY-MM-DD` (UTC).
pub fn format_date(secs: u64) -> String {
    // Civil-from-days, Howard Hinnant's algorithm.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test vector generated with ssh-keygen; fingerprint from `ssh-keygen -lf`.
    const PUB: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPCiQ4JKZ4ytCkt2jcR5wRuDme4A+7To8EOX6+uTnRyy tristan@zentrale";

    #[test]
    fn parses_public_key() {
        let k = PublicKey::parse(PUB).unwrap();
        assert_eq!(k.key_type, "ssh-ed25519");
        assert_eq!(k.comment.as_deref(), Some("tristan@zentrale"));
        assert_eq!(k.line(), PUB);
        assert_eq!(k.fingerprint(), "SHA256:QTOnEgrk1+0r0DeWLrsMUMGTLahyPyGWitG3931i9Lg");
        assert!(PublicKey::parse("not a key").is_none());
        assert!(PublicKey::parse("ssh-ed25519 !!!").is_none());
    }

    #[test]
    fn formats_dates() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_772_496_000), "2026-03-03");
    }
}
