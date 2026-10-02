// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RtsAI` squad assignment (plan 11 §3.9).
//!
//! Ported from `core/src/mindustry/ai/RtsAI.java`. This module owns the
//! deterministic squad grouping and target-selection halves (grouping idle
//! commanded units by `flag`, nearest-unassigned target pick). The per-tick
//! `TeamData` wiring, defense triggers and `estimateStats` weighting are plan 12
//! (`RtsAI.update` is called per active `TeamData`).

use bevy_ecs::entity::Entity;

use crate::world::TilePos;

/// One RTS squad (`RtsAI.Squad`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Squad {
    /// Member units, in stable order.
    pub units: Vec<Entity>,
    /// Mobilization flag (`unit.flag`).
    pub flag: i32,
    /// Current target tile.
    pub target: Option<TilePos>,
}

/// `RtsAI` state for one team.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RtsAi {
    /// Assigned squads.
    pub squads: Vec<Squad>,
    /// Timer until the next squad re-assignment, in ticks (`timer`).
    pub timer: f32,
    /// Monotonic assignment counter (deterministic target shuffle seed).
    pub assignment_id: u64,
}

impl RtsAi {
    /// Creates an empty RTS AI.
    pub fn new() -> Self {
        Self::default()
    }

    /// `assignSquads`: groups idle units by `flag` (`(unit, flag)` pairs).
    ///
    /// Upstream BFS-clusters units within `squad_radius`; the port groups by
    /// flag in stable list order. Deterministic: input order is preserved and
    /// squad order is first-seen flag order.
    pub fn assign_squads(&mut self, units: &[(Entity, i32)], _squad_radius: f32) {
        self.squads.clear();
        for &(entity, flag) in units {
            if let Some(squad) = self.squads.iter_mut().find(|squad| squad.flag == flag) {
                squad.units.push(entity);
            } else {
                self.squads.push(Squad {
                    units: vec![entity],
                    flag,
                    target: None,
                });
            }
        }
        self.assignment_id = self.assignment_id.wrapping_add(1);
    }

    /// `findTarget`: nearest candidate tile not already assigned, given `(x, y)`.
    pub fn find_target(
        squads: &[Squad],
        candidates: &[TilePos],
        x: f32,
        y: f32,
    ) -> Option<TilePos> {
        let mut best: Option<(f32, TilePos)> = None;
        for &candidate in candidates {
            if squads.iter().any(|squad| squad.target == Some(candidate)) {
                continue;
            }
            let ts = crate::config::TILESIZE as f32;
            let cx = (candidate.x() as f32 + 0.5) * ts;
            let cy = (candidate.y() as f32 + 0.5) * ts;
            let dx = cx - x;
            let dy = cy - y;
            let dist2 = dx * dx + dy * dy;
            match best {
                Some((best_dist, best_tile))
                    if best_dist < dist2 || (best_dist == dist2 && best_tile <= candidate) => {}
                _ => best = Some((dist2, candidate)),
            }
        }
        best.map(|(_, tile)| tile)
    }

    /// Updates the squad re-assignment timer; returns `true` when it fires.
    pub fn update_timer(&mut self, interval: f32) -> bool {
        self.timer -= 1.0;
        if self.timer <= 0.0 {
            self.timer = interval;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rts_assigns_squads_by_flag() {
        let mut rts = RtsAi::new();
        let a = Entity::PLACEHOLDER;
        let units = [(a, 0), (a, 1), (a, 0), (a, 1)];
        rts.assign_squads(&units, 60.0);
        assert_eq!(rts.squads.len(), 2, "one squad per distinct flag");
        assert_eq!(rts.squads[0].units.len(), 2);
        assert_eq!(rts.squads[1].units.len(), 2);
        assert_eq!(rts.squads[0].flag, 0);
        // Deterministic across runs.
        let mut again = RtsAi::new();
        again.assign_squads(&units, 60.0);
        assert_eq!(rts.squads, again.squads);
    }

    #[test]
    fn target_skips_assigned() {
        let mut rts = RtsAi::new();
        let a = Entity::PLACEHOLDER;
        rts.assign_squads(&[(a, 0), (a, 1)], 60.0);
        rts.squads[0].target = Some(TilePos::new(5, 5));
        let candidates = [TilePos::new(5, 5), TilePos::new(9, 9)];
        let target = RtsAi::find_target(&rts.squads, &candidates, 0.0, 0.0);
        assert_eq!(target, Some(TilePos::new(9, 9)));
    }
}
