// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Status application API (`StatusEffect.apply`, `Damage.status`).
//!
//! Plan 02 owns the [`crate::content::registries::statuses::StatusEffect`]
//! metadata; plan 11's `StatusComp` owns transitions/opposites/interval damage.
//! This module is the application half: it routes per-entity and area status
//! application through a [`StatusApply`] implementation installed as an ECS
//! resource ([`StatusApplier`]). With no applier installed (headless bootstrap,
//! tests before plan 11) the calls are no-ops, exactly like a unit without a
//! `StatusComp`.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::StatusId;

/// Applies status effects to entities (implemented by plan 11's `StatusComp`).
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

/// Applies a status to one entity through the installed applier (no-op if none).
pub fn apply_status(world: &mut World, entity: Entity, status: StatusId, duration: f32) {
    if status == StatusId::NONE || duration <= 0.0 {
        return;
    }
    world.resource_scope::<StatusApplier, _>(|world, mut applier| {
        applier.0.apply(world, entity, status, duration);
    });
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
    rng: &mut crate::determinism::SimRng,
) -> usize {
    use crate::determinism::RngStream;
    use crate::ecs::EntitySeq;
    use crate::entities::comp::{Health, Pos, TeamComp};

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
}
