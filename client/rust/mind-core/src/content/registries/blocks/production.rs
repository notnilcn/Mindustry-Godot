// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `production`).
//
//! Generated block metadata (`Blocks.java` region `production`).
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

/// Loads the `production` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Production),
        requirements: vec![stack("copper", 12)],
        research_cost: Some(vec![stack("copper", 10)]),
        group: Some(BlockGroup::Drills),
        flags: vec![BlockFlag::Drill],
        has_items: Some(true),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_optional(consume_liquid("water", 0.05f32))],
        ..spec("mechanical-drill", BlockKind::Drill)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Production),
        requirements: vec![stack("copper", 18), stack("graphite", 10)],
        group: Some(BlockGroup::Drills),
        flags: vec![BlockFlag::Drill],
        has_items: Some(true),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_optional(consume_liquid("water", 0.058333334f32))],
        ..spec("pneumatic-drill", BlockKind::Drill)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Production),
        requirements: vec![
            stack("copper", 35),
            stack("graphite", 30),
            stack("silicon", 30),
            stack("titanium", 20),
        ],
        group: Some(BlockGroup::Drills),
        flags: vec![BlockFlag::Drill],
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1.1f32),
            consume_optional(consume_liquid("water", 0.08f32)),
        ],
        ..spec("laser-drill", BlockKind::Drill)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        category: Some(Category::Production),
        requirements: vec![
            stack("copper", 65),
            stack("silicon", 60),
            stack("titanium", 50),
            stack("thorium", 75),
        ],
        group: Some(BlockGroup::Drills),
        flags: vec![BlockFlag::Drill],
        item_capacity: Some(20),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(3.0f32),
            consume_optional(consume_liquid("water", 0.1f32)),
        ],
        ..spec("blast-drill", BlockKind::Drill)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Production),
        requirements: vec![
            stack("metaglass", 30),
            stack("graphite", 30),
            stack("lead", 30),
            stack("copper", 30),
        ],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(4e+01f32),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_liquid: Some(true),
        env_required: Some(EnvMask::of(vec![EnvFlag::GroundWater])),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.5f32)],
        ..spec("water-extractor", BlockKind::SolidPump)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Production),
        requirements: vec![stack("copper", 25), stack("lead", 25), stack("silicon", 10)],
        flags: vec![BlockFlag::Factory],
        liquid_capacity: Some(8e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        env_required: Some(EnvMask::of(vec![EnvFlag::Spores])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.3333334f32), consume_liquid("water", 0.3f32)],
        ..spec("cultivator", BlockKind::AttributeCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Production),
        requirements: vec![
            stack("copper", 150),
            stack("graphite", 175),
            stack("lead", 115),
            stack("thorium", 115),
            stack("silicon", 75),
        ],
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(4e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_liquid: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("sand", 1)]),
            consume_power(3.0f32),
            consume_liquid("water", 0.15f32),
        ],
        ..spec("oil-extractor", BlockKind::Fracker)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Production),
        requirements: vec![stack("graphite", 20), stack("beryllium", 60)],
        group: Some(BlockGroup::Liquids),
        flags: vec![BlockFlag::Factory],
        item_capacity: Some(0),
        liquid_capacity: Some(6e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(0.5f32)],
        ..spec("vent-condenser", BlockKind::AttributeCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Production),
        requirements: vec![stack("graphite", 25), stack("beryllium", 20)],
        research_cost: Some(vec![stack("beryllium", 100), stack("graphite", 40)]),
        flags: vec![BlockFlag::Drill],
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(2),
        consumes: vec![consume_power(0.18333334f32)],
        ..spec("cliff-crusher", BlockKind::WallCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Production),
        requirements: vec![
            stack("silicon", 80),
            stack("surge-alloy", 15),
            stack("beryllium", 100),
            stack("tungsten", 50),
        ],
        flags: vec![BlockFlag::Drill],
        item_capacity: Some(20),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        consumes: vec![
            consume_power(1.0f32),
            consume_liquid("hydrogen", 0.016666668f32),
        ],
        ..spec("large-cliff-crusher", BlockKind::WallCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Production),
        requirements: vec![stack("beryllium", 40)],
        research_cost: Some(vec![stack("beryllium", 10)]),
        flags: vec![BlockFlag::Drill],
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        consumes: vec![
            consume_power(0.15f32),
            consume_optional(consume_liquid("hydrogen", 0.004166667f32)),
        ],
        ..spec("plasma-bore", BlockKind::BeamDrill)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Production),
        requirements: vec![
            stack("silicon", 100),
            stack("oxide", 25),
            stack("beryllium", 100),
            stack("tungsten", 70),
        ],
        research_cost: Some(vec![
            stack("silicon", 1500),
            stack("oxide", 200),
            stack("beryllium", 3000),
            stack("tungsten", 1200),
        ]),
        flags: vec![BlockFlag::Drill],
        item_capacity: Some(20),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(4),
        consumes: vec![
            consume_power(0.8f32),
            consume_liquid("hydrogen", 0.008333334f32),
            consume_optional(consume_liquid("nitrogen", 0.05f32)),
        ],
        ..spec("large-plasma-bore", BlockKind::BeamDrill)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        category: Some(Category::Production),
        requirements: vec![
            stack("silicon", 70),
            stack("beryllium", 90),
            stack("graphite", 60),
        ],
        research_cost_multiplier: Some(0.5f32),
        group: Some(BlockGroup::Drills),
        flags: vec![BlockFlag::Drill],
        item_capacity: Some(40),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(4),
        consumes: vec![
            consume_power(2.6666667f32),
            consume_liquid("water", 0.16666667f32),
            consume_optional(consume_liquid("ozone", 0.05f32)),
        ],
        ..spec("impact-drill", BlockKind::BurstDrill)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Production),
        requirements: vec![
            stack("silicon", 300),
            stack("oxide", 20),
            stack("tungsten", 250),
            stack("thorium", 150),
        ],
        group: Some(BlockGroup::Drills),
        flags: vec![BlockFlag::Drill],
        item_capacity: Some(60),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(5),
        consumes: vec![
            consume_power(6.0f32),
            consume_liquid("hydrogen", 0.06666667f32),
            consume_optional(consume_liquid("cyanogen", 0.0125f32)),
        ],
        ..spec("eruption-drill", BlockKind::BurstDrill)
    })?;
    let _ = sink;
    Ok(())
}
