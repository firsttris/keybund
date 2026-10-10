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
    /// Profil ändern: Name, Farbe, Ordner, Hosts, Git-Identität, Key
    Edit(Box<commands::EditArgs>),
    /// Pubkey eines Profils in die Zwischenablage kopieren
    Copy {
        profile: String,
        /// Pubkey nur ausgeben statt kopieren
        #[arg(long)]
        print: bool,
    },
    /// Pubkey eintragen: auf Servern wie ssh-copy-id, bei GitHub/GitLab per API
    ///
    /// Für GitHub und GitLab meldest du dich im Browser an (Code bestätigen);
    /// selbst gehostete Instanzen brauchen ein Token (--token-stdin).
    InstallKey {
        profile: String,
        /// Nur dieser Host (Alias oder Hostname/IP); ohne: alle Hosts des Profils
        #[arg(long)]
        host: Option<String>,
        /// Persönliches Zugriffstoken von stdin lesen
        #[arg(long)]
        token_stdin: bool,
        /// Token im Schlüsselbund merken (secret-tool)
        #[arg(long)]
        remember: bool,
    },
    /// Verbindung testen (Exit-Code 0 bei Erfolg)
    Test {
        profile: String,
        /// Nur diesen Host (Alias oder Hostname/IP); ohne: alle Hosts des Profils
        #[arg(long)]
        host: Option<String>,
    },
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
    /// Bestehende ~/.ssh/config übernehmen (Profile mit den vorhandenen Keys)
    Import {
        /// Alle Vorschläge ohne Auswahl übernehmen
        #[arg(long)]
        all: bool,
        /// Ohne Rückfragen schreiben, inkl. Entfernen der alten Einträge
        #[arg(long, short)]
        yes: bool,
        /// Übernommene Einträge in ~/.ssh/config stehen lassen
        #[arg(long)]
        keep_old: bool,
    },
    /// Alles entfernen, was Keybund geschrieben hat (Keys bleiben)
    Uninstall {
        /// ~/.ssh/config und ~/.gitconfig aus den Backups von vor Keybund wiederherstellen
        #[arg(long)]
        restore_backups: bool,
        /// Auch profiles.toml löschen
        #[arg(long)]
        purge: bool,
        #[arg(long, short)]
        yes: bool,
    },
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
        Command::Edit(args) => commands::edit(&env, *args),
        Command::Copy { profile, print } => commands::copy(&env, &profile, print),
        Command::Test { profile, host } => commands::test(&env, &profile, host.as_deref()),
        Command::InstallKey { profile, host, token_stdin, remember } => commands::install_key(
            &env,
            &profile,
            host.as_deref(),
            &commands::TokenOptions { stdin: token_stdin, remember },
        ),
        Command::Which { target } => commands::which(&env, target.as_deref()),
        Command::Clone { url, profile, dest } => commands::clone(&env, &url, profile.as_deref(), dest),
        Command::Apply { dry_run, yes } => commands::apply(&env, dry_run, yes),
        Command::Doctor { fix } => commands::doctor(&env, fix),
        Command::Keys => commands::keys(&env),
        Command::Import { all, yes, keep_old } => commands::import(&env, all, yes, keep_old),
        Command::Default { profile } => commands::set_default(&env, &profile),
        Command::Uninstall { restore_backups, purge, yes } => commands::uninstall(&env, restore_backups, purge, yes),
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
