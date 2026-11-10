// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! [`Sim`] accessors for the opt-in block runtime and the hand-written post-tick
//! passes (`refresh_facing_drills`, assembler occupancy, thermal sums, consumer
//! damage/explosions, beam nodes).
//!
//! Upstream: none (port-only post-tick passes; the behavior hooks live in
//! `world/blocks`).

use super::*;

impl Sim {
    /// Whether the opt-in live building runtime is installed.
    pub fn has_block_runtime(&self) -> bool {
        self.block_runtime.is_some()
    }

    /// Read-only live runtime, if installed.
    pub fn block_runtime(&self) -> Option<&SimRuntime> {
        self.block_runtime.as_ref()
    }

    /// Mutable live runtime, if installed.
    pub fn block_runtime_mut(&mut self) -> Option<&mut SimRuntime> {
        self.block_runtime.as_mut()
    }

    /// `BeamDrill.updateFacing`/`WallCrafter.getEfficiency` (T5): rescans the
    /// facing edge of every registered facing drill once per tick.
    ///
    /// The behaviors cannot see the grid (`update_tile` takes only `&mut World`),
    /// so `Sim::tick` runs this pass after the scheduled systems and before the
    /// builder pass.
    pub(crate) fn refresh_facing_drills(&mut self) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        if runtime.facing_drills.is_empty() {
            return;
        }
        let drills = std::mem::take(&mut runtime.facing_drills);
        for entity in drills {
            if self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .is_none()
            {
                continue;
            }
            if runtime.is_facing_drill(&self.ecs.0, entity) {
                crate::world::behavior::production::refresh_drill_ore(
                    &mut self.ecs.0,
                    &self.grid,
                    runtime.content(),
                    entity,
                );
                runtime.facing_drills.push(entity);
            }
        }
    }

    /// `UnitAssemblerBuild.updateTile`'s `wasOccupied = checkSolid(spawn,
    /// false)` (`UnitAssembler.java:515`), computed before the scheduled
    /// building pass because the behavior cannot see the live grid (L06 T1).
    pub(crate) fn refresh_assembler_occupancy(&mut self) {
        use crate::world::blocks::units::UnitAssembler;
        use crate::world::blocks::units::behavior::{assembler_check_solid, assembler_spawn_at};
        let assemblers: Vec<Entity> = {
            let mut query = self
                .ecs
                .0
                .query_filtered::<Entity, bevy_ecs::query::With<UnitAssembler>>();
            query.iter(&self.ecs.0).collect()
        };
        if assemblers.is_empty() {
            return;
        }
        let mut results: Vec<(Entity, bool)> = Vec::with_capacity(assemblers.len());
        {
            let world = &self.ecs.0;
            let Some(runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            else {
                return;
            };
            let content = &runtime.content;
            let table = world.get_resource::<crate::world::block::BlockTable>();
            for entity in assemblers {
                let Some(state) = world.get::<UnitAssembler>(entity) else {
                    continue;
                };
                let Some(output) = state.plan().and_then(|plan| content.unit(plan.unit)) else {
                    continue;
                };
                let size = world
                    .get::<crate::entities::comp::Building>(entity)
                    .and_then(|building| table.and_then(|table| table.get(building.block)))
                    .map_or(1, |instance| instance.def.size);
                let Some(spawn) = assembler_spawn_at(world, entity, state.area_size, size) else {
                    continue;
                };
                let occupied =
                    assembler_check_solid(world, &self.grid, content, output, spawn, false);
                results.push((entity, occupied));
            }
        }
        for (entity, occupied) in results {
            if let Some(mut state) = self.ecs.0.get_mut::<UnitAssembler>(entity) {
                state.was_occupied = occupied;
            }
        }
    }

    /// `ThermalGeneratorBuild.onProximityAdded` rescan (T14): the behavior hook
    /// cannot see the live grid (`Sim.grid` is not an ECS resource), so the
    /// whole floor-attribute sum is recomputed for every registered thermal
    /// generator once per tick.
    pub(crate) fn refresh_thermal_generators(&mut self) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        if runtime.thermal_generators.is_empty() {
            return;
        }
        let generators = std::mem::take(&mut runtime.thermal_generators);
        for entity in generators {
            if self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .is_none()
            {
                continue;
            }
            if let Some(attribute) = runtime.thermal_attribute(&self.ecs.0, entity) {
                crate::world::blocks::power::behavior::refresh_thermal_sum_grid(
                    &mut self.ecs.0,
                    &self.grid,
                    entity,
                    attribute,
                );
                runtime.thermal_generators.push(entity);
            }
        }
    }

    /// `ConsumeItemExplode.update` (`ConsumeItemExplode.java:30-42`): an
    /// explosive item (`explosiveness >= threshold`) stored in a generator
    /// damages it by `damage` (4) with chance
    /// `build.delta() * baseChance * clamp(explosiveness - threshold)` and
    /// fires `Trigger.blastGenerator`, only while `rules.reactorExplosions`
    /// (`Blocks.java:2537,2571` use both consumers).
    pub(crate) fn update_item_explode_consumers(&mut self) {
        use crate::world::blocks::power::generator::{
            GeneratorConfig, first_explosive_item, item_explosiveness, reactor_explosions_enabled,
        };

        if !reactor_explosions_enabled(&self.ecs.0) {
            return;
        }
        // Collect candidates first: the body mutates health, draws RNG and
        // fires the trigger.
        let candidates: Vec<(
            Entity,
            crate::world::blocks::power::generator::ItemExplodeConfig,
        )> = {
            let mut query = self.ecs.0.query::<(Entity, &GeneratorConfig)>();
            query
                .iter(&self.ecs.0)
                .filter_map(|(entity, config)| config.explode_items.map(|config| (entity, config)))
                .collect()
        };
        if candidates.is_empty() {
            return;
        }
        let Some(runtime) = self.block_runtime() else {
            return;
        };
        let explosiveness = item_explosiveness(runtime.content());
        for (entity, explode) in candidates {
            // `Building.updateConsumption` runs `updateConsumers` only while
            // enabled with `shouldConsume() && productionValid()` and
            // `efficiency > 0` (`Building.java:2166-2204`); the generator's
            // same-tick efficiency comes from `update_generator_with`.
            let (enabled, efficiency) = self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .map(|building| (building.enabled, building.efficiency))
                .unwrap_or((false, 0.0));
            if !enabled || efficiency <= 0.0 {
                continue;
            }
            let Some(module) = self.ecs.0.get::<ItemModule>(entity) else {
                continue;
            };
            let Some(item) = first_explosive_item(module, &explosiveness, explode.threshold) else {
                continue;
            };
            let value = explosiveness.get(item.index()).copied().unwrap_or(0.0);
            let delta = crate::world::update::delta(&self.ecs.0, entity);
            let chance = explode.chance(delta, value);
            // `Mathf.chance(d)`: `d >= 1f || rand.nextFloat() < d`.
            let fired = chance >= 1.0
                || self
                    .rng_streams
                    .chance(crate::determinism::RngStream::Sim, chance as f64);
            if !fired {
                continue;
            }
            if let Some(mut health) = self.ecs.0.get_mut::<crate::entities::comp::Health>(entity) {
                health.health -= explode.damage;
            }
            // `Fx.generatespark.at(x + range, y + range)` (`range` = the
            // building's `size * tilesize / 2`, Java `ConsumeItemExplode`).
            let (x, y) = self
                .ecs
                .0
                .get::<crate::entities::comp::Pos>(entity)
                .map(|pos| (pos.x, pos.y))
                .unwrap_or((0.0, 0.0));
            crate::world::blocks::power::explosion::generatespark(&mut self.ecs.0, x, y);
            self.triggers
                .fire(crate::sim::Trigger::BlastGenerator, &mut self.ecs.0);
        }
    }

    /// Drains [`crate::world::consumers::PendingConsumerDamage`], the combat
    /// half of `ConsumeItemExplode.update` (`ConsumeItemExplode.java:30-42`).
    ///
    /// The generic consumer runtime (`world::consumers::runtime_update`, GAPS
    /// #58) queues `(entity, damage)` because the building pass only owns
    /// `&mut World`; here the queued damage becomes a real
    /// `Building.damage(damage)` (`BuildingComp.java:2066-2088` — block-health
    /// scale, shields and the death marker live in
    /// [`crate::combat::damage::area::apply_health`]), plus Java's
    /// `explodeEffect.at(build.x, build.y)` (`Fx.generatespark`) and
    /// `Events.fire(Trigger.blastGenerator)`.
    ///
    /// Runs after the scheduled building pass and before `resolve_deaths`, so a
    /// killing explosion still resolves in the same tick. The generator-family
    /// legacy pass ([`Self::update_item_explode_consumers`]) stays separate
    /// until the content lowering emits `ConsumeItemExplode` consumers.
    pub(crate) fn drain_consumer_damage(&mut self) {
        let pending = self
            .ecs
            .0
            .remove_resource::<crate::world::consumers::PendingConsumerDamage>()
            .map(|pending| pending.0)
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }
        for (entity, damage) in pending {
            if self.ecs.0.get_entity(entity).is_err() {
                continue;
            }
            crate::combat::damage::area::apply_health(&mut self.ecs.0, entity, damage);
            let (x, y) = self
                .ecs
                .0
                .get::<crate::entities::comp::Pos>(entity)
                .map(|pos| (pos.x, pos.y))
                .unwrap_or((0.0, 0.0));
            crate::world::blocks::power::explosion::generatespark(&mut self.ecs.0, x, y);
            self.triggers
                .fire(crate::sim::Trigger::BlastGenerator, &mut self.ecs.0);
        }
    }

    /// `PowerGenerator.onExplosion`'s **sim half** (J04 T4,
    /// `PowerGenerator.java:149-201`): radius damage, ignition fires (delayed
    /// by `dst / explosionSpeed` through the `SimClock`), boulder/prop
    /// breaking, fireball bullets and liquid puddles. The view half
    /// (`explodeEffect`/`explodeSound`, shake, scorch decal) stays with
    /// `ExplosionSink`.
    pub(crate) fn apply_generator_explosions(&mut self) {
        use crate::world::blocks::power::explosion::PendingGeneratorExplosions;

        // Due ignition fires (`Time.run(dst / explosionSpeed, ...)`): the
        // delay is scheduled as a tick count, matching the `SimClock` unit.
        let tick = self.state.tick;
        let due_fires: Vec<(i16, i16)> = {
            let mut due = Vec::new();
            if let Some(mut pending) = self
                .ecs
                .0
                .get_resource_mut::<crate::world::blocks::power::explosion::PendingExplosionFires>(
            ) {
                let mut rest = Vec::new();
                for (due_tick, tx, ty) in pending.entries.drain(..) {
                    if due_tick <= tick {
                        due.push((tx, ty));
                    } else {
                        rest.push((due_tick, tx, ty));
                    }
                }
                pending.entries = rest;
            }
            due
        };
        for (tx, ty) in due_fires {
            let _ = crate::combat::fires::create(&mut self.ecs.0, tx, ty, &mut self.rng_streams);
        }

        let pending = self
            .ecs
            .0
            .get_resource_mut::<PendingGeneratorExplosions>()
            .map(|mut pending| std::mem::take(&mut pending.0))
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }

        let Some(runtime) = self.block_runtime.as_ref() else {
            return;
        };
        let content = &runtime.content;
        let hooks = &runtime.hooks;
        let render = &runtime.render;
        let ts = crate::config::TILESIZE as f32;
        let (width, height) = (self.grid.tiles.width, self.grid.tiles.height);
        let mut broke_prop = false;
        for explosion in pending {
            // `if(explosionDamage > 0) Damage.damage(x, y, explosionRadius *
            // tilesize, explosionDamage)`.
            if explosion.damage > 0.0 {
                let _ = crate::combat::damage::area::damage_area(
                    &mut self.ecs.0,
                    content,
                    None,
                    explosion.x,
                    explosion.y,
                    explosion.radius * ts,
                    explosion.damage,
                    crate::combat::damage::area::DamageOptions::default(),
                );
            }

            // `Geometry.circle(tileX(), tileY(), explosionRadius, ...)`:
            // ignition fires (delayed by `dst / explosionSpeed`) and prop
            // breaking (`unitMoveBreakable`).
            if explosion.ignition_chance > 0.0 || explosion.breaks_props {
                let cx = (explosion.x / ts) as i32;
                let cy = (explosion.y / ts) as i32;
                let radius = explosion.radius.max(0.0) as i32;
                let mut breaks: Vec<(i16, i16)> = Vec::new();
                let mut fires: Vec<(f32, i16, i16)> = Vec::new();
                crate::maps::generators::basic::circle(cx, cy, width, height, radius, |tx, ty| {
                    let dst = (((cx - tx).pow(2) + (cy - ty).pow(2)) as f32).sqrt();
                    if explosion.ignition_chance > 0.0 {
                        let chance = explosion.ignition_chance
                            * if explosion.scale_ignition_chance {
                                1.0 - (dst / explosion.radius.max(0.0001)).sqrt()
                            } else {
                                1.0
                            };
                        if self.rng_streams.chance(
                            crate::determinism::RngStream::Sim,
                            chance.clamp(0.0, 1.0) as f64,
                        ) {
                            fires.push((dst / explosion.speed.max(0.0001), tx as i16, ty as i16));
                        }
                    }
                    if explosion.breaks_props
                        && self
                            .grid
                            .tiles
                            .getn(tx, ty)
                            .map(|tile| tile.block)
                            .and_then(|block| content.block(block))
                            .is_some_and(|def| def.unit_move_breakable)
                    {
                        breaks.push((tx as i16, ty as i16));
                    }
                });
                if !breaks.is_empty() {
                    // `ConstructBlock.deconstructFinish(t, t.block(), null)`:
                    // props have no building entity, so the tile op removes
                    // them directly.
                    let mut log = crate::world::ops::WorldEventLog::default();
                    for (tx, ty) in breaks {
                        let mut ctx = crate::world::ops::WorldCtx {
                            grid: &mut self.grid,
                            content,
                            ecs: &mut self.ecs.0,
                            hooks,
                            render,
                            log: &mut log,
                        };
                        ctx.set_air(tx, ty);
                        broke_prop = true;
                    }
                }
                for (delay, tx, ty) in fires {
                    // `Time.run(dst / explosionSpeed, () -> Fires.create(t))`.
                    let due = tick + delay.max(0.0).ceil() as u64;
                    self.ecs
                        .0
                        .get_resource_or_insert_with(
                            crate::world::blocks::power::explosion::PendingExplosionFires::default,
                        )
                        .entries
                        .push((due, tx, ty));
                }
            }

            // `if(explosionFireballs > 0)`: `Mathf.random(1, explosionFireballs)`
            // `Bullets.fireball.createNet(Team.derelict, ...)` — queued for the
            // fire/puddle pass, which owns the bullet spawn seam.
            if explosion.fireballs > 0 {
                let amount = 1 + self.rng_streams.random(
                    crate::determinism::RngStream::Sim,
                    explosion.fireballs.max(1),
                );
                for _ in 0..amount {
                    self.ecs
                        .0
                        .get_resource_or_insert_with(
                            crate::combat::puddles::PendingFireballs::default,
                        )
                        .0
                        .push((explosion.x, explosion.y));
                }
            }

            // `if(explosionPuddleLiquid != null)`: `explosionPuddles` deposits
            // at `Tmp.v1.trns(Mathf.random(360f), Mathf.random(range))`.
            if let Some(liquid) = explosion.puddle_liquid {
                for _ in 0..explosion.puddles.max(0) {
                    let angle =
                        self.rng_streams
                            .range(crate::determinism::RngStream::Sim, 0.0, 360.0);
                    let dist = self.rng_streams.range(
                        crate::determinism::RngStream::Sim,
                        0.0,
                        explosion.puddle_range,
                    );
                    let px = explosion.x + angle.to_radians().cos() * dist;
                    let py = explosion.y + angle.to_radians().sin() * dist;
                    let tx = (px / ts) as i32;
                    let ty = (py / ts) as i32;
                    if !self.grid.tiles.in_bounds(tx, ty) {
                        continue;
                    }
                    let _ = crate::combat::puddles::deposit(
                        &mut self.ecs.0,
                        &self.grid.tiles,
                        content,
                        tx as i16,
                        ty as i16,
                        liquid,
                        explosion.puddle_amount,
                        &mut self.rng_streams,
                    );
                }
            }
        }
        if broke_prop {
            // Prop breaking mutated blocks; rebuild placement/proximity caches.
            self.refresh_unit_runtime();
        }
    }

    /// `BeamNode.updateTile` -> `updateDirections` (T15): rebuilds the four
    /// directional links of every registered beam node whose
    /// `WorldGrid.tile_changes` view is stale.
    pub(crate) fn refresh_beam_nodes(&mut self) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        if runtime.beam_nodes.is_empty() {
            return;
        }
        let changes = self.grid.tile_changes;
        let nodes = std::mem::take(&mut runtime.beam_nodes);
        for entity in nodes {
            if self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .is_none()
            {
                continue;
            }
            let stale = self
                .ecs
                .0
                .get::<crate::world::blocks::power::nodes::BeamNodeState>(entity)
                .is_some_and(|state| state.last_change != changes);
            if stale && let Some(mut graphs) = self.ecs.0.remove_resource::<PowerGrids>() {
                crate::world::blocks::power::nodes::update_beam_directions(
                    &mut graphs,
                    &mut self.ecs.0,
                    &self.grid,
                    entity,
                );
                self.ecs.0.insert_resource(graphs);
            }
            runtime.beam_nodes.push(entity);
        }
    }
}
