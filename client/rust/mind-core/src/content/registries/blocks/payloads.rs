// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `payloads`).
//
//! Generated block metadata (`Blocks.java` region `payloads`).
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

/// Loads the `payloads` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![stack("graphite", 10), stack("copper", 10)],
        group: Some(BlockGroup::Payloads),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        update: Some(true),
        ..spec("payload-conveyor", BlockKind::PayloadConveyor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![stack("graphite", 15), stack("copper", 10)],
        group: Some(BlockGroup::Payloads),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        update: Some(true),
        configurable: Some(true),
        ..spec("payload-router", BlockKind::PayloadRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(800),
        category: Some(Category::Units),
        requirements: vec![stack("tungsten", 10)],
        research_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Payloads),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        update: Some(true),
        ..spec("reinforced-payload-conveyor", BlockKind::PayloadConveyor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(800),
        category: Some(Category::Units),
        requirements: vec![stack("tungsten", 15)],
        research_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Payloads),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        update: Some(true),
        configurable: Some(true),
        ..spec("reinforced-payload-router", BlockKind::PayloadRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("tungsten", 40),
            stack("silicon", 50),
            stack("graphite", 20),
        ],
        group: Some(BlockGroup::Units),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        fog_radius: Some(5),
        consumes: vec![consume_power(0.5f32)],
        ..spec("payload-mass-driver", BlockKind::PayloadMassDriver)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("phase-fabric", 10),
            stack("tungsten", 120),
            stack("silicon", 150),
            stack("graphite", 60),
            stack("oxide", 30),
        ],
        group: Some(BlockGroup::Units),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(3.0f32)],
        ..spec("large-payload-mass-driver", BlockKind::PayloadMassDriver)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("beryllium", 70),
            stack("silicon", 70),
            stack("oxide", 25),
            stack("graphite", 50),
        ],
        group: Some(BlockGroup::Payloads),
        item_capacity: Some(100),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.0f32)],
        ..spec("small-deconstructor", BlockKind::PayloadDeconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("beryllium", 250),
            stack("oxide", 100),
            stack("silicon", 250),
            stack("carbide", 50),
        ],
        group: Some(BlockGroup::Payloads),
        item_capacity: Some(320),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(3.0f32)],
        ..spec("deconstructor", BlockKind::PayloadDeconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("silicon", 50),
            stack("beryllium", 75),
            stack("tungsten", 40),
        ],
        group: Some(BlockGroup::Payloads),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(2.5f32)],
        ..spec("constructor", BlockKind::Constructor)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("silicon", 150),
            stack("oxide", 100),
            stack("tungsten", 200),
            stack("thorium", 80),
        ],
        group: Some(BlockGroup::Payloads),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(5.0f32)],
        ..spec("large-constructor", BlockKind::Constructor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("graphite", 80),
            stack("silicon", 160),
            stack("tungsten", 90),
        ],
        group: Some(BlockGroup::Payloads),
        item_capacity: Some(100),
        liquid_capacity: Some(1e+02f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        update: Some(true),
        fog_radius: Some(5),
        consumes: vec![consume_power(2.0f32)],
        ..spec("payload-loader", BlockKind::PayloadLoader)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("graphite", 140),
            stack("silicon", 220),
            stack("tungsten", 180),
        ],
        group: Some(BlockGroup::Payloads),
        item_capacity: Some(100),
        liquid_capacity: Some(1e+02f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_power: Some(true),
        outputs_liquid: Some(true),
        update: Some(true),
        fog_radius: Some(5),
        consumes: vec![consume_power(2.0f32)],
        ..spec("payload-unloader", BlockKind::PayloadUnloader)
    })?;
    let _ = sink;
    Ok(())
}
