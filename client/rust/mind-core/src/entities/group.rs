// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EntityGroup` / `EntityIndexer` — typed entity sets with O(1) removal.
//!
//! Ported from `core/src/mindustry/entities/EntityGroup.java` and
//! `entities/EntityIndexer.java`. Storage is an insertion-stable slot slab
//! (OD-05-A: *not* Java swap-removal) with a lookup-only entity→slot map, a
//! free-slot list for reuse, an optional id map (`mapping = true`), and an
//! optional spatial-indexer hook.

use std::collections::HashMap;

use bevy_ecs::entity::Entity;

/// Callback used to fix up a spatial index when an entity's slot changes.
///
/// Ported from `entities/EntityIndexer.java` (`change(T, int)`).
pub trait EntityIndexer {
    /// Called after entity `t` moves to slot `index`.
    fn change(&mut self, t: Entity, index: u32);
}

/// A no-op indexer for groups without a spatial index.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopIndexer;

impl EntityIndexer for NoopIndexer {
    fn change(&mut self, _t: Entity, _index: u32) {}
}

/// Typed entity set. Slots are append-only; removed slots are reused via a free
/// list, so iteration never swaps the relative order of surviving entities.
///
/// Ported from `core/src/mindustry/entities/EntityGroup.java`.
#[derive(Debug, Default)]
pub struct EntityGroup {
    slots: Vec<Option<Entity>>,
    free_slots: Vec<u32>,
    index: HashMap<Entity, u32>,
    id_map: HashMap<i32, Entity>,
    mapping: bool,
    spatial: bool,
    clearing: bool,
}

impl EntityGroup {
    /// Creates an empty group; `mapping` enables `get_by_id`, `spatial` is
    /// recorded for the (plan 10/11) quadtree hook.
    pub fn new(mapping: bool, spatial: bool) -> Self {
        Self {
            mapping,
            spatial,
            ..Self::default()
        }
    }

    /// Whether this group maintains an integer-id map.
    pub const fn is_mapping(&self) -> bool {
        self.mapping
    }

    /// Whether this group has a spatial index.
    pub const fn is_spatial(&self) -> bool {
        self.spatial
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the group is empty.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Whether `entity` belongs to the group.
    pub fn contains(&self, entity: Entity) -> bool {
        self.index.contains_key(&entity)
    }

    /// Slot index of `entity`, if present.
    pub fn index_of(&self, entity: Entity) -> Option<u32> {
        self.index.get(&entity).copied()
    }

    /// Entity at slot `index`, if occupied. Port of `EntityGroup.get(int)`.
    pub fn get_slot(&self, index: u32) -> Option<Entity> {
        self.slots.get(index as usize).copied().flatten()
    }

    /// Adds `entity` and returns its slot. Re-adding is idempotent.
    ///
    /// Port of `EntityGroup.add(T)`.
    pub fn add(&mut self, entity: Entity) -> u32 {
        if let Some(slot) = self.index.get(&entity) {
            return *slot;
        }
        let slot = match self.free_slots.pop() {
            Some(slot) => {
                self.slots[slot as usize] = Some(entity);
                slot
            }
            None => {
                let slot = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
                self.slots.push(Some(entity));
                slot
            }
        };
        self.index.insert(entity, slot);
        slot
    }

    /// Adds `entity` and registers its integer id in the id map.
    ///
    /// Port of `EntityGroup.add(T)` plus `Entities.id()` bookkeeping.
    pub fn add_index(&mut self, entity: Entity, id: i32) -> u32 {
        let slot = self.add(entity);
        if self.mapping {
            self.id_map.insert(id, entity);
        }
        slot
    }

    /// Removes `entity`, returning whether it was present.
    ///
    /// Port of `EntityGroup.remove(T)` (stable slab removal, not swap-remove).
    pub fn remove(&mut self, entity: Entity) -> bool {
        if self.clearing {
            return false;
        }
        let Some(slot) = self.index.remove(&entity) else {
            return false;
        };
        self.slots[slot as usize] = None;
        self.free_slots.push(slot);
        self.id_map.retain(|_, value| *value != entity);
        true
    }

    /// Removes `entity` at a known slot, falling back to a lookup when the hint
    /// is wrong (port of the Java `removeIndex` wrong-index fallback).
    pub fn remove_index(&mut self, entity: Entity, index: u32) -> bool {
        match self.index.get(&entity) {
            Some(slot) if *slot == index => self.remove(entity),
            // Wrong hint: fall back to the map lookup (never panics).
            Some(_) => self.remove(entity),
            None => false,
        }
    }

    /// Removes every member. Nested removals during `clear` are no-ops.
    ///
    /// Port of `EntityGroup.clear()` (`clearing` guard).
    pub fn clear(&mut self) {
        self.clearing = true;
        self.slots.clear();
        self.free_slots.clear();
        self.index.clear();
        self.id_map.clear();
        self.clearing = false;
    }

    /// Iterates live entities in slot order (index loop; no iterator allocation).
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().filter_map(|slot| *slot)
    }

    /// Entities in slot order, collected into `out` (reuses capacity).
    pub fn collect_into(&self, out: &mut Vec<Entity>) {
        out.clear();
        out.extend(self.iter());
    }

    /// `true` when no entity id appears in two groups' id maps — the
    /// `EntityGroup.checkIDCollisions` invariant helper.
    pub fn get_by_id(&self, id: i32) -> Option<Entity> {
        if self.mapping {
            self.id_map.get(&id).copied()
        } else {
            None
        }
    }

    /// Removes an entity by integer id. Port of `EntityGroup.removeByID(int)`.
    pub fn remove_by_id(&mut self, id: i32) -> Option<Entity> {
        let entity = self.id_map.get(&id).copied()?;
        self.remove(entity);
        Some(entity)
    }

    /// Sorted copy of the id map keys (deterministic debug/probe helper).
    pub fn ids(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self.id_map.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Re-inserts `entity` at an exact slot, fixing up the index. Used when a
    /// spatial index swaps slots (port of `EntityIndexer.change`).
    pub fn set_slot(&mut self, entity: Entity, index: u32) {
        if let Some(old) = self.index.get(&entity).copied() {
            let old = old as usize;
            if old < self.slots.len() && self.slots[old] == Some(entity) {
                self.slots[old] = None;
                if !self.free_slots.contains(&(old as u32)) {
                    self.free_slots.push(old as u32);
                }
            }
        }
        let index_usize = index as usize;
        if index_usize >= self.slots.len() {
            self.slots.resize(index_usize + 1, None);
        }
        self.slots[index_usize] = Some(entity);
        self.free_slots.retain(|slot| *slot != index);
        self.index.insert(entity, index);
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;

    fn entities(world: &mut World, count: usize) -> Vec<Entity> {
        (0..count).map(|_| world.spawn_empty().id()).collect()
    }

    #[test]
    fn add_remove_index_consistency() {
        let mut world = World::new();
        let ids = entities(&mut world, 64);
        let mut group = EntityGroup::new(true, false);
        for (i, id) in ids.iter().enumerate() {
            let slot = group.add_index(*id, i as i32);
            assert_eq!(group.index_of(*id), Some(slot));
        }
        assert_eq!(group.len(), 64);

        // Remove half; remaining order is stable (ascending slot order equals
        // original insertion order).
        for id in ids.iter().step_by(2) {
            assert!(group.remove(*id));
            assert!(!group.contains(*id));
        }
        assert_eq!(group.len(), 32);
        let remaining: Vec<Entity> = group.iter().collect();
        let expected: Vec<Entity> = ids.iter().skip(1).step_by(2).copied().collect();
        assert_eq!(remaining, expected);

        // Re-add reuses a free slot but stays consistent.
        let slot = group.add_index(ids[0], 0);
        assert_eq!(group.index_of(ids[0]), Some(slot));
        assert_eq!(group.get_by_id(0), Some(ids[0]));
    }

    #[test]
    fn remove_index_fallback() {
        let mut world = World::new();
        let ids = entities(&mut world, 4);
        let mut group = EntityGroup::new(false, false);
        let slot = group.add(ids[0]);
        // Wrong hint still removes (map fallback), missing entity is a no-op.
        assert!(group.remove_index(ids[0], slot.wrapping_add(7)));
        assert!(!group.remove_index(ids[1], 0));
        assert!(group.is_empty());
    }

    #[test]
    fn clear_guards_nested_removal() {
        let mut world = World::new();
        let ids = entities(&mut world, 8);
        let mut group = EntityGroup::new(true, false);
        for (i, id) in ids.iter().enumerate() {
            group.add_index(*id, i as i32);
        }
        group.clear();
        assert!(group.is_empty());
        assert_eq!(group.get_by_id(0), None);
    }
}
