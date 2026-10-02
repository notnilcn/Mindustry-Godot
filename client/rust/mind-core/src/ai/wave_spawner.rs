// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `WaveSpawner` runtime (plan 11 §3.10/§4.2).
//!
//! Ported from `core/src/mindustry/ai/WaveSpawner.java`. Owns the spawn-tile
//! list (tiles carrying the `spawn` overlay), the 121-tick spawning window, the
//! spawn-effect status durations and the per-group unit emission. The
//! `Rules.spawns` policy, attack-mode core search and difficulty multiplier are
//! plan 12 (`Rules`/`CampaignRules`); callers pass the resolved groups. Fx/shock-
//! wave call sites are plan 17 seams.

use bevy_ecs::entity::Entity;

use crate::game::spawn_group::SpawnGroup;
use crate::world::TilePos;

/// `WaveSpawner.spawnTime` window length (2 seconds at 60 Hz).
pub const SPAWN_WINDOW_TICKS: f32 = 121.0;
/// `UnitSpawnEvent`/`Fx.spawn` unmoving status duration.
pub const SPAWN_EFFECT_UNMOVING_TICKS: f32 = 30.0;
/// Spawn invulnerability duration.
pub const SPAWN_EFFECT_INVINCIBLE_TICKS: f32 = 60.0;

/// `WaveSpawner` state (plan 11 §3.10).
#[derive(Debug, Clone, Default)]
pub struct WaveSpawner {
    /// Tiles carrying the spawn overlay (`spawns`).
    pub spawns: Vec<TilePos>,
    /// Whether the spawn window is open (`spawning`).
    pub spawning: bool,
    /// Remaining spawn-window ticks (`spawnTime`).
    pub spawn_time: f32,
    /// Last spawned wave (`wave`).
    pub wave: i32,
    /// Units emitted by the last [`WaveSpawner::spawn_enemies`] call.
    pub last_spawn_count: u32,
}

impl WaveSpawner {
    /// Creates an empty spawner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the spawn-tile list (`TileOverlayChangeEvent` rebuild).
    pub fn set_spawns(&mut self, spawns: Vec<TilePos>) {
        self.spawns = spawns;
    }

    /// `countSpawns()`.
    pub fn count_spawns(&self) -> usize {
        self.spawns.len()
    }

    /// The spawn tiles (`getSpawns`).
    pub fn get_spawns(&self) -> &[TilePos] {
        &self.spawns
    }

    /// `getFirstSpawn()`.
    pub fn get_first_spawn(&self) -> Option<TilePos> {
        self.spawns.first().copied()
    }

    /// Whether `(x, y)` is within `range` of any spawn tile (`playerNear`).
    pub fn player_near(&self, x: f32, y: f32, range: f32) -> bool {
        let ts = crate::config::TILESIZE as f32;
        self.spawns.iter().any(|tile| {
            let cx = (tile.x() as f32 + 0.5) * ts;
            let cy = (tile.y() as f32 + 0.5) * ts;
            let dx = cx - x;
            let dy = cy - y;
            dx * dx + dy * dy <= range * range
        })
    }

    /// Advances the spawn window by one tick.
    pub fn update(&mut self) {
        if self.spawning {
            self.spawn_time -= 1.0;
            if self.spawn_time <= 0.0 {
                self.spawning = false;
                self.spawn_time = 0.0;
            }
        }
    }

    /// `spawnEnemies`: emits `groups` for `wave` through `spawn_unit`.
    ///
    /// The callback is the host's unit spawn (plan 11's `UnitSpawner` + spawn
    /// effect). Returns the number of units emitted. Spawn positions cycle the
    /// spawn tiles in list order; the facing angle points at the tile-list
    /// centroid (upstream faces the map center).
    pub fn spawn_enemies(
        &mut self,
        groups: &[SpawnGroup],
        wave: i32,
        mut spawn_unit: impl FnMut(&SpawnGroup, f32, f32, f32) -> Option<Entity>,
    ) -> u32 {
        if self.spawns.is_empty() {
            return 0;
        }
        self.wave = wave;
        self.spawning = true;
        self.spawn_time = SPAWN_WINDOW_TICKS;
        let ts = crate::config::TILESIZE as f32;
        let (mut cx, mut cy) = (0.0f32, 0.0f32);
        for tile in &self.spawns {
            cx += (tile.x() as f32 + 0.5) * ts;
            cy += (tile.y() as f32 + 0.5) * ts;
        }
        cx /= self.spawns.len() as f32;
        cy /= self.spawns.len() as f32;

        let mut emitted = 0u32;
        let mut cursor = 0usize;
        for group in groups {
            let count = group.get_spawned(wave);
            if count <= 0 {
                continue;
            }
            for _ in 0..count {
                let tile = self.spawns[cursor % self.spawns.len()];
                cursor += 1;
                if !group.can_spawn(tile.pack()) {
                    continue;
                }
                let x = (tile.x() as f32 + 0.5) * ts;
                let y = (tile.y() as f32 + 0.5) * ts;
                let rotation = (cy - y).atan2(cx - x).to_degrees();
                if spawn_unit(group, x, y, rotation).is_some() {
                    emitted += 1;
                }
            }
        }
        self.last_spawn_count = emitted;
        emitted
    }
}

/// `spawnEffect` status durations `(unmoving, invincible)` in ticks.
pub const fn spawn_effect_statuses() -> (f32, f32) {
    (SPAWN_EFFECT_UNMOVING_TICKS, SPAWN_EFFECT_INVINCIBLE_TICKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_enemies_emits_group_counts() {
        let mut spawner = WaveSpawner::new();
        spawner.set_spawns(vec![TilePos::new(2, 2), TilePos::new(10, 10)]);
        assert_eq!(spawner.count_spawns(), 2);
        let mut group = SpawnGroup::new("dagger");
        group.unit_amount = 3;
        group.unit_scaling = 1_000_000.0;
        let mut positions = Vec::new();
        let emitted = spawner.spawn_enemies(&[group], 0, |_group, x, y, _rot| {
            positions.push((x, y));
            Some(bevy_ecs::entity::Entity::PLACEHOLDER)
        });
        assert_eq!(emitted, 3);
        assert_eq!(positions.len(), 3);
        assert!(spawner.spawning);
        assert_eq!(spawner.spawn_time, SPAWN_WINDOW_TICKS);
        // Spawn positions alternate the two tiles deterministically.
        assert_ne!(positions[0], positions[1]);
    }

    #[test]
    fn empty_spawn_list_emits_nothing() {
        let mut spawner = WaveSpawner::new();
        let emitted = spawner.spawn_enemies(&[SpawnGroup::new("dagger")], 0, |_, _, _, _| {
            Some(bevy_ecs::entity::Entity::PLACEHOLDER)
        });
        assert_eq!(emitted, 0);
        assert!(!spawner.spawning);
    }

    #[test]
    fn spawn_window_closes() {
        let mut spawner = WaveSpawner::new();
        spawner.set_spawns(vec![TilePos::new(1, 1)]);
        spawner.spawn_enemies(&[SpawnGroup::new("dagger")], 0, |_, _, _, _| {
            Some(bevy_ecs::entity::Entity::PLACEHOLDER)
        });
        for _ in 0..(SPAWN_WINDOW_TICKS as i32) {
            spawner.update();
        }
        assert!(!spawner.spawning);
    }

    #[test]
    fn effect_status_durations_match_upstream() {
        assert_eq!(spawn_effect_statuses(), (30.0, 60.0));
    }
}
