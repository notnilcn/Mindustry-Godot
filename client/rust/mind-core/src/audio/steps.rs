// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mech step sounds and floor walk sounds (H07 T3).
//!
//! Ports `MechComp.update`'s step sound (`MechComp.java:33-56`) and
//! `UnitComp.update`'s floor walk sound (`UnitComp.java:736-745`) into a
//! view-only per-unit audio state component. The state is never saved, never
//! checksummed and only exists while an [`AudioSinkRes`] is installed, so
//! headless runs draw no `Fx` randomness and insert no component.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::audio::AudioSinkRes;
use crate::audio::sim::emit_unit_step;
use crate::content::registries::units::UnitTypeDef;
use crate::content::{BlockDef, ContentRegistry};
use crate::determinism::{RngStream, SimRng};
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::{MechComp, UnitCore, UnitTypeComp};
use crate::fx::angles::{trnsx_vec, trnsy_vec};
use crate::render::unit_draw::mech_walk_extend;
use crate::world::{TilePos, WorldGrid};

/// View-only step/walk audio state (not saved, not checksummed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Component)]
pub struct UnitStepAudio {
    /// `MechComp.walkExtension`.
    pub walk_extension: f32,
    /// `UnitComp.splashTimer`.
    pub splash_timer: f32,
}

/// `MechComp.update` step + `UnitComp.update` walk sound for one unit.
///
/// Runs from `unit_update_system` right after `movement::update_kinematics`
/// (`walk_time` already carries this tick's displacement). Without an
/// installed [`AudioSinkRes`] this is a no-op (no RNG draws, no component).
pub fn update_unit_step_audio(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    rng: &mut SimRng,
    entity: Entity,
    delta: (f32, f32),
) {
    let Some(sink) = world.get_resource::<AudioSinkRes>().cloned() else {
        return;
    };
    let Some(type_id) = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id) else {
        return;
    };
    let Some(def) = content.unit(type_id) else {
        return;
    };
    let Some((x, y)) = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y)) else {
        return;
    };
    let elevation = world
        .get::<UnitCore>(entity)
        .map(|core| core.elevation)
        .unwrap_or(0.0);
    if world.get::<UnitStepAudio>(entity).is_none() {
        world.entity_mut(entity).insert(UnitStepAudio::default());
    }
    let (mut walk_extension, mut splash_timer) = world
        .get::<UnitStepAudio>(entity)
        .map(|state| (state.walk_extension, state.splash_timer))
        .unwrap_or_default();

    // Mech half (`MechComp.java:33-56`).
    if let Some(mech) = world.get::<MechComp>(entity).copied() {
        let stride = def.mech_stride;
        let extend = mech_walk_extend(mech.walk_time, stride, false);
        let base = mech_walk_extend(mech.walk_time, stride, true);
        let extend_scl = base % 1.0;
        if extend_scl < walk_extension && base % 2.0 > 1.0 && elevation < 0.09 {
            // `-Mathf.sign(extend)`: `sign(0) = 1`, so `extend >= 0 -> -1`.
            let side = if extend < 0.0 { 1.0 } else { -1.0 };
            let width = def.hit_size / 2.0 * side;
            let length = stride * 1.35;
            let cx = x + trnsx_vec(mech.base_rotation, length, width);
            let cy = y + trnsy_vec(mech.base_rotation, length, width);
            let pitch = def.step_sound_pitch + (rng.next_float(RngStream::Fx) * 2.0 - 1.0) * 0.1;
            emit_unit_step(&sink, def.step_sound, cx, cy, def.step_sound_volume, pitch);
        }
        // Java assigns after the `if` (`MechComp.java:56`).
        walk_extension = extend_scl;
    }

    // Walk half (`UnitComp.java:736-745`).
    if !def.hovering && elevation < 0.001 && def.emit_walk_effect {
        splash_timer += (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
        if splash_timer >= 7.0 + def.hit_size / 8.0 {
            // H03: floor.walkEffect fires here (UnitComp.java:739).
            splash_timer = 0.0;
            if def.emit_walk_sound {
                let tile = TilePos::new(WorldGrid::to_tile(x) as i16, WorldGrid::to_tile(y) as i16);
                if let Some((sound, volume, pitch_min, pitch_max)) = grid
                    .floor_at(tile)
                    .and_then(|floor| content.block(floor))
                    .and_then(floor_walk_sound)
                {
                    let pitch = pitch_min + rng.next_float(RngStream::Fx) * (pitch_max - pitch_min);
                    emit_unit_step(&sink, sound, x, y, volume, pitch);
                }
            }
        }
    }

    if let Some(mut state) = world.get_mut::<UnitStepAudio>(entity) {
        state.walk_extension = walk_extension;
        state.splash_timer = splash_timer;
    }
}

/// Floor walk sound table (`Floor.java:38-40,209-211`; `Blocks.java:405-408`):
/// `mud` -> `stepMud` (0.08, pitch `[0.4, 0.5]`); any liquid floor ->
/// `stepWater` (0.1, pitch `[0.8, 1.2]`); everything else is silent.
pub fn floor_walk_sound(def: &BlockDef) -> Option<(crate::audio::SoundId, f32, f32, f32)> {
    if def.name == "mud" {
        return Some((crate::audio::SoundId::STEP_MUD, 0.08, 0.4, 0.5));
    }
    if def.is_liquid {
        return Some((crate::audio::SoundId::STEP_WATER, 0.1, 0.8, 1.2));
    }
    None
}

/// `LegsComp.java:187`:
/// `stepSound.at(x, y, stepSoundPitch + Mathf.range(stepSoundPitchRange),
/// stepSoundVolume)`. Provided for H01's leg group-stage port; the port's legs
/// are an approximation and do not call it yet.
pub fn emit_leg_step(world: &mut World, def: &UnitTypeDef, x: f32, y: f32, rng: &mut SimRng) {
    let Some(sink) = world.get_resource::<AudioSinkRes>().cloned() else {
        return;
    };
    let pitch = def.step_sound_pitch
        + (rng.next_float(RngStream::Fx) * 2.0 - 1.0) * def.step_sound_pitch_range;
    emit_unit_step(&sink, def.step_sound, x, y, def.step_sound_volume, pitch);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioEvent, SharedAudioLog};
    use crate::content::BlockId;
    use crate::sim::Sim;
    use crate::sim::unit_runtime::UnitRuntime;

    fn step_sim(size: i32) -> Sim {
        let mut sim = Sim::new(1, size, size, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        sim
    }

    /// Drives the step hook directly (no pathfinder): moves the unit `dx, dy`
    /// per call and advances `MechComp.walkTime` like `update_mech` does.
    fn walk(sim: &mut Sim, unit: Entity, dx: f32, dy: f32, steps: usize) {
        let mut runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("runtime");
        for _ in 0..steps {
            if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(unit) {
                pos.x += dx;
                pos.y += dy;
            }
            if let Some(mut mech) = sim.ecs.0.get_mut::<MechComp>(unit) {
                mech.walk_time += (dx * dx + dy * dy).sqrt();
            }
            update_unit_step_audio(
                &mut sim.ecs.0,
                &runtime.grid,
                &runtime.content,
                &mut runtime.rng,
                unit,
                (dx, dy),
            );
        }
        sim.ecs.0.insert_non_send(runtime);
    }

    fn step_events(log: &SharedAudioLog, sound: crate::audio::SoundId) -> Vec<AudioEvent> {
        log.events()
            .into_iter()
            .filter_map(|ticked| match ticked.event {
                AudioEvent::At { sound: s, .. } if s == sound => Some(ticked.event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn dagger_walk_emits_mech_steps() {
        let mut sim = step_sim(64);
        let log = SharedAudioLog::new();
        sim.ecs.0.insert_resource(AudioSinkRes::new(log.clone()));
        let unit = sim
            .spawn_live_unit("dagger", 1, 128.0, 128.0, 0.0)
            .expect("dagger");
        // 1.9 px keeps the walk phase off exact integers (real movement is
        // fractional; a 2 px step lands on `base % 2 == 1.0` and never fires).
        walk(&mut sim, unit, 1.9, 0.0, 100);
        let steps = step_events(&log, crate::audio::SoundId::MECH_STEP_SMALL);
        assert!(!steps.is_empty(), "dagger steps: {steps:?}");
        for event in &steps {
            let AudioEvent::At { volume, pitch, .. } = event else {
                unreachable!()
            };
            assert_eq!(*volume, 0.4, "dagger stepSoundVolume");
            assert!((0.9..=1.1).contains(pitch), "step pitch {pitch}");
        }
        // The per-unit state was written back.
        assert!(sim.ecs.0.get::<UnitStepAudio>(unit).is_some());
    }

    #[test]
    fn flying_unit_no_steps() {
        let mut sim = step_sim(64);
        let log = SharedAudioLog::new();
        sim.ecs.0.insert_resource(AudioSinkRes::new(log.clone()));
        let unit = sim
            .spawn_live_unit("dagger", 1, 128.0, 128.0, 0.0)
            .expect("dagger");
        if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(unit) {
            core.elevation = 1.0;
        }
        walk(&mut sim, unit, 2.0, 0.0, 100);
        assert!(
            step_events(&log, crate::audio::SoundId::MECH_STEP_SMALL).is_empty(),
            "flying mechs are silent"
        );
    }

    #[test]
    fn shallow_water_walk_sound() {
        let mut sim = step_sim(64);
        let log = SharedAudioLog::new();
        sim.ecs.0.insert_resource(AudioSinkRes::new(log.clone()));
        let unit = sim
            .spawn_live_unit("dagger", 1, 128.0, 128.0, 0.0)
            .expect("dagger");
        let water = sim
            .unit_runtime()
            .expect("rt")
            .content
            .block_id("shallow-water")
            .expect("shallow-water");
        // A pool under the walking path (x 128..328, row 16).
        for tx in 18..42 {
            for ty in 15..18 {
                sim.grid.tiles.get_mut(tx, ty).floor = water;
            }
        }
        sim.refresh_unit_runtime();
        walk(&mut sim, unit, 2.0, 0.0, 100);
        let steps = step_events(&log, crate::audio::SoundId::STEP_WATER);
        assert!(!steps.is_empty(), "water steps: {steps:?}");
        for event in &steps {
            let AudioEvent::At { volume, pitch, .. } = event else {
                unreachable!()
            };
            assert_eq!(*volume, 0.1);
            assert!((0.8..=1.2).contains(pitch), "water pitch {pitch}");
        }

        // Stone is silent for the floor half.
        let mut sim = step_sim(64);
        let log = SharedAudioLog::new();
        sim.ecs.0.insert_resource(AudioSinkRes::new(log.clone()));
        let unit = sim
            .spawn_live_unit("dagger", 1, 128.0, 128.0, 0.0)
            .expect("dagger");
        let stone = sim
            .unit_runtime()
            .expect("rt")
            .content
            .block_id("stone")
            .expect("stone");
        for tx in 18..42 {
            for ty in 15..18 {
                sim.grid.tiles.get_mut(tx, ty).floor = stone;
            }
        }
        sim.refresh_unit_runtime();
        walk(&mut sim, unit, 2.0, 0.0, 100);
        assert!(
            step_events(&log, crate::audio::SoundId::STEP_WATER).is_empty(),
            "stone floors are silent"
        );
    }

    #[test]
    fn headless_no_sink_no_component() {
        let mut sim = step_sim(64);
        let unit = sim
            .spawn_live_unit("dagger", 1, 128.0, 128.0, 0.0)
            .expect("dagger");
        walk(&mut sim, unit, 2.0, 0.0, 10);
        assert!(
            sim.ecs.0.get::<UnitStepAudio>(unit).is_none(),
            "no sink -> no component"
        );
    }
}
