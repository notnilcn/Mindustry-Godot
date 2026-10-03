// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! RTS selection helpers (plan 15 §3.9).
//!
//! Pure, platform-neutral queries over a caller-provided unit/building list
//! (plan 11's team quad trees supply the real ones). No ECS reads here.

use smallvec::SmallVec;

use crate::world::TilePos;

/// A unit candidate for selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectableUnit {
    /// Entity id.
    pub id: i32,
    /// Unit type content id (for `selectTypedUnits`).
    pub type_id: i32,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Team id.
    pub team: u8,
    /// `isCommandable`/`allowCommand`.
    pub commandable: bool,
}

/// Selection box (`command_rect`) in world pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SelectRect {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl SelectRect {
    /// Normalized min/max.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let x2 = self.x + self.w;
        let y2 = self.y + self.h;
        (
            self.x.min(x2),
            self.y.min(y2),
            self.x.max(x2),
            self.y.max(y2),
        )
    }

    /// Whether a point is inside.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (min_x, min_y, max_x, max_y) = self.bounds();
        x >= min_x && x <= max_x && y >= min_y && y <= max_y
    }
}

/// `selectUnitsRect`: commandable own-team units inside the rect.
pub fn select_units_rect(
    units: &[SelectableUnit],
    player_team: u8,
    rect: SelectRect,
    out: &mut SmallVec<[i32; 128]>,
) {
    out.clear();
    for unit in units {
        if unit.team == player_team && unit.commandable && rect.contains(unit.x, unit.y) {
            out.push(unit.id);
        }
    }
}

/// `tapCommandUnit`: closest commandable unit within `radius` (hit size grow 6).
pub fn select_unit_tap(
    units: &[SelectableUnit],
    player_team: u8,
    x: f32,
    y: f32,
    radius: f32,
) -> Option<i32> {
    let mut best: Option<(f32, i32)> = None;
    for unit in units {
        if unit.team != player_team || !unit.commandable {
            continue;
        }
        let dist = (unit.x - x).hypot(unit.y - y);
        if dist <= radius && best.is_none_or(|(best_dist, _)| dist < best_dist) {
            best = Some((dist, unit.id));
        }
    }
    best.map(|(_, id)| id)
}

/// `selectTypedUnits`: every own-team commandable unit of `type_id`.
pub fn select_typed_units(
    units: &[SelectableUnit],
    player_team: u8,
    type_id: i32,
    out: &mut SmallVec<[i32; 128]>,
) {
    out.clear();
    for unit in units {
        if unit.team == player_team && unit.commandable && unit.type_id == type_id {
            out.push(unit.id);
        }
    }
}

/// Command target resolution for a tap: nearest enemy unit within `radius`,
/// else `None` (caller falls back to a ground position).
pub fn enemy_unit_at(
    units: &[SelectableUnit],
    player_team: u8,
    x: f32,
    y: f32,
    radius: f32,
) -> Option<i32> {
    let mut best: Option<(f32, i32)> = None;
    for unit in units {
        if unit.team == player_team {
            continue;
        }
        let dist = (unit.x - x).hypot(unit.y - y);
        if dist <= radius && best.is_none_or(|(best_dist, _)| dist < best_dist) {
            best = Some((dist, unit.id));
        }
    }
    best.map(|(_, id)| id)
}

/// A commandable building candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectableBuilding {
    /// Tile position.
    pub pos: TilePos,
    /// Team id.
    pub team: u8,
    /// `allowCommand`.
    pub commandable: bool,
}

/// `selectedCommandBuildings` inside the rect.
pub fn select_buildings_rect(
    buildings: &[SelectableBuilding],
    player_team: u8,
    rect: SelectRect,
    out: &mut SmallVec<[TilePos; 32]>,
) {
    out.clear();
    for building in buildings {
        if building.team == player_team
            && building.commandable
            && rect.contains(building.pos.x() as f32, building.pos.y() as f32)
        {
            out.push(building.pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: i32, x: f32, y: f32, team: u8, commandable: bool) -> SelectableUnit {
        SelectableUnit {
            id,
            type_id: id.rem_euclid(3),
            x,
            y,
            team,
            commandable,
        }
    }

    #[test]
    fn rect_select_filters_team_and_commandable() {
        let units = vec![
            unit(1, 5.0, 5.0, 0, true),
            unit(2, 6.0, 6.0, 0, false),
            unit(3, 5.0, 5.0, 1, true),
            unit(4, 50.0, 50.0, 0, true),
        ];
        let mut out = SmallVec::new();
        select_units_rect(
            &units,
            0,
            SelectRect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            &mut out,
        );
        assert_eq!(out.as_slice(), &[1]);
    }

    #[test]
    fn tap_and_enemy_resolution() {
        let units = vec![
            unit(1, 5.0, 5.0, 0, true),
            unit(2, 5.5, 5.0, 0, true),
            unit(9, 6.0, 5.0, 1, true),
        ];
        assert_eq!(select_unit_tap(&units, 0, 5.0, 5.0, 11.0), Some(1));
        assert_eq!(select_unit_tap(&units, 0, 100.0, 100.0, 11.0), None);
        assert_eq!(enemy_unit_at(&units, 0, 6.0, 5.0, 11.0), Some(9));
    }

    #[test]
    fn typed_select_and_buildings() {
        let units = vec![
            unit(1, 0.0, 0.0, 0, true),
            unit(2, 1.0, 0.0, 0, true),
            unit(3, 2.0, 0.0, 1, true),
        ];
        let mut out = SmallVec::new();
        select_typed_units(&units, 0, 1, &mut out);
        assert_eq!(out.as_slice(), &[1]);
        select_typed_units(&units, 0, 2, &mut out);
        assert_eq!(out.as_slice(), &[2]);

        let buildings = vec![
            SelectableBuilding {
                pos: TilePos::new(3, 3),
                team: 0,
                commandable: true,
            },
            SelectableBuilding {
                pos: TilePos::new(3, 3),
                team: 1,
                commandable: true,
            },
        ];
        let mut selected = SmallVec::new();
        select_buildings_rect(
            &buildings,
            0,
            SelectRect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            &mut selected,
        );
        assert_eq!(selected.as_slice(), &[TilePos::new(3, 3)]);
    }
}
