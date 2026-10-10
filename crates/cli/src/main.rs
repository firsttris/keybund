//! `keybund` – the right SSH key, automatically, from the terminal.

mod commands;
mod system;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "keybund",
    version,
    about = "Der richtige SSH-Key, automatisch – erzeugt Standard-Configs für OpenSSH und Git"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Keys mit Fingerprint, Git-Ordnern und Hosts auflisten
    List {
        /// Auch archivierte Keys zeigen
        #[arg(long)]
        all: bool,
    },
    /// Einen Key im Detail zeigen
    Show {
        #[arg(value_name = "KEY")]
        profile: String,
    },
    /// Neuen Key anlegen (interaktiv, oder komplett über Optionen)
    New(Box<commands::NewArgs>),
    /// Key ändern: Name, Farbe, Git-Ordner, Hosts, Git-Identität, Key-Datei
    Edit(Box<commands::EditArgs>),
    /// Pubkey eines Keys in die Zwischenablage kopieren
    Copy {
        #[arg(value_name = "KEY")]
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
        #[arg(value_name = "KEY")]
        profile: String,
        /// Nur dieser Host (Alias oder Hostname/IP); ohne: alle Hosts des Keys
        #[arg(long)]
        host: Option<String>,
        /// Persönliches Zugriffstoken von stdin lesen
        #[arg(long)]
        token_stdin: bool,
        /// Token im Schlüsselbund merken (secret-tool)
        #[arg(long)]
        remember: bool,
    },
    /// Host-Keys der Server eines Keys prüfen und bestätigen
    Trust {
        #[arg(value_name = "KEY")]
        profile: String,
        /// Nur dieser Host (Alias oder Hostname/IP)
        #[arg(long)]
        host: Option<String>,
        /// Ohne Rückfrage übernehmen (nur wenn der Fingerprint anders geprüft wurde)
        #[arg(long, short)]
        yes: bool,
    },
    /// Verbindung testen (Exit-Code 0 bei Erfolg)
    Test {
        #[arg(value_name = "KEY")]
        profile: String,
        /// Nur diesen Host (Alias oder Hostname/IP); ohne: alle Hosts des Keys
        #[arg(long)]
        host: Option<String>,
    },
    /// Welcher Key wird hier benutzt? Ohne Argument: aktuelles Verzeichnis
    Which { target: Option<String> },
    /// Repo in den Git-Ordner des passenden Keys klonen
    Clone {
        url: String,
        /// Key, der benutzt werden soll (Standard: der Key für den Host)
        #[arg(long, short, visible_alias = "key", value_name = "KEY")]
        profile: Option<String>,
        /// Zielordner (Standard: <Git-Ordner>/<Reponame>)
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
    /// Bestehende ~/.ssh/config übernehmen (als Keys in Keybund)
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
    /// Standard-Key setzen (für Repos außerhalb aller Git-Ordner)
    Default {
        #[arg(value_name = "KEY")]
        profile: String,
    },
    /// Einstellungen zeigen oder ändern, z. B. `keybund config projects-dir ~/work`
    Config { key: Option<String>, value: Option<String> },
    /// Key archivieren: nach ~/.ssh/archive verschieben, Configs entfernen
    Archive {
        #[arg(value_name = "KEY")]
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
        Command::Trust { profile, host, yes } => commands::trust(&env, &profile, host.as_deref(), yes),
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
        Command::Config { key, value } => commands::config(&env, key.as_deref(), value.as_deref()),
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
