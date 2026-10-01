// SPDX-License-Identifier: GPL-3.0-only

//! Relay tables (plan 01 §3.8): match directory, membership, the append-only
//! command log (private, exposed only through views) and the per-identity rate
//! gate. Public accessor names are schema ABI: append-only, never rename.

use spacetimedb::{Identity, SpacetimeType, Timestamp, table};

/// Lifecycle state of a relay match.
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatchStatus {
    /// Open for joins; commands are not accepted yet.
    Lobby,
    /// Commands are accepted and relayed.
    Running,
    /// Terminal; plan 21 owns teardown.
    Ended,
}

/// Who authors match state (D2: relay today, authoritative sim later).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthorityMode {
    /// Clients run their own deterministic sim from relayed command rows.
    Relay,
    /// A server sim owns match state (deferred; plan 21).
    Authoritative,
}

/// Place-block payload (plan §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct PlaceBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Opaque content ID (never 0).
    pub block_id: u16,
    /// 0..=3 rotation.
    pub rotation: u8,
    /// Opaque block config word.
    pub config: u32,
}

/// Break-block payload (plan §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct BreakBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
}

/// Block-config payload (plan §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct ConfigBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Opaque config word.
    pub config: u32,
}

/// The plan-01 command envelope subset (plan §6.3). Exhaustive by design: no
/// opaque bytes, so cheap validation can reason about every variant.
///
/// 2.10.1 `SpacetimeType` derives only unit/newtype variants, so the payloads
/// are the product structs above: `Ping { nonce }` → `Ping(u64)`,
/// `PlaceBlock { .. }` → `PlaceBlock(PlaceBlock)`. The envelope (variant set +
/// meaning) is unchanged; plan 21 owns the schema hard-cut.
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub enum CommandKind {
    /// Explicit no-op (tests/timeline padding).
    Noop,
    /// Round-trip probe used by dev helpers and integration tests.
    Ping(u64),
    /// Place a block by content ID (opaque until plan 02/21 bind payloads).
    PlaceBlock(PlaceBlock),
    /// Break a block at a tile.
    BreakBlock(BreakBlock),
    /// Reconfigure an existing block.
    ConfigBlock(ConfigBlock),
}

/// One match directory row.
#[table(accessor = relay_match, public)]
pub struct RelayMatch {
    #[primary_key]
    #[auto_inc]
    pub match_id: u64,
    pub map_id: String,
    pub map_seed: u64,
    pub map_width_tiles: i32,
    pub map_height_tiles: i32,
    pub status: MatchStatus,
    pub authority: AuthorityMode,
    pub protocol_version: u32,
    pub created_by: Identity,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
}

/// Membership row: one per (match, identity).
#[table(accessor = relay_member, index(accessor = by_match_identity, btree(columns = [match_id, identity])), index(accessor = by_identity, btree(columns = [identity, match_id])))]
pub struct RelayMember {
    #[primary_key]
    #[auto_inc]
    pub member_id: u64,
    #[index(btree)]
    pub match_id: u64,
    pub identity: Identity,
    pub joined_at: Timestamp,
}

/// Append-only command log. PRIVATE: only ever exposed through
/// `my_match_commands` (plan §3.8/§3.12 invariant 3).
#[table(accessor = match_command, index(accessor = by_match_command, btree(columns = [match_id, command_id])))]
pub struct MatchCommand {
    /// Global commit order; numeric gaps are normal, never loss.
    #[primary_key]
    #[auto_inc]
    pub command_id: u64,
    #[index(btree)]
    pub match_id: u64,
    pub sender: Identity,
    /// Per-sender continuity, assigned by the server, starts at 1.
    pub sender_seq: u64,
    /// Sender's 60 Hz sim tick.
    pub client_tick: u64,
    pub kind: CommandKind,
    pub sent_at: Timestamp,
}

/// Server-only rate gate: one row per identity, window rolls on demand.
#[table(accessor = command_rate)]
pub struct CommandRate {
    #[primary_key]
    pub identity: Identity,
    pub window_start: Timestamp,
    pub count: u32,
    /// Last assigned `sender_seq`; persists across window rolls.
    pub last_sender_seq: u64,
}
