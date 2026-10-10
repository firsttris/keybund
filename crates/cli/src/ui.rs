//! Small output helpers. Colors are disabled automatically when stdout is
//! not a terminal or `NO_COLOR` is set.

use console::style;
use keybund_core::doctor::Status;

pub fn success(msg: &str) {
    println!("{} {msg}", style("✓").green().bold());
}

pub fn warn(msg: &str) {
    println!("{} {msg}", style("!").yellow().bold());
}

pub fn error(msg: &str) {
    eprintln!("{} {msg}", style("✗").red().bold());
}

pub fn hint(msg: &str) {
    println!("  {}", style(msg).dim());
}

pub fn heading(msg: &str) {
    println!("{}", style(msg).bold());
}

pub fn status(status: Status, msg: &str) {
    match status {
        Status::Ok => success(msg),
        Status::Warn => warn(msg),
        Status::Error => println!("{} {msg}", style("✗").red().bold()),
    }
}

/// Prints a unified diff with colored +/- lines.
pub fn diff(text: &str) {
    for line in text.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            println!("{}", style(line).bold());
        } else if line.starts_with('+') {
            println!("{}", style(line).green());
        } else if line.starts_with('-') {
            println!("{}", style(line).red());
        } else if line.starts_with("@@") {
            println!("{}", style(line).cyan());
        } else {
            println!("{line}");
        }
    }
}

pub fn row(label: &str, value: &str) {
    println!("  {:<14} {value}", style(label).dim());
}
