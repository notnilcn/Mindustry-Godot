// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-world application of a decoded save (`SaveVersion.readWorldEntities` half).
//! Upstream: core/src/mindustry/io/SaveVersion.java (world entity application)

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::UnitCore;
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::game::runtime::CampaignRuntime;
use crate::sim::Sim;
use crate::sim::unit_runtime::UnitRuntime;

use super::{DecodedSave, DecodedUnit};

impl Sim {
    /// Respawns the decoded world entities (units, fires, puddles) into the
    /// live runtime (`SaveVersion.readWorldEntities` application half).
    pub fn apply_decoded_entities(&mut self, save: &DecodedSave) {
        // H04 T13: remember which units this restore spawned so
        // `reset_loaded_controllers` can run Java's `UnitComp.afterRead` once
        // the session rules reach the world.
        self.loaded_unit_entities.clear();
        // I01 T10: `(entity, saved id)` pairs, restored after the spawn pass
        // (`SaveVersion.readWorldEntities` checks every id before reassigning
        // duplicates).
        let mut spawned: Vec<(Entity, i32)> = Vec::new();
        let default_team = self
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.default_team)
            .unwrap_or_else(|| crate::game::rules::Rules::default().default_team);
        for unit in &save.entities.units {
            // `UnitComp.java:868-871` despawns a `spawnedByCore` unit that is
            // not the player's; the host spawns a fresh player unit
            // (`spawn_player_unit`). Revision 3 carries the real flag; v1/v2
            // saves fall back to the core unit type names (`Blocks.java`
            // `coreShard..coreAcropolis` `unitType`).
            const CORE_UNITS: [&str; 6] = ["alpha", "beta", "gamma", "evoke", "incite", "emanate"];
            let core_unit_by_name = unit.revision < 3;
            if unit.team == default_team
                && (unit.spawned_by_core
                    || (core_unit_by_name && CORE_UNITS.contains(&unit.name.as_str())))
            {
                continue;
            }
            if let Some(entity) =
                self.spawn_live_unit(&unit.name, unit.team, unit.x, unit.y, unit.rotation)
            {
                self.loaded_unit_entities.push(entity);
                spawned.push((entity, unit.saved_id));
                apply_unit_record(&mut self.ecs.0, entity, unit);
            }
        }
        self.restore_unit_ids(&spawned);
        // `UnitTetherComp.afterRead` (UnitTetherComp.java:20-24): `spawner =
        // Groups.unit.getByID(spawnerUnitId)` once every unit (and its
        // restored id) exists; a missing spawner leaves `null`, so the
        // tethered unit despawns on its next update like Java.
        for (unit, (entity, _)) in save
            .entities
            .units
            .iter()
            .filter(|unit| {
                !(unit.team == default_team
                    && (unit.spawned_by_core
                        || (unit.revision < 3
                            && ["alpha", "beta", "gamma", "evoke", "incite", "emanate"]
                                .contains(&unit.name.as_str()))))
            })
            .zip(spawned.iter())
        {
            // Only a tether unit writes an id (the writer emits -1 without
            // `UnitTetherComp`), so a saved id restores the component too.
            if unit.spawner_unit_id < 0 {
                continue;
            }
            let spawner = self
                .ecs
                .0
                .iter_entities()
                .find(|candidate| {
                    candidate
                        .get::<crate::ecs::EntitySeq>()
                        .is_some_and(|seq| seq.0 as i32 == unit.spawner_unit_id)
                        && candidate.contains::<UnitCore>()
                })
                .map(|candidate| candidate.id());
            self.ecs
                .0
                .entity_mut(*entity)
                .insert(crate::entities::comp::unit::comp::UnitTetherComp { spawner });
        }
        // Fires and puddles (`Groups.fires`/`Groups.puddles` recreate from
        // their chunks; the tile-slot caches re-register on the next update
        // pass). Accumulator fields are transient upstream and start zeroed.
        let tile_size = crate::config::TILESIZE as f32;
        for fire in &save.entities.fires {
            self.ecs.0.spawn((
                Pos {
                    x: (fire.tile.0 as f32 + 0.5) * tile_size,
                    y: (fire.tile.1 as f32 + 0.5) * tile_size,
                },
                TeamComp { team: 0 },
                crate::combat::fires::FireState {
                    tile: fire.tile,
                    time: fire.time,
                    lifetime: fire.lifetime,
                    puddle_flammability: 0.0,
                    spread_timer: 0.0,
                    fireball_timer: 0.0,
                    damage_timer: 0.0,
                    warmup: 0.0,
                    animation: 0.0,
                },
            ));
        }
        for puddle in &save.entities.puddles {
            self.ecs.0.spawn((
                Pos {
                    x: (puddle.tile.0 as f32 + 0.5) * tile_size,
                    y: (puddle.tile.1 as f32 + 0.5) * tile_size,
                },
                crate::combat::puddles::PuddleState {
                    tile: puddle.tile,
                    liquid: puddle.liquid,
                    amount: puddle.amount,
                    accepting: 0.0,
                    update_time: 0.0,
                    last_ripple: 0.0,
                    effect_time: 0.0,
                },
            ));
        }
    }

    /// Restores the saved `EntitySeq` ids onto the respawned units
    /// (`SaveVersion.readWorldEntities`, `SaveVersion.java:495-528`):
    /// `EntityGroup.checkNextId(id); entity.id(id); if(used.add(id)) add();
    /// else reassign(nextId())`. The advanced ids guarantee that later unit
    /// spawns (`UnitRuntime.seq`, block outputs) never collide with a restored
    /// unit, so `unit_by_id` stays unambiguous (I01 T10).
    fn restore_unit_ids(&mut self, spawned: &[(Entity, i32)]) {
        if spawned.is_empty() {
            return;
        }
        let mut used: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
        let mut max_id: Option<u64> = None;
        let mut reassign: Vec<Entity> = Vec::new();
        for (entity, saved_id) in spawned {
            if *saved_id >= 0 && used.insert(*saved_id as u64) {
                if let Some(mut seq) = self.ecs.0.get_mut::<EntitySeq>(*entity) {
                    seq.0 = *saved_id as u64;
                }
                max_id = Some(max_id.map_or(*saved_id as u64, |max| max.max(*saved_id as u64)));
            } else {
                // Duplicate id (or none saved): `EntityGroup.nextId()`.
                reassign.push(*entity);
            }
        }
        // Duplicates take the ids right after the highest restored one; the
        // allocators then continue past all of them (`EntityGroup.lastId`).
        let mut next = max_id.map_or(0, |max| max.saturating_add(1));
        for entity in reassign {
            if let Some(mut seq) = self.ecs.0.get_mut::<EntitySeq>(entity) {
                seq.0 = next;
            }
            next = next.wrapping_add(1);
        }
        if let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() {
            next = next.max(runtime.seq);
            runtime.seq = next;
            self.ecs.0.insert_non_send(runtime);
        }
        // Block-output spawns (`UnitBlockSeq`) share the id space; the bump
        // inserts the resource (with its base floor) when absent.
        crate::world::blocks::units::behavior::UnitBlockSeq::bump_past(&mut self.ecs.0, max_id);
        // Future P0 building placements (`Command::Place`).
        self.bump_entity_seq(next);
    }

    /// `UnitComp.afterRead` (`UnitComp.java:585-593`): every unit read from a
    /// save re-derives `type.createController(unit)` unless its saved
    /// controller keeps state (`LogicAI`/`CommandAI`; `keepState`). The resume
    /// path calls this once the session rules are installed, because the
    /// world load spawns the units before `CampaignRuntime` exists and the
    /// spawn fallback (`BuildRules.defaultTeam` = derelict) marks a
    /// player-team unit as AI (`GroundAI` instead of `CommandAI`).
    pub fn reset_loaded_controllers(&mut self) {
        use crate::ai::controller::{AiKind, ControllerSlot};
        let entities = std::mem::take(&mut self.loaded_unit_entities);
        for entity in entities {
            let Some(kind) = self
                .ecs
                .0
                .get::<ControllerSlot>(entity)
                .map(|slot| slot.kind)
            else {
                continue;
            };
            // The player's own unit keeps its `Player` bridge; upstream keeps
            // the saved controller instance only when `keepState` is true.
            if kind == AiKind::Player || crate::ai::controller_registry::keep_state(kind) {
                continue;
            }
            let Some(team) = self.ecs.0.get::<TeamComp>(entity).map(|comp| comp.team) else {
                continue;
            };
            self.release_unit_to_ai(entity, team);
        }
    }
}

/// Applies a decoded unit record to a spawned unit entity
/// (`UnitComp.read` field half).
///
/// Shared by the entity-region restore (`apply_decoded_entities`) and the
/// nested unit-payload read (`Payload.read` unit branch), so a unit carried by
/// a payload block restores the same state as one in `Groups.unit`.
pub(crate) fn apply_unit_record(world: &mut World, entity: Entity, unit: &DecodedUnit) {
    if unit.health > 0.0
        && let Some(mut health) = world.get_mut::<Health>(entity)
    {
        health.health = unit.health;
    }
    // Revision 3: core spawn flag + elevation.
    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
        core.spawned_by_core = unit.spawned_by_core;
        core.elevation = unit.elevation;
    }
    if unit.shield > 0.0 {
        if let Some(mut shield) =
            world.get_mut::<crate::entities::comp::unit::comp::ShieldComp>(entity)
        {
            shield.shield = unit.shield;
        } else {
            world
                .entity_mut(entity)
                .insert(crate::entities::comp::unit::comp::ShieldComp {
                    shield: unit.shield,
                    ..Default::default()
                });
        }
    }
    if let Some((item, amount)) = unit.stack {
        if let Some(mut items) =
            world.get_mut::<crate::entities::comp::unit::comp::ItemsComp>(entity)
        {
            items.item = Some((item, amount));
        } else {
            world
                .entity_mut(entity)
                .insert(crate::entities::comp::unit::comp::ItemsComp {
                    item: Some((item, amount)),
                });
        }
    }
    if !unit.statuses.is_empty() {
        if let Some(mut status) =
            world.get_mut::<crate::entities::comp::unit::comp::StatusComp>(entity)
        {
            status.statuses = unit.statuses.clone();
            status.damage_times.resize(unit.statuses.len(), 0.0);
        } else {
            let len = unit.statuses.len();
            world
                .entity_mut(entity)
                .insert(crate::entities::comp::unit::comp::StatusComp {
                    statuses: unit.statuses.clone(),
                    damage_times: vec![0.0; len],
                    ..Default::default()
                });
        }
    }
    if let Some(command) = unit.command {
        world
            .entity_mut(entity)
            .insert(crate::ai::controller::UnitCommandComp(command));
    }
    if let Some(kind) = unit.controller
        && let Some(mut slot) = world.get_mut::<crate::ai::controller::ControllerSlot>(entity)
    {
        slot.kind = kind;
    }
    if !unit.plans.is_empty() {
        let plans: std::collections::VecDeque<crate::sim::builder::UnitBuildPlan> =
            unit.plans.iter().cloned().collect();
        if let Some(mut builder) = world.get_mut::<crate::sim::builder::BuilderPlans>(entity) {
            builder.plans = plans;
        } else {
            world
                .entity_mut(entity)
                .insert(crate::sim::builder::BuilderPlans {
                    plans,
                    ..Default::default()
                });
        }
    }
    // Revision 4: `UnitComp.setType` copies the read `data` into
    // the type's abilities index-aligned (`EmptyDataAbility`
    // placeholders when the count differs; UnitComp.java:549-557,
    // TypeIO.java:429-444).
    if !unit.ability_data.is_empty()
        && let Some(mut comp) = world.get_mut::<crate::combat::abilities::AbilityComp>(entity)
    {
        for (state, data) in comp.states.iter_mut().zip(&unit.ability_data) {
            state.data = *data;
        }
    }
    // Revision 2: restore the carried payload stack.
    if !unit.payloads.is_empty() {
        let stack = unit.payloads.clone();
        if let Some(mut payloads) =
            world.get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(entity)
        {
            payloads.stack = stack;
        } else {
            world
                .entity_mut(entity)
                .insert(crate::entities::comp::unit::comp::UnitPayloads { stack });
        }
    }
}
