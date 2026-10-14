// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-frame `control.sound.loop` sources (G07 T6).
//!
//! Ports the per-update loop call sites that do not fit the block ambient
//! snapshot: `BuilderComp.java:150-158`, `MinerComp.java:85-119`,
//! `UnitComp.java:762-767`, `TankComp.java:35,54`, `FireComp.java:46`,
//! `Weather.updateEffect` (`Weather.java:103-106`) and
//! `TractorBeamTurret.updateTile` (`:117`); plus the turret `SoundLoop`
//! inputs (`Turret.java:304,504`).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use super::ids::{MusicRef, SoundId};
use super::music::MusicRules;
use crate::content::ContentRegistry;
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::{MinerComp, StatusComp, TankComp, UnitCore, UnitTypeComp};
use crate::entities::comp::weather::WeatherRuntime;
use crate::entities::comp::{Building, Health, Pos, TeamComp, Vel};
use crate::game::runtime::CampaignRuntime;
use crate::sim::builder::BuilderPlans;
use crate::world::blocks::defense::turrets::{TurretKind, TurretState};

/// World position or the camera (weather loops).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopPos {
    /// Positional source.
    World(f32, f32),
    /// Camera-centred source (`Weather.updateEffect`).
    Camera,
}

/// One `control.sound.loop(sound, pos, volume[, pitch])` source this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopSource {
    /// Looped sound.
    pub sound: SoundId,
    /// Source position.
    pub pos: LoopPos,
    /// Call-site volume.
    pub volume: f32,
    /// Pitch multiplier.
    pub pitch: f32,
}

/// One turret `SoundLoop.update(x, y, play, volumeScl)` input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurretLoopSource {
    /// Stable per-instance key (`entity.to_bits()`).
    pub key: u64,
    /// Looped sound.
    pub sound: SoundId,
    /// `Turret.loopSoundVolume`.
    pub base_volume: f32,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Whether the loop should play this frame.
    pub play: bool,
    /// `activeSoundVolume()` scale.
    pub volume_scl: f32,
}

fn sorted_entities(world: &World) -> Vec<(u64, Entity)> {
    let mut entities: Vec<(u64, Entity)> = world
        .iter_entities()
        .map(|entity| {
            (
                entity.get::<EntitySeq>().map_or(u64::MAX, |seq| seq.0),
                entity.id(),
            )
        })
        .collect();
    entities.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));
    entities
}

fn building_centre(world: &World, content: &ContentRegistry, entity: Entity) -> Option<(f32, f32)> {
    let building = world.get::<Building>(entity)?;
    let def = content.block(building.block)?;
    Some((
        f32::from(building.tile.x()) * 8.0 + def.offset,
        f32::from(building.tile.y()) * 8.0 + def.offset,
    ))
}

/// Every `control.sound.loop` source live this frame, in deterministic order.
pub fn collect_frame_loops(
    world: &World,
    content: &ContentRegistry,
    view_tick: f32,
    infinite_build: bool,
    out: &mut Vec<LoopSource>,
) {
    out.clear();
    for (_, entity) in sorted_entities(world) {
        let dead = world.get::<UnitCore>(entity).is_some_and(|core| core.dead);
        let pos = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));

        if dead {
            continue;
        }

        // `BuilderComp.update`: the active plan within the build range.
        if let Some(plans) = world.get::<BuilderPlans>(entity)
            && plans.update_building
            && let Some(plan) = plans.plans.front()
            && let Some((ux, uy)) = pos
            && let Some(def) = world
                .get::<UnitTypeComp>(entity)
                .and_then(|unit| content.unit(unit.type_id))
        {
            let range = if infinite_build {
                f32::MAX
            } else {
                def.build_range
            };
            let off = content.block(plan.block).map_or(0.0, |block| block.offset);
            let (px, py) = (f32::from(plan.x) * 8.0 + off, f32::from(plan.y) * 8.0 + off);
            if (px - ux).powi(2) + (py - uy).powi(2) <= range * range {
                out.push(LoopSource {
                    sound: SoundId::LOOP_BUILD,
                    pos: LoopPos::World(px, py),
                    volume: 1.3,
                    pitch: 1.0,
                });
            }
        }

        let def = world
            .get::<UnitTypeComp>(entity)
            .and_then(|unit| content.unit(unit.type_id));

        // `MinerComp.update`: `loop(type.mineSound, this, type.mineSoundVolume)`.
        if let (Some(miner), Some(def), Some((ux, uy))) = (world.get::<MinerComp>(entity), def, pos)
            && miner.mining
            && miner.mine_tile.is_some()
            && !def.mine_sound.is_none_or_unset()
        {
            out.push(LoopSource {
                sound: def.mine_sound,
                pos: LoopPos::World(ux, uy),
                volume: def.mine_sound_volume,
                pitch: 1.0,
            });
        }

        // `UnitComp.update`: idle `loopSound` + speed-scaled `moveSound`.
        if let (Some(def), Some((ux, uy))) = (def, pos) {
            if !def.loop_sound.is_none_or_unset() && def.loop_sound_volume > 0.0 {
                out.push(LoopSource {
                    sound: def.loop_sound,
                    pos: LoopPos::World(ux, uy),
                    volume: def.loop_sound_volume,
                    pitch: 1.0,
                });
            }
            if !def.move_sound.is_none_or_unset()
                && let Some(vel) = world.get::<Vel>(entity)
            {
                let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
                let progress = if def.speed > 0.0 {
                    (speed / def.speed).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let volume = def.move_sound_volume * progress;
                if volume > 0.0 {
                    out.push(LoopSource {
                        sound: def.move_sound,
                        pos: LoopPos::World(ux, uy),
                        volume,
                        pitch: def.move_sound_pitch_min
                            + (def.move_sound_pitch_max - def.move_sound_pitch_min) * progress,
                    });
                }
            }
        }

        // `TankComp.update`: `loop(type.tankMoveSound, this, type.tankMoveVolume)`
        // while `walked` (≈ moving).
        if let (Some(_tank), Some(def), Some((ux, uy))) = (world.get::<TankComp>(entity), def, pos)
            && !def.tank_move_sound.is_none_or_unset()
            && world
                .get::<Vel>(entity)
                .is_some_and(|vel| (vel.x * vel.x + vel.y * vel.y).sqrt() >= 0.01)
        {
            out.push(LoopSource {
                sound: def.tank_move_sound,
                pos: LoopPos::World(ux, uy),
                volume: def.tank_move_volume,
                pitch: 1.0,
            });
        }

        // `FireComp.update`: `loop(Sounds.loopFire, this, 0.07f)`.
        if let Some(fire) = world.get::<crate::combat::fires::FireState>(entity) {
            out.push(LoopSource {
                sound: SoundId::LOOP_FIRE,
                pos: LoopPos::World(f32::from(fire.tile.0) * 8.0, f32::from(fire.tile.1) * 8.0),
                volume: 0.07,
                pitch: 1.0,
            });
        }

        // `Weather.updateEffect`: camera loop with a noise oscillator.
        if let Some(weather) = world.get::<WeatherRuntime>(entity)
            && let Some(def) = content.weather(weather.weather)
            && let Some(sound) = def.sound.as_deref().and_then(SoundId::by_name)
            && !sound.is_none_or_unset()
        {
            let noise = if def.sound_vol_osc_mag > 0.0 && def.sound_vol_osc_scl > 0.0 {
                // `Weather.updateEffect` (`Weather.java:102-105`):
                // `abs(Noise.rawNoise(Time.time / soundVolOscScl)) * soundVolOscMag`.
                crate::math::noise::raw_noise_1d(f64::from(view_tick / def.sound_vol_osc_scl)).abs()
                    as f32
                    * def.sound_vol_osc_mag
            } else {
                0.0
            };
            let volume = ((def.sound_vol + noise) * weather.opacity).max(def.sound_vol_min);
            if volume > 0.0 {
                out.push(LoopSource {
                    sound,
                    pos: LoopPos::Camera,
                    volume,
                    pitch: 1.0,
                });
            }
        }

        // `TractorBeamTurret.updateTile`: `loop(shootSound, this, 0.9)` while
        // the beam is active (`TurretState.any`).
        if let Some(state) = world.get::<TurretState>(entity)
            && state.config.kind == TurretKind::TractorBeam
            && state.any
            && let Some((x, y)) = building_centre(world, content, entity)
        {
            out.push(LoopSource {
                sound: SoundId::BEAM_PARALLAX,
                pos: LoopPos::World(x, y),
                volume: 0.9,
                pitch: 1.0,
            });
        }
    }
}

/// Every turret `SoundLoop` input this frame (`Turret.updateTile`).
pub fn collect_turret_loops(
    world: &World,
    content: &ContentRegistry,
    out: &mut Vec<TurretLoopSource>,
) {
    out.clear();
    for (_, entity) in sorted_entities(world) {
        let Some(state) = world.get::<TurretState>(entity) else {
            continue;
        };
        let config = &state.config;
        if config.loop_sound.is_none_or_unset() {
            continue;
        }
        let Some(building) = world.get::<Building>(entity) else {
            continue;
        };
        let Some((x, y)) = building_centre(world, content, entity) else {
            continue;
        };
        let (play, volume_scl) = match config.kind {
            // `LiquidTurret.shouldActiveSound = wasShooting && enabled`
            // (volume scale stays `shootWarmup` from the base class).
            TurretKind::Liquid => (state.was_shooting && building.enabled, state.shoot_warmup),
            // `ContinuousTurret`/`LaserTurret.shouldActiveSound = bullets.any()`.
            TurretKind::Continuous | TurretKind::Laser => (!state.bullets.is_empty(), 1.0),
            // `Turret.shouldActiveSound = shootWarmup > 0.01`.
            _ => (state.shoot_warmup > 0.01, state.shoot_warmup),
        };
        out.push(TurretLoopSource {
            key: entity.to_bits(),
            sound: config.loop_sound,
            base_volume: config.loop_sound_volume,
            x,
            y,
            play,
            volume_scl,
        });
    }
}

/// `Weapon.update` continuous-weapon `SoundLoop` (`Weapon.java:381-421`):
/// appends one loop source per live continuous mount (`vela`, `navanax`).
///
/// Keys are `entity.to_bits() ^ (1 << 63) ^ ((index + 1) << 56)`; unit
/// entities never set bit 63 (only packed building entities can), so the keys
/// cannot collide with [`collect_turret_loops`].
pub fn collect_weapon_loops(world: &World, out: &mut Vec<TurretLoopSource>) {
    for (_, entity) in sorted_entities(world) {
        if world.get::<UnitCore>(entity).is_some_and(|core| core.dead) {
            continue;
        }
        let Some(unit_weapons) = world.get::<crate::weapons::UnitWeapons>(entity) else {
            continue;
        };
        let unit_pos = world.get::<Pos>(entity).copied();
        for (index, weapon) in unit_weapons.weapons.iter().enumerate() {
            if !weapon.continuous || weapon.shoot_sound.is_none_or_unset() {
                continue;
            }
            let Some(mount) = unit_weapons.mounts.get(index) else {
                continue;
            };
            // `mount.bullet != null && mount.bullet.isAdded()`.
            let bullet = mount
                .bullet
                .filter(|bullet| world.get_entity(*bullet).is_ok());
            let position = bullet
                .and_then(|bullet| world.get::<Pos>(bullet).copied())
                .or(unit_pos);
            let Some(position) = position else {
                continue;
            };
            out.push(TurretLoopSource {
                key: entity.to_bits() ^ (1u64 << 63) ^ ((index as u64 + 1) << 56),
                sound: weapon.shoot_sound,
                base_volume: 1.0,
                x: position.x,
                y: position.y,
                play: bullet.is_some(),
                volume_scl: 1.0,
            });
        }
    }
}

/// Everything `SoundControl.update` reads from the live game state (G07 T7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioGameState {
    /// `state.isCampaign()`.
    pub in_campaign: bool,
    /// `state.rules.editor`.
    pub editor: bool,
    /// Rules music fields (`Rules.ambientMusic`/`darkMusic`/...).
    pub rules: MusicRules,
    /// `state.wave`.
    pub wave: i32,
    /// `state.enemies`.
    pub enemies: i32,
    /// Player core health fraction (`player.team().data().core().healthf()`).
    pub core_hp_fraction: Option<f32>,
    /// `state.boss() != null` — a live `boss`-status enemy unit.
    pub live_boss: bool,
    /// A spawn group at `wave - 2` has the `boss` effect
    /// (`SoundControl.java:75-83`).
    pub boss_spawn_group: bool,
}

/// `SpawnGroup.getSpawned` over the JSON spawn-group form
/// (`SoundControl.java:75-83` reads `Rules.spawns`).
fn json_spawned(group: &crate::io::json::rules::SpawnGroup, wave: i32) -> i32 {
    let spacing = group.spacing.max(1);
    if wave < group.begin || wave > group.end || (wave - group.begin) % spacing != 0 {
        return 0;
    }
    let steps = (wave - group.begin) / spacing;
    let scaled = steps as f32 / group.unit_scaling;
    group
        .unit_amount
        .saturating_add(scaled as i32)
        .min(group.max)
}

/// Reads the in-game music context state from the campaign session.
pub fn read_game_state(world: &World, content: &ContentRegistry) -> AudioGameState {
    let Some(campaign) = world.get_resource::<CampaignRuntime>() else {
        return AudioGameState::default();
    };
    let session = &campaign.session;
    let rules = &session.rules;

    let music = MusicRules {
        ambient: rules.ambient_music.as_ref().map(|containers| {
            containers
                .iter()
                .map(|container| MusicRef::new(container.name()))
                .collect()
        }),
        dark: rules.dark_music.as_ref().map(|containers| {
            containers
                .iter()
                .map(|container| MusicRef::new(container.name()))
                .collect()
        }),
        disable: rules.disable_music,
        always: rules.always_play_music,
        volume: rules.music_volume,
    };

    // First (lowest EntitySeq) player-team core's health fraction.
    let mut core_hp: Option<(u64, f32)> = None;
    for entity in world.iter_entities() {
        let Some(building) = entity.get::<Building>() else {
            continue;
        };
        if entity.get::<TeamComp>().map(|team| team.team) != Some(rules.default_team) {
            continue;
        }
        if content
            .block(building.block)
            .is_none_or(|def| def.kind != crate::content::BlockKind::CoreBlock)
        {
            continue;
        }
        let Some(health) = entity.get::<Health>() else {
            continue;
        };
        if health.max_health <= 0.0 {
            continue;
        }
        let seq = entity.get::<EntitySeq>().map_or(u64::MAX, |seq| seq.0);
        let fraction = health.health / health.max_health;
        match core_hp {
            Some((best, _)) if best <= seq => {}
            _ => core_hp = Some((seq, fraction)),
        }
    }

    let live_boss = content.status_id("boss").is_some_and(|boss| {
        world.iter_entities().any(|entity| {
            entity.get::<TeamComp>().map(|team| team.team) == Some(rules.wave_team)
                && entity
                    .get::<StatusComp>()
                    .is_some_and(|status| status.has_effect_of(boss))
        })
    });

    let boss_spawn_group = rules.spawns.iter().any(|group| {
        group.effect.as_deref() == Some("boss") && json_spawned(group, session.wave - 2) > 0
    });

    AudioGameState {
        in_campaign: session.is_campaign(),
        editor: rules.editor,
        rules: music,
        wave: session.wave,
        enemies: session.enemies,
        core_hp_fraction: core_hp.map(|(_, fraction)| fraction),
        live_boss,
        boss_spawn_group,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::SimCommand;
    use crate::sim::Sim;

    fn sim_with_player() -> (Sim, Entity) {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core_id = sim.world_block_id("core-shard").expect("core");
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block: core_id,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core placed");
        let unit = sim
            .spawn_player_unit("alpha", 1, 80.0, 80.0, 0.0)
            .expect("player unit");
        // The local player is team 1 (`sharded`); sync the placement guard like
        // a campaign load does (`sync_build_rules`).
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        (sim, unit)
    }

    #[test]
    fn builder_in_range_emits_loop_build() {
        let (mut sim, _unit) = sim_with_player();
        let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
        assert!(sim.queue_player_place(12, 12, wall, 0));
        sim.tick().expect("tick");
        let content = sim.block_runtime().expect("runtime").content();
        let mut out = Vec::new();
        collect_frame_loops(
            &sim.ecs.0,
            content,
            sim.tick_count() as f32,
            false,
            &mut out,
        );
        let builds: Vec<&LoopSource> = out
            .iter()
            .filter(|source| source.sound == SoundId::LOOP_BUILD)
            .collect();
        assert_eq!(builds.len(), 1, "{out:?}");
        assert_eq!(builds[0].volume, 1.3);
        // Plan centre: wall offset 0 -> 12 * 8.
        assert_eq!(builds[0].pos, LoopPos::World(96.0, 96.0));
    }

    #[test]
    fn fire_emits_loop_fire() {
        let mut world = World::new();
        let mut rng = crate::determinism::SimRng::new(3);
        crate::combat::fires::create(&mut world, 5, 6, &mut rng).expect("fire");
        let content = crate::world::harness::BuildHarness::load_content();
        let mut out = Vec::new();
        collect_frame_loops(&world, &content, 0.0, false, &mut out);
        let fires: Vec<&LoopSource> = out
            .iter()
            .filter(|source| source.sound == SoundId::LOOP_FIRE)
            .collect();
        assert_eq!(fires.len(), 1, "{out:?}");
        assert_eq!(fires[0].volume, 0.07);
        assert_eq!(fires[0].pos, LoopPos::World(40.0, 48.0));
    }

    #[test]
    fn rain_weather_emits_camera_loop() {
        let content = crate::world::harness::BuildHarness::load_content();
        let rain = content.weather_by_name("rain").expect("rain").id;
        let mut world = World::new();
        world.spawn((
            EntitySeq(0),
            WeatherRuntime {
                weather: rain,
                intensity: 1.0,
                opacity: 1.0,
                life: 600.0,
                effect_timer: 0.0,
                wind_x: 0.0,
                wind_y: 0.0,
            },
        ));
        let mut out = Vec::new();
        collect_frame_loops(&world, &content, 0.0, false, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].sound, SoundId::RAIN);
        assert_eq!(out[0].pos, LoopPos::Camera);
        assert!(
            (out[0].volume - 0.25).abs() < 1.0e-6,
            "rain volume {}",
            out[0].volume
        );

        // Snow at opacity 0 still holds `soundVolMin` (0.02).
        let snow = content.weather_by_name("snowing").expect("snowing").id;
        let mut world = World::new();
        world.spawn((
            EntitySeq(0),
            WeatherRuntime {
                weather: snow,
                intensity: 1.0,
                opacity: 0.0,
                life: 600.0,
                effect_timer: 0.0,
                wind_x: 0.0,
                wind_y: 0.0,
            },
        ));
        let mut out = Vec::new();
        collect_frame_loops(&world, &content, 0.0, false, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].sound, SoundId::WIND_HOWL);
        assert!(
            (out[0].volume - 0.02).abs() < 1.0e-6,
            "snow volume {}",
            out[0].volume
        );
    }

    /// H07 T11 (`Weapon.java:381-421`): a live continuous mount reports
    /// `play = true` at its bullet; with no bullet it reports the unit
    /// position and `play = false`.
    #[test]
    fn continuous_weapon_loop_tracks_mount_bullet() {
        use crate::ai::UnitHarness;

        let mut harness = UnitHarness::new(32, 32, 7);
        let vela = harness.spawn("vela", 0, 64.0, 64.0, 0.0).expect("vela");
        let index = {
            let world = &harness.build.world;
            let weapons = world
                .get::<crate::weapons::UnitWeapons>(vela)
                .expect("weapons");
            weapons
                .weapons
                .iter()
                .position(|weapon| weapon.continuous && !weapon.shoot_sound.is_none_or_unset())
                .expect("continuous weapon")
        };
        let key = |entity: Entity, index: usize| {
            entity.to_bits() ^ (1u64 << 63) ^ ((index as u64 + 1) << 56)
        };

        // No bullet: the source exists with `play = false` at the unit.
        let mut out = Vec::new();
        collect_weapon_loops(&harness.build.world, &mut out);
        let source = out
            .iter()
            .find(|source| source.key == key(vela, index))
            .expect("continuous source");
        assert!(!source.play);
        assert_eq!((source.x, source.y), (64.0, 64.0));
        assert_eq!(source.base_volume, 1.0);

        // A live bullet at another position: `play = true` at the bullet.
        let bullet = harness.build.world.spawn((Pos { x: 80.0, y: 90.0 },)).id();
        {
            let mut weapons = harness
                .build
                .world
                .get_mut::<crate::weapons::UnitWeapons>(vela)
                .expect("weapons");
            weapons.mounts[index].bullet = Some(bullet);
        }
        let mut out = Vec::new();
        collect_weapon_loops(&harness.build.world, &mut out);
        let source = out
            .iter()
            .find(|source| source.key == key(vela, index))
            .expect("continuous source");
        assert!(source.play);
        assert_eq!((source.x, source.y), (80.0, 90.0));

        // Despawned bullet: off again.
        let _ = harness.build.world.despawn(bullet);
        let mut out = Vec::new();
        collect_weapon_loops(&harness.build.world, &mut out);
        let source = out
            .iter()
            .find(|source| source.key == key(vela, index))
            .expect("continuous source");
        assert!(!source.play);
    }

    #[test]
    fn liquid_turret_loop_plays_when_shooting() {
        use crate::combat::harness::CombatHarness;

        let mut harness = CombatHarness::new(32, 16, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let wave = harness
            .spawn_test_turret("wave", tx, ty, 1)
            .expect("wave turret");
        // The fixture carries no `Building`; the real placement does, and the
        // loop key/centre come from it.
        let block = harness.content().block_id("wave").expect("wave block");
        let mut building = Building::new(crate::world::TilePos::new(4, 8), block, 0);
        building.enabled = true;
        harness.build.world.entity_mut(wave).insert(building);
        harness
            .build
            .world
            .get_mut::<TurretState>(wave)
            .expect("state")
            .was_shooting = true;
        let mut out = Vec::new();
        collect_turret_loops(&harness.build.world, harness.content(), &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].sound, SoundId::LOOP_SPRAY);
        assert!(out[0].play);
        assert_eq!(out[0].base_volume, 0.5);
    }

    #[test]
    fn read_game_state_defaults_without_campaign() {
        let world = World::new();
        let content = crate::world::harness::BuildHarness::load_content();
        assert_eq!(read_game_state(&world, &content), AudioGameState::default());
    }

    #[test]
    fn boss_spawn_group_uses_wave_minus_two() {
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;
        use crate::io::json::rules::SpawnGroup;

        let content = crate::world::harness::BuildHarness::load_content();
        let campaign = Campaign::from_registry(&content, &crate::game::planet::EmptyNeighborhood);
        let mut session = PlaySession::new(Rules {
            spawns: vec![SpawnGroup {
                begin: 10,
                end: 10,
                spacing: 1,
                unit_amount: 1,
                max: 100,
                unit_scaling: 1.0,
                effect: Some(String::from("boss")),
                ..SpawnGroup::default()
            }],
            ..Rules::default()
        });
        session.wave = 12;
        let mut world = World::new();
        world.insert_resource(CampaignRuntime::new(session, campaign));
        let state = read_game_state(&world, &content);
        assert!(state.boss_spawn_group, "wave-2 = 10 matches the group");
        assert_eq!(state.wave, 12);

        world
            .get_resource_mut::<CampaignRuntime>()
            .expect("campaign")
            .session
            .wave = 11;
        assert!(!read_game_state(&world, &content).boss_spawn_group);
    }
}
