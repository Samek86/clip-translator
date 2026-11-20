#![cfg_attr(not(windows), allow(dead_code))]

//! Translation, settings, and hotkey rules shared by the Windows app.
//!
//! The tray, clipboard, DPAPI, and global hotkey live in `winapp` and compile
//! only for Windows. Everything in this crate root is exercised by `cargo test`
//! on Linux.

mod azure;
mod config;
mod deepl;
mod hotkey_spec;
mod http;
mod language;
mod provider;
mod router;
mod secrets;
mod ui_text;
mod usage;

#[cfg(windows)]
mod winapp;

#[cfg(windows)]
pub use winapp::run;
