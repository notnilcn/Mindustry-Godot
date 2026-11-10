// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live unit and building death resolution plus the explosion terms and
//! core lookup the destroy path feeds.
//! Upstream: core/src/mindustry/entities/comp/UnitComp.java (killed/destroy),
//! core/src/mindustry/entities/comp/BuildingComp.java (onDestroyed, explosionItemCap)

use super::*;

/// `UnitComp.destroy()` shoot-on-death half (`UnitComp.java:910-919`): every
/// mount whose weapon has `shootOnDeath` and is not gated by
/// `killShooter && totalShots > 0` fires once before the unit is removed.
pub(super) fn run_shoot_on_death(world: &mut World, runtime: &mut UnitRuntime, entity: Entity) {
    sync_weapon_state(world, entity);
    let state = world
        .get::<crate::weapons::UnitState>(entity)
        .copied()
        .unwrap_or_default();
    let Some(mut weapons) = world.entity_mut(entity).take::<UnitWeapons>() else {
        return;
    };
    let UnitRuntime {
        content,
        grid,
        rng,
        fx,
        seq,
        ..
    } = runtime;
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    let mut spawned: Vec<Entity> = Vec::new();
    {
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
        for i in 0..weapons.weapons.len() {
            if i >= weapons.mounts.len() {
                break;
            }
            if !weapons.weapons[i].shoot_on_death {
                continue;
            }
            if weapons.weapons[i].bullet.kill_shooter && weapons.mounts[i].total_shots > 0 {
                continue;
            }
            weapons.mounts[i].reload = 0.0;
            weapons.mounts[i].shoot = true;
            crate::weapons::update_weapon(
                &mut ctx,
                entity,
                &state,
                &weapons.weapons,
                &mut weapons.mounts,
                &mut weapons.pending,
                i,
            );
        }
    }
    world.insert_resource(audio);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.entity_mut(entity).insert(weapons);
}

impl Sim {
    /// Death resolution after the entity pass (`HealthComp.damage` ->
    /// `kill()`): live units at zero health are killed through their death
    /// abilities (`UnitComp.killed`/`destroy`), and buildings at zero health
    /// are destroyed through the tile ops (`Building.kill` ->
    /// `Call.buildDestroyed`). Returns `(units, buildings)` removed.
    ///
    /// Without this pass damage only drained `Health` — nothing ever died in
    /// the live sim (PAR-0141 / PAR-0086).
    pub fn resolve_deaths(&mut self) -> (usize, usize) {
        let mut killed_units = 0usize;
        if let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() {
            let world = &mut self.ecs.0;
            let dead: Vec<Entity> = runtime
                .units
                .iter()
                .copied()
                .filter(|entity| {
                    world
                        .get::<Health>(*entity)
                        .is_some_and(|h| h.dead || (h.max_health > 0.0 && h.health <= 0.0))
                })
                .collect();
            for entity in dead {
                // `UnitComp.killed` (`UnitComp.java:980-991`): a dead flying
                // unit with `createWreck` (elevation != 0) is not destroyed
                // immediately — it falls as a `Falling` wreck and only runs
                // the destroy body once grounded or overkilled
                // (`UnitComp.update`, UnitComp.java:803-830).
                if let Some(def) = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .filter(|def| def.flying && def.create_wreck)
                {
                    let elevation = world
                        .get::<UnitCore>(entity)
                        .map_or(0.0, |core| core.elevation);
                    let health_now = world.get::<Health>(entity).map_or(0.0, |h| h.health);
                    let falling = world.get::<Falling>(entity).is_some();
                    if !falling && elevation >= 0.001 {
                        // `killed()`: `health = Math.min(health, 0); dead = true;`
                        if let Some(mut health) = world.get_mut::<Health>(entity) {
                            health.dead = true;
                            health.health = health.health.min(0.0);
                        }
                        world.entity_mut(entity).insert(Falling);
                        continue;
                    }
                    if falling
                        && elevation >= 0.001
                        && health_now > -def.health * def.wreck_health_multiplier
                    {
                        // Still falling (`Call.unitDestroy` not reached yet).
                        continue;
                    }
                }
                // `UnitComp.destroy()` (UnitComp.java:885-891): a unit carrying
                // an explosive item blows up (G01 T14).
                {
                    let stack = world
                        .get::<crate::entities::comp::unit::comp::ItemsComp>(entity)
                        .and_then(|comp| comp.item);
                    let core_spawned = world
                        .get::<UnitCore>(entity)
                        .is_some_and(|core| core.spawned_by_core);
                    if let Some((item, amount)) = stack
                        && let Some(item_def) = runtime.content.item(item)
                        && !core_spawned
                    {
                        let pos = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
                        let def = world
                            .get::<UnitTypeComp>(entity)
                            .and_then(|comp| runtime.content.unit(comp.type_id))
                            .cloned();
                        if let (Some((x, y)), Some(def)) = (pos, def) {
                            let explosiveness = 2.0 + item_def.explosiveness * amount as f32 * 1.53;
                            let flammability = item_def.flammability * amount as f32 / 1.9;
                            let power = item_def.charge * (amount as f32).powf(1.11) * 160.0;
                            let team = world
                                .get::<TeamComp>(entity)
                                .map(|team| team.team)
                                .unwrap_or(0);
                            let (damage, fire) = world
                                .get_resource::<CampaignRuntime>()
                                .map(|campaign| {
                                    let rules = &campaign.session.rules;
                                    (
                                        rules.damage_explosions
                                            && rules.unit_crash_damage(team) > 0.0,
                                        item_def.flammability > 1.0,
                                    )
                                })
                                .unwrap_or((true, item_def.flammability > 1.0));
                            let radius = (def.hit_size * 2.0 + def.leg_length / 1.7) / 2.0;
                            // `UnitComp.destroy` -> `Damage.dynamicExplosion`
                            // with a live `CombatCtx` (spawned fireballs/bolts
                            // join the runtime's bullet list).
                            let audio = world
                                .remove_resource::<crate::audio::AudioSinkRes>()
                                .unwrap_or_else(crate::audio::AudioSinkRes::noop);
                            let mut spawned: Vec<Entity> = Vec::new();
                            {
                                let mut ctx = crate::combat::bullet::CombatCtx {
                                    world: &mut *world,
                                    content: &runtime.content,
                                    grid: &runtime.grid,
                                    rng: &mut runtime.rng,
                                    fx: runtime.fx.as_ref(),
                                    audio: &audio,
                                    seq: &mut runtime.seq,
                                    spawned: &mut spawned,
                                };
                                crate::combat::damage::explosion::dynamic_explosion(
                                    &mut ctx,
                                    x,
                                    y,
                                    flammability,
                                    explosiveness,
                                    power,
                                    radius,
                                    damage,
                                    fire,
                                    Some(team),
                                );
                            }
                            runtime.bullets_created += spawned.len() as u64;
                            runtime.bullets.extend(spawned);
                            world.insert_resource(audio);
                        }
                    }
                }
                // `UnitComp.destroy()` fires `shootOnDeath` mounts (PAR-0088);
                // spawned bullets join the live list.
                run_shoot_on_death(world, &mut runtime, entity);
                // `UnitComp.destroy` crash damage (`UnitComp.java:921-928`): a
                // flying unit with `createWreck`, not spawned by a core, crashes
                // with area damage when `rules.unitCrashDamage(team) > 0`.
                {
                    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
                    let pos = world.get::<Pos>(entity).map(|p| (p.x, p.y));
                    let def = world
                        .get::<UnitTypeComp>(entity)
                        .and_then(|comp| runtime.content.unit(comp.type_id));
                    if let (Some((x, y)), Some(def)) = (pos, def) {
                        let rules = world
                            .get_resource::<CampaignRuntime>()
                            .map(|runtime| runtime.session.rules.clone())
                            .unwrap_or_default();
                        let crash = rules.unit_crash_damage(team);
                        let core_spawned = world
                            .get::<UnitCore>(entity)
                            .is_some_and(|core| core.spawned_by_core);
                        if def.flying && !core_spawned && def.create_wreck && crash > 0.0 {
                            let damage =
                                def.hit_size.powf(0.75) * def.crash_damage_multiplier * 2.5 * crash;
                            let radius = def.hit_size.powf(0.94) * 1.25;
                            // `shields.isEmpty() || !shields.contains(s ->
                            // s.absorbExplosion(x, y, crashDamage))`
                            // (UnitComp.java:924-930; G01 T14).
                            if !crate::combat::damage::explosion::explosion_absorbed(
                                world,
                                x,
                                y,
                                damage,
                                Some(team),
                            ) {
                                let _ = crate::combat::damage::area::damage_area(
                                    world,
                                    &runtime.content,
                                    Some(team),
                                    x,
                                    y,
                                    radius,
                                    damage,
                                    crate::combat::damage::area::DamageOptions::default(),
                                );
                            }
                        }
                    }
                }
                let UnitRuntime {
                    content,
                    grid,
                    rng,
                    fx,
                    seq,
                    ..
                } = &mut runtime;
                let before = *seq;
                // Read the team, type and pose before the kill despawns the
                // entity (`Logic.java:218-226` needs team/name for stats; the
                // wreck decals need name/pose, `UnitComp.java:932-941`).
                let unit_team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
                let unit_def = world
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
                    .and_then(|comp| content.unit(comp.type_id))
                    .cloned();
                let unit_name = unit_def.as_ref().map(|def| def.name.clone());
                let wreck: Option<WreckDecalInput> = unit_def.as_ref().map(|def| WreckDecalInput {
                    name: def.name.clone(),
                    hit_size: def.hit_size,
                    create_scorch: def.create_scorch,
                    x: world.get::<Pos>(entity).map(|p| p.x).unwrap_or(0.0),
                    y: world.get::<Pos>(entity).map(|p| p.y).unwrap_or(0.0),
                    rotation: world
                        .get::<UnitCore>(entity)
                        .map(|c| c.rotation)
                        .unwrap_or(0.0),
                    seed: world
                        .get::<crate::ecs::EntitySeq>(entity)
                        .map(|seq| seq.0)
                        .unwrap_or(0),
                });
                let fx_sink = fx.as_ref();
                if crate::entities::comp::unit::lifecycle::kill_unit_with_abilities(
                    world,
                    content,
                    &grid.tiles,
                    rng,
                    seq,
                    entity,
                ) {
                    // `UnitComp.java:932-941`: wreck decals (view-only, gated
                    // on `createScorch`).
                    if let Some(wreck) = &wreck
                        && wreck.create_scorch
                    {
                        crate::entities::comp::unit::lifecycle::emit_wreck_decals(
                            fx_sink,
                            &wreck.name,
                            wreck.hit_size,
                            wreck.x,
                            wreck.y,
                            wreck.rotation,
                            wreck.seed ^ 0x9e37_79b9_7f4a_7c15,
                        );
                    }
                    killed_units += 1;
                    runtime.units_removed += 1;
                    // `Logic` `UnitDestroyEvent` (`Logic.java:218-226`): a
                    // killed unit of a non-default team counts as an enemy
                    // kill; the campaign planet maps count every death by type
                    // name (H09 T10).
                    if let Some(mut campaign) =
                        world.get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                    {
                        let default_team = campaign.session.rules.default_team;
                        if unit_team != default_team {
                            campaign.session.stats.enemy_units_destroyed += 1;
                        }
                        if campaign.session.is_campaign()
                            && let Some((planet, _)) = campaign.session.sector
                            && let Some(name) = unit_name.as_deref()
                            && let Some(stats) = campaign.campaign.stats_for_mut(planet)
                        {
                            if unit_team != default_team {
                                stats.add_enemy_unit_destroyed(name, 1);
                            } else {
                                stats.add_unit_destroyed(name, 1);
                            }
                        }
                    }
                    // `FogControl.UnitDestroyEvent` (`FogControl.java:91-95`):
                    // a dead unit's team needs a dynamic redraw.
                    if let Some(mut fog) = world.get_resource_mut::<crate::game::fog::FogControl>()
                    {
                        fog.mark_dynamic(unit_team);
                    }
                }
                // Death-ability spawns (`SpawnDeathAbility`) join the live list.
                if runtime.seq != before {
                    let spawned: Vec<Entity> = world
                        .iter_entities()
                        .filter(|e| {
                            e.get::<EntitySeq>()
                                .is_some_and(|s| s.0 >= before && s.0 < runtime.seq)
                                && e.get::<UnitTypeComp>().is_some()
                        })
                        .map(|e| e.id())
                        .collect();
                    runtime.units_created += spawned.len() as u64;
                    runtime.units.extend(spawned);
                }
            }
            runtime
                .units
                .retain(|entity| world.get_entity(*entity).is_ok());
            world.insert_non_send(runtime);
        }

        let mut destroyed = 0usize;
        if self.block_runtime.is_some() {
            let mut dead: Vec<(u64, Entity)> = self
                .ecs
                .0
                .iter_entities()
                .filter(|e| {
                    e.get::<Building>().is_some()
                        && e.get::<Health>()
                            .is_some_and(|h| h.dead || (h.max_health > 0.0 && h.health <= 0.0))
                })
                .map(|e| (e.get::<EntitySeq>().map_or(u64::MAX, |s| s.0), e.id()))
                .collect();
            dead.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));
            for (_, entity) in dead {
                // `BuildingComp.onDestroyed` -> `splashLiquid`: a liquid
                // container spills `(amount/4).clamp(0,10)` per deposit, up to
                // `(amount/5).clamp(0,30)` deposits scattered over the block
                // footprint (`rules.damageExplosions` gate).
                let damage_explosions = self
                    .ecs
                    .0
                    .get_resource::<CampaignRuntime>()
                    .map(|campaign| campaign.session.rules.damage_explosions)
                    .unwrap_or(true);
                let spill: Vec<(crate::content::LiquidId, f32)> = if damage_explosions {
                    self.ecs
                        .0
                        .get::<crate::world::modules::LiquidModule>(entity)
                        .map(|module| {
                            module
                                .liquids
                                .iter()
                                .enumerate()
                                .filter(|(_, amount)| **amount > 0.0)
                                .map(|(id, amount)| {
                                    (crate::content::LiquidId::new(id as u16), *amount)
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let (center, size) = self
                    .ecs
                    .0
                    .get::<crate::entities::comp::Building>(entity)
                    .map(|building| {
                        let size = self
                            .ecs
                            .0
                            .get_resource::<crate::world::BlockTable>()
                            .and_then(|table| table.instance(building.block))
                            .map(|instance| instance.def.size)
                            .unwrap_or(1);
                        (
                            crate::world::BuildHarness::tile_center(
                                building.tile.x() as i32,
                                building.tile.y() as i32,
                            ),
                            size,
                        )
                    })
                    .unwrap_or(((0.0, 0.0), 1));
                // `Logic`'s `BlockDestroyEvent` handler: a rebuildable dead
                // block appends a ghost plan to its team's queue
                // (`tile.build.addPlan(true)`, Logic.java:42-50;
                // `BuildingComp.addPlan`, BuildingComp.java:335-374).
                let plan = self
                    .ecs
                    .0
                    .get::<crate::entities::comp::Building>(entity)
                    .map(|building| (building.tile, building.rotation as i8, building.block));
                // `BuildingComp.onDestroyed` explosion terms
                // (BuildingComp.java:1491-1508), read before the modules are
                // removed with the building.
                let explosion_terms = plan
                    .and_then(|(_, _, block)| {
                        let content = &self.unit_runtime()?.content;
                        let def = content.block(block)?;
                        Some(building_explosion_terms(&self.ecs.0, content, entity, def))
                    })
                    .unwrap_or((0.0, 0.0, 0.0));
                let ghost = {
                    let rebuildable = plan
                        .and_then(|(_, _, block)| {
                            self.unit_runtime()
                                .and_then(|runtime| runtime.content.block(block))
                                .map(|def| def.rebuildable)
                        })
                        .unwrap_or(true);
                    let ghost_blocks = self
                        .ecs
                        .0
                        .get_resource::<CampaignRuntime>()
                        .map(|campaign| campaign.session.rules.ghost_blocks)
                        .unwrap_or(true);
                    ghost_blocks && rebuildable
                };
                let team = self
                    .ecs
                    .0
                    .get::<TeamComp>(entity)
                    .map(|team| team.team)
                    .unwrap_or(0);
                // `Logic` `BlockDestroyEvent` (`Logic.java:182-186`): a dead AI
                // core with `coreDestroyClear` queues `TeamData.timeDestroy`
                // around it; `Sim::apply_campaign_world_requests` applies it
                // after this tick's schedule (`Core.app.post`).
                if self
                    .ecs
                    .0
                    .get::<crate::world::blocks::storage::core_block::CoreBuild>(entity)
                    .is_some()
                    && let Some(mut campaign) = self.ecs.0.get_resource_mut::<CampaignRuntime>()
                {
                    let rules = campaign.session.rules.clone();
                    if rules.core_destroy_clear
                        && team != rules.default_team
                        && rules.team_is_ai(team, campaign.session.is_campaign())
                    {
                        campaign.pending_time_destroy.push((
                            team,
                            center.0,
                            center.1,
                            rules.enemy_core_build_radius,
                        ));
                    }
                }
                let Some(runtime) = self.block_runtime.as_mut() else {
                    break;
                };
                if let Some((x, y, block)) =
                    runtime.destroy_building(&mut self.ecs.0, &mut self.grid, entity)
                {
                    if ghost
                        && let Some((tile, rotation, plan_block)) = plan
                        && let Some(mut campaign) = self.ecs.0.get_resource_mut::<CampaignRuntime>()
                    {
                        let data = campaign.session.teams.get(crate::ecs::TeamId(team));
                        // `checkPrevious`: drop existing plans on this tile.
                        data.plans
                            .retain(|plan| plan.x != tile.x() || plan.y != tile.y());
                        data.plans.push_front(crate::game::teams::BlockPlan {
                            x: tile.x(),
                            y: tile.y(),
                            rotation,
                            block: plan_block,
                            config: None,
                            removed: false,
                        });
                    }
                    destroyed += 1;
                    log::debug!("building destroyed at ({x}, {y}) block {}", block.raw());
                    self.push_event(crate::event::SimEvent::BlockBrokenEvent(
                        crate::event::BlockBrokenEvent {
                            x,
                            y,
                            block,
                            // `BuildingComp.onDestroyed` (destroy sound path).
                            destroyed: true,
                            team,
                        },
                    ));
                    // `BuildingComp.onDestroyed` -> `playDestroySound` + the
                    // explosion extras, with the CoreBlock game-over override
                    // (`BuildingComp.java:1524-1543`, `CoreBlock.java:708-716`).
                    self.emit_building_destroyed(block, x, y, team, explosion_terms);
                    if !spill.is_empty()
                        && let Some(mut unit_runtime) = self.ecs.0.remove_non_send::<UnitRuntime>()
                    {
                        let spread = size as f32 * 4.0;
                        for (liquid, amount) in &spill {
                            let splash = (amount / 4.0).clamp(0.0, 10.0);
                            // Java loops `for(int i = 0; i < clamp(amount / 5, 0, 30); i++)`
                            // over a float bound, i.e. `ceil` deposits.
                            let count = (amount / 5.0).clamp(0.0, 30.0).ceil() as i32;
                            for _ in 0..count {
                                let px = center.0
                                    + unit_runtime.rng.range(
                                        crate::determinism::RngStream::Sim,
                                        -spread,
                                        spread,
                                    );
                                let py = center.1
                                    + unit_runtime.rng.range(
                                        crate::determinism::RngStream::Sim,
                                        -spread,
                                        spread,
                                    );
                                let _ = crate::combat::puddles::deposit(
                                    &mut self.ecs.0,
                                    &self.grid.tiles,
                                    &unit_runtime.content,
                                    crate::world::WorldGrid::to_tile(px) as i16,
                                    crate::world::WorldGrid::to_tile(py) as i16,
                                    *liquid,
                                    splash,
                                    &mut unit_runtime.rng,
                                );
                            }
                        }
                        self.ecs.0.insert_non_send(unit_runtime);
                    }
                    // `BuildingComp.onDestroyed` -> `Damage.dynamicExplosion`
                    // (BuildingComp.java:1518; fire comes from the 8-arg
                    // overload, Damage.java:78-82). The scales apply only here
                    // (`flammability * block.flammabilityScale`,
                    // `explosiveness * 3.5f * block.explosivenessScale`); the
                    // destroy sounds above use the raw terms.
                    if damage_explosions {
                        let (flammability, explosiveness, power) = explosion_terms;
                        let (flammability, explosiveness) = plan
                            .and_then(|(_, _, block)| {
                                let content = &self.unit_runtime()?.content;
                                let def = content.block(block)?;
                                Some((
                                    flammability * def.flammability_scale,
                                    explosiveness * 3.5 * def.explosiveness_scale,
                                ))
                            })
                            .unwrap_or((flammability, explosiveness * 3.5));
                        self.with_combat_ctx(|ctx| {
                            crate::combat::damage::explosion::dynamic_explosion(
                                ctx,
                                center.0,
                                center.1,
                                flammability,
                                explosiveness,
                                power,
                                size as f32 * crate::config::TILESIZE as f32 / 2.0,
                                true,
                                true,
                                None,
                            );
                        });
                    }
                }
            }
            if destroyed > 0 {
                self.refresh_unit_runtime();
            }
        }
        (killed_units, destroyed)
    }

    /// `BuildingComp.playDestroySound` + the `onDestroyed` explosion extras
    /// (`BuildingComp.java:1524-1543`). The `CoreBlock.playDestroySound`
    /// override plays the last player core's death non-positionally at
    /// `volume * sfxVolume` (`CoreBlock.java:708-716`; the sim stores the raw
    /// volume — the client applies `sfxvol`).
    pub(crate) fn emit_building_destroyed(
        &mut self,
        block: crate::content::BlockId,
        x: i16,
        y: i16,
        team: u8,
        explosion_terms: (f32, f32, f32),
    ) {
        let Some(audio) = self
            .ecs
            .0
            .get_resource::<crate::audio::AudioSinkRes>()
            .cloned()
        else {
            return;
        };
        let Some(content) = self.block_runtime.as_ref().map(|runtime| runtime.content()) else {
            return;
        };
        let Some(def) = content.block(block) else {
            return;
        };
        let (cx, cy) = crate::audio::blocks::world_center(def, x, y);
        let (candidates, volume) = crate::audio::blocks::destroy_sound(def);
        // `CoreBlock.java:712`: the player's last core with `canGameOver` plays
        // `destroySound.play(volume * sfxVolume, random(pitch), 0)`.
        let last_player_core = def.kind == BlockKind::CoreBlock
            && self
                .ecs
                .0
                .get_resource::<CampaignRuntime>()
                .is_some_and(|campaign| {
                    campaign.session.rules.can_game_over
                        && campaign.session.rules.default_team == team
                })
            && !has_core_of_team(&self.ecs.0, content, team);
        if last_player_core {
            audio.emit(crate::audio::AudioEvent::play(
                crate::audio::SoundId::EXPLOSION_CORE,
                1.6,
                1.0,
                0.0,
                false,
                false,
            ));
        } else {
            let sound = crate::audio::sim::pick_random_sound(
                &mut crate::audio::sim::FxAudioRng {
                    rng: &mut self.rng_streams,
                },
                candidates,
            );
            // `destroyPitchMin == destroyPitchMax == 1` for every vanilla block
            // (`Block.java:340`).
            audio.emit(crate::audio::AudioEvent::at(
                sound, cx, cy, 1.0, volume, true,
            ));
        }
        let (flammability, explosiveness, power) = explosion_terms;
        if explosiveness > 40.0 {
            let sound = if self
                .rng_streams
                .next_float(crate::determinism::RngStream::Fx)
                < 0.5
            {
                crate::audio::SoundId::BLOCK_EXPLODE_EXPLOSIVE
            } else {
                crate::audio::SoundId::BLOCK_EXPLODE_EXPLOSIVE_ALT
            };
            audio.emit(crate::audio::AudioEvent::at(
                sound, cx, cy, 1.0, volume, true,
            ));
        } else if flammability > 5.0 {
            audio.emit(crate::audio::AudioEvent::at(
                crate::audio::SoundId::BLOCK_EXPLODE_FLAMMABLE,
                cx,
                cy,
                1.0,
                volume,
                true,
            ));
        }
        if power > 30_000.0 {
            audio.emit(crate::audio::AudioEvent::at(
                crate::audio::SoundId::BLOCK_EXPLODE_ELECTRIC_BIG,
                cx,
                cy,
                1.0,
                volume,
                true,
            ));
        } else if power > 2_000.0 {
            audio.emit(crate::audio::AudioEvent::at(
                crate::audio::SoundId::BLOCK_EXPLODE_ELECTRIC,
                cx,
                cy,
                1.0,
                volume,
                true,
            ));
        }
    }
}

/// `EntitySet::UpdateUnits` system: `GroundAI`/`FlyingAI` target + move,
/// kind kinematics and the weapon pass (`Groups.unit.update()`).
///
/// No-op unless the unit runtime is installed, so the P0 path is untouched.
/// Any live building of `team` whose block kind is `CoreBlock`
/// (`TeamData.cores`; used by the `CoreBlock.playDestroySound` override).
pub(super) fn has_core_of_team(world: &World, content: &ContentRegistry, team: u8) -> bool {
    world.iter_entities().any(|entity| {
        team_of(world, entity.id()) == Some(team)
            && entity
                .get::<crate::entities::comp::Building>()
                .and_then(|building| content.block(building.block))
                .is_some_and(|def| def.kind == BlockKind::CoreBlock)
    })
}

/// `BuildingComp.onDestroyed` explosion terms (`BuildingComp.java:1491-1508`):
/// `BuildingComp.explosionItemCap()` (`BuildingComp.java:1463-1465`): a
/// storage block linked to a core caps at `min(itemCapacity / 60, 6)`
/// (`StorageBlock.java:99-103`).
pub(super) fn explosion_item_cap(
    world: &World,
    entity: Entity,
    def: &crate::content::registries::blocks::BlockDef,
) -> i32 {
    if world
        .get::<crate::world::blocks::storage::storage_block::StorageBuild>(entity)
        .is_some_and(|storage| storage.linked_core.is_some())
    {
        return (def.item_capacity / 60).min(6);
    }
    def.item_capacity
}

/// `BuildingComp.onDestroyed` explosion inputs
/// (`BuildingComp.java:1490-1518`): `(flammability, explosiveness, power)`
/// summed over the live item/liquid/power modules plus the block's
/// `baseExplosiveness`. Liquids count half; buffered power uses
/// `power.status * capacity`. The caller applies `3.5 * explosivenessScale`
/// and `flammabilityScale`.
pub(crate) fn building_explosion_terms(
    world: &World,
    content: &ContentRegistry,
    entity: Entity,
    def: &crate::content::registries::blocks::BlockDef,
) -> (f32, f32, f32) {
    let mut explosiveness = def.base_explosiveness;
    let mut flammability = 0.0f32;
    let mut power = 0.0f32;
    if def.has_items
        && let Some(module) = world.get::<crate::world::modules::ItemModule>(entity)
    {
        let cap = explosion_item_cap(world, entity, def).max(0);
        for (index, amount) in module.items.iter().enumerate() {
            // `Math.min(items.get(item), explosionItemCap())`.
            let amount = (*amount).min(cap) as f32;
            if amount <= 0.0 {
                continue;
            }
            if let Some(item) = content.items().get(index) {
                explosiveness += item.explosiveness * amount;
                flammability += item.flammability * amount;
                power += item.charge * amount.powf(1.1) * 150.0;
            }
        }
    }
    if def.has_liquids
        && let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity)
    {
        for (index, amount) in module.liquids.iter().enumerate() {
            if *amount <= 0.0 {
                continue;
            }
            if let Some(liquid) = content.liquids().get(index) {
                flammability += liquid.flammability * *amount / 2.0;
                explosiveness += liquid.explosiveness * *amount / 2.0;
            }
        }
    }
    if let Some(index) = def.cons_power
        && let Some(consumer) = def.consumes.get(index)
        && let crate::content::registries::blocks::Consume::Power { buffered, .. } =
            consumer.consume
        && buffered > 0.0
        && let Some(module) = world.get::<crate::world::modules::PowerModule>(entity)
    {
        power += module.status * buffered;
    }
    (flammability, explosiveness, power)
}
