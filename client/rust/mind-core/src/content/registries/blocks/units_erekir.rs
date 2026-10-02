// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `units - erekir`).
//
//! Generated block metadata (`Blocks.java` region `units - erekir`).
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

/// Loads the `units - erekir` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![stack("silicon", 200), stack("beryllium", 150)],
        research_cost: Some(vec![
            stack("beryllium", 200),
            stack("graphite", 80),
            stack("silicon", 80),
        ]),
        group: Some(BlockGroup::Units),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        unit_plans: vec![unit_plan(
            "stell",
            2.1e+03f32,
            vec![stack("beryllium", 40), stack("silicon", 50)],
        )],
        consumes: vec![consume_power(1.5f32)],
        ..spec("tank-fabricator", BlockKind::UnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![stack("silicon", 250), stack("beryllium", 200)],
        research_cost_multiplier: Some(0.5f32),
        group: Some(BlockGroup::Units),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        unit_plans: vec![unit_plan(
            "elude",
            2.4e+03f32,
            vec![stack("graphite", 50), stack("silicon", 70)],
        )],
        consumes: vec![consume_power(1.5f32)],
        ..spec("ship-fabricator", BlockKind::UnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("silicon", 200),
            stack("beryllium", 250),
            stack("tungsten", 10),
        ],
        research_cost_multiplier: Some(0.65f32),
        group: Some(BlockGroup::Units),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        unit_plans: vec![unit_plan(
            "merui",
            2.4e+03f32,
            vec![stack("beryllium", 50), stack("silicon", 70)],
        )],
        consumes: vec![consume_power(1.5f32)],
        ..spec("mech-fabricator", BlockKind::UnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("beryllium", 200),
            stack("tungsten", 80),
            stack("silicon", 100),
        ],
        research_cost_multiplier: Some(0.75f32),
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![("stell", "locus")],
        consumes: vec![
            consume_power(3.0f32),
            consume_liquid("hydrogen", 0.05f32),
            consume_items(vec![stack("silicon", 40), stack("tungsten", 30)]),
        ],
        ..spec("tank-refabricator", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("beryllium", 250),
            stack("tungsten", 120),
            stack("silicon", 150),
            stack("oxide", 15),
        ],
        research_cost: Some(vec![
            stack("beryllium", 500),
            stack("tungsten", 200),
            stack("silicon", 300),
            stack("oxide", 80),
        ]),
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![("elude", "avert")],
        consumes: vec![
            consume_power(2.5f32),
            consume_liquid("hydrogen", 0.05f32),
            consume_items(vec![stack("silicon", 60), stack("tungsten", 40)]),
        ],
        ..spec("ship-refabricator", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("beryllium", 250),
            stack("tungsten", 175),
            stack("silicon", 150),
        ],
        research_cost_multiplier: Some(0.75f32),
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![("merui", "cleroi")],
        consumes: vec![
            consume_power(2.5f32),
            consume_liquid("hydrogen", 0.05f32),
            consume_items(vec![stack("silicon", 50), stack("tungsten", 40)]),
        ],
        ..spec("mech-refabricator", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("thorium", 250),
            stack("oxide", 200),
            stack("tungsten", 200),
            stack("silicon", 400),
        ],
        research_cost_multipliers: vec![("thorium", 0.2f32)],
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![
            ("locus", "precept"),
            ("cleroi", "anthicus"),
            ("avert", "obviate"),
        ],
        consumes: vec![
            consume_power(4.5f32),
            consume_liquid("nitrogen", 0.16666667f32),
            consume_items(vec![stack("thorium", 80), stack("silicon", 100)]),
        ],
        ..spec("prime-refabricator", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("thorium", 500),
            stack("oxide", 150),
            stack("carbide", 80),
            stack("silicon", 650),
        ],
        research_cost_multiplier: Some(0.4f32),
        group: Some(BlockGroup::Units),
        flags: vec![BlockFlag::UnitAssembler],
        assembler_plans: vec![
            assembler_plan(
                "vanquish",
                3e+03f32,
                vec![
                    payload_unit("stell", 4),
                    payload_block("tungsten-wall-large", 10),
                ],
            ),
            assembler_plan(
                "conquer",
                1.08e+04f32,
                vec![
                    payload_unit("locus", 6),
                    payload_block("carbide-wall-large", 20),
                ],
            ),
        ],
        consumes: vec![consume_power(2.5f32), consume_liquid("cyanogen", 0.15f32)],
        ..spec("tank-assembler", BlockKind::UnitAssembler)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("carbide", 100),
            stack("oxide", 200),
            stack("tungsten", 550),
            stack("silicon", 900),
            stack("thorium", 400),
        ],
        group: Some(BlockGroup::Units),
        flags: vec![BlockFlag::UnitAssembler],
        assembler_plans: vec![
            assembler_plan(
                "quell",
                3.6e+03f32,
                vec![
                    payload_unit("elude", 4),
                    payload_block("beryllium-wall-large", 12),
                ],
            ),
            assembler_plan(
                "disrupt",
                1.08e+04f32,
                vec![
                    payload_unit("avert", 6),
                    payload_block("carbide-wall-large", 20),
                ],
            ),
        ],
        consumes: vec![consume_power(2.5f32), consume_liquid("cyanogen", 0.2f32)],
        ..spec("ship-assembler", BlockKind::UnitAssembler)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("carbide", 200),
            stack("thorium", 600),
            stack("oxide", 200),
            stack("tungsten", 550),
            stack("silicon", 1000),
        ],
        group: Some(BlockGroup::Units),
        flags: vec![BlockFlag::UnitAssembler],
        assembler_plans: vec![
            assembler_plan(
                "tecta",
                4.2e+03f32,
                vec![
                    payload_unit("merui", 5),
                    payload_block("tungsten-wall-large", 12),
                ],
            ),
            assembler_plan(
                "collaris",
                1.08e+04f32,
                vec![
                    payload_unit("cleroi", 6),
                    payload_block("carbide-wall-large", 20),
                ],
            ),
        ],
        consumes: vec![consume_power(3.0f32), consume_liquid("cyanogen", 0.2f32)],
        ..spec("mech-assembler", BlockKind::UnitAssembler)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("carbide", 300),
            stack("thorium", 500),
            stack("oxide", 250),
            stack("phase-fabric", 400),
        ],
        research_cost_multiplier: Some(0.75f32),
        group: Some(BlockGroup::Payloads),
        update: Some(true),
        consumes: vec![consume_power(3.5f32)],
        ..spec("basic-assembler-module", BlockKind::UnitAssemblerModule)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Units),
        requirements: vec![
            stack("graphite", 90),
            stack("silicon", 90),
            stack("tungsten", 80),
        ],
        flags: vec![BlockFlag::Repair],
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.0f32), consume_liquid("ozone", 0.05f32)],
        ..spec("unit-repair-tower", BlockKind::RepairTower)
    })?;
    let _ = sink;
    Ok(())
}
