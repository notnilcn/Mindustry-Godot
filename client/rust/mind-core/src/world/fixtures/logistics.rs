// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logistics test fixtures: a deterministic item source/sink on the flat
//! `BuildHarness` map, reused by plans 09/11 (plan 08 §7.2).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::determinism::{Checksum, Checksummer};
use crate::entities::comp::Building;
use crate::world::blocks::distribution::{conveyor::ConveyorBuild, transfer};
use crate::world::{BlockTable, BuildHarness};

/// Fixed seed for logistics scenarios.
pub const LOGISTICS_FIXTURE_SEED: u64 = 1234;

/// A fixture item source: emits `per_tick` items into `offload` each update.
#[derive(Debug, Clone, Copy)]
pub struct SourceBehavior {
    /// Item emitted.
    pub item: ItemId,
    /// Items emitted per tick.
    pub per_tick: i32,
}

impl crate::world::BuildingBehavior for SourceBehavior {
    fn update_tile(&self, world: &mut World, e: Entity) {
        for _ in 0..self.per_tick {
            transfer::offload(world, e, self.item);
        }
    }
}

/// Convenience constructor for [`SourceBehavior`].
pub fn source_behavior(item: ItemId) -> SourceBehavior {
    SourceBehavior { item, per_tick: 1 }
}

/// FNV-1a checksum over logistics state (belt arrays + item modules), in stable
/// building sequence order. Supplements [`BuildHarness::checksum`], which only
/// covers grid/block identity.
pub fn logistics_checksum(world: &World) -> Checksum {
    let mut entities: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            entity_ref.get::<Building>()?;
            let seq = entity_ref
                .get::<crate::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            Some((seq, entity_ref.id()))
        })
        .collect();
    entities.sort_by_key(|(seq, entity)| (*seq, entity.index()));

    let mut c = Checksummer::new();
    for (seq, entity) in entities {
        c.part(&seq);
        if let Some(belt) = world.get::<ConveyorBuild>(entity) {
            c.part(&1u8);
            c.part(&belt.len);
            for i in 0..belt.len as usize {
                c.part(&belt.ids[i].raw());
                c.part(&belt.xs[i].to_bits());
                c.part(&belt.ys[i].to_bits());
            }
            c.part(&belt.clog_heat.to_bits());
        } else {
            c.part(&0u8);
        }
        if let Some(items) = world.get::<crate::world::ItemModule>(entity) {
            c.part(&2u8);
            c.part(&items.total);
            for (item, amount) in items.stacks() {
                c.part(&item.raw());
                c.part(&amount);
            }
        } else {
            c.part(&0u8);
        }
    }
    c.finish()
}

/// Ensures the default behavior registry contains the logistics families.
pub fn registry_has_logistics(harness: &BuildHarness) -> bool {
    harness
        .world
        .get_resource::<BlockTable>()
        .is_some_and(|table| table.get_named("conveyor").is_some())
}
