// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pathfinding cost types (plan 11 §3.7/§6.4).
//!
//! Ported from `core/src/mindustry/ai/Pathfinder.java` (`costGround`,
//! `costLegs`, `costNaval`, `costNeoplasm`, `costNone`, `costHover`) and
//! `ai/ControlPathfinder.java` (`costId` ids). Indices are ABI: append only.

use super::path_tile::PathTile;

/// Ground AI cost ids (`Pathfinder` statics).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cost {
    /// Ground mechs/tanks (`costGround`, id 0).
    Ground = 0,
    /// Legged units (`costLegs`, id 1).
    Legs = 1,
    /// Naval units (`costNaval`, id 2).
    Naval = 2,
    /// Neoplasm crawlers (`costNeoplasm`, id 3).
    Neoplasm = 3,
    /// Always passable (`costNone`, id 4).
    None = 4,
    /// Hover units (`costHover`, id 5).
    Hover = 5,
}

/// Number of pathfinder costs (`maxCosts`; 8 reserved).
pub const MAX_COSTS: usize = 8;

impl Cost {
    /// ABI index.
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Parses a cost from its id.
    pub fn from_id(id: u8) -> Option<Cost> {
        match id {
            0 => Some(Cost::Ground),
            1 => Some(Cost::Legs),
            2 => Some(Cost::Naval),
            3 => Some(Cost::Neoplasm),
            4 => Some(Cost::None),
            5 => Some(Cost::Hover),
            _ => None,
        }
    }

    /// Whether `tile` is passable for this cost (M0 block-layer subset).
    pub const fn passable(self, tile: PathTile) -> bool {
        match self {
            Cost::None => true,
            Cost::Ground | Cost::Hover => !tile.solid(),
            Cost::Legs => !tile.solid(),
            Cost::Naval => !tile.solid() && tile.deep(),
            Cost::Neoplasm => !tile.solid(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::path_tile::PathTile;
    use super::*;

    #[test]
    fn cost_ids_are_frozen() {
        assert_eq!(Cost::Ground.id(), 0);
        assert_eq!(Cost::Legs.id(), 1);
        assert_eq!(Cost::Naval.id(), 2);
        assert_eq!(Cost::Neoplasm.id(), 3);
        assert_eq!(Cost::None.id(), 4);
        assert_eq!(Cost::Hover.id(), 5);
        assert_eq!(Cost::from_id(4), Some(Cost::None));
        assert_eq!(Cost::from_id(9), None);
    }

    #[test]
    fn ground_rejects_solid() {
        let open = PathTile::default();
        let wall = PathTile::from_parts(0, 0, true, false, false);
        assert!(Cost::Ground.passable(open));
        assert!(!Cost::Ground.passable(wall));
        assert!(Cost::None.passable(wall));
    }
}
