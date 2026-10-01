// SPDX-License-Identifier: GPL-3.0-only

//! Single home for the module's tunables (plan 01 §3.7): protocol version, relay
//! rate limits and default map bounds. Kept in lockstep with
//! `client/rust/mind-stdb/src/protocol.rs` (bump both together, §6.4).

/// Our protocol version (separate from crate versions). Bump on any command
/// envelope / view-shape change; plan 23 checks the live row against the client.
pub const PROTOCOL_VERSION: u32 = 1;

/// Minimum client build accepted with [`PROTOCOL_VERSION`] (plan 21 grows this).
pub const MIN_CLIENT_BUILD: u32 = 1;

/// Save-format version advertised to clients; plan 04 owns save serialization.
pub const SAVE_FORMAT_VERSION: u32 = 1;

/// Commands accepted per identity per rate window (plan 21 tunes against real
/// build spam; plan 01 default is intentionally generous for tests).
pub const DEFAULT_COMMANDS_PER_SECOND: u32 = 600;

/// Rate-limit window length in milliseconds.
pub const DEFAULT_COMMAND_RATE_WINDOW_MS: u32 = 1_000;

/// Cheap cap for how many commands one transaction may carry (plan 21 batching).
pub const DEFAULT_MAX_COMMIT_COMMANDS_PER_TRANSACTION: u32 = 256;

/// Default map bounds used when a match is created without explicit size
/// (Mindustry maps are far smaller; plan 06/21 pass real values).
pub const DEFAULT_MAP_WIDTH_TILES: i32 = 1_000;

/// Default map height; see [`DEFAULT_MAP_WIDTH_TILES`].
pub const DEFAULT_MAP_HEIGHT_TILES: i32 = 1_000;
