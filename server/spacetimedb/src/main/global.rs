// SPDX-License-Identifier: GPL-3.0-only

//! Single home for the module's tunables (plan 01 §3.7; plan 21 §3.4/§6.8).
//! Kept in lockstep with `client/rust/mind-stdb/src/protocol.rs` (bump both
//! together, §6.4).

// Forward-declared tunables consumed by later plan-21 milestones (M3–M5); the
// values are part of the frozen schema design and intentionally live here now.
#![allow(dead_code)]

/// Our protocol version (separate from crate versions). Bump on any command
/// envelope / view-shape change; plan 23 checks the live row against the client.
pub const PROTOCOL_VERSION: u32 = 2;

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

/// Default player cap for a newly created match (plan 21 §3.3).
pub const DEFAULT_MAX_PLAYERS: u16 = 8;

/// Sender-side input delay stamped into `client_tick` (plan §3.5): 6 ticks.
pub const INPUT_DELAY_TICKS: u64 = 6;

/// Advertised checksum publish cadence (plan §3.7).
pub const CHECKSUM_INTERVAL_TICKS: u64 = 120;

/// Command-id checkpoint spacing for checksum comparison (plan §3.7).
pub const CHECKPOINT_INTERVAL_COMMANDS: u64 = 128;

/// Host digest cadence (plan §3.7).
pub const DIGEST_INTERVAL_TICKS: u64 = 600;

/// Host dynamic-snapshot cadence (plan §3.7).
pub const SNAPSHOT_INTERVAL_TICKS: u64 = 3_600;

/// Snapshot chunk size in bytes (plan §3.8).
pub const SNAPSHOT_CHUNK_BYTES: u32 = 16 * 1024;

/// Local command-ring tail replay window (plan §3.7).
pub const REPLAY_TAIL: u64 = 4_096;

/// Full-resync retry limit before surfacing a toast (plan §3.7).
pub const RESYNC_RETRY_LIMIT: u32 = 3;

/// Disconnected-member grace before `tick_maintenance` removes the row.
pub const MEMBER_GRACE_SECS: u64 = 120;

/// Chat text cap (Mindustry `maxTextLength`).
pub const MAX_TEXT: usize = 150;

/// Plan-snapshot chunk cap (plan 15 §6.5).
pub const MAX_PLAN_CHUNK_BYTES: usize = 8 * 1024;

/// Per-player plan-snapshot cap.
pub const MAX_PLANS_BYTES: usize = 128 * 1024;

/// Incomplete plan group timeout (plan §6.6).
pub const PLAN_GROUP_TIMEOUT_SECS: u64 = 30;

/// Rule JSON cap (plan §6.3).
pub const MAX_RULES_JSON: usize = 100_000;

/// Place-block config blob cap (plan §6.3).
pub const MAX_PLACE_CONFIG_BYTES: usize = 4 * 1024;

/// Config-block value blob cap (plan §6.3).
pub const MAX_CONFIG_BYTES: usize = 16 * 1024;

/// Building/plan list cap for one command (plan §6.3).
pub const MAX_POSITIONS: usize = 256;

/// Unit list cap for one command (plan §6.3).
pub const MAX_UNITS: usize = 200;

/// Bootstrap admin identities (hex), seeded on every publish (OD-21-D default:
/// empty; `server/build.sh --admin <hex>` would inject one). Never a
/// self-claim reducer.
pub const ADMIN_IDENTITIES: &[&str] = &[];
