// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `liquid`).
//
//! Generated block metadata (`Blocks.java` region `liquid`).
//! Regenerate with `parity/tools/gen_blocks.py` after upstream content changes.

#![allow(unused_imports)]

use super::{
    BlockFlag, BlockKind, BlockSink, BlockSpec, BuildVisibility, liquid_stack, spec, stack,
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

/// Loads the `liquid` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        requirements: vec![stack("copper", 15), stack("metaglass", 10)],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(2e+01f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("mechanical-pump", BlockKind::Pump)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Liquid),
        requirements: vec![
            stack("copper", 70),
            stack("metaglass", 50),
            stack("silicon", 20),
            stack("titanium", 35),
        ],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(8e+01f32),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        consumes: vec![consume_power(0.3f32)],
        ..spec("rotary-pump", BlockKind::Pump)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Liquid),
        requirements: vec![
            stack("copper", 80),
            stack("metaglass", 90),
            stack("silicon", 30),
            stack("titanium", 40),
            stack("thorium", 35),
        ],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(2e+02f32),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.3f32)],
        ..spec("impulse-pump", BlockKind::Pump)
    })?;

    sink.push(BlockSpec {
        health: Some(45),
        category: Some(Category::Liquid),
        requirements: vec![stack("metaglass", 1)],
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(2e+01f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("conduit", BlockKind::Conduit)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        category: Some(Category::Liquid),
        requirements: vec![stack("titanium", 2), stack("metaglass", 1)],
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(4e+01f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("pulse-conduit", BlockKind::Conduit)
    })?;

    sink.push(BlockSpec {
        health: Some(220),
        category: Some(Category::Liquid),
        requirements: vec![
            stack("thorium", 2),
            stack("metaglass", 1),
            stack("plastanium", 1),
        ],
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(5e+01f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("plated-conduit", BlockKind::ArmoredConduit)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        requirements: vec![stack("graphite", 4), stack("metaglass", 2)],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1.2e+02f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("liquid-router", BlockKind::LiquidRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Liquid),
        requirements: vec![stack("titanium", 10), stack("metaglass", 15)],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(7e+02f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("liquid-container", BlockKind::LiquidRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(500),
        category: Some(Category::Liquid),
        requirements: vec![stack("titanium", 30), stack("metaglass", 40)],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1.8e+03f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("liquid-tank", BlockKind::LiquidRouter)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        requirements: vec![stack("graphite", 4), stack("metaglass", 8)],
        group: Some(BlockGroup::Liquids),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        destructible: Some(true),
        ..spec("liquid-junction", BlockKind::LiquidJunction)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        requirements: vec![stack("graphite", 4), stack("metaglass", 8)],
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(1e+02f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        configurable: Some(true),
        ..spec("bridge-conduit", BlockKind::LiquidBridge)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        requirements: vec![
            stack("phase-fabric", 5),
            stack("silicon", 7),
            stack("metaglass", 20),
            stack("titanium", 10),
        ],
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(1e+02f32),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_liquid: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(0.3f32)],
        ..spec("phase-conduit", BlockKind::LiquidBridge)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Liquid),
        requirements: vec![
            stack("beryllium", 40),
            stack("tungsten", 30),
            stack("silicon", 20),
        ],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1.6e+02f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("hydrogen", 0.025f32)],
        ..spec("reinforced-pump", BlockKind::Pump)
    })?;

    sink.push(BlockSpec {
        health: Some(250),
        category: Some(Category::Liquid),
        requirements: vec![stack("beryllium", 2)],
        research_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(5e+01f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("reinforced-conduit", BlockKind::ArmoredConduit)
    })?;

    sink.push(BlockSpec {
        health: Some(250),
        category: Some(Category::Liquid),
        requirements: vec![stack("graphite", 4), stack("beryllium", 8)],
        build_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Liquids),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        destructible: Some(true),
        ..spec("reinforced-liquid-junction", BlockKind::LiquidJunction)
    })?;

    sink.push(BlockSpec {
        health: Some(250),
        category: Some(Category::Liquid),
        requirements: vec![stack("graphite", 8), stack("beryllium", 20)],
        group: Some(BlockGroup::Liquids),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        liquid_capacity: Some(1.2e+02f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec(
            "reinforced-bridge-conduit",
            BlockKind::DirectionLiquidBridge,
        )
    })?;

    sink.push(BlockSpec {
        health: Some(250),
        category: Some(Category::Liquid),
        requirements: vec![stack("graphite", 8), stack("beryllium", 4)],
        research_cost_multiplier: Some(3.0f32),
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1.5e+02f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("reinforced-liquid-router", BlockKind::LiquidRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(400),
        category: Some(Category::Liquid),
        requirements: vec![stack("tungsten", 10), stack("beryllium", 16)],
        research_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1e+03f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("reinforced-liquid-container", BlockKind::LiquidRouter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(900),
        category: Some(Category::Liquid),
        requirements: vec![stack("tungsten", 40), stack("beryllium", 50)],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(2.7e+03f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("reinforced-liquid-tank", BlockKind::LiquidRouter)
    })?;
    let _ = sink;
    Ok(())
}
