// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `EntitySet::UpdateUnits` system: per-unit AI dispatch, kind kinematics,
//! the weapon pass and the leg/crawl damage follow-up.
//! Upstream: core/src/mindustry/core/Logic.java (updateEntities)
//! core/src/mindustry/entities/comp/UnitComp.java
//! core/src/mindustry/entities/comp/LegsComp.java
//! core/src/mindustry/entities/comp/CrawlComp.java

use super::ai::defender::update_defender_ai;
use super::ai::flying::update_flying_ai;
use super::ai::follow::update_flying_follow_ai;
use super::ai::ground::{GroundAiRules, update_ground_ai};
use super::ai::hug::update_hug_ai;
use super::ai::{builder_falls_back, resolve_command_kind};
use super::ai_weapons::{
    AiWeaponCtx, AiWeaponsState, command_ai_weapon_pass, logic_ai_weapon_pass, update_ai_weapons,
};
use super::command_queue::pop_live_command_queue;
use super::deaths::has_core_of_team;
use super::physics::rebuild_avoidance;
use super::targeting::{entity_pos_within, face_target, unit_control_cost};
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
            // `Player` possession: the host-uploaded axis drives the unit
            // (`DesktopInput.updateMovement`); no target acquisition or pathing.
            crate::ai::types::player::update_player(world, &runtime.grid, &runtime.content, entity);
        } else if slot.kind == AiKind::Command {
            // `CommandAI.updateUnit` -> `defaultBehavior`/`commandController`
            // (T10a/T10b: MinerAI/RepairAI).
            let command_kind = resolve_command_kind(world, &runtime.content, entity, &slot);
            if command_kind == Some(crate::content::registries::commands::ControllerKind::Repair) {
                // `RepairAI.updateTargeting` replaces the enemy targeting: the
                // closest damaged friendly building, else `super` targeting.
                let mut repair = world
                    .get::<crate::ai::types::repair::RepairAiState>(entity)
                    .copied()
                    .unwrap_or_default();
                {
                    let UnitRuntime {
                        content,
                        grid,
                        pathfinder,
                        ..
                    } = &mut runtime;
                    let mut ctx = crate::ai::ai_controller::AiCtx {
                        world,
                        grid,
                        content,
                        pathfinder,
                        team,
                    };
                    crate::ai::types::repair::update_repair_ai(&mut ctx, entity, &mut repair);
                }
                if let Some(mut stored) =
                    world.get_mut::<crate::ai::types::repair::RepairAiState>(entity)
                {
                    *stored = repair;
                } else if world.get_entity(entity).is_ok() {
                    world.entity_mut(entity).insert(repair);
                }
            } else if matches!(
                command_kind,
                Some(crate::content::registries::commands::ControllerKind::Builder)
                    | Some(crate::content::registries::commands::ControllerKind::BuilderAssist)
            ) {
                // `UnitCommand.loadAll`: rebuild -> `BuilderAI()` (alwaysFlee
                // false, fleeRange 370); assist -> `BuilderAI{onlyAssist=true}`
                // (UnitCommand.java:73-116).
                let only_assist = command_kind
                    == Some(crate::content::registries::commands::ControllerKind::BuilderAssist);
                let mut builder = world
                    .get::<crate::ai::types::builder::BuilderAiState>(entity)
                    .copied()
                    .unwrap_or_else(|| {
                        let mut state =
                            crate::ai::types::builder::BuilderAiState::with_flee(false, 370.0);
                        state.only_assist = only_assist;
                        state
                    });
                {
                    let UnitRuntime {
                        content,
                        grid,
                        pathfinder,
                        ..
                    } = &mut runtime;
                    let mut ctx = crate::ai::ai_controller::AiCtx {
                        world,
                        grid,
                        content,
                        pathfinder,
                        team,
                    };
                    crate::ai::types::builder::update_builder_ai(&mut ctx, entity, &mut builder);
                }
                if let Some(mut stored) =
                    world.get_mut::<crate::ai::types::builder::BuilderAiState>(entity)
                {
                    *stored = builder;
                } else if world.get_entity(entity).is_ok() {
                    world.entity_mut(entity).insert(builder);
                }
                // `BuilderAI.shouldShoot()`: `!unit.isBuilding() && canAttack`
                // (BuilderAI.java:262-264); `isBuilding` is
                // `buildSpeedMultiplier > 0 && activelyBuilding` (UnitComp.java).
                let is_building = world
                    .get::<crate::sim::builder::BuilderPlans>(entity)
                    .is_some_and(|plans| plans.update_building && !plans.plans.is_empty());
                let can_attack = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .is_some_and(|def| def.can_attack);
                let _ = command_ai_weapon_pass(
                    world,
                    &runtime.content,
                    &queries,
                    entity,
                    team,
                    can_attack && !is_building,
                );
            } else {
                // Targeting runs for the weapons (`updateTargeting`, autoTarget);
                // movement only follows an explicit order (`targetPos`); with none
                // the unit idles in place instead of walking to the core anchor.
                let mount_target =
                    command_ai_weapon_pass(world, &runtime.content, &queries, entity, team, true);
                // `CommandAI.updateUnit`'s stance handling (T11): boost and
                // pursueTarget (edit `slot` before the movement match).
                let mut slot = slot;
                let stances = world
                    .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                    .copied()
                    .unwrap_or_default();
                let def_range = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .map(|def| def.max_range)
                    .unwrap_or(0.0);
                if stances.has_name(&runtime.content, "boost") {
                    // `shouldBoost()`: `updateBoosting(false)` while an attack
                    // target is in range, else true (CommandAI.java:150-163).
                    let in_range = mount_target
                        .is_some_and(|target| entity_pos_within(world, entity, target, def_range));
                    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
                        core.boosting = !in_range;
                    }
                }
                if slot.target.is_none()
                    && stances.has_name(&runtime.content, "pursuetarget")
                    && let Some(target) = mount_target
                    && let Some(pos) = world.get::<Pos>(target).copied()
                {
                    // `pursueTarget(target, true)`: `commandTarget(target, false)`.
                    slot.target = Some(TilePos::new(
                        crate::world::WorldGrid::to_tile(pos.x) as i16,
                        crate::world::WorldGrid::to_tile(pos.y) as i16,
                    ));
                }
                // `CommandAI.defaultBehavior` attack tracking (G01 T16):
                // `targetPos.set(attackTarget)` every tick, dead targets are
                // cleared, and the unit stops within `range - 10`.
                let mut move_target = slot.target;
                if let Some(target) = world
                    .get::<crate::ai::types::command::AttackTargetComp>(entity)
                    .and_then(|comp| comp.0)
                {
                    let alive = world.get_entity(target).is_ok()
                        && world
                            .get::<Health>(target)
                            .is_none_or(|health| !health.dead);
                    if !alive {
                        world
                            .entity_mut(entity)
                            .insert(crate::ai::types::command::AttackTargetComp(None));
                        move_target = None;
                        if let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                            stored.target = None;
                        }
                    } else if let Some(target_pos) = world.get::<Pos>(target).copied() {
                        let within = world.get::<Pos>(entity).copied().is_some_and(|pos| {
                            let dx = target_pos.x - pos.x;
                            let dy = target_pos.y - pos.y;
                            dx * dx + dy * dy <= (def_range - 10.0).max(0.0).powi(2)
                        });
                        move_target = if within {
                            None
                        } else {
                            Some(TilePos::new(
                                crate::world::WorldGrid::to_tile(target_pos.x) as i16,
                                crate::world::WorldGrid::to_tile(target_pos.y) as i16,
                            ))
                        };
                    }
                }
                if command_kind == Some(crate::content::registries::commands::ControllerKind::Miner)
                {
                    // `CommandAI.updateUnit`: the mine command auto-adds
                    // `mineAuto` when no item stance is set (T11).
                    let mut stances = world
                        .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                        .copied()
                        .unwrap_or_default();
                    if !stances.has_name(&runtime.content, "mineauto")
                        && !stances.any_item_stance(&runtime.content)
                        && let Some(def) = runtime.content.unit_stance_by_name("mineauto")
                    {
                        crate::ai::unit_stance_runtime::apply_stance(&mut stances.0, def);
                        world.entity_mut(entity).insert(stances);
                    }
                    let mut miner = world
                        .get::<crate::ai::types::miner::MinerAiState>(entity)
                        .copied()
                        .unwrap_or_default();
                    {
                        let UnitRuntime {
                            content,
                            grid,
                            pathfinder,
                            ..
                        } = &mut runtime;
                        let mut ctx = crate::ai::ai_controller::AiCtx {
                            world,
                            grid,
                            content,
                            pathfinder,
                            team,
                        };
                        crate::ai::types::miner::update_miner_ai(&mut ctx, entity, &mut miner);
                    }
                    if let Some(mut stored) =
                        world.get_mut::<crate::ai::types::miner::MinerAiState>(entity)
                    {
                        *stored = miner;
                    } else if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).insert(miner);
                    }
                } else {
                    if world
                        .get::<crate::entities::comp::unit::comp::MinerComp>(entity)
                        .is_some_and(|comp| comp.mine_tile.is_some())
                    {
                        // `CommandAI.command`: a non-mine command clears `mineTile`.
                        crate::entities::comp::unit::miner::clear_mine_tile(world, entity);
                    }
                    // The default `defaultBehavior` movement runs for commands
                    // without a controller (`None`: move/enterPayload/...) and for
                    // the not-yet-dispatched Repair/Builder controllers.
                    match move_target {
                        Some(target) => {
                            let flying = world
                                .get::<PhysicsComp>(entity)
                                .is_some_and(|physics| physics.flying);
                            // Refresh the lazy control path before borrowing
                            // fields (`unit.type.pathCostId`).
                            let cost = unit_control_cost(world, &runtime.content, entity);
                            runtime.control_path(cost);
                            let UnitRuntime {
                                grid,
                                content,
                                control_paths,
                                ..
                            } = &mut runtime;
                            let arrived = crate::ai::types::command::update_command_move(
                                world,
                                grid,
                                content,
                                &mut control_paths[usize::from(cost.id())],
                                entity,
                                target,
                                flying,
                            );
                            if arrived {
                                // `finishPath`: `targetPos = null`.
                                if let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                                    stored.target = None;
                                }
                                // Then the next queued waypoint becomes the
                                // target (`CommandAI.java:476-490`, I01 T13).
                                pop_live_command_queue(world, &runtime.grid, entity);
                            }
                        }
                        None => {
                            // Java `CommandAI.defaultBehavior` idles without
                            // touching `vel`; drag brings the unit to a stop.
                        }
                    }
                }
                // `CommandAI` -> `faceTarget()` (CommandAI.java:400/420).
                face_target(world, &runtime.content, entity, mount_target);
            }
        } else if slot.kind == AiKind::Missile {
            // `MissileAI.updateMovement` (live dispatch, PAR-0088): steer at
            // the shooter's aim, always move forward, die on enemy-building
            // contact or at the `TimedKill` lifetime.
            let shooter = world
                .get::<MissileShooter>(entity)
                .map(|shooter| shooter.0)
                .unwrap_or(None);
            // `AIController.target` snapshot from the shot's aimed entity
            // (`BulletType.create` `target`; `MissileAI.java:33-36`).
            let target = world
                .get::<crate::ai::types::missile::MissileTarget>(entity)
                .map(|target| target.0)
                .unwrap_or(None);
            let mut state = MissileAi {
                unit: Some(entity),
                target,
                shooter,
            };
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                let _ = update_missile(&mut ctx, entity, &mut state);
            }
            if let Some(mut stored) = world.get_mut::<MissileShooter>(entity) {
                stored.0 = state.shooter;
            }
            // `AIController.updateTargeting` -> `updateWeapons`
            // (AIController.java:114-116, 172-238): a controllable weapon finds
            // its own target within `weapon.range()`. For the scathe missile the
            // shoot-on-death explosion is a proximity fuse against ground units
            // (`findTarget(mountX, mountY, wrange, bullet.collidesAir,
            // bullet.collidesGround)`).
            let mount_target = {
                let def = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id));
                def.and_then(|def| {
                    let weapon = def.weapons.first()?;
                    let range = weapon.range();
                    let (air, ground) = runtime
                        .content
                        .bullet(weapon.bullet.id)
                        .map(|bullet| (bullet.collides_air, bullet.collides_ground))
                        .unwrap_or((false, true));
                    let (x, y) = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y))?;
                    queries.closest_target(
                        world,
                        team,
                        x,
                        y,
                        range,
                        air,
                        ground,
                        true,
                        def.target_under_blocks,
                    )
                })
            };
            update_ai_weapons(
                world,
                &runtime.content,
                &queries,
                entity,
                team,
                AiWeaponCtx {
                    main_target: mount_target,
                    retarget: true,
                    flying_find: false,
                    core_override: None,
                    should_shoot: true,
                    ..Default::default()
                },
            );
        } else if slot.kind == AiKind::Suicide {
            // `SuicideAI.updateUnit` (`ai/types/suicide.rs`): ram the nearest
            // hostile in a straight line (terrain ignored) and fire the
            // close-range death weapon (`shootCone 180`).
            let mount_target = {
                let pos = world.get::<Pos>(entity).copied();
                let def = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id));
                match (pos, def) {
                    (Some(pos), Some(def)) => queries.closest_target(
                        world,
                        team,
                        pos.x,
                        pos.y,
                        def.max_range.max(def.range),
                        def.target_air,
                        def.target_ground,
                        true,
                        def.target_under_blocks,
                    ),
                    _ => None,
                }
            };
            if let Some(mut state) = world.get_mut::<AiWeaponsState>(entity) {
                state.main_target = mount_target;
                state.retarget = 0.0;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(AiWeaponsState {
                    main_target: mount_target,
                    retarget: 0.0,
                    ..AiWeaponsState::default()
                });
            }
            update_ai_weapons(
                world,
                &runtime.content,
                &queries,
                entity,
                team,
                AiWeaponCtx {
                    main_target: mount_target,
                    retarget: false,
                    flying_find: false,
                    core_override: None,
                    should_shoot: true,
                    ..Default::default()
                },
            );
            let mut state = crate::ai::types::suicide::SuicideAi {
                unit: Some(entity),
                target: mount_target,
                suicidal: false,
            };
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                let _ = crate::ai::types::suicide::update_suicide(&mut ctx, entity, &mut state);
            }
        } else if slot.kind == AiKind::Builder && !builder_falls_back(world, team) {
            // `BuilderAI` (T10c): pull ghost plans from the team queue and
            // move toward the active plan; `Sim::update_builders` executes it.
            let mut state = world
                .get::<crate::ai::types::builder::BuilderAiState>(entity)
                .copied()
                .unwrap_or_else(|| {
                    // AI-team core builders construct with
                    // `BuilderAI(true, coreFleeRange)` (`UnitTypes.java:2495`,
                    // 4352; the def carries the range).
                    let core_flee_range = world
                        .get::<UnitTypeComp>(entity)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .and_then(|def| match def.controller {
                            crate::content::registries::units::ControllerKind::Builder {
                                core_flee_range,
                            } => Some(core_flee_range),
                            _ => None,
                        })
                        .unwrap_or(400.0);
                    crate::ai::types::builder::BuilderAiState::with_flee(true, core_flee_range)
                });
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::builder::update_builder_ai(&mut ctx, entity, &mut state);
            }
            if let Some(mut stored) =
                world.get_mut::<crate::ai::types::builder::BuilderAiState>(entity)
            {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
            let _ = command_ai_weapon_pass(world, &runtime.content, &queries, entity, team, true);
        } else if slot.kind == AiKind::Boost {
            // `BoostAI.defaultBehavior` (`ai/types/boost.rs`): enable boost
            // without issuing movement; the lift-off runs in `update_kinematics`
            // (`UnitComp.updateBoosting`).
            let mut state = crate::ai::types::boost::BoostAi {
                unit: Some(entity),
                boosting: true,
            };
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::boost::update_boost(&mut ctx, entity, &mut state);
            }
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
            // `CargoAI` (`CargoAI.java:26-97`, H01 T13): the tether unit
            // shuttles items between its loader and a configured unload point.
            let mut state = world
                .get::<crate::ai::types::cargo::CargoAi>(entity)
                .copied()
                .unwrap_or_default();
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                let _ = crate::ai::types::cargo::update_cargo(&mut ctx, entity, &mut state);
            }
            if let Some(mut stored) = world.get_mut::<crate::ai::types::cargo::CargoAi>(entity) {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
        } else if slot.kind == AiKind::Assembler {
            // `AssemblerAI` (`AssemblerAI.java:11-24`, H01 T13): hold at the
            // arranged spot and face the locked angle.
            let state = world
                .get::<crate::ai::types::assembler::AssemblerAi>(entity)
                .copied()
                .unwrap_or_default();
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::assembler::update_assembler(&mut ctx, entity, &state);
            }
            if let Some(mut stored) =
                world.get_mut::<crate::ai::types::assembler::AssemblerAi>(entity)
            {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
        } else if slot.kind == AiKind::Logic {
            // `LogicAI` (T1): `AIController.updateUnit` runs
            // `updateTargeting()` (weapons) before `updateMovement()`.
            //
            // A bare `Logic` slot without the component (only reachable through
            // a decoded fixture that sets the kind directly; Java cannot reach
            // this state) is left inert.
            if let Some(mut state) = world
                .get::<crate::ai::types::logic::LogicAi>(entity)
                .cloned()
            {
                let can_boost = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .is_some_and(|def| def.can_boost);
                let should_shoot = state.shoot && !(can_boost && state.boost);
                let explicit = matches!(
                    state.aim_control,
                    crate::logic::enums::LUnitControl::Target
                        | crate::logic::enums::LUnitControl::Targetp
                );
                let pos_target = (state.aim_control == crate::logic::enums::LUnitControl::Target)
                    .then_some(state.pos_target);
                let main_target = if state.aim_control == crate::logic::enums::LUnitControl::Targetp
                {
                    state.main_target
                } else {
                    None
                };
                logic_ai_weapon_pass(
                    world,
                    &runtime.content,
                    &queries,
                    entity,
                    team,
                    main_target,
                    pos_target,
                    should_shoot,
                    explicit,
                );
                let alive = {
                    // `controlPath.getPathPosition(unit, ...)` reads
                    // `unit.type.pathCostId` (ControlPathfinder.java:1137-1138).
                    let cost = unit_control_cost(world, &runtime.content, entity);
                    runtime.control_path(cost);
                    let UnitRuntime {
                        content,
                        grid,
                        pathfinder,
                        control_paths,
                        ..
                    } = &mut runtime;
                    let control_path = &mut control_paths[usize::from(cost.id())];
                    let mut ctx = crate::ai::ai_controller::AiCtx {
                        world,
                        grid,
                        content,
                        pathfinder,
                        team,
                    };
                    crate::ai::types::logic::update_logic(
                        &mut ctx,
                        entity,
                        &mut state,
                        control_path,
                    )
                };
                if alive {
                    if let Some(mut stored) =
                        world.get_mut::<crate::ai::types::logic::LogicAi>(entity)
                    {
                        *stored = state;
                    } else if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).insert(state);
                    }
                } else {
                    // `unit.resetController()` (the unit loop holds the
                    // resolved def; `reset_controller`'s `with_content` cannot
                    // see the `UnitRuntime` removed by this system).
                    let def = world
                        .get::<UnitTypeComp>(entity)
                        .and_then(|comp| runtime.content.unit(comp.type_id));
                    crate::logic::executor::unit_control::reset_controller_def(
                        world, entity, def, team,
                    );
                }
            }
        } else if slot.kind == AiKind::NoAi {
            // `NoAI.updateUnit(){}`: the dummy never runs a controller (the
            // kinematics/environment tail still runs, as in Java `Unit.update`).
        } else {
            let flying = slot.kind == AiKind::Flying
                || world
                    .get::<PhysicsComp>(entity)
                    .is_some_and(|physics| physics.flying);
            if flying {
                // `FlyingAI` (FlyingAI.java:16-72): main target via
                // `findMainTarget` (core in range, else `targetFlags`, else the
                // core), circle/close on it, shuttle to the spawner when idle
                // on the default team.
                update_flying_ai(
                    world,
                    &mut runtime,
                    &queries,
                    entity,
                    team,
                    slot,
                    ground_rules,
                );
            } else {
                // `GroundAI.updateMovement` (GroundAI.java:21-85): field walk to
                // the enemy-core set with the Java targeting cadence.
                update_ground_ai(
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
