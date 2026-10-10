//! Clipboard and browser access.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

/// Copies text to the clipboard. On Linux the external tools are preferred
/// because they keep serving the clipboard after keybund exits.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let x11 = std::env::var_os("DISPLAY").is_some();
        let mut candidates: Vec<(&str, &[&str])> = Vec::new();
        if wayland {
            candidates.push(("wl-copy", &[]));
        }
        if x11 {
            candidates.push(("xclip", &["-selection", "clipboard"]));
            candidates.push(("xsel", &["--clipboard", "--input"]));
        }
        for (program, args) in candidates {
            if pipe_to(program, args, text).is_ok() {
                return Ok(());
            }
        }
        bail!("Keine Zwischenablage gefunden (wl-copy, xclip oder xsel installieren) – mit --print ausgeben");
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard = arboard::Clipboard::new().context("Zwischenablage nicht verfügbar")?;
        clipboard.set_text(text.to_string()).context("Kopieren fehlgeschlagen")?;
        Ok(())
    }
}

#[allow(dead_code)]
fn pipe_to(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("{program} nicht gefunden"))?;
    child.stdin.take().context("stdin")?.write_all(text.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("{program} fehlgeschlagen");
    }
    Ok(())
}

/// Opens a URL in the default browser.
pub fn open_url(url: &str) -> Result<()> {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("{program} nicht gefunden"))?;
    Ok(())
}
