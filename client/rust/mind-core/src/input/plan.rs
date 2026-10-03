// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Client plan mirror, spatial tree and preview state (plan 15 §3.7).
//!
//! `ClientPlan` is the UI/input copy of plan 07's `BuildPlan` (with an
//! `anim_scale` for previews). The authoritative build queue belongs to plan 11;
//! [`crate::input::queue::BuildQueue`] is the minimal seam this plan calls
//! through. See `R6`: `PlanTree` preserves insertion-order iteration (the part
//! of Arc's `QuadTree` that `find` consumers observe); the spatial index itself
//! is a flat insertion-ordered vec until a generic port exists.

use std::collections::HashMap;

use crate::content::BlockId;
use crate::world::TilePos;
use crate::world::config::ConfigValue;
use crate::world::plan::BuildPlan;

/// A client-side placement/deconstruction plan (`Build.plan()` copy).
#[derive(Debug, Clone, PartialEq)]
pub struct ClientPlan {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Rotation `0..=3`.
    pub rotation: u8,
    /// Block to place.
    pub block: BlockId,
    /// Config payload.
    pub config: ConfigValue,
    /// Whether this is a deconstruction order.
    pub breaking: bool,
    /// Preview animation scale (`1.0` when settled).
    pub anim_scale: f32,
}

/// Alias used by the bridge DP (`Placement.BuildPlan`).
pub type PlanCopy = ClientPlan;

impl ClientPlan {
    /// A fresh placement plan at scale 1.
    pub fn place(x: i32, y: i32, rotation: u8, block: BlockId) -> Self {
        Self {
            x,
            y,
            rotation,
            block,
            config: ConfigValue::None,
            breaking: false,
            anim_scale: 1.0,
        }
    }

    /// A fresh deconstruction plan.
    pub fn break_plan(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            rotation: 0,
            block: BlockId::AIR,
            config: ConfigValue::None,
            breaking: true,
            anim_scale: 1.0,
        }
    }

    /// `BuildPlan.placeable` (ignores team).
    pub fn placeable(&self) -> bool {
        !self.breaking && self.block != BlockId::AIR
    }

    /// `BuildPlan.tile`.
    pub fn tile(&self) -> TilePos {
        TilePos::new(self.x as i16, self.y as i16)
    }

    /// `BuildPlan.samePos`.
    pub fn same_pos(&self, other: &ClientPlan) -> bool {
        self.x == other.x && self.y == other.y
    }

    /// Converts from plan 07's `BuildPlan`.
    pub fn from_build_plan(plan: &BuildPlan) -> Self {
        Self {
            x: plan.x,
            y: plan.y,
            rotation: plan.rotation,
            block: plan.block,
            config: plan.config.clone(),
            breaking: plan.breaking,
            anim_scale: 1.0,
        }
    }

    /// Converts to plan 07's `BuildPlan`.
    pub fn to_build_plan(&self) -> BuildPlan {
        BuildPlan {
            x: self.x,
            y: self.y,
            rotation: self.rotation,
            block: self.block,
            config: self.config.clone(),
            breaking: self.breaking,
        }
    }
}

/// Insertion-ordered plan set (`Arc QuadTree`, iteration-order contract only).
#[derive(Debug, Clone, Default)]
pub struct PlanTree {
    entries: Vec<ClientPlan>,
    index: HashMap<i32, usize>,
}

impl PlanTree {
    /// Empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Packs a tile coordinate (matches `Point2.pack`).
    pub fn pack(x: i32, y: i32) -> i32 {
        (x & 0xffff) | ((y & 0xffff) << 16)
    }

    /// `QuadTree.insert`: same-position plans replace in place, else append.
    pub fn insert(&mut self, plan: ClientPlan) {
        let key = Self::pack(plan.x, plan.y);
        if let Some(slot) = self.index.get(&key).copied() {
            self.entries[slot] = plan;
            return;
        }
        self.index.insert(key, self.entries.len());
        self.entries.push(plan);
    }

    /// Removes the plan at `(x, y)`; returns whether one existed.
    pub fn remove(&mut self, x: i32, y: i32) -> bool {
        let key = Self::pack(x, y);
        let Some(slot) = self.index.remove(&key) else {
            return false;
        };
        self.entries.remove(slot);
        for value in self.index.values_mut() {
            if *value > slot {
                *value -= 1;
            }
        }
        true
    }

    /// Iterates in insertion order (`QuadTree` iteration contract).
    pub fn iter(&self) -> impl Iterator<Item = &ClientPlan> + '_ {
        self.entries.iter()
    }

    /// Number of plans.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears the tree.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// The plan at `(x, y)`, if any.
    pub fn get(&self, x: i32, y: i32) -> Option<&ClientPlan> {
        self.index
            .get(&Self::pack(x, y))
            .map(|slot| &self.entries[*slot])
    }

    /// Plans whose tile falls in `[x1, y1]..=[x2, y2]`.
    pub fn find_rect(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> Vec<ClientPlan> {
        let (min_x, max_x) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        let (min_y, max_y) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
        self.entries
            .iter()
            .filter(|plan| plan.x >= min_x && plan.x <= max_x && plan.y >= min_y && plan.y <= max_y)
            .cloned()
            .collect()
    }
}

/// Mirror of `player.unit().plans` (`InputHandler.lastPlans`).
#[derive(Debug, Clone, Default)]
pub struct PlanMirror {
    plans: Vec<ClientPlan>,
}

impl PlanMirror {
    /// Empty mirror.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the mirror contents (`updateState` sync).
    pub fn sync(&mut self, plans: &[ClientPlan]) {
        self.plans.clear();
        self.plans.extend_from_slice(plans);
    }

    /// Mirrored plans.
    pub fn plans(&self) -> &[ClientPlan] {
        &self.plans
    }

    /// Number mirrored.
    pub fn len(&self) -> usize {
        self.plans.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }

    /// Clears the mirror.
    pub fn clear(&mut self) {
        self.plans.clear();
    }
}

/// Preview state handed to plan 16 (no drawing in plan 15, deviation I6).
#[derive(Debug, Clone)]
pub struct PreviewState {
    /// Selected block (`input.block`).
    pub block: Option<BlockId>,
    /// Placement rotation.
    pub rotation: u8,
    /// Active mode.
    pub place_mode: crate::input::PlaceMode,
    /// Cursor tile.
    pub cursor_tile: (i32, i32),
    /// Raw cursor tile (unrounded).
    pub cursor_raw: (f32, f32),
    /// Line/area plans.
    pub line_plans: Vec<ClientPlan>,
    /// Schematic/select plans.
    pub select_plans: Vec<ClientPlan>,
    /// Whether a placement line is valid.
    pub valid: bool,
    /// `splan` move target (moving an existing plan).
    pub splan: bool,
    /// Command drag rectangle `(x, y, w, h)` in world pixels.
    pub command_rect: Option<(f32, f32, f32, f32)>,
    /// Selected unit ids.
    pub selected_units: Vec<i32>,
    /// Commanded building positions.
    pub command_buildings: Vec<(i16, i16)>,
    /// Resolved system cursor (plan 15 §3.8).
    pub cursor: crate::input::CursorKind,
    /// RTS command target marker in world pixels.
    pub target: Option<(f32, f32)>,
    /// Per-plan `valid_place` cache (reused across frames).
    pub cached_valid: Vec<bool>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            block: None,
            rotation: 1,
            place_mode: crate::input::PlaceMode::None,
            cursor_tile: (0, 0),
            cursor_raw: (0.0, 0.0),
            line_plans: Vec::new(),
            select_plans: Vec::new(),
            valid: false,
            splan: false,
            command_rect: None,
            selected_units: Vec::new(),
            command_buildings: Vec::new(),
            cursor: crate::input::CursorKind::Arrow,
            target: None,
            cached_valid: Vec::new(),
        }
    }
}

impl PreviewState {
    /// Clears all transient preview data (`updateState` menu branch).
    pub fn clear(&mut self) {
        self.line_plans.clear();
        self.select_plans.clear();
        self.valid = false;
        self.splan = false;
        self.command_rect = None;
        self.selected_units.clear();
        self.command_buildings.clear();
        self.cursor = crate::input::CursorKind::Arrow;
        self.target = None;
        self.cached_valid.clear();
    }

    /// The §6.3 `line_plans` JSON shape (consumed by 16/14).
    pub fn line_plans_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.line_plans
                .iter()
                .map(|plan| {
                    serde_json::json!({
                        "x": plan.x,
                        "y": plan.y,
                        "rot": plan.rotation,
                        "block": plan.block.raw(),
                        "valid": !plan.breaking,
                    })
                })
                .collect(),
        )
    }

    /// Full handoff JSON for plan 16 (rendering input only; no drawing here).
    pub fn handoff_json(&self) -> serde_json::Value {
        let plans_json = |plans: &[ClientPlan]| {
            serde_json::Value::Array(
                plans
                    .iter()
                    .map(|plan| {
                        serde_json::json!({
                            "x": plan.x,
                            "y": plan.y,
                            "rot": plan.rotation,
                            "block": plan.block.raw(),
                            "breaking": plan.breaking,
                            "anim_scale": plan.anim_scale,
                        })
                    })
                    .collect(),
            )
        };
        serde_json::json!({
            "block": self.block.map(|block| block.raw()),
            "rotation": self.rotation,
            "mode": self.place_mode.name(),
            "cursor": {
                "tile": [self.cursor_tile.0, self.cursor_tile.1],
                "raw": [self.cursor_raw.0, self.cursor_raw.1],
                "kind": self.cursor.name(),
            },
            "line_plans": plans_json(&self.line_plans),
            "select_plans": plans_json(&self.select_plans),
            "valid": self.valid,
            "splan": self.splan,
            "command_rect": self.command_rect.map(|(x, y, w, h)| {
                serde_json::json!({"x": x, "y": y, "w": w, "h": h})
            }),
            "selected_units": self.selected_units,
            "command_buildings": self
                .command_buildings
                .iter()
                .map(|(x, y)| serde_json::json!([x, y]))
                .collect::<Vec<_>>(),
            "target": self.target.map(|(x, y)| serde_json::json!([x, y])),
            "cached_valid": self.cached_valid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_plan_replaced() {
        // ApplicationTests.blockOverlapRemoved (input half).
        let mut tree = PlanTree::new();
        tree.insert(ClientPlan::place(3, 4, 0, BlockId::STONE_WALL));
        tree.insert(ClientPlan::place(3, 4, 2, BlockId::STONE_WALL));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.get(3, 4).map(|plan| plan.rotation), Some(2));
        assert!(tree.remove(3, 4));
        assert!(tree.is_empty());
        assert!(!tree.remove(3, 4));
    }

    #[test]
    fn tree_iteration_is_insertion_order() {
        let mut tree = PlanTree::new();
        for (x, y) in [(5, 5), (1, 1), (3, 2), (9, 9)] {
            tree.insert(ClientPlan::place(x, y, 0, BlockId::STONE_WALL));
        }
        let order: Vec<(i32, i32)> = tree.iter().map(|plan| (plan.x, plan.y)).collect();
        assert_eq!(order, vec![(5, 5), (1, 1), (3, 2), (9, 9)]);
        tree.remove(1, 1);
        let order: Vec<(i32, i32)> = tree.iter().map(|plan| (plan.x, plan.y)).collect();
        assert_eq!(order, vec![(5, 5), (3, 2), (9, 9)]);
        assert_eq!(tree.find_rect(2, 1, 6, 6).len(), 2);
    }

    #[test]
    fn build_plan_round_trip() {
        let build = BuildPlan::place(7, 8, 1, BlockId::STONE_WALL);
        let client = ClientPlan::from_build_plan(&build);
        assert_eq!(client.to_build_plan(), build);
        assert!(client.placeable());
        assert!(!ClientPlan::break_plan(0, 0).placeable());
    }

    #[test]
    fn mirror_sync_and_preview_clear() {
        let mut mirror = PlanMirror::new();
        mirror.sync(&[ClientPlan::place(1, 2, 0, BlockId::STONE_WALL)]);
        assert_eq!(mirror.len(), 1);
        mirror.clear();
        assert!(mirror.is_empty());

        let mut preview = PreviewState::default();
        preview
            .line_plans
            .push(ClientPlan::place(1, 1, 0, BlockId::STONE_WALL));
        assert_eq!(preview.rotation, 1);
        preview.clear();
        assert!(preview.line_plans.is_empty());
    }
}
