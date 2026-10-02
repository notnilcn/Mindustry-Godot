// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Player` controller bridge (plan 11 §4.2). Plan 15 owns possession and input;
//! this module only holds the player id so the unit/command relay and save codec
//! can reference it. In headless mode the bridge is inert.

use bevy_ecs::entity::Entity;

/// `Player` controller state (`player.id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerBridge {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Owning player id.
    pub player_id: i32,
}

impl PlayerBridge {
    /// Creates a bridge for `player_id`.
    pub fn new(unit: Entity, player_id: i32) -> Self {
        Self {
            unit: Some(unit),
            player_id,
        }
    }
}

/// `Player.updateUnit`: plan 15 writes the unit's command target from input; the
/// headless bridge does nothing.
pub fn update_player(_unit: Entity) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_bridge_carries_id() {
        let bridge = PlayerBridge {
            unit: None,
            player_id: 3,
        };
        assert_eq!(bridge.player_id, 3);
    }
}
