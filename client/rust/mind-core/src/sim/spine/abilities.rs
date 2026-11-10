// SPDX-License-Identifier: GPL-3.0-only

//! Entity-kind byte and the `EntitySet::UpdateUnits` ability-queue drain system.
//!
//! Upstream: core/src/mindustry/entities/abilities/MoveEffectAbility.java,
//! core/src/mindustry/entities/abilities/UnitSpawnAbility.java.

use crate::sim::*;

/// Entity kind byte used by the checksum stream (`0 = building`).
pub const KIND_BUILDING: u8 = 0;

/// Applies a world-processor `SpawnBulletI` request synchronously
/// (`LExecutor.SpawnBulletI` → `BulletType.create`, `LExecutor.java:1712`).
///
/// The sim owns the live bullet half: the [`unit_runtime::UnitRuntime`] holds
/// the content registry, grid snapshot, combat RNG, FX sink and entity
/// sequence, exactly like [`Sim::with_combat_ctx`]. The request is issued from
/// [`crate::logic::world::run_spawn_bullet`] *inside* the VM instruction so
/// the instruction's `result` variable receives the spawned bullet before the
/// next instruction (Java's immediate `result.setobj(type.create(...))`).
///
/// Returns `None` without an installed unit runtime or when the bullet def is
/// unknown/removed; the new bullet joins `runtime.bullets` so the next
/// `UpdateBullets` pass runs it.
pub(crate) fn spawn_logic_bullet(
    world: &mut bevy_ecs::world::World,
    spawn: &crate::combat::bullet::BulletSpawn,
) -> Option<Entity> {
    let mut runtime = world.remove_non_send::<unit_runtime::UnitRuntime>()?;
    let audio = world
        .remove_resource::<crate::audio::AudioSinkRes>()
        .unwrap_or_else(crate::audio::AudioSinkRes::noop);
    let mut spawned: Vec<Entity> = Vec::new();
    let result = {
        let unit_runtime::UnitRuntime {
            content,
            grid,
            rng,
            fx,
            seq,
            ..
        } = &mut runtime;
        let mut ctx = crate::combat::bullet::CombatCtx {
            world,
            content,
            grid,
            rng,
            fx: fx.as_ref(),
            audio: &audio,
            seq,
            spawned: &mut spawned,
        };
        ctx.spawn(spawn)
    };
    world.insert_resource(audio);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
    result
}

/// `EntitySet::UpdateUnits` system (GAPS #61): drains the queues filled by
/// [`crate::combat::abilities::update_unit_abilities`] in the same tick.
///
/// * [`crate::combat::abilities::PendingMoveLightning`] — a real
///   [`crate::combat::lightning::create`] branch (`MoveLightningAbility.java:82-84`);
/// * [`crate::combat::abilities::PendingAbilityUnitSpawns`] — `Units.canCreate`
///   gating, then a tracked spawn through [`unit_runtime::UnitRuntime`]
///   (`UnitSpawnAbility.java:48-60`).
///
/// Registered after [`unit_runtime::unit_update_system`] inside
/// `EntitySet::UpdateUnits`, so the lightning `hitCreate` bullets still run in
/// this tick's `UpdateBullets` pass (Java creates them inside the unit pass) and
/// the spawned unit joins the tracked runtime immediately. Without a live unit
/// runtime the queues stay queued (the bare harness owns no sequencer).
pub(crate) fn drain_ability_queues_system(world: &mut bevy_ecs::world::World) {
    let strikes = world
        .remove_resource::<crate::combat::abilities::PendingMoveLightning>()
        .unwrap_or_default();
    let spawns = world
        .remove_resource::<crate::combat::abilities::PendingAbilityUnitSpawns>()
        .unwrap_or_default();
    if strikes.0.is_empty() && spawns.0.is_empty() {
        return;
    }
    let Some(mut runtime) = world.remove_non_send::<unit_runtime::UnitRuntime>() else {
        world.insert_resource(strikes);
        world.insert_resource(spawns);
        return;
    };
    // `Lightning.create` needs the combat context (content/grid/rng/fx/seq).
    if !strikes.0.is_empty() {
        let audio = world
            .remove_resource::<crate::audio::AudioSinkRes>()
            .unwrap_or_else(crate::audio::AudioSinkRes::noop);
        let mut spawned: Vec<Entity> = Vec::new();
        {
            let unit_runtime::UnitRuntime {
                content,
                grid,
                rng,
                fx,
                seq,
                ..
            } = &mut runtime;
            let mut ctx = crate::combat::bullet::CombatCtx {
                world,
                content,
                grid,
                rng,
                fx: fx.as_ref(),
                audio: &audio,
                seq,
                spawned: &mut spawned,
            };
            crate::combat::abilities::drain_pending_move_lightning(&mut ctx, &strikes);
        }
        world.insert_resource(audio);
        runtime.bullets_created += spawned.len() as u64;
        runtime.bullets.extend(spawned);
    }
    // `UnitSpawnAbility.update`: `if(timer >= spawnTime && Units.canCreate(...))
    // { this.unit.create(team) ... }`. `UnitRuntime::spawn` owns the tracked
    // entity sequence and the `unitsCreated` counter (`UnitType.spawn`).
    for spawn in spawns.0 {
        let Some(def) = runtime.content.unit_by_name(spawn.unit) else {
            continue;
        };
        if !crate::entities::comp::unit::queries::can_create_unit(world, spawn.team, def) {
            continue;
        }
        let Some(entity) = runtime.spawn(
            world,
            spawn.unit,
            spawn.team,
            spawn.x,
            spawn.y,
            spawn.rotation,
        ) else {
            continue;
        };
        crate::game::stats::notify_unit_created(world, entity, Some(spawn.unit));
        // `spawnEffect.at(x, y, 0f)` (`UnitSpawnAbility.java:52`).
        use crate::content::registries::fx_meta::EffectRef;
        runtime.fx.effect(
            &EffectRef::Named(crate::content::EffectId::SPAWN),
            spawn.x,
            spawn.y,
            0.0,
            crate::content::Rgba::WHITE,
        );
    }
    world.insert_non_send(runtime);
}
