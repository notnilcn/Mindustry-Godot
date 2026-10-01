// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless` — thin binary over the `mind_headless` library.
//!
//! Plan 22 reuses the library as the dedicated `server` mode (§12 C8).

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(mind_headless::run_from_args(std::env::args_os()) as u8)
}
