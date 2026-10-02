// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `distribution`).
//
//! Generated block metadata (`Blocks.java` region `distribution`).
//! Regenerate with `parity/tools/gen_blocks.py` after upstream content changes.

#![allow(unused_imports)]

use super::{
    BlockFlag, BlockKind, BlockSink, BlockSpec, BuildVisibility, assembler_plan, liquid_stack,
    payload_block, payload_unit, spec, stack, unit_plan,
};
use super::{
    BlockGroup, EnvFlag, EnvMask, TARGET_PRIORITY_BASE, TARGET_PRIORITY_CORE,
    TARGET_PRIORITY_TRANSPORT, TARGET_PRIORITY_TURRET, TARGET_PRIORITY_UNDER, TARGET_PRIORITY_WALL,
};
use super::{
    consume_coolant, consume_item, consume_items, consume_liquid, consume_liquids,
    consume_optional, consume_power, consume_power_buffered, rgba_hex,
};
use crate::content::{Category, ContentError};

/// Loads the `distribution` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        health: Some(45),
        requirements: vec![stack("copper", 1)],
        research_cost: Some(vec![stack("copper", 5)]),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        update: Some(true),
        ..spec("conveyor", BlockKind::Conveyor)
    })?;

    sink.push(BlockSpec {
        health: Some(65),
        requirements: vec![stack("copper", 1), stack("lead", 1), stack("titanium", 1)],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        update: Some(true),
        ..spec("titanium-conveyor", BlockKind::Conveyor)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        requirements: vec![
            stack("plastanium", 1),
            stack("silicon", 1),
            stack("graphite", 1),
        ],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        update: Some(true),
        ..spec("plastanium-conveyor", BlockKind::StackConveyor)
    })?;

    sink.push(BlockSpec {
        health: Some(280),
        requirements: vec![
            stack("plastanium", 1),
            stack("thorium", 1),
            stack("metaglass", 1),
        ],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        update: Some(true),
        ..spec("armored-conveyor", BlockKind::ArmoredConveyor)
    })?;

    sink.push(BlockSpec {
        health: Some(30),
        requirements: vec![stack("copper", 3)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Transportation),
        update: Some(true),
        ..spec("junction", BlockKind::Junction)
    })?;

    sink.push(BlockSpec {
        requirements: vec![stack("lead", 6), stack("copper", 6)],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        ..spec("bridge-conveyor", BlockKind::BufferedItemBridge)
    })?;

    sink.push(BlockSpec {
        requirements: vec![
            stack("phase-fabric", 5),
            stack("silicon", 7),
            stack("lead", 10),
            stack("graphite", 10),
        ],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(0.3f32)],
        ..spec("phase-conveyor", BlockKind::ItemBridge)
    })?;

    sink.push(BlockSpec {
        requirements: vec![stack("lead", 2), stack("copper", 2)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Transportation),
        destructible: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("sorter", BlockKind::Sorter)
    })?;

    sink.push(BlockSpec {
        requirements: vec![stack("lead", 2), stack("copper", 2)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Transportation),
        destructible: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("inverted-sorter", BlockKind::Sorter)
    })?;

    sink.push(BlockSpec {
        requirements: vec![stack("copper", 3)],
        build_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Transportation),
        item_capacity: Some(1),
        has_items: Some(true),
        update: Some(true),
        ..spec("router", BlockKind::Router)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        requirements: vec![stack("lead", 4), stack("copper", 4)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Transportation),
        item_capacity: Some(1),
        has_items: Some(true),
        update: Some(true),
        ..spec("distributor", BlockKind::Router)
    })?;

    sink.push(BlockSpec {
        requirements: vec![stack("lead", 2), stack("copper", 4)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Transportation),
        item_capacity: Some(0),
        has_items: Some(true),
        destructible: Some(true),
        ..spec("overflow-gate", BlockKind::OverflowGate)
    })?;

    sink.push(BlockSpec {
        requirements: vec![stack("lead", 2), stack("copper", 4)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Transportation),
        item_capacity: Some(0),
        has_items: Some(true),
        destructible: Some(true),
        ..spec("underflow-gate", BlockKind::OverflowGate)
    })?;

    sink.push(BlockSpec {
        health: Some(70),
        requirements: vec![stack("titanium", 25), stack("silicon", 30)],
        group: Some(BlockGroup::Transportation),
        item_capacity: Some(0),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("unloader", BlockKind::Unloader)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        requirements: vec![
            stack("titanium", 125),
            stack("silicon", 75),
            stack("lead", 125),
            stack("thorium", 50),
        ],
        item_capacity: Some(120),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(1.75f32)],
        ..spec("mass-driver", BlockKind::MassDriver)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        requirements: vec![stack("beryllium", 1)],
        research_cost: Some(vec![stack("beryllium", 5)]),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(1),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        update: Some(true),
        ..spec("duct", BlockKind::Duct)
    })?;

    sink.push(BlockSpec {
        health: Some(140),
        requirements: vec![stack("beryllium", 2), stack("tungsten", 1)],
        research_cost: Some(vec![stack("beryllium", 300), stack("tungsten", 100)]),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(1),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        update: Some(true),
        ..spec("armored-duct", BlockKind::Duct)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        requirements: vec![stack("beryllium", 10)],
        research_cost: Some(vec![stack("beryllium", 30)]),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(1),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("duct-router", BlockKind::DuctRouter)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        requirements: vec![stack("graphite", 8), stack("beryllium", 8)],
        research_cost_multiplier: Some(1.5f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(1),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        update: Some(true),
        ..spec("overflow-duct", BlockKind::OverflowDuct)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        requirements: vec![stack("graphite", 8), stack("beryllium", 8)],
        research_cost_multiplier: Some(1.5f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(1),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        update: Some(true),
        ..spec("underflow-duct", BlockKind::OverflowDuct)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        requirements: vec![stack("beryllium", 15)],
        research_cost_multiplier: Some(0.3f32),
        build_cost_multiplier: Some(2.0f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(4),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        solid: Some(true),
        update: Some(true),
        ..spec("duct-bridge", BlockKind::DuctBridge)
    })?;

    sink.push(BlockSpec {
        health: Some(120),
        requirements: vec![
            stack("graphite", 20),
            stack("silicon", 20),
            stack("tungsten", 10),
        ],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        item_capacity: Some(0),
        has_items: Some(true),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("duct-unloader", BlockKind::DirectionalUnloader)
    })?;

    sink.push(BlockSpec {
        health: Some(130),
        requirements: vec![stack("surge-alloy", 1), stack("tungsten", 1)],
        research_cost: Some(vec![stack("surge-alloy", 30), stack("tungsten", 80)]),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        has_power: Some(true),
        conductive_power: Some(true),
        update: Some(true),
        consumes: vec![consume_power(0.016666668f32)],
        ..spec("surge-conveyor", BlockKind::StackConveyor)
    })?;

    sink.push(BlockSpec {
        health: Some(130),
        requirements: vec![stack("surge-alloy", 5), stack("tungsten", 1)],
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_items: Some(true),
        has_power: Some(true),
        conductive_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(0.05f32)],
        ..spec("surge-router", BlockKind::StackRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        requirements: vec![
            stack("silicon", 80),
            stack("surge-alloy", 50),
            stack("oxide", 20),
        ],
        research_cost: Some(vec![
            stack("silicon", 2500),
            stack("surge-alloy", 20),
            stack("oxide", 30),
        ]),
        item_capacity: Some(200),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(0.13333334f32),
            consume_liquid("nitrogen", 0.16666667f32),
        ],
        ..spec("unit-cargo-loader", BlockKind::UnitCargoLoader)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        requirements: vec![stack("silicon", 60), stack("tungsten", 60)],
        research_cost: Some(vec![stack("silicon", 3000), stack("oxide", 20)]),
        flags: vec![BlockFlag::UnitCargoUnloadPoint],
        item_capacity: Some(100),
        has_items: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("unit-cargo-unload-point", BlockKind::UnitCargoUnloadPoint)
    })?;
    let _ = sink;
    Ok(())
}
