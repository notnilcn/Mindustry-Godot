// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch-argument parsing for the Godot client (plan 22 §3.4).
//!
//! The pure parser lives in `mind_core::platform::args`; this re-export keeps
//! the plan-22 `mind-gdext/src/platform/args.rs` seam and gives the client a
//! single import path.

pub use mind_core::platform::args::{LaunchArgs, parse};

/// Parses the running process argument list (`OS.get_cmdline_args` layout).
pub fn parse_process_args() -> LaunchArgs {
    parse(std::env::args().skip(1))
}
