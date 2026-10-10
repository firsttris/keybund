//! `keybund` – SSH keys per project, from the terminal.

mod commands;
mod system;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "keybund", version, about = "SSH-Keys pro Projekt – erzeugt Standard-Configs für OpenSSH und Git")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Profile mit Key-Fingerprint, Ordnern und Hosts auflisten
    List {
        /// Auch archivierte Profile zeigen
        #[arg(long)]
        all: bool,
    },
    /// Ein Profil im Detail zeigen
    Show { profile: String },
    /// Neues Projekt anlegen (interaktiv, oder komplett über Optionen)
    New(Box<commands::NewArgs>),
    /// Pubkey eines Profils in die Zwischenablage kopieren
    Copy {
        profile: String,
        /// Pubkey nur ausgeben statt kopieren
        #[arg(long)]
        print: bool,
    },
    /// Verbindung testen (Exit-Code 0 bei Erfolg)
    Test { profile: String },
    /// Welcher Key wird hier benutzt? Ohne Argument: aktuelles Verzeichnis
    Which { target: Option<String> },
    /// Repo ins richtige Profil klonen
    Clone {
        url: String,
        #[arg(long, short)]
        profile: Option<String>,
        /// Zielordner (Standard: <Profilordner>/<Reponame>)
        #[arg(long)]
        dest: Option<PathBuf>,
    },
    /// Config-Änderungen als Diff zeigen und schreiben
    Apply {
        /// Nur den Diff zeigen
        #[arg(long)]
        dry_run: bool,
        /// Ohne Rückfrage schreiben
        #[arg(long, short)]
        yes: bool,
    },
    /// Alle Checks ausführen (Exit-Code ≠ 0 bei Problemen)
    Doctor {
        /// Automatische Korrekturen anwenden
        #[arg(long)]
        fix: bool,
    },
    /// Vorhandene Keys in ~/.ssh auflisten
    Keys,
    /// Default-Profil setzen (Key außerhalb aller Projektordner)
    Default { profile: String },
    /// Profil archivieren: Key nach ~/.ssh/archive, Configs entfernen
    Archive {
        profile: String,
        #[arg(long, short)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            ui::error(&format!("{e:#}"));
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let env = keybund_core::Env::from_system()?;
    match cli.command {
        Command::List { all } => commands::list(&env, all),
        Command::Show { profile } => commands::show(&env, &profile),
        Command::New(args) => commands::new(&env, *args),
        Command::Copy { profile, print } => commands::copy(&env, &profile, print),
        Command::Test { profile } => commands::test(&env, &profile),
        Command::Which { target } => commands::which(&env, target.as_deref()),
        Command::Clone { url, profile, dest } => commands::clone(&env, &url, profile.as_deref(), dest),
        Command::Apply { dry_run, yes } => commands::apply(&env, dry_run, yes),
        Command::Doctor { fix } => commands::doctor(&env, fix),
        Command::Keys => commands::keys(&env),
        Command::Default { profile } => commands::set_default(&env, &profile),
        Command::Archive { profile, yes } => commands::archive(&env, &profile, yes),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
