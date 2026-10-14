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

/// `Pathfinder.impassable` (Pathfinder.java:34).
pub const IMPASSABLE: i32 = -1;

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

    /// Traversal cost of `tile` for the pathing `team` (`Pathfinder.costTypes`;
    /// Pathfinder.java:53-91). `IMPASSABLE` (-1) marks a blocked tile.
    pub fn cost(self, team: u8, tile: PathTile) -> i32 {
        match self {
            // (allDeep || ((team == t && !teamPassable) || t == 0) && solid)
            Cost::Ground => {
                let blocked = tile.all_deep()
                    || ((tile.team() == team && !tile.team_passable()) || tile.team() == 0)
                        && tile.solid();
                if blocked {
                    IMPASSABLE
                } else {
                    1 + i32::from(tile.health()) * 5
                        + if tile.near_solid() { 2 } else { 0 }
                        + if tile.near_liquid() { 6 } else { 0 }
                        + if tile.deep() { 6000 } else { 0 }
                        + if tile.damages() { 30 } else { 0 }
                }
            }
            // legSolid ? impassable : 1 + deep 6000 + solid 5
            Cost::Legs => {
                if tile.leg_solid() {
                    IMPASSABLE
                } else {
                    1 + if tile.deep() { 6000 } else { 0 } + if tile.solid() { 5 } else { 0 }
                }
            }
            // (!liquid || (solid && (team == t || t == 0)) ? 7000 : 1 + health*5)
            //   + (nearGround || nearSolid ? 14 : 0) + (deep ? 0 : 1) + damages 35
            Cost::Naval => {
                let base = if !tile.liquid()
                    || (tile.solid() && (tile.team() == team || tile.team() == 0))
                {
                    7000
                } else {
                    1 + i32::from(tile.health()) * 5
                };
                base + if tile.near_ground() || tile.near_solid() {
                    14
                } else {
                    0
                } + if tile.deep() { 0 } else { 1 }
                    + if tile.damages() { 35 } else { 0 }
            }
            // (deep || (t == 0 && solid)) ? impassable
            //   : 1 + health*3 + nearSolid 2 + nearLiquid 2
            Cost::Neoplasm => {
                if tile.deep() || (tile.team() == 0 && tile.solid()) {
                    IMPASSABLE
                } else {
                    1 + i32::from(tile.health()) * 3
                        + if tile.near_solid() { 2 } else { 0 }
                        + if tile.near_liquid() { 2 } else { 0 }
                }
            }
            // flat cost
            Cost::None => 1,
            // (((team == t && !teamPassable) || t == 0) && solid) ? impassable
            //   : 1 + health*5 + nearSolid 2
            Cost::Hover => {
                let blocked = ((tile.team() == team && !tile.team_passable()) || tile.team() == 0)
                    && tile.solid();
                if blocked {
                    IMPASSABLE
                } else {
                    1 + i32::from(tile.health()) * 5 + if tile.near_solid() { 2 } else { 0 }
                }
            }
        }
    }

    /// `Flowfield.passable` (Pathfinder.java:698-703): the cost is not
    /// `impassable` and naval's land cost (>= 6000) is not passable either.
    pub fn passable(self, team: u8, tile: PathTile) -> bool {
        let amount = self.cost(team, tile);
        amount != IMPASSABLE && !(self == Cost::Naval && amount >= 6000)
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
        assert!(Cost::Ground.passable(0, open));
        assert!(!Cost::Ground.passable(0, wall));
        assert!(Cost::None.passable(0, wall));
    }

    /// T15: `Pathfinder.costTypes` ground/legs/naval/neoplasm/none/hover
    /// (Pathfinder.java:53-91).
    #[test]
    fn ground_cost_matches_java() {
        // Plain floor: 1.
        assert_eq!(Cost::Ground.cost(1, PathTile::default()), 1);
        // near-solid: 1 + 2.
        let near = PathTile::from_bits(
            0, 0, false, false, false, false, false, true, false, false, false, false, false, false,
        );
        assert_eq!(Cost::Ground.cost(1, near), 3);
        // Deep: 1 + 6000 (and +6 when the neighbour is a deep liquid).
        let deep_tile = PathTile::from_bits(
            0, 0, false, false, false, false, true, false, false, true, false, false, true, false,
        );
        assert_eq!(Cost::Ground.cost(1, deep_tile), 6001);
        let near_liquid_deep = PathTile::from_bits(
            0, 0, false, false, false, true, true, false, false, true, false, false, true, false,
        );
        assert_eq!(Cost::Ground.cost(1, near_liquid_deep), 6007);
        // All-deep is impassable.
        let all_deep = PathTile::from_bits(
            0, 0, false, false, false, false, true, false, false, true, false, true, true, false,
        );
        assert_eq!(Cost::Ground.cost(1, all_deep), IMPASSABLE);
        // Own-team solid: impassable.
        let own_wall = PathTile::from_bits(
            10, 1, true, false, false, false, false, false, false, false, false, false, false,
            false,
        );
        assert_eq!(Cost::Ground.cost(1, own_wall), IMPASSABLE);
        // Team-0 solid: impassable.
        assert_eq!(
            Cost::Ground.cost(
                1,
                PathTile::from_bits(
                    10, 0, true, false, false, false, false, false, false, false, false, false,
                    false, false
                )
            ),
            IMPASSABLE
        );
        // Enemy-team solid with health 10: 1 + 50 = 51 (units path through and shoot).
        let enemy_wall = PathTile::from_bits(
            10, 2, true, false, false, false, false, false, false, false, false, false, false,
            false,
        );
        assert_eq!(Cost::Ground.cost(1, enemy_wall), 51);
        // Damaging floor: +30.
        let damaging = PathTile::from_bits(
            0, 0, false, false, false, false, true, false, false, false, true, false, false, false,
        );
        assert_eq!(Cost::Ground.cost(1, damaging), 31);
        // Legs: solid costs 5, leg-solid impassable.
        let solid = PathTile::from_bits(
            0, 2, true, false, false, false, false, false, false, false, false, false, false, false,
        );
        assert_eq!(Cost::Legs.cost(1, solid), 6);
        let leg_solid = PathTile::from_bits(
            0, 2, true, false, true, false, false, false, false, false, false, false, false, false,
        );
        assert_eq!(Cost::Legs.cost(1, leg_solid), IMPASSABLE);
        // None: flat 1.
        assert_eq!(Cost::None.cost(1, all_deep), 1);
        // Hover: like ground but no all-deep term (a solid own-team tile still blocks).
        assert_eq!(Cost::Hover.cost(1, all_deep), 1);
        assert_eq!(Cost::Hover.cost(1, own_wall), IMPASSABLE);
        assert_eq!(Cost::Hover.cost(1, enemy_wall), 51);
    }

    /// T15: naval land costs >= 6000 are not passable (`Flowfield.passable`).
    #[test]
    fn naval_cost_land_is_not_passable() {
        let land = PathTile::default();
        assert_eq!(Cost::Naval.cost(1, land), 7001);
        assert!(!Cost::Naval.passable(1, land));
        // Deep liquid floor: 1 + 0 (deep) + 1 (not deep -> 0) = ... Java's
        // `deep ? 0 : 1` and nearGround/nearSolid terms decide.
        let water = PathTile::from_bits(
            0, 0, false, true, false, false, false, false, false, true, false, true, true, false,
        );
        assert_eq!(Cost::Naval.cost(1, water), 1);
        assert!(Cost::Naval.passable(1, water));
    }
}
