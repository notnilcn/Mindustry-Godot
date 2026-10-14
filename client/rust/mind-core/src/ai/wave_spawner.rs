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

use crate::content::ContentRegistry;
use crate::determinism::{RngStream, SimRng};
use crate::game::spawn_group::SpawnGroup;
use crate::world::TilePos;

/// `WaveSpawner.spawnTime` window length (2 seconds at 60 Hz).
pub const SPAWN_WINDOW_TICKS: f32 = 121.0;
/// `UnitSpawnEvent`/`Fx.spawn` unmoving status duration.
pub const SPAWN_EFFECT_UNMOVING_TICKS: f32 = 30.0;
/// Spawn invulnerability duration.
pub const SPAWN_EFFECT_INVINCIBLE_TICKS: f32 = 60.0;

/// Per-wave spawn context resolved by `run_wave_system` (`WaveSpawner`'s
/// `eachGroundSpawn`/`eachFlyerSpawn` outputs, WaveSpawner.java:122-200).
#[derive(Debug, Clone, Default)]
pub struct SpawnContext {
    /// `world.width() * tilesize`.
    pub width_px: f32,
    /// `world.height() * tilesize`.
    pub height_px: f32,
    /// Ground spawn positions: `(x, y, packed filter pos, shockwave)`.
    /// Overlay tiles pass `true`, attack-mode core spawns `false`.
    pub ground: Vec<(f32, f32, i32, bool)>,
    /// Flyer spawn positions: `(x, y, packed filter pos)`.
    pub flyer: Vec<(f32, f32, i32)>,
    /// `planet.campaignRules.difficulty.enemySpawnMultiplier` while
    /// `state.isCampaign()` (`WaveSpawner.java:70-78`).
    pub campaign_mult: Option<f32>,
}

/// `WaveSpawner.eachFlyerSpawn` off-map edge position for a spawn tile
/// (WaveSpawner.java:176-190): the map-centre line at
/// `max(width, height) * sqrt2 * tilesize`, clamped to the map (`margin = 0`).
pub fn flyer_edge_position(width: i32, height: i32, tile: TilePos) -> (f32, f32) {
    let ts = crate::config::TILESIZE as f32;
    let w = width as f32;
    let h = height as f32;
    let angle = (tile.y() as f32 - h / 2.0).atan2(tile.x() as f32 - w / 2.0);
    let trns = w.max(h) * std::f32::consts::SQRT_2 * ts;
    (
        (w * ts / 2.0 + angle.cos() * trns).clamp(0.0, w * ts),
        (h * ts / 2.0 + angle.sin() * trns).clamp(0.0, h * ts),
    )
}

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
            // `Tile.worldx()/worldy()` (Tile.java:166-171).
            let cx = tile.x() as f32 * ts;
            let cy = tile.y() as f32 * ts;
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

    /// `spawnEnemies` (WaveSpawner.java:62-120): emits `groups` for `wave`
    /// through `spawn_unit`.
    ///
    /// Faithful per-spawn counting: N units **per spawn position** (not N split
    /// across spawns), campaign difficulty scaling (`campaign_mult`), ground
    /// spread `rnd(tilesize * 2)` and the flyer `range(0)` draw pair.
    /// Shockwaves are applied by the caller before this call (same tick).
    /// Returns the number of units emitted.
    pub fn spawn_enemies(
        &mut self,
        groups: &[SpawnGroup],
        wave: i32,
        ctx: &SpawnContext,
        content: &ContentRegistry,
        rng: &mut SimRng,
        mut spawn_unit: impl FnMut(&SpawnGroup, f32, f32, f32) -> Option<Entity>,
    ) -> u32 {
        self.wave = wave;
        self.spawning = true;
        self.spawn_time = SPAWN_WINDOW_TICKS;
        // `Angles.angle(x, y, world.width()/2f*tilesize, world.height()/2f*tilesize)`.
        let rotation_at = |x: f32, y: f32| {
            (ctx.height_px / 2.0 - y)
                .atan2(ctx.width_px / 2.0 - x)
                .to_degrees()
        };

        let mut emitted = 0u32;
        for group in groups {
            // `if(group.type == null) continue`.
            let Some(def) = content.unit_id(&group.unit).and_then(|id| content.unit(id)) else {
                continue;
            };
            let mut spawned = group.get_spawned(wave);
            if spawned == 0 {
                continue;
            }
            if let Some(mult) = ctx.campaign_mult {
                // Bosses round down so 1.5x difficulty keeps 1 boss
                // (WaveSpawner.java:72-78).
                spawned = if group.effect.as_deref() == Some("boss") {
                    (spawned as f32 * mult) as i32
                } else {
                    (spawned as f32 * mult).round() as i32
                }
                .max(1);
            }

            if def.flying {
                // `spread = margin / 1.5f` = 0: Java still draws
                // `Mathf.range(0)` twice per unit — keep the draw count.
                for &(x, y, filter) in &ctx.flyer {
                    if filter != -1 && !group.can_spawn(filter) {
                        continue;
                    }
                    for _ in 0..spawned {
                        let _ = rng.range(RngStream::Sim, -0.0, 0.0);
                        let _ = rng.range(RngStream::Sim, -0.0, 0.0);
                        if spawn_unit(group, x, y, rotation_at(x, y)).is_some() {
                            emitted += 1;
                        }
                    }
                }
            } else {
                let spread = crate::config::TILESIZE as f32 * 2.0;
                for &(x, y, filter, _shockwave) in &ctx.ground {
                    if filter != -1 && !group.can_spawn(filter) {
                        continue;
                    }
                    for _ in 0..spawned {
                        // `Tmp.v1.rnd(spread)`: `setToRandomDirection` first.
                        let theta = rng.range(RngStream::Sim, 0.0, 360.0).to_radians();
                        let sx = x + theta.cos() * spread;
                        let sy = y + theta.sin() * spread;
                        if spawn_unit(group, sx, sy, rotation_at(sx, sy)).is_some() {
                            emitted += 1;
                        }
                    }
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

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn two_spawns_double_the_group() {
        let mut spawner = WaveSpawner::new();
        spawner.set_spawns(vec![TilePos::new(10, 10), TilePos::new(50, 10)]);
        assert_eq!(spawner.count_spawns(), 2);
        let mut group = SpawnGroup::new("dagger");
        group.unit_amount = 3;
        group.unit_scaling = 1_000_000.0;
        let ctx = SpawnContext {
            width_px: 512.0,
            height_px: 512.0,
            ground: vec![(80.0, 80.0, -1, true), (400.0, 80.0, -1, true)],
            ..SpawnContext::default()
        };
        let mut rng = SimRng::new(1);
        let mut positions = Vec::new();
        let emitted = spawner.spawn_enemies(
            &[group],
            0,
            &ctx,
            &content(),
            &mut rng,
            |_group, x, y, _rot| {
                positions.push((x, y));
                Some(bevy_ecs::entity::Entity::PLACEHOLDER)
            },
        );
        // N per spawn, not N split across spawns.
        assert_eq!(emitted, 6);
        assert_eq!(positions.len(), 6);
        let near = |cx: f32, cy: f32| {
            positions
                .iter()
                .filter(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= 16.0 + 1e-3)
                .count()
        };
        assert_eq!(near(80.0, 80.0), 3);
        assert_eq!(near(400.0, 80.0), 3);
        assert!(spawner.spawning);
        assert_eq!(spawner.spawn_time, SPAWN_WINDOW_TICKS);
    }

    #[test]
    fn campaign_multiplier_rounds() {
        let ctx = |mult: f32| SpawnContext {
            ground: vec![(80.0, 80.0, -1, true)],
            campaign_mult: Some(mult),
            ..SpawnContext::default()
        };
        let mut group = SpawnGroup::new("dagger");
        group.unit_amount = 3;
        group.unit_scaling = 1_000_000.0;
        let mut spawner = WaveSpawner::new();
        let mut rng = SimRng::new(1);
        let mut count = 0;
        let _ = spawner.spawn_enemies(
            std::slice::from_ref(&group),
            0,
            &ctx(1.5),
            &content(),
            &mut rng,
            |_, _, _, _| {
                count += 1;
                Some(bevy_ecs::entity::Entity::PLACEHOLDER)
            },
        );
        // `Mathf.round(3 * 1.5)` = 5.
        assert_eq!(count, 5);

        // A boss rounds down: `(int)(1 * 1.5)` = 1.
        let mut boss = group.clone();
        boss.unit_amount = 1;
        boss.unit_scaling = 1_000_000.0;
        boss.effect = Some(String::from("boss"));
        let mut spawner = WaveSpawner::new();
        let mut rng = SimRng::new(1);
        let mut count = 0;
        let _ = spawner.spawn_enemies(&[boss], 0, &ctx(1.5), &content(), &mut rng, |_, _, _, _| {
            count += 1;
            Some(bevy_ecs::entity::Entity::PLACEHOLDER)
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn flyers_spawn_at_map_edge() {
        // 64x64 map, spawn tile (32,60): the map-centre line exits at
        // (256.0, 512.0) (WaveSpawner.java:176-184).
        let (x, y) = flyer_edge_position(64, 64, TilePos::new(32, 60));
        assert!((x - 256.0).abs() < 0.01, "flyer edge x {x}");
        assert!((y - 512.0).abs() < 0.01, "flyer edge y {y}");
    }

    #[test]
    fn spawned_flyers_use_edge_positions() {
        let mut spawner = WaveSpawner::new();
        spawner.set_spawns(vec![TilePos::new(32, 60)]);
        let mut group = SpawnGroup::new("flare");
        group.unit_amount = 2;
        group.unit_scaling = 1_000_000.0;
        let (ex, ey) = flyer_edge_position(64, 64, TilePos::new(32, 60));
        let ctx = SpawnContext {
            width_px: 512.0,
            height_px: 512.0,
            flyer: vec![(ex, ey, -1)],
            ..SpawnContext::default()
        };
        let mut rng = SimRng::new(1);
        let mut positions = Vec::new();
        let emitted =
            spawner.spawn_enemies(&[group], 0, &ctx, &content(), &mut rng, |_, x, y, _| {
                positions.push((x, y));
                Some(bevy_ecs::entity::Entity::PLACEHOLDER)
            });
        assert_eq!(emitted, 2);
        assert!(
            positions
                .iter()
                .all(|(x, y)| (*x - ex).abs() < 0.01 && (*y - ey).abs() < 0.01)
        );
    }

    #[test]
    fn empty_spawn_list_emits_nothing() {
        let mut spawner = WaveSpawner::new();
        let mut rng = SimRng::new(1);
        let emitted = spawner.spawn_enemies(
            &[SpawnGroup::new("dagger")],
            0,
            &SpawnContext::default(),
            &content(),
            &mut rng,
            |_, _, _, _| Some(bevy_ecs::entity::Entity::PLACEHOLDER),
        );
        assert_eq!(emitted, 0);
        // `spawnEnemies` still opens the spawn window (Java sets `spawning` first).
        assert!(spawner.spawning);
    }

    #[test]
    fn spawn_window_closes() {
        let mut spawner = WaveSpawner::new();
        spawner.set_spawns(vec![TilePos::new(1, 1)]);
        let mut rng = SimRng::new(1);
        let _ = spawner.spawn_enemies(
            &[SpawnGroup::new("dagger")],
            0,
            &SpawnContext::default(),
            &content(),
            &mut rng,
            |_, _, _, _| Some(bevy_ecs::entity::Entity::PLACEHOLDER),
        );
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
