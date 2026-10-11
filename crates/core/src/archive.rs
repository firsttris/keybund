//! Offboarding: archive a profile's key and stop generating its configs.

use serde::{Deserialize, Serialize};

use crate::model::Config;
use crate::{fsutil, provider, store, Env, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveResult {
    /// New location of the private key.
    pub key_path: String,
    /// Pages where the key should be removed at the provider.
    pub remove_key_urls: Vec<String>,
}

/// Moves the key pair to `~/.ssh/archive/`, marks the profile archived and
/// saves `profiles.toml`. Configs are updated by the next apply.
pub fn archive(env: &Env, config: &mut Config, id: &str) -> Result<ArchiveResult> {
    let profile = config.profile(id)?;
    if profile.archived {
        return Err(Error::Archived(id.to_string()));
    }
    let archive_dir = env.ssh_archive_dir();
    fsutil::ensure_dir(&archive_dir, Some(0o700))?;
    let src = env.expand(&profile.key.path);
    let file_name = src.file_name().ok_or_else(|| Error::KeyMissing(profile.key.path.clone()))?;
    let dst = archive_dir.join(file_name);
    let mut dst_pub = dst.clone().into_os_string();
    dst_pub.push(".pub");
    let dst_pub = std::path::PathBuf::from(dst_pub);
    if dst.exists() || dst_pub.exists() {
        return Err(Error::KeyExists(env.contract(&dst)));
    }
    let remove_key_urls: Vec<String> =
        profile.hosts.iter().filter_map(|h| provider::keys_url(h.provider, &h.hostname)).collect();
    // Another active key entry uses the same file: it stays where it is.
    let shared = config.active_profiles().any(|p| p.id != id && p.key.path == profile.key.path);

    // Check the result before anything is moved, so a refusal leaves no half-done archive.
    let mut after = config.clone();
    {
        let p = after.profile_mut(id)?;
        p.archived = true;
    }
    if after.default_profile.as_deref() == Some(id) {
        after.default_profile = None;
    }
    store::validate(&after)?;

    if shared {
        let key_path = profile.key.path.clone();
        *config = after;
        store::save(env, config)?;
        crate::history::forget(env, id)?;
        return Ok(ArchiveResult { key_path, remove_key_urls });
    }

    let src_pub = env.expand(&profile.key.pub_path());
    let known_hosts = env.expand(&crate::render::profile_known_hosts(id));
    let known_hosts_dst = archive_dir.join(format!("known_hosts-{id}"));
    // Everything that moves, so a failed save can put it back.
    let moves: Vec<(std::path::PathBuf, std::path::PathBuf)> =
        [(src.clone(), dst.clone()), (src_pub.clone(), dst_pub.clone()), (known_hosts, known_hosts_dst)]
            .into_iter()
            .filter(|(from, _)| from.exists())
            .collect();
    for (from, to) in &moves {
        std::fs::rename(from, to).map_err(|e| Error::io(from, e))?;
    }

    let key_path = env.contract(&dst);
    let was_default = config.default_profile.as_deref() == Some(id);
    let before = config.clone();
    let profile = config.profile_mut(id)?;
    profile.key.path = key_path.clone();
    profile.archived = true;
    if was_default {
        config.default_profile = None;
    }
    if let Err(e) = store::save(env, config) {
        for (from, to) in &moves {
            let _ = std::fs::rename(to, from);
        }
        *config = before;
        return Err(e);
    }
    crate::history::forget(env, id)?;
    // The agent may still hold the key (AddKeysToAgent); until it is dropped,
    // plain `ssh host` would keep offering it. Best effort: no agent, no harm.
    if dst_pub.exists() {
        let mut cmd = env.command("ssh-add");
        cmd.arg("-d").arg(&dst_pub).stdin(std::process::Stdio::null());
        let _ = cmd.output();
    }
    Ok(ArchiveResult { key_path, remove_key_urls })
}
