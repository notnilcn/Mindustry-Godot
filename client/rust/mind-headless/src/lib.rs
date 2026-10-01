// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless` — the headless test-rig.
//!
//! Loads `mind-core`, runs scenarios/dumps/benchmarks and never links Godot or
//! opens a socket. `main.rs` is a thin wrapper over this library so plan 22 can
//! reuse the same harness as its dedicated `server` mode (§12 C8).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod cli;
pub mod exec;
pub mod paths;
pub mod registry;
pub mod report;

use clap::Parser;

pub use cli::Cli;

/// Parses CLI arguments and executes the requested command.
///
/// Returns the process exit code: `0` pass, `1` assertion/golden mismatch,
/// `2` usage/IO error.
pub fn run_from_args<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => exec::run(cli),
        Err(error) => {
            let _ = error.print();
            error.exit_code()
        }
    }
}
