// SPDX-License-Identifier: GPL-3.0-only

//! Relay tables (plan 01 §3.8, extended by plan 21 §3.4/§6.1): match directory,
//! membership, the append-only command log (private, exposed only through views)
//! and the per-identity rate gate. Public accessor names are schema ABI:
//! append-only, never rename.

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

/// Match gamemode tag (plan 21 §3.3; variant names are ABI, append-only).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gamemode {
    /// Wave survival.
    Survival,
    /// Unrestricted building, no wave pressure.
    Sandbox,
    /// Attack a fixed enemy base.
    Attack,
    /// Player-versus-player.
    Pvp,
    /// Map editor session.
    Editor,
}

/// Server-browser visibility (plan 21 §3.3; variant names are ABI).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// Listed in `all_matches`.
    Public,
    /// Reachable only by match id (shared out of band).
    Unlisted,
}

/// Membership role (plan 21 §3.3; variant names are ABI).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemberRole {
    /// Full participant; may emit sim commands.
    Player,
    /// Read-only observer; sim commands are rejected.
    Spectator,
}

/// Place-block payload (plan 21 §3.4/§6.3). `block` is a content name; the
/// opaque config blob comes from plan 04's TypeIO codec.
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct PlaceBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Content name (append-only ABI).
    pub block: String,
    /// 0..=3 rotation.
    pub rotation: u8,
    /// Opaque block configuration bytes (≤ 4 KiB).
    pub config: Vec<u8>,
}

/// Break-block payload (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct BreakBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
}

/// Block-config payload (plan 21 §6.3); `value` is plan 04's TypeIO blob.
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct ConfigBlock {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Opaque configuration bytes (≤ 16 KiB).
    pub value: Vec<u8>,
}

/// Rotate a placed building (plan 15 request; plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct Rotate {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// `false` clockwise, `true` counter-clockwise.
    pub direction: bool,
}

/// Remove queued/team plans at packed positions (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct DeletePlans {
    /// Arc `Point2.pack`ed positions (≤ 256).
    pub positions: Vec<i32>,
}

/// Command buildings to move/attack (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct CommandBuilding {
    /// Arc `Point2.pack`ed positions (≤ 256).
    pub positions: Vec<i32>,
    /// Target x.
    pub x: f32,
    /// Target y.
    pub y: f32,
}

/// Withdraw/deposit/drop an item (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct Inventory {
    /// `0` withdraw, `1` deposit, `2` drop.
    pub kind: u8,
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Item content name, when relevant.
    pub item: Option<String>,
    /// Amount.
    pub amount: i32,
    /// Drop angle.
    pub angle: f32,
}

/// Pick up / drop a payload (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct Payload {
    /// `0` pickup unit, `1` pickup build, `2` drop.
    pub kind: u8,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Target unit/building id.
    pub target: Option<i32>,
}

/// Possess a unit or return to the player (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct UnitControl {
    /// Unit entity id, or `None` to clear.
    pub unit: Option<i32>,
}

/// Select a controllable building (plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct BuildingControlSelect {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
}

/// Issue a unit command (plan 21 §6.3; plan 11 shapes).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct UnitCommand {
    /// Unit entity ids (≤ 200).
    pub units: Vec<i32>,
    /// Command content id.
    pub command: u16,
    /// Target x.
    pub x: f32,
    /// Target y.
    pub y: f32,
}

/// Queue a unit command (plan 11 §6.5; plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct UnitCommandQueue {
    /// Unit entity ids (≤ 200).
    pub units: Vec<i32>,
    /// Command content id.
    pub command: u16,
    /// Target x.
    pub x: f32,
    /// Target y.
    pub y: f32,
}

/// Set a unit stance (plan 11 §6.5; plan 21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct UnitStance {
    /// Unit entity ids (≤ 200).
    pub units: Vec<i32>,
    /// Stance content id.
    pub stance: u16,
    /// Whether the stance is enabled.
    pub enabled: bool,
}

/// Spawn the sending player's unit (plan 21 §3.3/§6.3; self only).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct PlayerSpawn {
    /// Requested unit content name, when the client overrides the default.
    pub unit: Option<String>,
    /// Team id.
    pub team: u8,
}

/// Host-owned bullet spawn (plan 10/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct Bullet {
    /// Bullet content id.
    pub def: u16,
    /// Team id.
    pub team: u8,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Angle.
    pub angle: f32,
    /// Damage override.
    pub damage: f32,
    /// Velocity scale.
    pub velocity_scl: f32,
    /// Lifetime scale.
    pub lifetime_scl: f32,
    /// Aim x (`NaN` when absent).
    pub aim_x: f32,
    /// Aim y (`NaN` when absent).
    pub aim_y: f32,
    /// Extra bullet data (≤ 4 KiB).
    pub data: Vec<u8>,
}

/// Replace the match rules (host/admin; plan 12/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct SetRules {
    /// Serialized rules JSON (≤ 100 000 chars).
    pub rules_json: String,
    /// Epoch the client expects; must equal `relay_match.rules_epoch`.
    pub rules_epoch: u32,
}

/// Edit one rule field (host/admin; plan 12/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct SetRule {
    /// Rule name (≤ 64 chars).
    pub rule: String,
    /// Serialized value (≤ 16 KiB).
    pub json: String,
}

/// Unlock a research node (host/admin; plan 12/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct ResearchUnlock {
    /// Content name (≤ 100 chars).
    pub content: String,
}

/// Complete an objective (host/admin; plan 12/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct CompleteObjective {
    /// Objective index.
    pub index: u32,
    /// Epoch the client expects.
    pub rules_epoch: u32,
}

/// Run `count` waves (host/admin; plan 11/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct RunWave {
    /// Wave count (≤ 10).
    pub count: u8,
}

/// Switch a player's team (host/admin; plan 12/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct AdminSwitchTeam {
    /// Target identity.
    pub target: Identity,
    /// Team id.
    pub team: u8,
}

/// Host/admin remote tile operation (plan 06/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct AdminTileOp {
    /// Plan-06 op selector.
    pub op: u8,
    /// Packed positions (≤ 1024).
    pub points: Vec<i32>,
    /// First integer argument.
    pub arg0: i32,
    /// Second integer argument.
    pub arg1: i32,
    /// Third integer argument.
    pub arg2: i32,
    /// First optional content name (≤ 100).
    pub name0: Option<String>,
    /// Second optional content name (≤ 100).
    pub name1: Option<String>,
}

/// Logic variable sync (plan 13/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct LogicSync {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Variable name (≤ 64 chars).
    pub var_name: String,
    /// Variable id.
    pub var_id: i32,
    /// Serialized value (≤ 1 KiB).
    pub value: Vec<u8>,
}

/// Client logic data channel (plan 13/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct LogicClientData {
    /// Channel name (≤ 64 chars).
    pub channel: String,
    /// Serialized value (≤ 8 KiB).
    pub value: Vec<u8>,
    /// Whether the client requested reliable delivery.
    pub reliable: bool,
}

/// Menu choice result (plan 14/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct MenuChoose {
    /// Menu id.
    pub menu_id: i32,
    /// Chosen option index.
    pub option: i32,
}

/// Menu builder result (plan 14/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct MenuBuilderChoose {
    /// Menu id.
    pub menu_id: i32,
    /// Result payload (≤ 8 KiB).
    pub result: Vec<u8>,
}

/// Text input result (plan 14/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct TextInputResult {
    /// Input id.
    pub id: i32,
    /// Entered text (≤ 256 chars), or `None` when cancelled.
    pub text: Option<String>,
}

/// Mod/data extension command (plan 05/21 §6.3).
#[derive(SpacetimeType, Clone, PartialEq, Eq, Debug)]
pub struct Custom {
    /// Extension kind.
    pub kind: u16,
    /// Extension data (≤ 1 KiB).
    pub data: Vec<u8>,
}

/// The full plan-21 command envelope variant set (plan §3.4/§6.3).
///
/// Plan 01's variants stay by name; the variant *names* are the ABI and are
/// append-only. 2.10.1 `SpacetimeType` derives only unit/newtype variants, so
/// product payloads are the structs above. The envelope
/// (`MatchCommand { command_id, match_id, sender, sender_seq, client_tick,
/// kind, sent_at }`) is unchanged.
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub enum CommandKind {
    /// Explicit no-op (tests/timeline padding).
    Noop,
    /// Round-trip probe used by dev helpers and integration tests.
    Ping(u64),
    /// Place a block by content name.
    PlaceBlock(PlaceBlock),
    /// Break a block at a tile.
    BreakBlock(BreakBlock),
    /// Reconfigure an existing block.
    ConfigBlock(ConfigBlock),
    /// Rotate a placed building.
    Rotate(Rotate),
    /// Remove queued/team plans.
    DeletePlans(DeletePlans),
    /// Command buildings to move/attack.
    CommandBuilding(CommandBuilding),
    /// Withdraw/deposit/drop an item.
    Inventory(Inventory),
    /// Pick up / drop a payload.
    Payload(Payload),
    /// Possess a unit or return to the player.
    UnitControl(UnitControl),
    /// Clear the controlled unit.
    UnitClear,
    /// Select a controllable building.
    BuildingControlSelect(BuildingControlSelect),
    /// Issue a unit command.
    UnitCommand(UnitCommand),
    /// Queue a unit command.
    UnitCommandQueue(UnitCommandQueue),
    /// Set a unit stance.
    UnitStance(UnitStance),
    /// Spawn the sending player's unit.
    PlayerSpawn(PlayerSpawn),
    /// Host-owned bullet spawn.
    Bullet(Bullet),
    /// Replace the match rules.
    SetRules(SetRules),
    /// Edit one rule field.
    SetRule(SetRule),
    /// Unlock a research node.
    ResearchUnlock(ResearchUnlock),
    /// Complete an objective.
    CompleteObjective(CompleteObjective),
    /// Clear all objectives.
    ClearObjectives,
    /// Capture the current sector.
    SectorCapture,
    /// Persist the current sector.
    SaveSector,
    /// Skip the current wave.
    SkipWave,
    /// Run `count` waves.
    RunWave(RunWave),
    /// Switch a player's team.
    AdminSwitchTeam(AdminSwitchTeam),
    /// Host/admin remote tile operation.
    AdminTileOp(AdminTileOp),
    /// Logic variable sync.
    LogicSync(LogicSync),
    /// Client logic data channel.
    LogicClientData(LogicClientData),
    /// Menu choice result.
    MenuChoose(MenuChoose),
    /// Menu builder result.
    MenuBuilderChoose(MenuBuilderChoose),
    /// Text input result.
    TextInputResult(TextInputResult),
    /// Mod/data extension.
    Custom(Custom),
}

/// One match directory row.
#[table(accessor = relay_match, public, index(accessor = by_visibility, btree(columns = [visibility])))]
pub struct RelayMatch {
    #[primary_key]
    #[auto_inc]
    pub match_id: u64,
    pub map_id: String,
    pub map_seed: u64,
    pub map_hash: u64,
    pub map_width_tiles: i32,
    pub map_height_tiles: i32,
    pub status: MatchStatus,
    pub authority: AuthorityMode,
    pub protocol_version: u32,
    pub created_by: Identity,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
    // Plan 21 §3.4 extensions:
    /// Authoritative checkpoint source (D2); equals `created_by` today.
    pub host: Identity,
    pub mode: Gamemode,
    pub mode_name: String,
    pub visibility: Visibility,
    /// Salted FNV-1a of the password; never the plaintext.
    pub password_hash: Option<u64>,
    pub rules_json: String,
    pub rules_epoch: u32,
    pub build_id: String,
    pub content_hash: u64,
    pub is_dedicated: bool,
    pub player_count: u16,
    pub max_players: u16,
    pub campaign_id: Option<u64>,
    pub sector_planet: Option<String>,
    pub sector_id: Option<u32>,
    pub last_command_id: u64,
    pub last_snapshot_id: Option<u64>,
    pub closed_at: Option<Timestamp>,
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
    // Plan 21 §3.4 extensions:
    pub role: MemberRole,
    pub team: Option<u8>,
    pub connected: bool,
    pub ready: bool,
    pub last_seen_at: Timestamp,
    pub kicked_reason: Option<String>,
}

/// Append-only command log. PRIVATE: only ever exposed through
/// `my_match_commands` (plan 01 §3.8/§3.12 invariant 3).
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
    // Plan 21 §3.4 per-class counters:
    pub count_input: u32,
    pub count_plan: u32,
    pub count_ui: u32,
    pub count_logic: u32,
    pub count_admin: u32,
}

/// Server-only per-sender echo/rejection bookkeeping (plan §3.5/§6.1);
/// exposed to the caller through `my_sender_command_state`.
#[table(accessor = sender_command_state)]
pub struct SenderCommandState {
    #[primary_key]
    pub identity: Identity,
    pub last_accepted_seq: u64,
    pub last_accepted_command_id: u64,
    pub reject_count: u32,
    pub last_gap_at: Option<Timestamp>,
    pub updated_at: Timestamp,
}

/// Host-published state snapshot equivalent (plan §6.1).
#[table(accessor = match_state, public, index(accessor = by_match_state, btree(columns = [match_id])))]
pub struct MatchState {
    #[primary_key]
    #[auto_inc]
    pub state_id: u64,
    pub match_id: u64,
    pub rules_json: String,
    pub rules_epoch: u32,
    pub wave: i32,
    pub wavetime: f32,
    pub enemies: i32,
    pub paused: bool,
    pub game_over: bool,
    pub sim_tick: u64,
    pub last_command_id: u64,
    pub rng_sim: Vec<u8>,
    pub next_entity_id: i32,
    pub snapshot_id: Option<u64>,
    pub updated_by: Identity,
    pub updated_at: Timestamp,
}

/// Server-only kick record (plan §6.1); the target watches `my_kick`.
#[table(accessor = match_kick, index(accessor = by_kick_target, btree(columns = [target])))]
pub struct MatchKick {
    #[primary_key]
    #[auto_inc]
    pub kick_id: u64,
    #[index(btree)]
    pub match_id: u64,
    pub target: Identity,
    pub reason: String,
    pub by: Identity,
    pub at: Timestamp,
}
