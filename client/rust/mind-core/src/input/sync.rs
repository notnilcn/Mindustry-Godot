// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan snapshots + player-input sync payloads (plan 15 §3.7/§6.4/§6.5, M6).
//!
//! Plan 21 owns transport; this module defines the exact data 15 hands it:
//! `getSyncedPlans` (breaking filtered, mobile select plans appended), the
//! 0.5 s / 75-per-chunk / 1000-cap snapshot batching from
//! `NetClient.sync()` and the `Call.clientSnapshot` player payload. Nothing here
//! touches the sim (`I1`–`I3`).

use serde::{Deserialize, Serialize};

use crate::content::{BlockId, ContentType};
use crate::world::config::ConfigValue;

use super::plan::ClientPlan;

/// `Vars.maxPlayerPreviewPlans`.
pub const MAX_PLAYER_PREVIEW_PLANS: usize = 1000;
/// Plan-snapshot cadence in ticks (0.5 s at the fixed 60 Hz sim).
pub const PLAN_SNAPSHOT_INTERVAL_TICKS: u64 = 30;
/// `NetClient.sync` plan batch size (`900 / 12`).
pub const PLAN_SNAPSHOT_CHUNK: usize = 75;

/// A network plan config value (`TypeIO` `Number|Boolean|Content` whitelist).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireConfig {
    /// No config / clear.
    None,
    /// A number.
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// A content reference (`(type, id)`).
    Content {
        /// `ContentType` ordinal.
        content_type: u8,
        /// Per-type content id.
        id: u16,
    },
}

impl WireConfig {
    /// Encodes a `world::config::ConfigValue`, collapsing values outside the
    /// network whitelist to `None` (07 §6.3; such plans are not queued).
    pub fn from_config(value: &ConfigValue) -> Self {
        match value {
            ConfigValue::None => WireConfig::None,
            ConfigValue::Number(number) => WireConfig::Number(*number),
            ConfigValue::Bool(flag) => WireConfig::Bool(*flag),
            ConfigValue::Item(id) => WireConfig::Content {
                content_type: ContentType::Item as u8,
                id: id.raw(),
            },
            ConfigValue::Liquid(id) => WireConfig::Content {
                content_type: ContentType::Liquid as u8,
                id: id.raw(),
            },
            ConfigValue::Block(id) => WireConfig::Content {
                content_type: ContentType::Block as u8,
                id: id.raw(),
            },
            ConfigValue::Unit(id) => WireConfig::Content {
                content_type: ContentType::Unit as u8,
                id: id.raw(),
            },
            ConfigValue::Content(content_type, id) => WireConfig::Content {
                content_type: *content_type as u8,
                id: *id,
            },
            _ => WireConfig::None,
        }
    }
}

/// One plan on the wire (`ClientBuildPlans.items`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanWire {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Rotation `0..=3`.
    pub rotation: i8,
    /// Block content id.
    pub block: u16,
    /// Network config.
    pub config: WireConfig,
    /// Deconstruction flag (filtered by `getSyncedPlans`, kept for 19/20).
    pub breaking: bool,
}

impl PlanWire {
    /// Encodes a client plan.
    pub fn from_client(plan: &ClientPlan) -> Self {
        Self {
            x: plan.x as i16,
            y: plan.y as i16,
            rotation: plan.rotation as i8,
            block: plan.block.raw(),
            config: WireConfig::from_config(&plan.config),
            breaking: plan.breaking,
        }
    }

    /// Decodes to a client plan (`BlockId` raw round-trip).
    pub fn to_client(&self) -> ClientPlan {
        ClientPlan {
            x: self.x as i32,
            y: self.y as i32,
            rotation: self.rotation.rem_euclid(4) as u8,
            block: BlockId::new(self.block),
            config: wire_to_config(&self.config),
            breaking: self.breaking,
            anim_scale: 1.0,
        }
    }
}

/// Reverses [`WireConfig::from_config`] for the round-trippable subset.
pub fn wire_to_config(value: &WireConfig) -> ConfigValue {
    match value {
        WireConfig::None => ConfigValue::None,
        WireConfig::Number(number) => ConfigValue::Number(*number),
        WireConfig::Bool(flag) => ConfigValue::Bool(*flag),
        WireConfig::Content { content_type, id } => {
            match ContentType::ALL.get(*content_type as usize).copied() {
                Some(ContentType::Item) => ConfigValue::Content(ContentType::Item, *id),
                Some(content_type) => ConfigValue::Content(content_type, *id),
                None => ConfigValue::None,
            }
        }
    }
}

/// `getSyncedPlans`: non-breaking queue plans, plus mobile select plans.
pub fn get_synced_plans(
    last_plans: &[ClientPlan],
    select_plans: &[ClientPlan],
    mobile: bool,
    out: &mut Vec<ClientPlan>,
) {
    out.clear();
    out.extend(last_plans.iter().filter(|plan| !plan.breaking).cloned());
    if mobile {
        out.extend(select_plans.iter().filter(|plan| !plan.breaking).cloned());
    }
}

/// A `clientPlanSnapshot` payload. `plans == None` is the explicit empty send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSnapshot {
    /// `player.lastPreviewPlanGroup` (same id for every chunk of one send).
    pub plan_group_id: u32,
    /// Issuing player id.
    pub player: i32,
    /// Plans, or `None` for the empty snapshot.
    pub plans: Option<Vec<PlanWire>>,
}

/// Builds the ordered `clientPlanSnapshot` sends for one 0.5 s event.
///
/// Mirrors `NetClient.sync()`: truncate to [`MAX_PLAYER_PREVIEW_PLANS`]; empty →
/// one `None` payload; `< 75` → one payload; else chunks of
/// [`PLAN_SNAPSHOT_CHUNK`], all under `plan_group_id`.
pub fn build_plan_snapshots(
    last_plans: &[ClientPlan],
    select_plans: &[ClientPlan],
    mobile: bool,
    player: i32,
    plan_group_id: u32,
) -> Vec<PlanSnapshot> {
    let mut synced = Vec::new();
    get_synced_plans(last_plans, select_plans, mobile, &mut synced);
    synced.truncate(MAX_PLAYER_PREVIEW_PLANS);

    if synced.is_empty() {
        return vec![PlanSnapshot {
            plan_group_id,
            player,
            plans: None,
        }];
    }
    if synced.len() < PLAN_SNAPSHOT_CHUNK {
        return vec![PlanSnapshot {
            plan_group_id,
            player,
            plans: Some(synced.iter().map(PlanWire::from_client).collect()),
        }];
    }
    synced
        .chunks(PLAN_SNAPSHOT_CHUNK)
        .map(|chunk| PlanSnapshot {
            plan_group_id,
            player,
            plans: Some(chunk.iter().map(PlanWire::from_client).collect()),
        })
        .collect()
}

/// Drives the 0.5 s plan-snapshot cadence and `lastPreviewPlanGroup` counter.
#[derive(Debug, Clone)]
pub struct PlanSnapshotTimer {
    /// Interval in ticks.
    pub interval_ticks: u64,
    /// Next tick a snapshot is due.
    pub next_tick: u64,
    /// `player.lastPreviewPlanGroup`.
    pub group_id: u32,
}

impl PlanSnapshotTimer {
    /// Creates a timer that first fires `interval_ticks` after `start_tick`.
    pub fn new(start_tick: u64, interval_ticks: u64) -> Self {
        Self {
            interval_ticks,
            next_tick: start_tick.saturating_add(interval_ticks),
            group_id: 0,
        }
    }

    /// Whether a snapshot is due at `tick`.
    pub fn ready(&self, tick: u64) -> bool {
        tick >= self.next_tick
    }

    /// Advances after a send: increments the group id and re-arms.
    pub fn advance(&mut self, tick: u64) -> u32 {
        self.group_id = self.group_id.wrapping_add(1);
        self.next_tick = tick.saturating_add(self.interval_ticks);
        self.group_id
    }
}

/// The `Call.clientSnapshot` player-input payload (`NetClient.sync`, §3.3.2).
///
/// Plan 21 transports it; 15 produces it. Mouse/aim/shooting are player state
/// (05/11/21 own it in-sim), never part of the input checksum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerInputSync {
    /// `lastSent++` monotonic snapshot id.
    pub snapshot_id: u32,
    /// Controlled unit id, `None` when dead/none.
    pub unit: Option<i32>,
    /// Player dead flag.
    pub dead: bool,
    /// Unit/player x.
    pub x: f32,
    /// Unit/player y.
    pub y: f32,
    /// Unit aim x.
    pub aim_x: f32,
    /// Unit aim y.
    pub aim_y: f32,
    /// Unit facing (degrees).
    pub rotation: f32,
    /// Mech base rotation (degrees).
    pub base_rotation: f32,
    /// Unit velocity x.
    pub vel_x: f32,
    /// Unit velocity y.
    pub vel_y: f32,
    /// Mined tile packed position, if any.
    pub mine_tile: Option<i32>,
    /// `player.boosting`.
    pub boosting: bool,
    /// `player.shooting`.
    pub shooting: bool,
    /// `ui.chatfrag.shown()`.
    pub chatting: bool,
    /// `control.input.isBuilding`.
    pub is_building: bool,
    /// `player.selectedBlock` content id.
    pub selected_block: Option<u16>,
    /// `player.selectedRotation`.
    pub selected_rotation: u8,
    /// `unit.plans` when the player is a living builder.
    pub plans: Option<Vec<PlanWire>>,
    /// Camera center x.
    pub camera_x: f32,
    /// Camera center y.
    pub camera_y: f32,
    /// Camera width.
    pub camera_w: f32,
    /// Camera height.
    pub camera_h: f32,
}

impl Default for PlayerInputSync {
    fn default() -> Self {
        Self {
            snapshot_id: 0,
            unit: None,
            dead: false,
            x: 0.0,
            y: 0.0,
            aim_x: 0.0,
            aim_y: 0.0,
            rotation: 0.0,
            base_rotation: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            mine_tile: None,
            boosting: false,
            shooting: false,
            chatting: false,
            is_building: true,
            selected_block: None,
            selected_rotation: 1,
            plans: None,
            camera_x: 0.0,
            camera_y: 0.0,
            camera_w: 0.0,
            camera_h: 0.0,
        }
    }
}

impl PlayerInputSync {
    /// Encodes the builder's queued plans into the payload (`None` when not a
    /// builder, matching `player.isBuilder() && unit != null ? unit.plans : null`).
    pub fn with_plans(mut self, plans: Option<&[ClientPlan]>) -> Self {
        self.plans = plans.map(|plans| plans.iter().map(PlanWire::from_client).collect());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(x: i32, y: i32, breaking: bool) -> ClientPlan {
        let mut value = ClientPlan::place(x, y, 0, BlockId::STONE_WALL);
        value.breaking = breaking;
        value
    }

    #[test]
    fn synced_plans_filter_breaking_and_mobile_selects() {
        let queue = vec![plan(1, 1, false), plan(2, 2, true), plan(3, 3, false)];
        let selects = vec![plan(4, 4, false), plan(5, 5, true)];
        let mut out = Vec::new();
        get_synced_plans(&queue, &selects, false, &mut out);
        assert_eq!(out.len(), 2);
        get_synced_plans(&queue, &selects, true, &mut out);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|plan| !plan.breaking));
    }

    #[test]
    fn snapshot_empty_single_and_chunked() {
        // Empty -> one None payload.
        let empty = build_plan_snapshots(&[], &[], false, 7, 4);
        assert_eq!(empty.len(), 1);
        assert!(empty[0].plans.is_none());
        assert_eq!(empty[0].plan_group_id, 4);

        // < 75 -> one payload.
        let small: Vec<ClientPlan> = (0..50).map(|i| plan(i, 0, false)).collect();
        let one = build_plan_snapshots(&small, &[], false, 7, 0);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].plans.as_ref().map(Vec::len), Some(50));

        // 200 -> chunks of 75 + remainder.
        let big: Vec<ClientPlan> = (0..200).map(|i| plan(i, 0, false)).collect();
        let chunks = build_plan_snapshots(&big, &[], false, 7, 9);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].plans.as_ref().map(Vec::len), Some(75));
        assert_eq!(chunks[1].plans.as_ref().map(Vec::len), Some(75));
        assert_eq!(chunks[2].plans.as_ref().map(Vec::len), Some(50));
        assert!(chunks.iter().all(|snapshot| snapshot.plan_group_id == 9));
    }

    #[test]
    fn snapshot_truncates_to_1000() {
        let big: Vec<ClientPlan> = (0..1200).map(|i| plan(i, 0, false)).collect();
        let chunks = build_plan_snapshots(&big, &[], false, 1, 0);
        let total: usize = chunks
            .iter()
            .map(|snapshot| snapshot.plans.as_ref().map(Vec::len).unwrap_or(0))
            .sum();
        assert_eq!(total, MAX_PLAYER_PREVIEW_PLANS);
    }

    #[test]
    fn snapshots_include_mobile_select_plans() {
        let queue = vec![plan(1, 1, false)];
        let selects = vec![plan(2, 2, false)];
        let desktop = build_plan_snapshots(&queue, &selects, false, 1, 0);
        assert_eq!(desktop[0].plans.as_ref().map(Vec::len), Some(1));
        let mobile = build_plan_snapshots(&queue, &selects, true, 1, 0);
        assert_eq!(mobile[0].plans.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn snapshot_timer_cadence() {
        let mut timer = PlanSnapshotTimer::new(0, PLAN_SNAPSHOT_INTERVAL_TICKS);
        assert!(!timer.ready(29));
        assert!(timer.ready(30));
        let id = timer.advance(30);
        assert_eq!(id, 1);
        assert!(!timer.ready(59));
        assert!(timer.ready(60));
    }

    #[test]
    fn wire_round_trips() {
        let mut source = ClientPlan::place(3, -4, 2, BlockId::STONE_WALL);
        source.config = ConfigValue::Number(1.5);
        let wire = PlanWire::from_client(&source);
        assert_eq!(wire.rotation, 2);
        assert_eq!(wire.config, WireConfig::Number(1.5));
        assert_eq!(wire.to_client(), source);

        let content = ClientPlan {
            config: ConfigValue::Content(ContentType::Block, 12),
            ..ClientPlan::place(0, 0, 0, BlockId::STONE_WALL)
        };
        assert_eq!(
            PlanWire::from_client(&content).config,
            WireConfig::Content {
                content_type: ContentType::Block as u8,
                id: 12,
            }
        );
    }

    #[test]
    fn player_input_sync_builds_plans() {
        let plans = vec![plan(1, 1, false)];
        let sync = PlayerInputSync {
            snapshot_id: 3,
            shooting: true,
            ..PlayerInputSync::default()
        }
        .with_plans(Some(&plans));
        assert_eq!(sync.snapshot_id, 3);
        assert!(sync.shooting);
        assert_eq!(sync.plans.as_ref().map(Vec::len), Some(1));
        assert!(sync.with_plans(None).plans.is_none());
    }
}
