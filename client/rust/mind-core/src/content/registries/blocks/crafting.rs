// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `crafting`).
//
//! Generated block metadata (`Blocks.java` region `crafting`).
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

/// Loads the `crafting` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![stack("copper", 75), stack("lead", 30)],
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_items(vec![stack("coal", 2)])],
        ..spec("graphite-press", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("titanium", 100),
            stack("silicon", 25),
            stack("lead", 100),
            stack("graphite", 50),
        ],
        item_capacity: Some(20),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1.8f32),
            consume_items(vec![stack("coal", 3)]),
            consume_liquid("water", 0.1f32),
        ],
        ..spec("multi-press", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![stack("copper", 30), stack("lead", 25)],
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("coal", 1), stack("sand", 2)]),
            consume_power(0.5f32),
        ],
        ..spec("silicon-smelter", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("titanium", 120),
            stack("metaglass", 80),
            stack("plastanium", 35),
            stack("silicon", 60),
        ],
        item_capacity: Some(30),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![
                stack("coal", 4),
                stack("sand", 6),
                stack("pyratite", 1),
            ]),
            consume_power(4.0f32),
        ],
        ..spec("silicon-crucible", BlockKind::AttributeCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("copper", 60),
            stack("graphite", 30),
            stack("lead", 30),
        ],
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("lead", 1), stack("sand", 1)]),
            consume_power(0.6f32),
        ],
        ..spec("kiln", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(320),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("silicon", 80),
            stack("lead", 115),
            stack("graphite", 60),
            stack("titanium", 80),
        ],
        liquid_capacity: Some(6e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_liquid("oil", 0.25f32),
            consume_power(3.0f32),
            consume_items(vec![stack("titanium", 2)]),
        ],
        ..spec("plastanium-compressor", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("silicon", 130),
            stack("lead", 120),
            stack("thorium", 75),
        ],
        item_capacity: Some(30),
        has_items: Some(true),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("thorium", 4), stack("sand", 10)]),
            consume_power(5.0f32),
        ],
        ..spec("phase-weaver", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("silicon", 80),
            stack("lead", 80),
            stack("thorium", 70),
        ],
        item_capacity: Some(20),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(4.0f32),
            consume_items(vec![
                stack("copper", 3),
                stack("lead", 4),
                stack("titanium", 2),
                stack("silicon", 3),
            ]),
        ],
        ..spec("surge-smelter", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("lead", 65),
            stack("silicon", 40),
            stack("titanium", 60),
        ],
        liquid_capacity: Some(36.0f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_liquid: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1.0f32),
            consume_items(vec![stack("titanium", 1)]),
            consume_liquid("water", 0.2f32),
        ],
        ..spec("cryofluid-mixer", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![stack("copper", 50), stack("lead", 25)],
        has_items: Some(true),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(0.2f32),
            consume_items(vec![stack("coal", 1), stack("lead", 2), stack("sand", 2)]),
        ],
        ..spec("pyratite-mixer", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![stack("lead", 30), stack("titanium", 20)],
        has_items: Some(true),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("pyratite", 1), stack("spore-pod", 1)]),
            consume_power(0.4f32),
        ],
        ..spec("blast-mixer", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        health: Some(200),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("copper", 30),
            stack("lead", 35),
            stack("graphite", 45),
        ],
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1.0f32),
            consume_items(vec![stack("scrap", 1)]),
        ],
        ..spec("melter", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![stack("copper", 30), stack("titanium", 25)],
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.1f32), consume_liquid("slag", 0.06666667f32)],
        ..spec("separator", BlockKind::Separator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("plastanium", 40),
            stack("titanium", 100),
            stack("silicon", 150),
            stack("thorium", 80),
        ],
        item_capacity: Some(20),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(4.0f32),
            consume_items(vec![stack("scrap", 1)]),
            consume_liquid("slag", 0.12f32),
        ],
        ..spec("disassembler", BlockKind::Separator)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(320),
        category: Some(Category::Crafting),
        requirements: vec![stack("lead", 35), stack("silicon", 30)],
        liquid_capacity: Some(6e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("spore-pod", 1)]),
            consume_power(0.7f32),
        ],
        ..spec("spore-press", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Crafting),
        requirements: vec![stack("copper", 30), stack("lead", 25)],
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("scrap", 1)]),
            consume_power(0.5f32),
        ],
        ..spec("pulverizer", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("titanium", 20),
            stack("graphite", 40),
            stack("lead", 30),
        ],
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("oil", 0.1f32), consume_power(0.7f32)],
        ..spec("coal-centrifuge", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        category: Some(Category::Crafting),
        requirements: vec![stack("graphite", 5), stack("lead", 15)],
        has_liquids: Some(true),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(0.5f32)],
        ..spec("incinerator", BlockKind::Incinerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![stack("beryllium", 70), stack("graphite", 80)],
        research_cost: Some(vec![stack("beryllium", 150), stack("graphite", 50)]),
        item_capacity: Some(30),
        has_items: Some(true),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![
            EnvFlag::Space,
            EnvFlag::Terrestrial,
            EnvFlag::Underwater,
        ])),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        consumes: vec![
            consume_items(vec![stack("graphite", 1), stack("sand", 4)]),
            consume_power(5.0f32),
        ],
        ..spec("silicon-arc-furnace", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("silicon", 50),
            stack("graphite", 40),
            stack("beryllium", 130),
            stack("tungsten", 80),
        ],
        research_cost_multiplier: Some(1.2f32),
        group: Some(BlockGroup::Liquids),
        item_capacity: Some(0),
        liquid_capacity: Some(5e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_liquid("water", 0.16666667f32),
            consume_power(1.0f32),
        ],
        ..spec("electrolyzer", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("oxide", 60),
            stack("beryllium", 180),
            stack("silicon", 150),
        ],
        research_cost: Some(vec![
            stack("silicon", 2000),
            stack("oxide", 900),
            stack("beryllium", 2400),
        ]),
        research_cost_multiplier: Some(1.1f32),
        item_capacity: Some(0),
        liquid_capacity: Some(6e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(2.0f32)],
        ..spec("atmospheric-concentrator", BlockKind::HeatCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("tungsten", 120),
            stack("graphite", 80),
            stack("silicon", 100),
            stack("beryllium", 120),
        ],
        research_cost_multiplier: Some(1.1f32),
        liquid_capacity: Some(3e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_liquid("ozone", 0.033333335f32),
            consume_items(vec![stack("beryllium", 1)]),
            consume_power(0.5f32),
        ],
        ..spec("oxidation-chamber", BlockKind::HeatProducer)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("tungsten", 30),
            stack("oxide", 30),
            stack("beryllium", 30),
        ],
        research_cost_multiplier: Some(4.0f32),
        item_capacity: Some(0),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1.6666666f32)],
        ..spec("electric-heater", BlockKind::HeatProducer)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("tungsten", 50),
            stack("oxide", 20),
            stack("beryllium", 20),
        ],
        research_cost: Some(vec![
            stack("tungsten", 1200),
            stack("oxide", 900),
            stack("beryllium", 2400),
        ]),
        research_cost_multiplier: Some(4.0f32),
        item_capacity: Some(0),
        liquid_capacity: Some(1.2e+02f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("slag", 0.6666667f32)],
        ..spec("slag-heater", BlockKind::HeatProducer)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("oxide", 30),
            stack("carbide", 30),
            stack("beryllium", 30),
        ],
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_items(vec![stack("phase-fabric", 1)])],
        ..spec("phase-heater", BlockKind::HeatProducer)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![stack("tungsten", 10), stack("graphite", 10)],
        research_cost_multiplier: Some(1e+01f32),
        group: Some(BlockGroup::Heat),
        ..spec("heat-redirector", BlockKind::HeatConductor)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Crafting),
        requirements: vec![stack("surge-alloy", 3), stack("graphite", 8)],
        research_cost_multiplier: Some(2.0f32),
        research_cost_multipliers: vec![("graphite", 7.0f32)],
        group: Some(BlockGroup::Heat),
        ..spec("small-heat-redirector", BlockKind::HeatConductor)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![stack("tungsten", 15), stack("graphite", 10)],
        research_cost_multiplier: Some(1e+01f32),
        group: Some(BlockGroup::Heat),
        ..spec("heat-router", BlockKind::HeatConductor)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Crafting),
        requirements: vec![stack("tungsten", 15)],
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("slag", 0.0f32)],
        ..spec("slag-incinerator", BlockKind::ItemIncinerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("tungsten", 110),
            stack("thorium", 150),
            stack("oxide", 60),
        ],
        item_capacity: Some(20),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("tungsten", 2), stack("graphite", 3)]),
            consume_power(2.0f32),
        ],
        ..spec("carbide-crucible", BlockKind::HeatCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("carbide", 70),
            stack("graphite", 60),
            stack("silicon", 40),
            stack("oxide", 40),
        ],
        liquid_capacity: Some(8e+01f32),
        has_items: Some(true),
        build_visibility: Some(BuildVisibility::DebugOnly),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(0.033333335f32),
            consume_items(vec![stack("sand", 1)]),
            consume_liquid("slag", 0.6666667f32),
        ],
        ..spec("slag-centrifuge", BlockKind::GenericCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("silicon", 100),
            stack("graphite", 80),
            stack("tungsten", 80),
            stack("oxide", 80),
        ],
        item_capacity: Some(20),
        liquid_capacity: Some(4e+02f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("silicon", 3)]),
            consume_liquid("slag", 2.6666667f32),
            consume_power(1.5f32),
        ],
        ..spec("surge-crucible", BlockKind::HeatCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("carbide", 80),
            stack("silicon", 120),
            stack("beryllium", 140),
            stack("oxide", 40),
        ],
        liquid_capacity: Some(8e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_liquid("arkycite", 2.6666667f32),
            consume_items(vec![stack("graphite", 1)]),
            consume_power(2.0f32),
        ],
        ..spec("cyanogen-synthesizer", BlockKind::HeatCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("carbide", 90),
            stack("silicon", 100),
            stack("thorium", 100),
            stack("tungsten", 200),
        ],
        item_capacity: Some(40),
        liquid_capacity: Some(4e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("thorium", 2), stack("sand", 6)]),
            consume_liquid("ozone", 0.13333334f32),
            consume_power(8.0f32),
        ],
        ..spec("phase-synthesizer", BlockKind::HeatCrafter)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Crafting),
        requirements: vec![
            stack("oxide", 70),
            stack("graphite", 20),
            stack("carbide", 10),
            stack("thorium", 80),
        ],
        item_capacity: Some(20),
        has_items: Some(true),
        build_visibility: Some(BuildVisibility::DebugOnly),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("thorium", 3)]),
            consume_liquid("nitrogen", 0.016666668f32),
        ],
        ..spec("heat-reactor", BlockKind::HeatProducer)
    })?;
    Ok(())
}
