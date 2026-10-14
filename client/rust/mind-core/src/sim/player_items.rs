// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Item withdraw/deposit/drop against the live player unit
//! (`InputHandler.requestItem`/`transferInventory`/`dropItem` region,
//! `core/src/mindustry/input/InputHandler.java:497-535`, `677-688`).
//!
//! The relay path (`WorldApplyRuntime.player_item`) stays for headless/relay
//! callers; with a live player unit the real stack is the unit's `ItemsComp`
//! (`UnitComp.stack`) and every transfer runs Java's range/`interactable`/
//! `acceptStack` checks (`Vars.itemTransferRange` = 220 world units).

use bevy_ecs::entity::Entity;

use crate::content::ItemId;
use crate::entities::comp::unit::comp::ItemsComp;
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::world::TilePos;
use crate::world::blocks::distribution::transfer::{
    dispatch_accept_stack, dispatch_handle_stack, dispatch_remove_stack,
};

use super::Sim;

impl Sim {
    /// `Vars.itemTransferRange`: maximum world distance for a unit ⇄ building
    /// item transfer (`InputHandler.java:497`, `513`).
    pub const ITEM_TRANSFER_RANGE: f32 = 220.0;

    /// The player unit's carried stack (`unit.stack`), if any.
    pub fn player_unit_stack(&self) -> Option<(ItemId, i32)> {
        let entity = self.player_unit?;
        self.ecs
            .0
            .get::<ItemsComp>(entity)
            .and_then(|items| items.item)
    }

    /// `InputHandler.requestItem`/`transferInventory`/`dropItem` against the
    /// live player unit. `kind` is the `SimCommand::Inventory` discriminant
    /// (`0` withdraw, `1` deposit, `2` drop). Returns `true` when items moved.
    pub fn apply_player_inventory(
        &mut self,
        kind: u8,
        pos: TilePos,
        item: Option<u16>,
        amount: i32,
    ) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let Some(unit_pos) = self.ecs.0.get::<Pos>(unit).map(|pos| (pos.x, pos.y)) else {
            return false;
        };
        let unit_team = self
            .ecs
            .0
            .get::<TeamComp>(unit)
            .map(|team| team.team)
            .unwrap_or(0);

        // `dropItem` (`InputHandler.java:677-688`): clear the stack; the drop
        // position only feeds FX.
        if kind == 2 {
            let had = self
                .ecs
                .0
                .get::<ItemsComp>(unit)
                .and_then(|items| items.item)
                .is_some();
            if let Some(mut items) = self.ecs.0.get_mut::<ItemsComp>(unit) {
                items.clear_item();
            }
            return had;
        }
        if kind > 1 {
            return false;
        }

        let Some(entity) = self.grid.entity_at(pos) else {
            return false;
        };
        // Building centre (`Building.tile` world position + `block.offset`).
        let Some((bx, by)) = self.ecs.0.get::<Building>(entity).map(|building| {
            let offset = self
                .block_runtime
                .as_ref()
                .and_then(|runtime| runtime.content().block(building.block))
                .map(|def| def.offset)
                .unwrap_or(0.0);
            let unit = crate::config::TILESIZE as f32;
            (
                building.tile.x() as f32 * unit + offset,
                building.tile.y() as f32 * unit + offset,
            )
        }) else {
            return false;
        };
        let dx = bx - unit_pos.0;
        let dy = by - unit_pos.1;
        if dx * dx + dy * dy > Self::ITEM_TRANSFER_RANGE * Self::ITEM_TRANSFER_RANGE {
            return false;
        }
        let building_team = self
            .ecs
            .0
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);

        match kind {
            0 => {
                // `requestItem` (`InputHandler.java:497-511`): needs a specific
                // item, an `interactable` building (same team or derelict) and
                // a positive amount.
                let Some(raw) = item else {
                    return false;
                };
                let item_id = ItemId::new(raw);
                if !(building_team == unit_team || building_team == 0) {
                    return false;
                }
                if amount <= 0 {
                    return false;
                }
                // `UnitComp.maxAccepted(item)`: one item type at a time.
                let held = self
                    .ecs
                    .0
                    .get::<ItemsComp>(unit)
                    .and_then(|items| items.item);
                let max_accepted = match held {
                    Some((held_item, held_amount)) if held_item != item_id && held_amount > 0 => 0,
                    Some((_, held_amount)) => self.player_item_capacity(unit) - held_amount,
                    None => self.player_item_capacity(unit),
                };
                let take = amount.min(max_accepted.max(0));
                if take <= 0 {
                    return false;
                }
                let removed = dispatch_remove_stack(&mut self.ecs.0, entity, item_id, take);
                if removed > 0 {
                    let mut items = self
                        .ecs
                        .0
                        .get::<ItemsComp>(unit)
                        .copied()
                        .unwrap_or_default();
                    items.add_item(item_id, removed);
                    self.ecs.0.entity_mut(unit).insert(items);
                }
                removed > 0
            }
            1 => {
                // `transferInventory` (`InputHandler.java:513-535`): deposit the
                // whole held stack into a building that accepts it (the wire's
                // item/amount are ignored; Java uses `unit.item()`/`stack.amount`).
                // `BuildingComp.allowDeposit()` (`BuildingComp.java:745`)
                // gates the transfer on `!state.rules.onlyDepositCore` (cores
                // set `alwaysAllowDeposit`).
                let Some((held, held_amount)) = self
                    .ecs
                    .0
                    .get::<ItemsComp>(unit)
                    .and_then(|items| items.item)
                else {
                    return false;
                };
                if held_amount <= 0 {
                    return false;
                }
                if !self.building_allows_deposit(entity) {
                    return false;
                }
                let accepted =
                    dispatch_accept_stack(&self.ecs.0, entity, held, held_amount, Some(unit));
                if accepted <= 0 {
                    return false;
                }
                // `transferItemTo` (`InputHandler.java:248-260`).
                let left = held_amount - accepted;
                if let Some(mut items) = self.ecs.0.get_mut::<ItemsComp>(unit) {
                    items.item = if left > 0 { Some((held, left)) } else { None };
                }
                dispatch_handle_stack(&mut self.ecs.0, entity, held, accepted);
                true
            }
            _ => false,
        }
    }

    /// `BuildingComp.allowDeposit()` (`BuildingComp.java:745`):
    /// `block.alwaysAllowDeposit || !state.rules.onlyDepositCore`. Cores set
    /// `alwaysAllowDeposit = true` (`CoreBlock.java:75`); Erekir sets the rule
    /// (`Planets.java:82`).
    fn building_allows_deposit(&self, entity: Entity) -> bool {
        let only_core = crate::game::runtime::rules_of(&self.ecs.0)
            .is_some_and(|rules| rules.only_deposit_core);
        if !only_core {
            return true;
        }
        self.ecs
            .0
            .get::<Building>(entity)
            .and_then(|building| {
                self.block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(building.block))
            })
            .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock)
    }

    /// `InputHandler.tryDropItems` (`InputHandler.java:2286-2308`) deposit
    /// check without mutation: the held stack amount the building at `pos`
    /// would accept, `0` when none can. Java gates the deposit on
    /// `acceptStack(...) > 0 && build.interactable(team) && block.hasItems`.
    pub fn player_deposit_accepted(&self, pos: TilePos) -> i32 {
        let Some(unit) = self.player_unit else {
            return 0;
        };
        let Some((held, held_amount)) = self.player_unit_stack() else {
            return 0;
        };
        if held_amount <= 0 {
            return 0;
        }
        let Some(entity) = self.grid.entity_at(pos) else {
            return 0;
        };
        let unit_team = self
            .ecs
            .0
            .get::<TeamComp>(unit)
            .map(|team| team.team)
            .unwrap_or(0);
        let building_team = self
            .ecs
            .0
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        if !(building_team == unit_team || building_team == 0) {
            return 0;
        }
        let has_items = self
            .ecs
            .0
            .get::<Building>(entity)
            .and_then(|building| {
                self.block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(building.block))
            })
            .is_some_and(|def| def.has_items);
        if !has_items {
            return 0;
        }
        if !self.building_allows_deposit(entity) {
            return 0;
        }
        dispatch_accept_stack(&self.ecs.0, entity, held, held_amount, Some(unit))
    }

    /// The player unit's `itemCapacity` (`UnitType.itemCapacity`).
    fn player_item_capacity(&self, unit: Entity) -> i32 {
        self.ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
            .and_then(|comp| {
                self.unit_runtime()
                    .and_then(|runtime| runtime.content.unit(comp.type_id))
            })
            .map_or(0, |def| def.item_capacity.max(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::SimCommand;
    use crate::world::blocks::defense::turrets::TurretState;
    use crate::world::blocks::storage::core_block::CoreBuild;
    use crate::world::modules::ItemModule;

    /// `sim_with_player()` shape: 64x64, both runtimes, core-shard at (10,10)
    /// team 1 and the alpha team 1 at world (80,80).
    fn sim_with_player() -> (Sim, Entity, ItemId) {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core = sim.world_block_id("core-shard").expect("core");
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block: core,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core placed");
        let copper = sim
            .unit_runtime()
            .expect("runtime")
            .content
            .item_id("copper")
            .expect("copper");
        let unit = sim
            .spawn_player_unit("alpha", 1, 80.0, 80.0, 0.0)
            .expect("player unit");
        (sim, unit, copper)
    }

    fn team_core(sim: &Sim) -> Option<Entity> {
        sim.ecs
            .0
            .iter_entities()
            .find_map(|entity_ref| entity_ref.get::<CoreBuild>().map(|_| entity_ref.id()))
    }

    fn core_copper(sim: &Sim, copper: ItemId) -> i32 {
        let core = team_core(sim).expect("core");
        sim.ecs
            .0
            .get::<ItemModule>(core)
            .map(|module| module.get(copper))
            .unwrap_or(0)
    }

    fn set_core_copper(sim: &mut Sim, copper: ItemId, amount: i32) {
        let core = team_core(sim).expect("core");
        let mut module = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
        let have = module.get(copper);
        module.remove(copper, have);
        module.add(copper, amount, 100_000);
    }

    fn set_player_stack(sim: &mut Sim, unit: Entity, item: ItemId, amount: i32) {
        sim.ecs.0.entity_mut(unit).insert(ItemsComp {
            item: Some((item, amount)),
        });
    }

    #[test]
    fn withdraw_from_core_caps_at_item_capacity() {
        let (mut sim, _unit, copper) = sim_with_player();
        set_core_copper(&mut sim, copper, 100);
        assert!(sim.apply_player_inventory(0, TilePos::new(10, 10), Some(copper.raw()), 50));
        assert_eq!(core_copper(&sim, copper), 70, "50 capped at the 30 stack");
        assert_eq!(sim.player_unit_stack(), Some((copper, 30)));
    }

    #[test]
    fn withdraw_rejects_other_item_when_holding() {
        let (mut sim, _unit, copper) = sim_with_player();
        set_core_copper(&mut sim, copper, 100);
        assert!(sim.apply_player_inventory(0, TilePos::new(10, 10), Some(copper.raw()), 30));
        let lead = sim
            .unit_runtime()
            .expect("runtime")
            .content
            .item_id("lead")
            .expect("lead");
        // `maxAccepted`: a different item while holding -> 0.
        assert!(!sim.apply_player_inventory(0, TilePos::new(10, 10), Some(lead.raw()), 10));
        assert_eq!(sim.player_unit_stack(), Some((copper, 30)));
        assert_eq!(core_copper(&sim, lead), 0);
    }

    #[test]
    fn deposit_into_duo_takes_ammo_amount() {
        let (mut sim, unit, copper) = sim_with_player();
        let duo = sim.world_block_id("duo").expect("duo");
        sim.command(SimCommand::Place {
            x: 13,
            y: 10,
            block: duo,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("duo placed");
        set_player_stack(&mut sim, unit, copper, 30);
        assert!(sim.apply_player_inventory(1, TilePos::new(13, 10), None, 0));
        // 15 copper x ammoMultiplier 2 fills the duo's 30 ammo.
        assert_eq!(sim.player_unit_stack(), Some((copper, 15)));
        let turret = sim.grid.entity_at(TilePos::new(13, 10)).expect("turret");
        let state = sim.ecs.0.get::<TurretState>(turret).expect("turret state");
        assert_eq!(state.total_ammo, 30);
    }

    #[test]
    fn only_deposit_core_blocks_turret_deposit() {
        // `Planets.java:82` (Erekir) sets `onlyDepositCore`; `duo` has no
        // `alwaysAllowDeposit`, a core does (`CoreBlock.java:75`).
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let (mut sim, unit, copper) = sim_with_player();
        let duo = sim.world_block_id("duo").expect("duo");
        sim.command(SimCommand::Place {
            x: 13,
            y: 10,
            block: duo,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("duo placed");
        let campaign = {
            let content = &sim.unit_runtime().expect("rt").content;
            Campaign::from_registry(content, &EmptyNeighborhood)
        };
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            only_deposit_core: true,
            ..Rules::default()
        });
        session.phase = crate::game::State::Playing;
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));

        // The rule is on: the duo refuses the deposit, the core accepts it.
        set_player_stack(&mut sim, unit, copper, 30);
        assert_eq!(sim.player_deposit_accepted(TilePos::new(13, 10)), 0);
        assert!(!sim.apply_player_inventory(1, TilePos::new(13, 10), None, 0));
        assert_eq!(sim.player_unit_stack(), Some((copper, 30)));
        assert_eq!(sim.player_deposit_accepted(TilePos::new(10, 10)), 30);
        assert!(sim.apply_player_inventory(1, TilePos::new(10, 10), None, 0));
        assert_eq!(sim.player_unit_stack(), None);
        // Rule off: the duo takes it again (`allowDeposit` true).
        set_player_stack(&mut sim, unit, copper, 30);
        sim.ecs
            .0
            .get_resource_mut::<CampaignRuntime>()
            .expect("campaign")
            .session
            .rules
            .only_deposit_core = false;
        // `duo` caps the accepted stack (15 items = 30 ammo, see
        // `deposit_accepted_matches_apply`).
        assert_eq!(sim.player_deposit_accepted(TilePos::new(13, 10)), 15);
    }

    #[test]
    fn deposit_into_core_takes_all() {
        let (mut sim, unit, copper) = sim_with_player();
        set_core_copper(&mut sim, copper, 70);
        set_player_stack(&mut sim, unit, copper, 30);
        assert!(sim.apply_player_inventory(1, TilePos::new(10, 10), Some(copper.raw()), 30));
        assert_eq!(sim.player_unit_stack(), None);
        assert_eq!(core_copper(&sim, copper), 100);
    }

    #[test]
    fn transfer_out_of_range_is_ignored() {
        let (mut sim, _unit, copper) = sim_with_player();
        let container = sim.world_block_id("container").expect("container");
        sim.command(SimCommand::Place {
            x: 40,
            y: 40,
            block: container,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("container placed");
        let entity = sim.grid.entity_at(TilePos::new(40, 40)).expect("container");
        sim.ecs
            .0
            .get_mut::<ItemModule>(entity)
            .expect("module")
            .add(copper, 10, 100);
        // Alpha at (80,80): ~339 px > itemTransferRange (220).
        assert!(!sim.apply_player_inventory(0, TilePos::new(40, 40), Some(copper.raw()), 5));
        assert_eq!(
            sim.ecs
                .0
                .get::<ItemModule>(entity)
                .expect("module")
                .get(copper),
            10
        );
    }

    #[test]
    fn deposit_accepted_matches_apply() {
        let (mut sim, unit, copper) = sim_with_player();
        let duo = sim.world_block_id("duo").expect("duo");
        sim.command(SimCommand::Place {
            x: 13,
            y: 10,
            block: duo,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("duo placed");
        set_player_stack(&mut sim, unit, copper, 30);
        // 30 copper / ammoMultiplier 2 = 15 ammo room -> 15 items.
        assert_eq!(sim.player_deposit_accepted(TilePos::new(13, 10)), 15);
        assert_eq!(sim.player_deposit_accepted(TilePos::new(20, 20)), 0);
    }

    #[test]
    fn drop_clears_stack() {
        let (mut sim, unit, copper) = sim_with_player();
        set_player_stack(&mut sim, unit, copper, 5);
        assert!(sim.apply_player_inventory(2, TilePos::new(10, 10), None, 0));
        assert_eq!(sim.player_unit_stack(), None);
    }
}
