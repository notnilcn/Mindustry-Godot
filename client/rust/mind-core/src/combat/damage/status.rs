// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Status application and per-tick update (`StatusEffect.apply`/`update`,
//! `StatusComp.applyStatus`/`update`, `Damage.status`).
//!
//! [`crate::content::registries::statuses::StatusEffect`] owns the metadata;
//! this module is the runtime half. [`UnitStatusApply`] is the production
//! [`StatusApply`] implementation installed as an ECS resource
//! ([`install_status_applier`]); [`update_unit_status`]/[`update_statuses`]
//! port `StatusComp.update` (duration decay, removal, per-frame and interval
//! damage, and the stat multipliers read by combat/movement/weapons).
//!
//! Plan 02/deferred transitions are ported exactly: opposites cancel over time
//! and affinities trigger the handler's damage/extend data. Game triggers
//! (`Trigger.shock`/`blastFreeze`) and view-only effects are not simulated.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::registries::statuses::{StatusEffect, TransitionSpec};
use crate::content::{ContentRegistry, StatusId};
use crate::determinism::SimRng;
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::UnitTypeComp;
use crate::entities::comp::unit::comp::{StatusComp, StatusEntry};
use crate::entities::comp::{Health, Pos, TeamComp};

use super::armor::apply_armor_opt;

/// Applies status effects to entities (implemented by [`UnitStatusApply`]).
pub trait StatusApply: Send + Sync + 'static {
    /// Applies `status` to `entity` for `duration` ticks.
    fn apply(&mut self, world: &mut World, entity: Entity, status: StatusId, duration: f32);

    /// Whether `entity` is immune to `status` (`StatusEffect` immunities).
    fn is_immune(&self, _world: &World, _entity: Entity, _status: StatusId) -> bool {
        false
    }
}

/// World resource holding the installed [`StatusApply`] implementation.
#[derive(Resource)]
pub struct StatusApplier(pub Box<dyn StatusApply>);

/// Production [`StatusApply`] over [`StatusComp`] (`StatusComp.applyStatus`).
///
/// Owns a snapshot of the status transition tables, unit immunities and unit
/// armor so the `&mut World`-only application path needs no content registry.
pub struct UnitStatusApply {
    /// Status records in dense id order (`content.statuses()`).
    statuses: Vec<StatusEffect>,
    /// Unit immunities indexed by unit content id (`UnitType.immunities`).
    immunities: Vec<Vec<StatusId>>,
    /// Unit armor indexed by unit content id (`UnitType.armor`).
    unit_armor: Vec<f32>,
}

impl UnitStatusApply {
    /// Snapshots the content tables an application needs.
    pub fn from_content(content: &ContentRegistry) -> Self {
        Self {
            statuses: content.statuses().to_vec(),
            immunities: content
                .units()
                .iter()
                .map(|unit| unit.immunities.clone())
                .collect(),
            unit_armor: content.units().iter().map(|unit| unit.armor).collect(),
        }
    }

    fn meta(&self, status: StatusId) -> Option<&StatusEffect> {
        self.statuses.get(status.index())
    }

    /// Unit armor (status override included) for affinity transition damage.
    fn armor_of(&self, world: &World, entity: Entity) -> f32 {
        if let Some(status) = world.get::<StatusComp>(entity)
            && status.armor_override >= 0.0
        {
            return status.armor_override;
        }
        world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| self.unit_armor.get(comp.type_id.index()).copied())
            .unwrap_or(0.0)
    }
}

impl StatusApply for UnitStatusApply {
    fn apply(&mut self, world: &mut World, entity: Entity, status: StatusId, duration: f32) {
        if status == StatusId::NONE || duration <= 0.0 {
            return;
        }
        if self.is_immune(world, entity, status) {
            return;
        }
        if world.get::<StatusComp>(entity).is_none() {
            return;
        }

        // Re-application extends the existing entry (`StatusComp.applyStatus`).
        {
            let Some(mut comp) = world.get_mut::<StatusComp>(entity) else {
                return;
            };
            if let Some(existing) = comp
                .statuses
                .iter_mut()
                .find(|entry| entry.effect == status)
            {
                existing.duration = existing.duration.max(duration);
                return;
            }
        }

        // Find the first transition reaction among the active entries.
        let mut reaction: Option<(usize, TransitionSpec)> = None;
        if let Some(comp) = world.get::<StatusComp>(entity) {
            for (index, entry) in comp.statuses.iter().enumerate() {
                if let Some(spec) = self
                    .meta(entry.effect)
                    .and_then(|effect| effect.transition(status))
                {
                    reaction = Some((index, spec.clone()));
                    break;
                }
            }
        }

        match reaction {
            Some((index, TransitionSpec::Opposite)) => {
                if let Some(mut comp) = world.get_mut::<StatusComp>(entity) {
                    let entry = &mut comp.statuses[index];
                    entry.duration -= duration * 0.5;
                    if entry.duration <= 0.0 {
                        entry.duration = duration;
                        entry.effect = status;
                    }
                }
            }
            Some((index, TransitionSpec::Affinity(handler))) => {
                if handler.damage > 0.0 {
                    let armor = self.armor_of(world, entity);
                    let applied = apply_armor_opt(handler.damage, armor, handler.damage_pierce);
                    super::area::apply_health(world, entity, applied);
                }
                if let Some(mut comp) = world.get_mut::<StatusComp>(entity)
                    && let Some(entry) = comp.statuses.get_mut(index)
                    && let Some((result, cap)) = handler.extend
                {
                    entry.effect = result;
                    entry.duration += duration;
                    if let Some(cap) = cap {
                        entry.duration = entry.duration.min(cap);
                    }
                }
            }
            None => {
                if self.meta(status).is_some_and(|effect| !effect.reactive)
                    && let Some(mut comp) = world.get_mut::<StatusComp>(entity)
                {
                    comp.apply(StatusEntry::new(status, duration));
                }
            }
        }
    }

    fn is_immune(&self, world: &World, entity: Entity, status: StatusId) -> bool {
        world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| self.immunities.get(comp.type_id.index()))
            .is_some_and(|immunities| immunities.contains(&status))
    }
}

/// Installs/replaces the [`UnitStatusApply`] applier (`StatusApplier` resource).
pub fn install_status_applier(world: &mut World, content: &ContentRegistry) {
    let applier = UnitStatusApply::from_content(content);
    if let Some(mut slot) = world.get_resource_mut::<StatusApplier>() {
        slot.0 = Box::new(applier);
    } else {
        world.insert_resource(StatusApplier(Box::new(applier)));
    }
}

/// Applies a status to one entity through the installed applier (no-op if none).
pub fn apply_status(world: &mut World, entity: Entity, status: StatusId, duration: f32) {
    if status == StatusId::NONE || duration <= 0.0 {
        return;
    }
    if world.get_resource::<StatusApplier>().is_none() {
        return;
    }
    world.resource_scope::<StatusApplier, _>(|world, mut applier| {
        applier.0.apply(world, entity, status, duration);
    });
}

/// Advances every unit's status list one tick (`StatusComp.update`).
///
/// Iteration is `(EntitySeq, entity.index())` ordered so damage application and
/// removal are replay-stable.
pub fn update_statuses(world: &mut World, content: &ContentRegistry) {
    let mut entities: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<StatusComp>().is_some())
        .map(|entity_ref| {
            let seq = entity_ref
                .get::<EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            (seq, entity_ref.id())
        })
        .collect();
    entities.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    for (_, entity) in entities {
        update_unit_status(world, content, entity);
    }
}

/// Advances one unit's statuses one tick (`StatusComp.update`).
///
/// Durations decay by one tick, expired non-permanent entries are removed,
/// `damage`/`intervalDamage` are applied, and the transient multipliers are
/// recomputed for the movement/weapons/damage consumers.
pub fn update_unit_status(world: &mut World, content: &ContentRegistry, entity: Entity) {
    let Some(mut comp) = world.get::<StatusComp>(entity).cloned() else {
        return;
    };
    comp.reset_modifiers();

    let mut index = 0usize;
    while index < comp.statuses.len() {
        comp.statuses[index].duration = (comp.statuses[index].duration - 1.0).max(0.0);
        let effect_id = comp.statuses[index].effect;
        let Some(effect) = content.status(effect_id) else {
            comp.statuses.remove(index);
            comp.damage_times.remove(index);
            continue;
        };
        if comp.statuses[index].duration <= 0.0 && !effect.permanent {
            comp.statuses.remove(index);
            comp.damage_times.remove(index);
            continue;
        }

        // `StatusEffect.update`: per-frame damage/heal.
        if effect.damage > 0.0 {
            super::area::apply_health(world, entity, effect.damage);
        } else if effect.damage < 0.0 {
            super::area::heal_health(world, entity, -effect.damage);
        }

        // `StatusEffect.update`: interval damage.
        if effect.interval_damage_time > 0.0 {
            if index >= comp.damage_times.len() {
                comp.damage_times.resize(index + 1, 0.0);
            }
            let timer = &mut comp.damage_times[index];
            *timer += 1.0;
            if *timer >= effect.interval_damage_time {
                *timer %= effect.interval_damage_time;
                if effect.interval_damage_pierce {
                    super::area::apply_health(world, entity, effect.interval_damage);
                } else {
                    super::area::damage_entity(
                        world,
                        content,
                        entity,
                        effect.interval_damage,
                        false,
                        1.0,
                    );
                }
            }
        }

        comp.speed_multiplier *= effect.speed_multiplier;
        comp.damage_multiplier *= effect.damage_multiplier;
        comp.health_multiplier *= effect.health_multiplier;
        comp.reload_multiplier *= effect.reload_multiplier;
        comp.build_speed_multiplier *= effect.build_speed_multiplier;
        comp.drag_multiplier *= effect.drag_multiplier;
        comp.disarmed |= effect.disarm;
        index += 1;
    }

    if let Some(mut stored) = world.get_mut::<StatusComp>(entity) {
        *stored = comp;
    }
}

/// `Damage.status`: applies `effect` to enemies of `team` within `radius`.
///
/// `chance` is a `0..1` probability applied per entity in deterministic
/// `EntitySeq` order.
#[allow(clippy::too_many_arguments)]
pub fn status_area(
    world: &mut World,
    team: Option<u8>,
    x: f32,
    y: f32,
    radius: f32,
    effect: StatusId,
    duration: f32,
    chance: f32,
    rng: &mut SimRng,
) -> usize {
    use crate::determinism::RngStream;

    if effect == StatusId::NONE || radius <= 0.0 {
        return 0;
    }
    let radius2 = radius * radius;
    let mut targets: Vec<(u64, Entity)> = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        if entity_ref.get::<Health>().is_none() {
            continue;
        }
        if let Some(source) = team
            && entity_ref
                .get::<TeamComp>()
                .is_some_and(|t| t.team == source)
        {
            continue;
        }
        let dx = pos.x - x;
        let dy = pos.y - y;
        if dx * dx + dy * dy > radius2 {
            continue;
        }
        let seq = entity_ref
            .get::<EntitySeq>()
            .map(|s| s.0)
            .unwrap_or(u64::MAX);
        targets.push((seq, entity_ref.id()));
    }
    targets.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    let mut applied = 0usize;
    for (_, entity) in targets {
        if chance < 1.0 && !rng.chance(RngStream::Sim, chance as f64) {
            continue;
        }
        apply_status(world, entity, effect, duration);
        applied += 1;
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::ShieldComp;
    use bevy_ecs::world::World;
    use std::sync::{Arc, Mutex};

    struct Recording {
        entries: Arc<Mutex<Vec<(Entity, StatusId, f32)>>>,
    }

    impl StatusApply for Recording {
        fn apply(&mut self, _world: &mut World, entity: Entity, status: StatusId, duration: f32) {
            self.entries
                .lock()
                .unwrap()
                .push((entity, status, duration));
        }
    }

    #[test]
    fn apply_status_routes_through_resource() {
        use crate::entities::comp::Pos;
        let mut world = World::new();
        let entity = world.spawn(Pos { x: 1.0, y: 2.0 }).id();
        let entries = Arc::new(Mutex::new(Vec::new()));
        world.insert_resource(StatusApplier(Box::new(Recording {
            entries: entries.clone(),
        })));
        apply_status(&mut world, entity, StatusId::new(3), 60.0);
        let recorded = entries.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], (entity, StatusId::new(3), 60.0));
        drop(recorded);
        // `none` and non-positive durations are ignored.
        apply_status(&mut world, entity, StatusId::NONE, 60.0);
        apply_status(&mut world, entity, StatusId::new(3), 0.0);
        assert_eq!(entries.lock().unwrap().len(), 1);
    }

    fn harness_with_applier(seed: u64) -> UnitHarness {
        let mut harness = UnitHarness::new(32, 32, seed);
        install_status_applier(&mut harness.build.world, &harness.build.content);
        harness
    }

    #[test]
    fn apply_tick_and_expire() {
        let mut harness = harness_with_applier(7);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let wet = harness.content().status_id("wet").expect("wet");
        apply_status(&mut harness.build.world, unit, wet, 3.0);
        assert_eq!(
            harness
                .build
                .world
                .get::<StatusComp>(unit)
                .expect("status")
                .get_duration(wet),
            3.0
        );
        // Refresh keeps the longer duration.
        apply_status(&mut harness.build.world, unit, wet, 2.0);
        assert_eq!(
            harness
                .build
                .world
                .get::<StatusComp>(unit)
                .expect("status")
                .get_duration(wet),
            3.0
        );
        update_statuses(&mut harness.build.world, &harness.build.content);
        assert_eq!(
            harness
                .build
                .world
                .get::<StatusComp>(unit)
                .expect("status")
                .get_duration(wet),
            2.0
        );
        update_statuses(&mut harness.build.world, &harness.build.content);
        update_statuses(&mut harness.build.world, &harness.build.content);
        assert!(
            !harness
                .build
                .world
                .get::<StatusComp>(unit)
                .expect("status")
                .has_effect_of(wet)
        );
    }

    #[test]
    fn speed_health_and_damage_modifiers_apply() {
        let mut harness = harness_with_applier(11);
        let frozen = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let burning_unit = harness.spawn("dagger", 0, 96.0, 96.0, 0.0).expect("dagger");
        let freezing = harness.content().status_id("freezing").expect("freezing");
        let burning = harness.content().status_id("burning").expect("burning");
        let start = harness.snapshot(burning_unit).expect("snapshot").health;
        apply_status(&mut harness.build.world, frozen, freezing, 10.0);
        apply_status(&mut harness.build.world, burning_unit, burning, 10.0);
        update_statuses(&mut harness.build.world, &harness.build.content);
        let comp = harness
            .build
            .world
            .get::<StatusComp>(frozen)
            .expect("status")
            .clone();
        assert!((comp.speed_multiplier - 0.6).abs() < 1e-6);
        assert!((comp.health_multiplier - 0.8).abs() < 1e-6);
        let after = harness.snapshot(burning_unit).expect("snapshot").health;
        assert!(after < start, "burning dealt damage ({start} -> {after})");
    }

    #[test]
    fn interval_damage_and_shield_absorb() {
        let mut harness = harness_with_applier(13);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        harness
            .build
            .world
            .get_mut::<ShieldComp>(unit)
            .unwrap()
            .shield = 5.0;
        let corroded = harness.content().status_id("corroded").expect("corroded");
        apply_status(&mut harness.build.world, unit, corroded, 60.0);
        // 15 ticks per interval -> one hit on the 15th update.
        for _ in 0..15 {
            update_statuses(&mut harness.build.world, &harness.build.content);
        }
        let shield = harness.build.world.get::<ShieldComp>(unit).unwrap().shield;
        assert!(shield < 5.0, "interval damage consumed the shield first");
    }

    #[test]
    fn immunity_blocks_application() {
        let mut harness = harness_with_applier(17);
        // `latum` is a Neoplasm unit: immune to burning/melting.
        let unit = harness.spawn("latum", 0, 64.0, 64.0, 0.0).expect("latum");
        let burning = harness.content().status_id("burning").expect("burning");
        apply_status(&mut harness.build.world, unit, burning, 60.0);
        assert!(
            !harness
                .build
                .world
                .get::<StatusComp>(unit)
                .expect("status")
                .has_effect_of(burning)
        );
    }

    #[test]
    fn opposites_cancel_and_flip() {
        let mut harness = harness_with_applier(19);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let wet = harness.content().status_id("wet").expect("wet");
        let burning = harness.content().status_id("burning").expect("burning");
        apply_status(&mut harness.build.world, unit, wet, 10.0);
        // `handleOpposite`: `result.time -= time * 0.5`; the flip only happens
        // once the weakened entry reaches zero.
        apply_status(&mut harness.build.world, unit, burning, 10.0);
        let comp = harness.build.world.get::<StatusComp>(unit).unwrap().clone();
        assert!(comp.has_effect_of(wet));
        assert!(!comp.has_effect_of(burning));
        assert_eq!(comp.get_duration(wet), 5.0);
        apply_status(&mut harness.build.world, unit, burning, 10.0);
        let comp = harness.build.world.get::<StatusComp>(unit).unwrap().clone();
        assert!(comp.has_effect_of(burning));
        assert!(!comp.has_effect_of(wet));
        assert_eq!(comp.get_duration(burning), 10.0);
    }
}
