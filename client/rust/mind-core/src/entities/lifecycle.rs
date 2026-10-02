// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Entity lifecycle: id allocation, spawn/remove, and pooled reuse.
//!
//! Ported from `core/src/mindustry/entities/EntityGroup.java`
//! (`nextId`/`checkNextId`, `queueFree`) and Arc `Pools.free`. Entity ids are
//! monotonic and never reused across resets, so saves/replays stay stable.

use std::collections::BTreeMap;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use super::comp::base::{BaseEntity, DefId, SimId};
use super::groups::Groups;
use super::meta::{EntityDef, EntityDefId};

/// Monotonic entity-id allocator (port of `EntityGroup.nextId()`).
#[derive(Debug, Clone, Default)]
pub struct EntityIds {
    next: i32,
}

impl EntityIds {
    /// Creates an allocator starting at id `0`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates and returns the next id (`nextId`).
    pub fn next_id(&mut self) -> i32 {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        id
    }

    /// Next id that will be allocated.
    pub const fn peek(&self) -> i32 {
        self.next
    }

    /// Ensures `id + 1` is the next allocation after loading (`checkNextId`).
    pub fn check_next_id(&mut self, id: i32) {
        if id >= self.next {
            self.next = id.saturating_add(1);
        }
    }

    /// Resets the allocator (fresh match; Java keeps `lastId` across resets).
    pub fn reset(&mut self) {
        self.next = 0;
    }
}

/// Per-def free list of pooled entities.
pub struct EntityPool {
    free: Vec<Entity>,
    reset: fn(&mut World, Entity),
}

impl std::fmt::Debug for EntityPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityPool")
            .field("free", &self.free.len())
            .finish()
    }
}

impl EntityPool {
    /// Creates a pool with an optional reset callback.
    pub fn new(reset: fn(&mut World, Entity)) -> Self {
        Self {
            free: Vec::new(),
            reset,
        }
    }

    /// Number of reusable entities.
    pub fn free_len(&self) -> usize {
        self.free.len()
    }

    /// Returns a pooled entity or spawns a new one from `bundle`.
    pub fn spawn<B: bevy_ecs::bundle::Bundle>(&mut self, world: &mut World, bundle: B) -> Entity {
        match self.free.pop() {
            Some(entity) => {
                world.entity_mut(entity).insert(bundle);
                (self.reset)(world, entity);
                entity
            }
            None => world.spawn(bundle).id(),
        }
    }

    /// Returns `entity` to the free list (does not despawn).
    pub fn recycle(&mut self, entity: Entity) {
        if !self.free.contains(&entity) {
            self.free.push(entity);
        }
    }
}

/// Per-def pools, indexed by [`EntityDefId`] (port of `Groups.updatePooling`).
#[derive(Debug, Default)]
pub struct EntityPools {
    pools: BTreeMap<u16, EntityPool>,
}

impl EntityPools {
    /// Creates an empty pool set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns (creating if needed) the pool for `def`.
    pub fn get(&mut self, def: EntityDefId) -> &mut EntityPool {
        self.pools
            .entry(def.0)
            .or_insert_with(|| EntityPool::new(|_, _| {}))
    }

    /// Total reusable entities across all defs.
    pub fn free_total(&self) -> usize {
        self.pools.values().map(EntityPool::free_len).sum()
    }
}

/// Generated entity contract: a static def name and a component bundle.
pub trait SimEntity: Sized {
    /// Parity def name (`BuildingComp`, `Unit`, ...).
    fn def_name() -> &'static str;
    /// The entity's component bundle.
    fn bundle(&self) -> impl bevy_ecs::bundle::Bundle;
}

/// Spawns `bundle` as a member of `def`'s groups with base components.
///
/// Mirrors the generated `create()`/`add()` chain: allocate an id, insert
/// `BaseEntity`/`SimId`/`DefId`, and register group membership from the def's
/// precomputed [`super::groups::GroupMask`].
pub fn sim_spawn<B: bevy_ecs::bundle::Bundle>(
    world: &mut World,
    groups: &mut Groups,
    ids: &mut EntityIds,
    def: &EntityDef,
    bundle: B,
) -> Entity {
    let id = ids.next_id();
    let entity = world
        .spawn((
            bundle,
            BaseEntity { added: true },
            SimId { id },
            DefId { def: def.id },
        ))
        .id();
    groups.add(entity, id, def.groups);
    entity
}

/// Removes `entity`: drops group membership, despawns, and recycles when pooled.
pub fn sim_remove(
    world: &mut World,
    groups: &mut Groups,
    pools: &mut EntityPools,
    def: &EntityDef,
    entity: Entity,
) {
    groups.remove(entity, def.groups);
    if def.pooled {
        pools.get(def.id).recycle(entity);
    }
    world.despawn(entity);
}

/// Reads `SimId` if present.
pub fn sim_id(world: &World, entity: Entity) -> Option<i32> {
    world.get::<SimId>(entity).map(|id| id.id)
}

/// Reads `DefId` if present.
pub fn def_id(world: &World, entity: Entity) -> Option<EntityDefId> {
    world.get::<DefId>(entity).map(|def| def.def)
}

/// Whether an entity carries the `Local` marker.
pub fn is_local(world: &World, entity: Entity) -> bool {
    world.get::<super::comp::Local>(entity).is_some()
}

/// Whether an entity carries the `Remote` marker.
pub fn is_remote(world: &World, entity: Entity) -> bool {
    world.get::<super::comp::Remote>(entity).is_some()
}

/// Component used only to prove generic spawn signatures compile in tests.
#[derive(Debug, Clone, Copy, Component)]
pub struct Marker(pub u8);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::groups::{GroupMask, compute_group_mask};
    use crate::entities::meta::EntityDef;

    fn test_def(pooled: bool) -> EntityDef {
        EntityDef {
            id: EntityDefId(0),
            name: "Test",
            class_id: 1000,
            groups: compute_group_mask(&["BaseEntity"], &[]),
            pooled,
            serialize: true,
            genio: true,
            legacy: false,
            components: vec!["BaseEntity"],
            fields: Vec::new(),
            methods: Vec::new(),
        }
    }

    #[test]
    fn ids_are_monotonic_and_check_next_id_advances() {
        let mut ids = EntityIds::new();
        assert_eq!(ids.next_id(), 0);
        assert_eq!(ids.next_id(), 1);
        ids.check_next_id(10);
        assert_eq!(ids.next_id(), 11);
        ids.reset();
        assert_eq!(ids.next_id(), 0);
    }

    #[test]
    fn spawn_remove_updates_groups_and_pool() {
        let mut world = World::new();
        let mut groups = Groups::new();
        let mut ids = EntityIds::new();
        let mut pools = EntityPools::new();
        let def = test_def(true);

        let entity = sim_spawn(&mut world, &mut groups, &mut ids, &def, Marker(1));
        assert_eq!(sim_id(&world, entity), Some(0));
        assert_eq!(def_id(&world, entity), Some(def.id));
        assert_eq!(groups.all.len(), 1);

        sim_remove(&mut world, &mut groups, &mut pools, &def, entity);
        assert_eq!(groups.all.len(), 0);
        assert_eq!(pools.free_total(), 1);
        assert!(world.get_entity(entity).is_err());
    }

    #[test]
    fn pool_spawn_reuses_entities() {
        let mut world = World::new();
        let mut pool = EntityPool::new(|_, _| {});
        let first = pool.spawn(&mut world, (Marker(1),));
        pool.recycle(first);
        assert_eq!(pool.free_len(), 1);
        let second = pool.spawn(&mut world, (Marker(2),));
        assert_eq!(first, second);
    }

    #[test]
    fn local_remote_markers() {
        let mut world = World::new();
        let local = world.spawn((super::super::comp::Local,)).id();
        let plain = world.spawn_empty().id();
        assert!(is_local(&world, local));
        assert!(!is_remote(&world, local));
        assert!(!is_local(&world, plain));
    }

    #[test]
    fn group_mask_is_nonempty() {
        let def = test_def(false);
        assert_ne!(def.groups, GroupMask::empty());
    }
}
