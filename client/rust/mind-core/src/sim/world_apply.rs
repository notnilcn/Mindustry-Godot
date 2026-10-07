// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Relay-side world-mutation apply (plan 15 §3.3.1; plan 05 §6.4).
//!
//! Implements the deterministic apply half of the player input mutations the
//! P0 `Sim` could not reach: `InputHandler.transferInventory`/`requestItem`/
//! `dropItem` (`Inventory`), `InputHandler.deletePlans` (`DeletePlans`) and
//! `InputHandler.commandBuilding` (`CommandBuilding`).
//!
//! State ownership follows the D2 relay model: the controlled player's carried
//! stack and the per-building item inventories are relay-owned (`Sim`), with
//! no authoritative server sim. Every collection is a `BTreeMap`/`Vec` keyed in
//! ascending order, so no `HashMap` iteration can leak into sim state.
//!
//! Unit payload pickup/drop (`Payloadc`) is not ported: `entities::comp::unit::
//! comp::PayloadComp` carries only `capacity`, and plan 08's `PayloadHolder` is
//! the building-holder path, not the unit carrier. `SimCommand::Payload` stays
//! `CommandError::Unsupported` (owner plan 08/11, recorded in `sim::mod`).

use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::content::id::ItemId;
use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use crate::determinism::{Checksum, Checksummer, CommandError};
use crate::input::queue::BuildQueue;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;

/// Deterministic relay-side world-mutation state owned by [`crate::sim::Sim`].
///
/// Built lazily on the first world-mutation command (see
/// `Sim::ensure_world_apply`) so P0 worlds never pay for content boot.
pub struct WorldApplyRuntime {
    content: ContentRegistry,
    /// Per-building item stacks keyed by packed `TilePos` (`ItemModule`).
    inventories: BTreeMap<i32, ItemModule>,
    /// The controlled player's carried stack (`Unit.stack`).
    player_item: Option<(ItemId, i32)>,
    /// Build-plan queue (`BuildQueue` over plan 11's `BuilderComp.plans`).
    plans: BuildQueue,
    /// `command_pos` per commanded building keyed by packed `TilePos`
    /// (`Building.onCommand`/`getCommandPosition`).
    commanded: BTreeMap<i32, (f32, f32)>,
}

impl WorldApplyRuntime {
    /// Boots the vanilla content registry (infallible for committed content;
    /// on an unexpected failure commands degrade to deterministic no-ops).
    pub fn new() -> Self {
        let mut content =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(content) => content,
                Err(_) => ContentRegistry::new(true),
            };
        let _ = content.init();
        Self {
            content,
            inventories: BTreeMap::new(),
            player_item: None,
            plans: BuildQueue::new(),
            commanded: BTreeMap::new(),
        }
    }

    /// Read-only content registry.
    pub fn content(&self) -> &ContentRegistry {
        &self.content
    }

    /// Number of items in the item content space (dense array size).
    pub fn items_len(&self) -> usize {
        self.content.items().len()
    }

    /// The controlled player's carried stack (`Unit.stack`).
    pub fn player_item(&self) -> Option<(ItemId, i32)> {
        self.player_item
    }

    /// Seeds/clears the controlled player's carried stack (host/test surface).
    pub fn set_player_item(&mut self, item: Option<(u16, i32)>) -> Result<(), CommandError> {
        self.player_item = match item {
            Some((raw, amount)) => Some((self.resolve_item(raw)?, amount.max(0))),
            None => None,
        };
        Ok(())
    }

    /// The item module tracked for a building tile, if any.
    pub fn inventory_at(&self, pos: TilePos) -> Option<&ItemModule> {
        self.inventories.get(&pos.pack())
    }

    /// Amount of `item` stored at `pos` (`0` when the tile has no inventory).
    pub fn held_at(&self, pos: TilePos, item: ItemId) -> i32 {
        self.inventory_at(pos).map(|m| m.get(item)).unwrap_or(0)
    }

    /// `command_pos` recorded for a commanded building, if any.
    pub fn commanded_target(&self, pos: TilePos) -> Option<(f32, f32)> {
        self.commanded.get(&pos.pack()).copied()
    }

    /// The build-plan queue (`BuilderComp.plans` seam).
    pub fn plans(&self) -> &BuildQueue {
        &self.plans
    }

    /// Mutable build-plan queue (host/test seeding surface).
    pub fn plans_mut(&mut self) -> &mut BuildQueue {
        &mut self.plans
    }

    fn resolve_item(&self, raw: u16) -> Result<ItemId, CommandError> {
        let id = ItemId::new(raw);
        if id.index() < self.content.items().len() {
            Ok(id)
        } else {
            Err(CommandError::UnknownContent(raw))
        }
    }

    /// The item capacity of a building block, or `None` when it holds no items.
    pub fn block_item_capacity(&self, block: BlockId) -> Option<i32> {
        self.content
            .block(block)
            .filter(|def| def.has_items)
            .map(|def| def.item_capacity)
    }

    fn add_player_item(&mut self, item: ItemId, amount: i32) {
        if amount <= 0 {
            return;
        }
        match self.player_item {
            Some((held, current)) if held == item => {
                self.player_item = Some((item, current.saturating_add(amount)));
            }
            _ => self.player_item = Some((item, amount)),
        }
    }

    /// Applies one `requestItem`/`transferInventory`/`dropItem` mutation.
    ///
    /// `kind` is the wire discriminant (`0` withdraw, `1` deposit, `2` drop).
    /// `block` is the block at the target tile (ignored for `drop`); the
    /// building-side transfer is validated through the block's item capacity.
    ///
    /// Upstream moves items between the player unit and the building through
    /// `Call.takeItems`/`Call.transferItemTo`; the player half is client-owned
    /// under D2 and mirrored here so the building half stays consistent.
    pub fn apply_inventory(
        &mut self,
        pos: TilePos,
        block: BlockId,
        kind: u8,
        item: Option<u16>,
        amount: i32,
    ) -> Result<(), CommandError> {
        match kind {
            0 => {
                // `requestItem`: building -> player unit.
                let raw = item.ok_or(CommandError::InvalidTarget)?;
                let item_id = self.resolve_item(raw)?;
                let available = self.held_at(pos, item_id);
                let take = amount.clamp(0, available);
                if take > 0 {
                    if let Some(module) = self.inventories.get_mut(&pos.pack()) {
                        module.remove(item_id, take);
                    }
                    self.add_player_item(item_id, take);
                }
                Ok(())
            }
            1 => {
                // `transferInventory`: player unit -> building.
                let raw = item.ok_or(CommandError::InvalidTarget)?;
                let item_id = self.resolve_item(raw)?;
                let held = match self.player_item {
                    Some((held, current)) if held == item_id => current,
                    _ => 0,
                };
                let requested = amount.clamp(0, held);
                if requested <= 0 {
                    return Ok(());
                }
                let capacity = self.block_item_capacity(block).unwrap_or(0);
                if capacity <= 0 {
                    return Ok(());
                }
                let slots = self.content.items().len();
                let module = self
                    .inventories
                    .entry(pos.pack())
                    .or_insert_with(|| ItemModule::with_items(slots));
                let accepted = module.add(item_id, requested, capacity);
                if accepted > 0 {
                    let left = held - accepted;
                    self.player_item = if left > 0 {
                        Some((item_id, left))
                    } else {
                        None
                    };
                }
                Ok(())
            }
            2 => {
                // `dropItem`: clears the unit stack; the drop position is FX-only.
                let _ = (pos, block, item);
                self.player_item = None;
                Ok(())
            }
            _ => Err(CommandError::InvalidTarget),
        }
    }

    /// Applies one inventory mutation against a live building's `ItemModule`.
    ///
    /// The live block runtime owns the real item storage (drills, containers,
    /// factories share the same `ItemModule` their behaviors read), so the
    /// relay's `BTreeMap` mirror would silently desync from the simulation.
    /// The player-carried stack stays relay-owned exactly as in
    /// [`Self::apply_inventory`]; only the building side moves to ECS.
    pub fn apply_inventory_entity(
        &mut self,
        world: &mut World,
        entity: Entity,
        kind: u8,
        item: Option<u16>,
        amount: i32,
    ) -> Result<(), CommandError> {
        match kind {
            0 => {
                // `requestItem`: building -> player unit.
                let raw = item.ok_or(CommandError::InvalidTarget)?;
                let item_id = self.resolve_item(raw)?;
                let take = match world.get_mut::<ItemModule>(entity) {
                    Some(mut module) => {
                        let take = amount.clamp(0, module.get(item_id));
                        if take > 0 {
                            module.remove(item_id, take);
                        }
                        take
                    }
                    None => 0,
                };
                if take > 0 {
                    self.add_player_item(item_id, take);
                }
                Ok(())
            }
            1 => {
                // `transferInventory`: player unit -> building.
                let raw = item.ok_or(CommandError::InvalidTarget)?;
                let item_id = self.resolve_item(raw)?;
                let held = match self.player_item {
                    Some((held, current)) if held == item_id => current,
                    _ => 0,
                };
                let requested = amount.clamp(0, held);
                if requested <= 0 {
                    return Ok(());
                }
                let block = world
                    .get::<crate::entities::comp::Building>(entity)
                    .map(|building| building.block);
                let capacity = block
                    .and_then(|block| {
                        world
                            .get_resource::<BlockTable>()
                            .and_then(|table| table.get(block))
                            .filter(|inst| inst.def.has_items)
                            .map(|inst| inst.def.item_capacity)
                    })
                    .unwrap_or(0);
                if capacity <= 0 {
                    return Ok(());
                }
                let accepted = match world.get_mut::<ItemModule>(entity) {
                    Some(mut module) => module.add(item_id, requested, capacity),
                    None => 0,
                };
                if accepted > 0 {
                    let left = held - accepted;
                    self.player_item = if left > 0 {
                        Some((item_id, left))
                    } else {
                        None
                    };
                }
                Ok(())
            }
            2 => {
                // `dropItem`: clears the unit stack; the drop position is FX-only.
                let _ = (world, entity, item);
                self.player_item = None;
                Ok(())
            }
            _ => Err(CommandError::InvalidTarget),
        }
    }

    /// `deletePlans`: remove every queued plan matching a packed position.
    ///
    /// Upstream scans `player.team().data().plans` (O(n²)); the `BuildQueue`
    /// seam is the plan-11 storage this plan calls through (plan 15 §3.7).
    pub fn apply_delete_plans(&mut self, positions: &[i32]) -> Result<(), CommandError> {
        for &packed in positions {
            let pos = TilePos::from_pack(packed);
            self.plans.remove_plan(pos.x() as i32, pos.y() as i32);
        }
        Ok(())
    }

    /// `commandBuilding`: record `command_pos` for one building tile
    /// (`Building.onCommand`). Callers validate the tile/building first.
    pub fn apply_command_building(&mut self, pos: TilePos, x: f32, y: f32) {
        self.commanded.insert(pos.pack(), (x, y));
    }

    /// Deterministic digest of the relay world state (ascending key order).
    pub fn checksum(&self) -> Checksum {
        let mut c = Checksummer::new();
        c.part(&(self.inventories.len() as u64));
        for (key, module) in &self.inventories {
            c.part(&(*key as u64));
            c.part(&module.total);
            for amount in &module.items {
                c.part(amount);
            }
        }
        match self.player_item {
            Some((item, amount)) => {
                c.part(&1u8);
                c.part(&item.raw());
                c.part(&amount);
            }
            None => {
                c.part(&0u8);
                c.part(&0u16);
                c.part(&0i32);
            }
        }
        c.part(&(self.plans.len() as u64));
        for plan in self.plans.iter() {
            c.part(&(plan.x as u64));
            c.part(&(plan.y as u64));
            c.part(&plan.block.raw());
        }
        c.part(&(self.commanded.len() as u64));
        for (key, (x, y)) in &self.commanded {
            c.part(&(*key as u64));
            c.part(&x.to_bits());
            c.part(&y.to_bits());
        }
        c.finish()
    }
}

impl Default for WorldApplyRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Unit payload kinds (`requestUnitPayload`/`requestBuildPayload`/drop).
pub const PAYLOAD_PICKUP_UNIT: u8 = 0;
/// Pick up a whole building.
pub const PAYLOAD_PICKUP_BUILD: u8 = 1;
/// Drop the carried payload.
pub const PAYLOAD_DROP: u8 = 2;

/// Inventory kinds (`requestItem`/`transferInventory`/`dropItem`).
pub const INVENTORY_WITHDRAW: u8 = 0;
/// Deposit the carried item into a building.
pub const INVENTORY_DEPOSIT: u8 = 1;
/// Drop the carried item on the ground.
pub const INVENTORY_DROP: u8 = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::plan::ClientPlan;

    #[test]
    fn deposit_transfers_carried_stack_into_building() {
        let mut runtime = WorldApplyRuntime::new();
        let copper = runtime.content().item_by_name("copper").expect("copper").id;
        let container = runtime
            .content()
            .block_by_name("container")
            .expect("container")
            .id;
        runtime
            .set_player_item(Some((copper.raw(), 10)))
            .expect("item");
        let pos = TilePos::new(3, 4);
        runtime
            .apply_inventory(pos, container, INVENTORY_DEPOSIT, Some(copper.raw()), 6)
            .expect("deposit");
        assert_eq!(runtime.held_at(pos, copper), 6);
        assert_eq!(runtime.player_item(), Some((copper, 4)));
    }

    #[test]
    fn withdraw_moves_building_items_to_player() {
        let mut runtime = WorldApplyRuntime::new();
        let copper = runtime.content().item_by_name("copper").expect("copper").id;
        let container = runtime
            .content()
            .block_by_name("container")
            .expect("container")
            .id;
        let capacity = runtime.block_item_capacity(container).expect("capacity");
        let slots = runtime.items_len();
        let pos = TilePos::new(1, 1);
        let mut module = ItemModule::with_items(slots);
        module.add(copper, capacity.min(20), capacity);
        runtime.inventories.insert(pos.pack(), module);
        runtime
            .apply_inventory(pos, container, INVENTORY_WITHDRAW, Some(copper.raw()), 7)
            .expect("withdraw");
        assert_eq!(runtime.held_at(pos, copper), capacity.min(20) - 7);
        assert_eq!(runtime.player_item(), Some((copper, 7)));
    }

    #[test]
    fn drop_clears_player_item_and_unknown_item_errors() {
        let mut runtime = WorldApplyRuntime::new();
        let copper = runtime.content().item_by_name("copper").expect("copper").id;
        runtime
            .set_player_item(Some((copper.raw(), 5)))
            .expect("item");
        runtime
            .apply_inventory(TilePos::new(0, 0), BlockId::AIR, INVENTORY_DROP, None, 0)
            .expect("drop");
        assert_eq!(runtime.player_item(), None);
        assert_eq!(
            runtime.set_player_item(Some((u16::MAX, 1))).unwrap_err(),
            CommandError::UnknownContent(u16::MAX)
        );
    }

    #[test]
    fn delete_plans_removes_matching_positions() {
        use crate::content::BlockId;
        let mut runtime = WorldApplyRuntime::new();
        runtime
            .plans_mut()
            .add_build(ClientPlan::place(2, 3, 0, BlockId::STONE_WALL), false);
        runtime
            .plans_mut()
            .add_build(ClientPlan::place(5, 6, 0, BlockId::STONE_WALL), false);
        assert_eq!(runtime.plans().len(), 2);
        runtime
            .apply_delete_plans(&[TilePos::new(2, 3).pack()])
            .expect("delete");
        assert_eq!(runtime.plans().len(), 1);
        assert!(runtime.plans().get(5, 6).is_some());
    }

    #[test]
    fn command_building_records_target() {
        let mut runtime = WorldApplyRuntime::new();
        let pos = TilePos::new(9, 9);
        assert_eq!(runtime.commanded_target(pos), None);
        runtime.apply_command_building(pos, 12.5, -3.0);
        assert_eq!(runtime.commanded_target(pos), Some((12.5, -3.0)));
    }

    #[test]
    fn checksum_is_deterministic() {
        fn build() -> WorldApplyRuntime {
            let mut runtime = WorldApplyRuntime::new();
            let copper = runtime.content().item_by_name("copper").expect("copper").id;
            runtime.set_player_item(Some((copper.raw(), 4))).unwrap();
            runtime
                .apply_inventory(TilePos::new(1, 1), BlockId::new(0), INVENTORY_DROP, None, 0)
                .unwrap();
            runtime.apply_command_building(TilePos::new(2, 2), 1.0, 2.0);
            runtime
        }
        assert_eq!(build().checksum(), build().checksum());
    }
}
