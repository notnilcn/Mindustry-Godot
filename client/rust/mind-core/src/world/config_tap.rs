// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Building.onConfigureBuildTapped` dispatch (H02 T6, PAR-0220).
//!
//! Java computes this client-side in `InputHandler.tileTapped`
//! (`InputHandler.java:1993-2005`) and sends `configure` calls; the port's
//! input layer holds no rules, so the sim computes the tap result read-only
//! and the host applies the returned configs (`SimCommand::ConfigTap`).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockKind;
use crate::entities::comp::Building;
use crate::world::WorldGrid;
use crate::world::behavior::ConfigTapResult;
use crate::world::block::BlockTable;

/// `Block.clearOnDoubleTap` (`Block.java:101`; the eleven vanilla overrides,
/// `grep -rn "clearOnDoubleTap = true" J/world/blocks`).
///
/// By-kind instead of a `BlockDef` field so the content golden does not move.
pub fn clear_on_double_tap(world: &World, e: Entity) -> bool {
    let Some(building) = world.get::<Building>(e) else {
        return false;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(building.block))
        .is_some_and(|inst| {
            matches!(
                inst.def.kind,
                BlockKind::PayloadRouter
                    | BlockKind::Unloader
                    | BlockKind::UnitFactory
                    | BlockKind::Constructor
                    | BlockKind::ItemSource
                    | BlockKind::LiquidSource
                    | BlockKind::Sorter
                    | BlockKind::DuctRouter
                    | BlockKind::UnitCargoUnloadPoint
                    | BlockKind::PayloadSource
                    | BlockKind::DirectionalUnloader
            )
        })
}

/// `config.getSelected().onConfigureBuildTapped(other)`: resolves the selected
/// building's behavior and returns the Java result. Read-only — the caller
/// applies the configs.
pub fn config_tap(
    world: &World,
    grid: &WorldGrid,
    selected: Entity,
    other: Entity,
) -> ConfigTapResult {
    let behavior = world
        .get::<Building>(selected)
        .and_then(|building| {
            world
                .get_resource::<BlockTable>()
                .and_then(|table| table.get(building.block))
        })
        .map(|inst| inst.behavior.clone());
    match behavior {
        Some(behavior) => behavior.on_configure_build_tapped(world, grid, selected, other),
        None => ConfigTapResult::SwitchAfter(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::SimCommand;
    use crate::sim::{ConfigTapKind, Sim};
    use crate::world::TilePos;
    use crate::world::config::ConfigValue;

    fn runtime_sim() -> Sim {
        let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
        let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
        sim.grid.fill(stone, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim
    }

    fn place(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
        let id = sim.world_block_id(name).expect("block");
        sim.command(SimCommand::Place {
            x,
            y,
            block: id,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("placed");
        sim.grid.entity_at(TilePos::new(x, y)).expect("entity")
    }

    /// The production path: apply the tap command, return the recorded kind.
    fn tap(sim: &mut Sim, from: (i16, i16), to: (i16, i16)) -> ConfigTapKind {
        sim.command(SimCommand::ConfigTap {
            x: from.0,
            y: from.1,
            other_x: to.0,
            other_y: to.1,
        })
        .expect("tap");
        sim.last_config_tap.expect("kind")
    }

    fn bridge_link(sim: &Sim, e: Entity) -> i32 {
        sim.ecs
            .0
            .get::<crate::world::blocks::distribution::ItemBridgeBuild>(e)
            .map(|bridge| bridge.link)
            .unwrap_or(-2)
    }

    #[test]
    fn bridge_tap_links_then_unlinks() {
        let mut sim = runtime_sim();
        let a = place(&mut sim, "bridge-conveyor", 5, 5);
        let b = place(&mut sim, "bridge-conveyor", 8, 5);
        let packed = TilePos::new(8, 5).pack();
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, a, b),
            ConfigTapResult::Consume(vec![(a, ConfigValue::Number(f64::from(packed)))])
        );
        assert_eq!(tap(&mut sim, (5, 5), (8, 5)), ConfigTapKind::Consume);
        assert_eq!(bridge_link(&sim, a), packed);
        // `if(link == other.pos()){ configure(-1); }`.
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, a, b),
            ConfigTapResult::Consume(vec![(a, ConfigValue::Number(-1.0))])
        );
        assert_eq!(tap(&mut sim, (5, 5), (8, 5)), ConfigTapKind::Consume);
        assert_eq!(bridge_link(&sim, a), -1);
    }

    #[test]
    fn bridge_reverse_tap() {
        let mut sim = runtime_sim();
        let a = place(&mut sim, "bridge-conveyor", 5, 5);
        let b = place(&mut sim, "bridge-conveyor", 8, 5);
        tap(&mut sim, (5, 5), (8, 5));
        assert_eq!(bridge_link(&sim, a), TilePos::new(8, 5).pack());
        // `other instanceof ItemBridgeBuild b && b.link == pos()`.
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, b, a),
            ConfigTapResult::SwitchAfter(vec![
                (b, ConfigValue::Number(f64::from(TilePos::new(5, 5).pack()))),
                (a, ConfigValue::Number(-1.0)),
            ])
        );
        assert_eq!(tap(&mut sim, (8, 5), (5, 5)), ConfigTapKind::Switch);
        assert_eq!(bridge_link(&sim, b), TilePos::new(5, 5).pack());
        assert_eq!(bridge_link(&sim, a), -1);
    }

    #[test]
    fn bridge_out_of_range_switches() {
        let mut sim = runtime_sim();
        let a = place(&mut sim, "bridge-conveyor", 5, 5);
        let b = place(&mut sim, "bridge-conveyor", 10, 5);
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, a, b),
            ConfigTapResult::SwitchAfter(Vec::new())
        );
    }

    #[test]
    fn liquid_bridge_links_like_a_bridge() {
        let mut sim = runtime_sim();
        let a = place(&mut sim, "bridge-conduit", 5, 5);
        let b = place(&mut sim, "bridge-conduit", 8, 5);
        let packed = TilePos::new(8, 5).pack();
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, a, b),
            ConfigTapResult::Consume(vec![(a, ConfigValue::Number(f64::from(packed)))])
        );
        assert_eq!(tap(&mut sim, (5, 5), (8, 5)), ConfigTapKind::Consume);
        assert_eq!(bridge_link(&sim, a), packed);
    }

    #[test]
    fn sorter_double_tap_clears() {
        let mut sim = runtime_sim();
        let sorter = place(&mut sim, "sorter", 5, 5);
        sim.command(SimCommand::Configure {
            x: 5,
            y: 5,
            value: crate::determinism::ConfigValue::Content("copper".to_owned()),
        })
        .expect("configure");
        assert!(
            sim.ecs
                .0
                .get::<crate::world::blocks::distribution::SorterBuild>(sorter)
                .is_some_and(|sorter| sorter.sort_item.is_some())
        );
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, sorter, sorter),
            ConfigTapResult::ConsumeAndDeselect(vec![(sorter, ConfigValue::None)])
        );
        assert_eq!(tap(&mut sim, (5, 5), (5, 5)), ConfigTapKind::Deselect);
        assert!(
            sim.ecs
                .0
                .get::<crate::world::blocks::distribution::SorterBuild>(sorter)
                .is_some_and(|sorter| sorter.sort_item.is_none())
        );
    }

    #[test]
    fn mass_driver_self_tap_unlinks() {
        let mut sim = runtime_sim();
        let driver = place(&mut sim, "mass-driver", 5, 5);
        // Unlinked: `if(link == -1) deselect(); configure(-1);`.
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, driver, driver),
            ConfigTapResult::ConsumeAndDeselect(vec![(driver, ConfigValue::Number(-1.0))])
        );
        sim.command(SimCommand::Configure {
            x: 5,
            y: 5,
            value: crate::determinism::ConfigValue::Int(TilePos::new(8, 5).pack()),
        })
        .expect("configure");
        // Linked: `configure(-1); return false;` without deselect.
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, driver, driver),
            ConfigTapResult::Consume(vec![(driver, ConfigValue::Number(-1.0))])
        );
        assert_eq!(tap(&mut sim, (5, 5), (5, 5)), ConfigTapKind::Consume);
    }

    #[test]
    fn node_self_tap_autolinks() {
        let mut sim = runtime_sim();
        let node = place(&mut sim, "power-node", 5, 5);
        let _battery_a = place(&mut sim, "battery", 8, 5);
        let _battery_b = place(&mut sim, "battery", 5, 8);
        let link_count = |sim: &Sim| {
            sim.ecs
                .0
                .get::<crate::world::modules::PowerModule>(node)
                .map(|module| module.links.len())
                .unwrap_or(0)
        };
        // Placement already autolinks (`PowerNode.placed`); clear to stage
        // Java's `power.links.size == 0` branch of the double tap.
        sim.command(SimCommand::Configure {
            x: 5,
            y: 5,
            value: crate::determinism::ConfigValue::Point2Array(Vec::new()),
        })
        .expect("clear links");
        assert_eq!(link_count(&sim), 0, "node starts unlinked");
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, node, node),
            ConfigTapResult::ConsumeAndDeselect(vec![(
                node,
                ConfigValue::Point2Array(
                    vec![TilePos::new(3, 0).pack(), TilePos::new(0, 3).pack()].into()
                ),
            )])
        );
        assert_eq!(tap(&mut sim, (5, 5), (5, 5)), ConfigTapKind::Deselect);
        assert_eq!(link_count(&sim), 2);
        // Second self-tap clears every link (`configure(new Point2[0])`).
        assert_eq!(tap(&mut sim, (5, 5), (5, 5)), ConfigTapKind::Deselect);
        assert_eq!(link_count(&sim), 0);
    }

    #[test]
    fn default_tap_switches_on_other_and_consumes_self() {
        let mut sim = runtime_sim();
        let a = place(&mut sim, "copper-wall", 5, 5);
        let b = place(&mut sim, "copper-wall", 6, 5);
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, a, b),
            ConfigTapResult::SwitchAfter(Vec::new())
        );
        assert_eq!(
            config_tap(&sim.ecs.0, &sim.grid, a, a),
            ConfigTapResult::Consume(Vec::new())
        );
    }
}
