// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! I03 T6 processor link conformance.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::*;

// ---- I03 T6: processor links, config lifecycle and guard rails ----

/// Processor links as `(name, x, y, valid)`.
fn processor_links(sim: &Sim, e: Entity) -> Vec<(String, i32, i32, bool)> {
    sim.ecs
        .0
        .get::<LogicBlockState>(e)
        .map(|state| {
            state
                .links
                .iter()
                .map(|link| (link.name.clone(), link.x, link.y, link.valid))
                .collect()
        })
        .unwrap_or_default()
}

/// Processor program text.
fn processor_code(sim: &Sim, e: Entity) -> String {
    sim.ecs
        .0
        .get::<LogicBlockState>(e)
        .map(|state| state.code.clone())
        .unwrap_or_default()
}

/// `LogicBlock.config(Integer pos)` (`LogicBlock.java:85-114`): a packed tile
/// position toggles that building's link; the same configure again removes it.
#[test]
fn link_toggle_via_number_config() {
    let mut sim = campaign_sim();
    let processor = place(&mut sim, "micro-processor", 10, 10);
    place(&mut sim, "switch", 12, 10);
    let packed = TilePos::new(12, 10).pack();
    assert!(processor_links(&sim, processor).is_empty());
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Int(packed),
    );
    let links = processor_links(&sim, processor);
    assert_eq!(links.len(), 1, "one link added: {links:?}");
    assert_eq!(links[0].0, "switch1");
    assert!(links[0].3, "the link resolves");
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Int(packed),
    );
    assert!(
        processor_links(&sim, processor).is_empty(),
        "the second configure unlinks"
    );
    assert_events_drained(&sim);
}

/// `LogicBlock.config(Integer)`: when the unlink removes a link and the target
/// was disabled by this processor, `autoResetEnabled` re-enables it
/// (`LogicBlock.java:99-103`).
#[test]
fn unlink_reenables_disabled() {
    let mut sim = campaign_sim();
    let processor = place(&mut sim, "micro-processor", 10, 10);
    let message = place(&mut sim, "message", 12, 10);
    let packed = TilePos::new(12, 10).pack();
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Int(packed),
    );
    assert_eq!(processor_links(&sim, processor).len(), 1);
    // `control enabled message 0` disabled it with the processor as disabler.
    {
        let mut building = sim
            .ecs
            .0
            .get_mut::<crate::entities::comp::Building>(message)
            .expect("message building");
        building.enabled = false;
        building.last_disabler = Some(processor);
    }
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Int(packed),
    );
    assert!(processor_links(&sim, processor).is_empty(), "unlinked");
    assert!(
        building_enabled(&sim, message),
        "unlink re-enables the disabled target"
    );
    assert_events_drained(&sim);
}

/// `LogicBuild.removeFromProximity()` (`LogicBlock.java:454-462`): removing the
/// processor re-enables every linked building it disabled.
#[test]
fn processor_removed_reenables() {
    let mut sim = campaign_sim();
    let processor = place(&mut sim, "micro-processor", 10, 10);
    let message = place(&mut sim, "message", 12, 10);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Int(TilePos::new(12, 10).pack()),
    );
    assert_eq!(processor_links(&sim, processor).len(), 1);
    // One tick assembles the program, which fills `executor.links` (Java
    // `updateCode` runs from `updateTile`).
    tick(&mut sim, 1);
    {
        let mut building = sim
            .ecs
            .0
            .get_mut::<crate::entities::comp::Building>(message)
            .expect("message building");
        building.enabled = false;
        building.last_disabler = Some(processor);
    }
    sim.command(crate::determinism::SimCommand::Break {
        x: 10,
        y: 10,
        player: None,
    })
    .expect("break");
    assert!(
        sim.grid.entity_at(TilePos::new(10, 10)).is_none(),
        "processor removed"
    );
    assert!(
        building_enabled(&sim, message),
        "removal re-enables the disabled link"
    );
    assert_events_drained(&sim);
}

/// `LogicBlock.accessible()` (`LogicBlock.java:120-122`): a privileged world
/// processor rejects code configs and cannot be broken while
/// `allowEditWorldProcessors` is off and the editor is not active.
#[test]
fn world_processor_not_accessible() {
    let mut sim = campaign_sim();
    place_world_processor(&mut sim, 10, 10, "set x 1\nstop\n");
    let processor = sim.grid.entity_at(TilePos::new(10, 10)).expect("placed");
    assert_eq!(processor_code(&sim, processor), "set x 1\nstop\n");
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign runtime")
        .session
        .rules
        .allow_edit_world_processors = false;
    let bytes = crate::logic::blocks::logic_block::compress("set y 2\nstop\n", &[]);
    configure(
        &mut sim,
        10,
        10,
        crate::determinism::ConfigValue::Bytes(bytes),
    );
    assert_eq!(
        processor_code(&sim, processor),
        "set x 1\nstop\n",
        "configure rejected while inaccessible"
    );
    let _ = sim.command(crate::determinism::SimCommand::Break {
        x: 10,
        y: 10,
        player: None,
    });
    assert!(
        sim.grid.entity_at(TilePos::new(10, 10)).is_some(),
        "an inaccessible world processor cannot be broken"
    );
    assert_events_drained(&sim);
}

/// `LogicBlock.onConfigureBuildTapped` (`LogicBlock.java:761-773`): a tap on a
/// valid link target toggles the link and is consumed; an out-of-range tap
/// falls through to the default switch behaviour.
#[test]
fn tap_links_valid_target() {
    let mut sim = campaign_sim();
    let processor = place(&mut sim, "micro-processor", 10, 10);
    place(&mut sim, "switch", 12, 10);
    sim.command(crate::determinism::SimCommand::ConfigTap {
        x: 10,
        y: 10,
        other_x: 12,
        other_y: 10,
    })
    .expect("tap");
    assert_eq!(
        sim.last_config_tap,
        Some(crate::sim::ConfigTapKind::Consume)
    );
    assert_eq!(
        processor_links(&sim, processor).len(),
        1,
        "tap adds the link"
    );
    assert_eq!(processor_links(&sim, processor)[0].0, "switch1");
    sim.command(crate::determinism::SimCommand::ConfigTap {
        x: 10,
        y: 10,
        other_x: 12,
        other_y: 10,
    })
    .expect("tap");
    assert!(
        processor_links(&sim, processor).is_empty(),
        "the second tap unlinks"
    );
    // Out of range (micro processor `range = 80` world units = 10 tiles): the
    // tap is not consumed and the panel switches to the tapped building.
    place(&mut sim, "switch", 30, 10);
    sim.command(crate::determinism::SimCommand::ConfigTap {
        x: 10,
        y: 10,
        other_x: 30,
        other_y: 10,
    })
    .expect("tap");
    assert_eq!(sim.last_config_tap, Some(crate::sim::ConfigTapKind::Switch));
    assert!(processor_links(&sim, processor).is_empty());
    assert_events_drained(&sim);
}

/// `Build.canPickup()` overrides: a logic processor cannot be picked up
/// (`LogicBlock.java:471`); a visible wall can.
#[test]
fn processor_cannot_be_picked_up() {
    let sim = campaign_sim();
    let content = sim.block_runtime().expect("runtime").content();
    let processor = content
        .block(content.block_id("world-processor").expect("block"))
        .expect("def");
    assert!(!crate::world::build::block_can_pickup(processor));
    let wall = content
        .block(content.block_id("copper-wall").expect("block"))
        .expect("def");
    assert!(crate::world::build::block_can_pickup(wall));
}
