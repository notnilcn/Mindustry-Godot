// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `EntitySet::UpdateUnits` system: per-unit AI dispatch, kind kinematics,
//! the weapon pass and the leg/crawl damage follow-up.
//! Upstream: core/src/mindustry/core/Logic.java (updateEntities)
//! core/src/mindustry/entities/comp/UnitComp.java
//! core/src/mindustry/entities/comp/LegsComp.java
//! core/src/mindustry/entities/comp/CrawlComp.java

use super::ai::builder_falls_back;
use super::ai::defender::update_defender_ai;
use super::ai::follow::update_flying_follow_ai;
use super::ai::ground::GroundAiRules;
use super::ai::hug::update_hug_ai;
use super::ai_arms::assembler::assembler_arm;
use super::ai_arms::boost::boost_arm;
use super::ai_arms::builder::builder_arm;
use super::ai_arms::cargo::cargo_arm;
use super::ai_arms::command::command_arm;
use super::ai_arms::ground_flying::ground_flying_arm;
use super::ai_arms::logic::logic_arm;
use super::ai_arms::missile::missile_arm;
use super::ai_arms::no_ai::no_ai_arm;
use super::ai_arms::player::player_arm;
use super::ai_arms::suicide::suicide_arm;
use super::deaths::has_core_of_team;
use super::physics::rebuild_avoidance;
use super::*;

pub fn unit_update_system(world: &mut World) {
    // `CombatEnv` sync (G01 T14): `Rules.fire` and `Env.oxygen` gate
    // `Fires.create`; `state.envAttrs` heat (0.0 until a live source lands).
    if let Some(campaign) = world.get_resource::<CampaignRuntime>() {
        let rules = &campaign.session.rules;
        world.insert_resource(crate::combat::fires::CombatEnv {
            fire: rules.fire,
            oxygen: rules.env & crate::game::planet::env_bit(crate::content::EnvFlag::Oxygen) != 0,
            heat: 0.0,
        });
    }
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    runtime.units.retain(|entity| {
        world.get_entity(*entity).is_ok() && world.get::<UnitTypeComp>(*entity).is_some()
    });
    if runtime.units.is_empty() {
        world.insert_non_send(runtime);
        return;
    }

    // One target index per tick (buildings by team + live units), reused by
    // every unit's acquisition pass. Bullets/fires/puddles never enter it.
    let queries = TargetQueries::build(world, &runtime.content);
    // `AvoidanceProcess`: while active, rebuild the occupancy grid from the
    // current positions before any unit moves this tick (queries read the
    // previous frame's buffer; the tick-start grid is that data).
    if runtime.avoidance_active {
        rebuild_avoidance(world, &mut runtime);
    }
    // `state.rules` fields `GroundAI` reads, snapshotted once per tick so the
    // unit loop can mutate the world freely.
    let ground_rules = world
        .get_resource::<CampaignRuntime>()
        .map(|campaign| GroundAiRules {
            waves: campaign.session.rules.waves,
            default_team: campaign.session.rules.default_team,
            drop_zone_radius: campaign.session.rules.drop_zone_radius,
            wave_team: campaign.session.rules.wave_team,
            pvp: campaign.session.rules.pvp,
        })
        .unwrap_or_default();
    // `Pathfinder.EnemyCoreField.getPositions` target sets, one per team per
    // tick (plan G01 §6.9: stable targets keep the `(team, cost)` field cache).
    let mut core_target_cache: BTreeMap<u8, Vec<TilePos>> = BTreeMap::new();
    // `GameState.hasSpawns()` (`GameState.java:68-70`): `rules.waves &&
    // ((rules.waveTeam has a core && rules.attackMode) || !rules.spawns.isEmpty())`.
    // Snapshotted once per tick for the per-unit drop-zone push gate (T5).
    let has_spawns = world
        .get_resource::<CampaignRuntime>()
        .is_some_and(|campaign| {
            let rules = &campaign.session.rules;
            rules.waves
                && ((rules.attack_mode
                    && has_core_of_team(world, &runtime.content, rules.wave_team))
                    || !rules.spawns.is_empty())
        });
    let entities = runtime.units.clone();
    for entity in entities {
        if world.get_entity(entity).is_err() {
            continue;
        }
        let before = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
        // `VelComp.update` is `@MethodPriority(-1)`, so the velocity model
        // integrates `vel` (tile collisions + drag) before the controller runs.
        // `before` is read first so the kind kinematics `delta` includes this
        // tick's movement.
        crate::entities::comp::unit::velocity::vel_update(
            world,
            &runtime.grid,
            &runtime.content,
            entity,
        );
        let Some(slot) = world.get::<ControllerSlot>(entity).copied() else {
            continue;
        };
        let team = world
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);

        // `UnitComp.update` (`UnitComp.java:869-871`): remove units spawned by
        // the core that are not player-controlled.
        if world
            .get::<crate::entities::comp::unit::comp::UnitCore>(entity)
            .is_some_and(|core| core.spawned_by_core)
            && slot.kind != AiKind::Player
            && !world
                .get::<crate::entities::comp::Health>(entity)
                .is_none_or(|health| health.dead)
        {
            let _ = world.despawn(entity);
            continue;
        }

        // `UnitComp.update` (`UnitComp.java:859-861`): `if(!net.client() &&
        // !dead && shouldUpdateController()) controller.updateUnit();` — a dead
        // unit never runs its controller/targeting/weapons. A `Falling` wreck
        // keeps integrating its velocity with `drag = 0.01` and sinks at
        // `type.fallSpeed` (UnitComp.java:803-830); any other dead unit is
        // destroyed by `resolve_deaths` at the end of the tick.
        if world
            .get::<crate::entities::comp::Health>(entity)
            .is_some_and(|health| health.dead)
        {
            if world.get::<Falling>(entity).is_some() {
                crate::entities::comp::unit::environment::update_falling_wreck(
                    world,
                    &runtime.grid,
                    &runtime.content,
                    entity,
                );
            }
            continue;
        }

        if slot.kind == AiKind::Player {
            player_arm(world, &mut runtime, entity);
        } else if slot.kind == AiKind::Command {
            command_arm(world, &mut runtime, &queries, entity, team, slot);
        } else if slot.kind == AiKind::Missile {
            missile_arm(world, &mut runtime, &queries, entity, team);
        } else if slot.kind == AiKind::Suicide {
            suicide_arm(world, &mut runtime, &queries, entity, team);
        } else if slot.kind == AiKind::Builder && !builder_falls_back(world, team) {
            builder_arm(world, &mut runtime, &queries, entity, team);
        } else if slot.kind == AiKind::Boost {
            boost_arm(world, &mut runtime, entity, team);
        } else if slot.kind == AiKind::Defender {
            // `DefenderAI` (`DefenderAI.java:12-45`): escort the healthiest
            // player-controllable ally; no weapons pass of its own.
            update_defender_ai(world, &mut runtime, entity, team, ground_rules);
        } else if slot.kind == AiKind::FlyingFollow {
            // `FlyingFollowAI` (`FlyingFollowAI.java:11-60`): follow an allied
            // leader on wave teams, else the `FlyingAI` fallback.
            update_flying_follow_ai(
                world,
                &mut runtime,
                &queries,
                entity,
                team,
                slot,
                ground_rules,
            );
        } else if slot.kind == AiKind::Hug {
            // `HugAI` (`HugAI.java:16-61`): ray-checked approach, contact
            // circle and the enemy-core override.
            update_hug_ai(
                world,
                &mut runtime,
                &queries,
                entity,
                team,
                ground_rules,
                &mut core_target_cache,
            );
        } else if slot.kind == AiKind::Cargo {
            cargo_arm(world, &mut runtime, entity, team);
        } else if slot.kind == AiKind::Assembler {
            assembler_arm(world, &mut runtime, entity, team);
        } else if slot.kind == AiKind::Logic {
            logic_arm(world, &mut runtime, &queries, entity, team);
        } else if slot.kind == AiKind::NoAi {
            no_ai_arm();
        } else {
            ground_flying_arm(
                world,
                &mut runtime,
                &queries,
                entity,
                team,
                slot,
                ground_rules,
                &mut core_target_cache,
            );
        }

        // Kind kinematics (`Unit.update` tail: legs/mech/tank/crawl/water).
        if world.get_entity(entity).is_ok() {
            let after = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
            let delta = match (before, after) {
                (Some(before), Some(after)) => (after.0 - before.0, after.1 - before.1),
                _ => (0.0, 0.0),
            };
            // Snapshot the legs before the kind pass so the splash pass can
            // tell which feet actually stepped this tick
            // (`LegsComp.java:196-202`).
            let legs_before = world
                .get::<crate::entities::comp::unit::LegsComp>(entity)
                .cloned();
            let _ =
                movement::update_kinematics(world, &runtime.grid, &runtime.content, entity, delta);
            // GAPS #10: leg splash + crawler crush (the damage half of the
            // kind updates, which needs `rules.unitDamage` and the content
            // registry).
            apply_movement_damage(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
                delta,
                legs_before.as_ref(),
            );

            // H07 T3: `MechComp.update` step + `UnitComp.update` walk sounds
            // (view-only; no-op without an audio sink).
            crate::audio::steps::update_unit_step_audio(
                world,
                &runtime.grid,
                &runtime.content,
                &mut runtime.rng,
                entity,
                delta,
            );

            // `MinerComp.update` runs after the controller and the kind
            // kinematics for every unit (G01 T10a).
            crate::entities::comp::unit::miner::update_miner_comp(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
            );
            // `UnitComp.update` environment half (G01 T13): bounds/drowning/
            // floor status/env death/drop-zone push/solid kill.
            crate::entities::comp::unit::environment::update_unit_environment(
                world,
                &runtime.grid,
                &runtime.content,
                runtime.wave_spawner.get_spawns(),
                has_spawns,
                entity,
            );

            // `TimedKillComp.update`: `time = min(time + delta, lifetime)`;
            // at the lifetime the unit is marked dead and `resolve_deaths`
            // runs the normal kill path (including `shootOnDeath`).
            if let Some(mut timed) = world.get_mut::<TimedKillComp>(entity) {
                timed.time = (timed.time + 1.0).min(timed.lifetime);
                if timed.lifetime > 0.0
                    && timed.time >= timed.lifetime
                    && let Some(mut health) = world.get_mut::<Health>(entity)
                {
                    health.health = 0.0;
                    health.dead = true;
                }
            }
        }
    }

    // Weapon state sync + firing pass (`UnitWeapons.update`).
    let units = runtime.units.clone();
    for entity in units {
        if world.get_entity(entity).is_ok() {
            sync_weapon_state(world, entity);
        }
    }
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    // H02 T11: shock mines queued by `unitOn` fire their tendrils in push
    // order (Java creates them inside the unit pass).
    let pending = world
        .remove_resource::<crate::world::behavior::PendingShockMineTriggers>()
        .unwrap_or_default();
    let mut spawned: Vec<Entity> = Vec::new();
    {
        let UnitRuntime {
            content,
            grid,
            rng,
            fx,
            seq,
            ..
        } = &mut runtime;
        let mut ctx = CombatCtx {
            world,
            content,
            grid,
            rng,
            fx: fx.as_ref(),
            audio: &audio,
            seq,
            spawned: &mut spawned,
        };
        for (mine, team) in pending.0 {
            let state = ctx
                .world
                .get::<crate::world::blocks::defense::shields::ShockMineState>(mine)
                .cloned();
            if let Some(state) = state {
                crate::world::blocks::defense::shields::shock_mine_trigger(
                    &mut ctx, mine, &state, team,
                );
            }
        }
        crate::weapons::update_weapons(&mut ctx);
    }
    world.insert_resource(audio);
    // Units spawned by `spawnUnit` bullets bypass `spawned` (no bullet entity).
    adopt_spawned_units(world, &mut runtime);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
}

/// Leg splash + crawler crush damage — the damage half of the kind movement
/// bodies, run from the live unit loop after [`movement::update_kinematics`].
///
/// * `LegsComp.java:196-202`: on a step (`l.group` change) a legged unit deals
///   `legSplashDamage * rules.unitDamage(team)` in `legSplashRange` at the
///   foot's base, units and buildings included (`false, true` = no armor
///   pierce, air hits).
/// * `CrawlComp.java:56-93`: while moving, every tile in the crawl circle whose
///   building belongs to another team takes `crushDamage * Time.delta *
///   rules.unitDamage(team)`. `t.build.damage` is `BuildingComp.damage(float)`
///   (`BuildingComp.java:2064-2088`: `rules.blockHealth`, the status/shield
///   guards and death), i.e. [`crate::combat::damage::area::apply_health`].
///
/// The kind modules keep only geometry/scan state (`entities/comp/unit/{legs,
/// crawl}.rs`); the damage pass needs the content registry and the live rules,
/// so it lives here (GAPS #10).
pub(super) fn apply_movement_damage(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
    legs_before: Option<&crate::entities::comp::unit::LegsComp>,
) {
    if world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .is_some_and(|status| status.disarmed)
    {
        return;
    }
    let Some(def) = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    let team = team_of(world, entity).unwrap_or(0);
    let unit_damage = crate::game::runtime::rules_of(world)
        .map(|rules| rules.unit_damage(team))
        .unwrap_or(1.0);

    if def.leg_splash_damage > 0.0
        && let Some(before) = legs_before
    {
        // A leg stepped this tick when its foot moved (`update_legs` snaps a
        // foot to its destination on a re-step).
        let stepped: Vec<(f32, f32)> = world
            .get::<crate::entities::comp::unit::LegsComp>(entity)
            .map(|after| {
                after
                    .legs
                    .iter()
                    .enumerate()
                    .filter(|(index, leg)| {
                        before.legs.get(*index).is_none_or(|old| {
                            (old.foot_x - leg.foot_x).abs() > 0.0001
                                || (old.foot_y - leg.foot_y).abs() > 0.0001
                        })
                    })
                    .map(|(_, leg)| (leg.base_x, leg.base_y))
                    .collect()
            })
            .unwrap_or_default();
        for (x, y) in stepped {
            crate::combat::damage::area::damage_area(
                world,
                content,
                Some(team),
                x,
                y,
                def.leg_splash_range,
                def.leg_splash_damage * unit_damage,
                crate::combat::damage::area::DamageOptions::default(),
            );
        }
    }

    if def.crush_damage > 0.0
        && world
            .get::<crate::entities::comp::unit::CrawlComp>(entity)
            .is_some()
    {
        if (delta.0 * delta.0 + delta.1 * delta.1).sqrt() <= 0.001 {
            return;
        }
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            return;
        };
        let hit_size = world
            .get::<crate::entities::comp::unit::comp::HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let tile = crate::config::TILESIZE as f32;
        // `int radius = (int)Math.max(0, hitSize / tilesize * 2f)`.
        let radius = (hit_size / tile * 2.0).max(0.0) as i32;
        let mut targets: Vec<Entity> = Vec::new();
        for cx in -radius..=radius {
            for cy in -radius..=radius {
                if cx * cx + cy * cy > radius {
                    continue;
                }
                let tx = WorldGrid::to_tile(pos.x + cx as f32 * tile);
                let ty = WorldGrid::to_tile(pos.y + cy as f32 * tile);
                let Some(build) = grid.tiles.getn(tx, ty).and_then(|t| t.build) else {
                    continue;
                };
                if team_of(world, build) != Some(team) {
                    targets.push(build);
                }
            }
        }
        // Overlapping tile samples can name the same building; damage it once.
        targets.sort_by_key(|entity| entity.index());
        targets.dedup();
        for build in targets {
            crate::combat::damage::area::apply_health(world, build, def.crush_damage * unit_damage);
        }
    }
}

/// Adopts units spawned by `spawnUnit` bullets (`BulletType.create` returns no
/// bullet for them, so the bullet drain cannot see them) into the live unit
/// list. Call after any pass that can create bullets; ordering is
/// `(EntitySeq, entity.index())`.
pub(super) fn adopt_spawned_units(world: &mut World, runtime: &mut UnitRuntime) {
    // T9: query only the `SpawnedByBullet` archetype instead of scanning every
    // entity in the world. Ordering is restored by the sort below.
    use bevy_ecs::query::With;
    let mut query = world
        .query_filtered::<(Entity, Option<&EntitySeq>), With<bullet::spawn::SpawnedByBullet>>();
    let found = sorted_by_seq(
        query
            .iter(world)
            .map(|(entity, seq)| (seq.map_or(u64::MAX, |seq| seq.0), entity)),
    );
    for entity in found {
        // `Logic`'s `UnitCreateEvent` (`Logic.java:228-235`) for payload/
        // ability spawns (H09 T10); the runtime is out of the world here, so
        // the type name comes from the local one.
        let name = unit_def(world, &runtime.content, entity).map(|def| def.name.clone());
        crate::game::stats::notify_unit_created(world, entity, name.as_deref());
        world
            .entity_mut(entity)
            .remove::<crate::combat::bullet::spawn::SpawnedByBullet>();
        runtime.units.push(entity);
        runtime.units_created += 1;
    }
}
