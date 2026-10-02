// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuildPlan` data (`core/src/mindustry/entities/units/BuildPlan.java`).
//!
//! Shared by the player input path (plan 15) and builder units (plan 11). No
//! `Arc` collections; input helpers (`screenToWorld`) live in plan 15.

use crate::content::BlockId;
use crate::world::TilePos;
use crate::world::config::ConfigValue;

/// A pending placement order (`BuildPlan`).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildPlan {
    /// Target x.
    pub x: i32,
    /// Target y.
    pub y: i32,
    /// Rotation.
    pub rotation: u8,
    /// Block to place.
    pub block: BlockId,
    /// Per-plan config.
    pub config: ConfigValue,
    /// Whether the plan is a deconstruction order.
    pub breaking: bool,
}

impl BuildPlan {
    /// Creates a placement plan.
    pub fn place(x: i32, y: i32, rotation: u8, block: BlockId) -> Self {
        Self {
            x,
            y,
            rotation,
            block,
            config: ConfigValue::None,
            breaking: false,
        }
    }

    /// Creates a deconstruction plan.
    pub fn break_plan(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            rotation: 0,
            block: BlockId::AIR,
            config: ConfigValue::None,
            breaking: true,
        }
    }

    /// `BuildPlan.placeable`.
    pub fn placeable(&self) -> bool {
        !self.breaking && self.block != BlockId::AIR
    }

    /// `BuildPlan.isRotation`.
    pub fn is_rotation(&self) -> bool {
        self.placeable() && self.rotation != 0
    }

    /// `BuildPlan.samePos`.
    pub fn same_pos(&self, other: &BuildPlan) -> bool {
        self.x == other.x && self.y == other.y
    }

    /// `BuildPlan.copy`.
    pub fn copy(&self) -> Self {
        self.clone()
    }

    /// `BuildPlan.tile`.
    pub fn tile(&self) -> TilePos {
        TilePos::new(self.x as i16, self.y as i16)
    }

    /// `BuildPlan.isDone` (always false for a pending plan).
    pub fn is_done(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_helpers() {
        let plan = BuildPlan::place(3, 4, 2, BlockId::STONE_WALL);
        assert!(plan.placeable());
        assert!(plan.is_rotation());
        assert_eq!(plan.tile(), TilePos::new(3, 4));
        assert!(plan.same_pos(&BuildPlan::place(3, 4, 0, BlockId::AIR)));
        assert!(!BuildPlan::break_plan(3, 4).placeable());
    }
}
