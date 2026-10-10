//! Keybund core library.
//!
//! Keybund assigns every project a profile (SSH key, hosts, folders, Git
//! identity) and derives standard OpenSSH and Git configuration from it. The
//! core never runs a daemon and never reads private keys: it only renders
//! config files and calls `ssh`, `ssh-keygen` and `git`.
//!
//! All functions take an [`Env`] so they can be pointed at a temporary home
//! directory in tests.

pub mod apply;
pub mod archive;
pub mod clone;
pub mod desktop;
pub mod doctor;
pub mod env;
pub mod error;
pub mod gitenv;
pub mod history;
pub mod hostkeys;
pub mod import;
pub mod keygen;
pub mod keyinstall;
pub mod model;
pub mod probe;
pub mod provider;
pub mod render;
pub mod store;
pub mod uninstall;
pub mod which;

mod fsutil;
mod process;

pub use env::Env;
pub use error::{Error, Result};
pub use model::*;
