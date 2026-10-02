// SPDX-License-Identifier: GPL-3.0-only

//! Version and format constants.
//!
//! Ported from Mindustry's `Version.java` build reporting and the P0 formats in
//! `00_FOUNDATION_IMPLEMENTATION_PLAN.md` §6.

/// Version of the `mind-core` crate, taken from `Cargo.toml`.
pub const MIND_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name used by dumps and the data directory.
pub const APP_NAME: &str = "Mindustry-Godot";

/// Format version of the canonical JSON state dump (§6.3).
pub const DUMP_FORMAT: u32 = 1;

/// Format version of scenario files (§6.1).
pub const SCENARIO_FORMAT: u32 = 1;

/// Format version of command-log JSONL files (§6.2).
pub const COMMAND_LOG_FORMAT: u32 = 1;

/// Build number written into save meta tags (`Version.build`; 0 = dev build,
/// matching the upstream default when no build is stamped).
pub const BUILD: i32 = 0;
