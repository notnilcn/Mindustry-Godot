// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Sorter` build behavior (`world/blocks/distribution/Sorter.java`) — plan 08
//! M2. Instant transfer, the rotation-bitfield branch choice, `sortItem` config
//! and the rev 1 legacy-buffer / rev 2 item IO shapes.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;
use crate::world::item_buffer::DirectionalItemBuffer;

use super::transfer;

/// `Sorter.SorterBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct SorterBuild {
    /// Configured target item (`sortItem`).
    pub sort_item: Option<ItemId>,
}

fn instant_transfer(block: BlockId, table: &BlockTable) -> bool {
    table
        .get(block)
        .is_some_and(|inst| matches!(inst.def.kind, BlockKind::Sorter | BlockKind::OverflowGate))
}

/// `Sorter` behavior (`sorter`, `inverted-sorter`).
#[derive(Debug, Clone, Copy)]
pub struct SorterBehavior {
    /// `Sorter.invert`.
    pub invert: bool,
}

impl SorterBehavior {
    /// `sorter`.
    pub const NORMAL: SorterBehavior = SorterBehavior { invert: false };
    /// `inverted-sorter`.
    pub const INVERTED: SorterBehavior = SorterBehavior { invert: true };
}

impl BuildingBehavior for SorterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<SorterBuild>(e).is_none() {
            world.entity_mut(e).insert(SorterBuild::default());
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<SorterBuild>(e).and_then(|s| s.sort_item) {
            Some(item) => ConfigValue::Item(item),
            None => ConfigValue::None,
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        if let Some(mut sorter) = world.get_mut::<SorterBuild>(e) {
            sorter.sort_item = match value {
                ConfigValue::Item(item) => Some(item),
                _ => None,
            };
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, item: ItemId) -> bool {
        let (target, _flip) = self.get_tile_target(world, e, item, source);
        let Some(target) = target else {
            return false;
        };
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(target))
            .is_some_and(|(a, b)| a.team == b.team);
        same_team && transfer::dispatch_accept_item(world, target, e, item)
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let (target, flip) = self.get_tile_target(world, e, item, source);
        if let Some(dir) = flip
            && let Some(mut building) = world.get_mut::<Building>(e)
        {
            building.rotation ^= 1 << dir;
        }
        if let Some(target) = target {
            transfer::dispatch_handle_item(world, target, e, item);
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        2
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let id = world
            .get::<SorterBuild>(e)
            .and_then(|s| s.sort_item)
            .map(|item| item.raw() as i16)
            .unwrap_or(-1);
        w.s(id);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut sorter = world.get::<SorterBuild>(e).cloned().unwrap_or_default();
        if let Ok(id) = r.s() {
            sorter.sort_item = (id >= 0).then(|| ItemId::new(id as u16));
        }
        if revision == 1 {
            let mut legacy = DirectionalItemBuffer::new(20);
            let _ = legacy.read(r);
        }
        world.entity_mut(e).insert(sorter);
    }
}

impl SorterBehavior {
    /// `SorterBuild.getTileTarget`; returns the target and, when the side-choice
    /// branch was taken, the direction whose rotation bit should flip.
    fn get_tile_target(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        source: Entity,
    ) -> (Option<Entity>, Option<u8>) {
        let Some(dir) = nonneg(transfer::relative_to_edge(world, e, source)) else {
            return (None, None);
        };
        let enabled = world.get::<Building>(e).map(|b| b.enabled).unwrap_or(false);
        let sort_item = world.get::<SorterBuild>(e).and_then(|s| s.sort_item);

        if ((sort_item == Some(item)) != self.invert) == enabled {
            let near = transfer::nearby(world, e, dir);
            let same_source = instant_of(world, source);
            let same_near = near.is_some_and(|n| instant_of(world, n));
            if same_source && same_near {
                return (None, None);
            }
            (near, None)
        } else {
            let dir_a = (dir + 3) % 4;
            let dir_b = (dir + 1) % 4;
            let a = transfer::nearby(world, e, dir_a);
            let b = transfer::nearby(world, e, dir_b);
            let source_inst = instant_of(world, source);
            let ac = a.is_some_and(|candidate| {
                !(source_inst && instant_of(world, candidate))
                    && transfer::dispatch_accept_item(world, candidate, e, item)
            });
            let bc = b.is_some_and(|candidate| {
                !(source_inst && instant_of(world, candidate))
                    && transfer::dispatch_accept_item(world, candidate, e, item)
            });
            if ac && !bc {
                (a, None)
            } else if bc && !ac {
                (b, None)
            } else if !bc {
                (None, None)
            } else {
                let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
                let chosen = if (rotation & (1 << dir)) == 0 { a } else { b };
                (chosen, Some(dir))
            }
        }
    }
}

fn instant_of(world: &World, entity: Entity) -> bool {
    world
        .get::<Building>(entity)
        .zip(world.get_resource::<BlockTable>())
        .is_some_and(|(building, table)| instant_transfer(building.block, table))
}

fn nonneg(dir: i8) -> Option<u8> {
    (dir >= 0).then_some(dir as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::modules::ItemModule;

    #[test]
    fn sorter_routes_configured_item_forward() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let coal = harness.content().item_id("coal").expect("coal");
        let conveyor = harness.content().block_id("conveyor").expect("conveyor");
        let sorter = harness.content().block_id("sorter").expect("sorter");
        let vault = harness.content().block_id("vault").expect("vault");
        assert!(harness.place(4, 4, conveyor, 0, true));
        assert!(harness.place(5, 4, sorter, 0, true));
        assert!(harness.place(6, 4, vault, 0, true));
        assert!(harness.place(5, 3, vault, 0, true));
        assert!(harness.place(5, 5, vault, 0, true));
        assert!(harness.configure(5, 4, ConfigValue::Item(copper)));
        let belt = harness.build_at(4, 4).expect("belt");
        let sorter_e = harness.build_at(5, 4).expect("sorter");

        // Copper matches the config -> forward output (6,4).
        assert!(transfer::dispatch_accept_item(
            &harness.world,
            sorter_e,
            belt,
            copper
        ));
        transfer::dispatch_handle_item(&mut harness.world, sorter_e, belt, copper);
        let forward = harness
            .world
            .get::<ItemModule>(harness.build_at(6, 4).unwrap())
            .map(|items| items.get(copper))
            .unwrap_or(0);
        assert_eq!(forward, 1, "configured item should route forward");

        // Coal does not match -> routed to a side.
        assert!(transfer::dispatch_accept_item(
            &harness.world,
            sorter_e,
            belt,
            coal
        ));
        transfer::dispatch_handle_item(&mut harness.world, sorter_e, belt, coal);
        let side = [(5, 3), (5, 5)]
            .iter()
            .filter_map(|(x, y)| harness.build_at(*x, *y))
            .filter_map(|e| harness.world.get::<ItemModule>(e))
            .map(|items| items.get(coal))
            .sum::<i32>();
        assert_eq!(side, 1, "unconfigured item should route to a side");
    }

    #[test]
    fn sorter_config_roundtrip() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let sorter = harness.content().block_id("sorter").expect("sorter");
        assert!(harness.place(5, 4, sorter, 0, true));
        assert!(harness.configure(5, 4, ConfigValue::Item(copper)));
        let read = harness
            .build_at(5, 4)
            .map(|e| crate::world::config::read_config(&harness.world, e));
        assert_eq!(read, Some(ConfigValue::Item(copper)));
    }
}
