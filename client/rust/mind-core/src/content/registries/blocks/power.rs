// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `power`).
//
//! Generated block metadata (`Blocks.java` region `power`).
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

/// Loads the `power` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        category: Some(Category::Power),
        requirements: vec![stack("copper", 2), stack("lead", 6)],
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        crush_fragile: Some(true),
        ..spec("power-node", BlockKind::PowerNode)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Power),
        requirements: vec![stack("titanium", 5), stack("lead", 10), stack("silicon", 3)],
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("power-node-large", BlockKind::PowerNode)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Power),
        requirements: vec![
            stack("titanium", 7),
            stack("lead", 10),
            stack("silicon", 15),
            stack("surge-alloy", 15),
        ],
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("surge-tower", BlockKind::PowerNode)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Power),
        requirements: vec![
            stack("silicon", 10),
            stack("plastanium", 5),
            stack("metaglass", 10),
        ],
        group: Some(BlockGroup::Power),
        solid: Some(true),
        update: Some(true),
        insulated: Some(true),
        ..spec("diode", BlockKind::PowerDiode)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Power),
        requirements: vec![stack("copper", 5), stack("lead", 20)],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Battery],
        has_power: Some(true),
        outputs_power: Some(true),
        solid: Some(true),
        destructible: Some(true),
        consumes: vec![consume_power_buffered(4e+03f32)],
        ..spec("battery", BlockKind::Battery)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Power),
        requirements: vec![
            stack("titanium", 20),
            stack("lead", 50),
            stack("silicon", 30),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Battery],
        has_power: Some(true),
        outputs_power: Some(true),
        solid: Some(true),
        destructible: Some(true),
        consumes: vec![consume_power_buffered(5e+04f32)],
        ..spec("battery-large", BlockKind::Battery)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Power),
        requirements: vec![stack("copper", 25), stack("lead", 15)],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        ..spec("combustion-generator", BlockKind::ConsumeGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Power),
        requirements: vec![
            stack("copper", 40),
            stack("graphite", 35),
            stack("lead", 50),
            stack("silicon", 35),
            stack("metaglass", 40),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("thermal-generator", BlockKind::ThermalGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Power),
        requirements: vec![
            stack("copper", 35),
            stack("graphite", 25),
            stack("lead", 40),
            stack("silicon", 30),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("water", 0.1f32)],
        ..spec("steam-generator", BlockKind::ConsumeGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Power),
        requirements: vec![
            stack("copper", 70),
            stack("titanium", 50),
            stack("lead", 100),
            stack("silicon", 65),
            stack("metaglass", 50),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("pyratite", 1)]),
            consume_liquid("cryofluid", 0.1f32),
        ],
        ..spec("differential-generator", BlockKind::ConsumeGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Power),
        requirements: vec![
            stack("lead", 100),
            stack("silicon", 75),
            stack("phase-fabric", 25),
            stack("plastanium", 75),
            stack("thorium", 50),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("rtg-generator", BlockKind::ConsumeGenerator)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Power),
        requirements: vec![stack("lead", 10), stack("silicon", 8)],
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("solar-panel", BlockKind::SolarGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Power),
        requirements: vec![
            stack("lead", 60),
            stack("silicon", 70),
            stack("phase-fabric", 15),
        ],
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("solar-panel-large", BlockKind::SolarGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(1400),
        category: Some(Category::Power),
        requirements: vec![
            stack("lead", 300),
            stack("silicon", 200),
            stack("graphite", 150),
            stack("thorium", 150),
            stack("metaglass", 50),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Reactor, BlockFlag::Generator],
        item_capacity: Some(30),
        liquid_capacity: Some(3e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_items(vec![stack("thorium", 1)]),
            consume_liquid("cryofluid", 0.02f32),
        ],
        ..spec("thorium-reactor", BlockKind::NuclearReactor)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        health: Some(900),
        category: Some(Category::Power),
        requirements: vec![
            stack("lead", 500),
            stack("silicon", 300),
            stack("graphite", 400),
            stack("thorium", 100),
            stack("surge-alloy", 250),
            stack("metaglass", 250),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Reactor, BlockFlag::Generator],
        liquid_capacity: Some(8e+01f32),
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        consumes_power: Some(false),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(25.0f32),
            consume_items(vec![stack("blast-compound", 1)]),
            consume_liquid("cryofluid", 0.25f32),
        ],
        ..spec("impact-reactor", BlockKind::ImpactReactor)
    })?;

    sink.push(BlockSpec {
        health: Some(90),
        category: Some(Category::Power),
        requirements: vec![stack("beryllium", 8)],
        research_cost: Some(vec![stack("beryllium", 5)]),
        group: Some(BlockGroup::Power),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_power: Some(true),
        outputs_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(1),
        consumes: vec![consume_power_buffered(1e+03f32)],
        crush_fragile: Some(true),
        ..spec("beam-node", BlockKind::BeamNode)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(9e+01f32),
        category: Some(Category::Power),
        requirements: vec![
            stack("beryllium", 30),
            stack("oxide", 10),
            stack("silicon", 10),
        ],
        group: Some(BlockGroup::Power),
        priority: Some(TARGET_PRIORITY_TRANSPORT),
        has_power: Some(true),
        outputs_power: Some(true),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(2),
        consumes: vec![consume_power_buffered(4e+04f32)],
        ..spec("beam-tower", BlockKind::BeamNode)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(1.3e+02f32),
        category: Some(Category::Power),
        requirements: vec![
            stack("beryllium", 250),
            stack("silicon", 250),
            stack("oxide", 150),
            stack("carbide", 75),
            stack("surge-alloy", 75),
            stack("phase-fabric", 75),
        ],
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("beam-link", BlockKind::LongPowerNode)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Power),
        requirements: vec![stack("beryllium", 60)],
        research_cost: Some(vec![stack("beryllium", 15)]),
        group: Some(BlockGroup::Liquids),
        flags: vec![BlockFlag::Generator],
        liquid_capacity: Some(2e+01f32),
        has_liquids: Some(true),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        fog_radius: Some(3),
        ..spec("turbine-condenser", BlockKind::ThermalGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Power),
        requirements: vec![
            stack("graphite", 40),
            stack("tungsten", 20),
            stack("oxide", 40),
            stack("silicon", 30),
        ],
        research_cost: Some(vec![
            stack("graphite", 2000),
            stack("tungsten", 1000),
            stack("oxide", 10),
            stack("silicon", 1500),
        ]),
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        liquid_capacity: Some(1e+02f32),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquids(vec![])],
        ..spec("chemical-combustion-chamber", BlockKind::ConsumeGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Power),
        requirements: vec![
            stack("graphite", 100),
            stack("carbide", 60),
            stack("oxide", 60),
            stack("silicon", 100),
        ],
        research_cost_multiplier: Some(0.4f32),
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        liquid_capacity: Some(1.5e+02f32),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquids(vec![])],
        ..spec("pyrolysis-generator", BlockKind::ConsumeGenerator)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Power),
        requirements: vec![
            stack("graphite", 240),
            stack("carbide", 60),
            stack("oxide", 80),
            stack("silicon", 480),
            stack("surge-alloy", 120),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        liquid_capacity: Some(3e+01f32),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("cyanogen", 0.15f32)],
        ..spec("flux-reactor", BlockKind::VariableReactor)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Power),
        requirements: vec![
            stack("tungsten", 750),
            stack("carbide", 300),
            stack("oxide", 150),
            stack("silicon", 500),
            stack("phase-fabric", 150),
            stack("surge-alloy", 200),
        ],
        group: Some(BlockGroup::Power),
        flags: vec![BlockFlag::Generator],
        liquid_capacity: Some(8e+01f32),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_liquid("arkycite", 1.3333334f32),
            consume_liquid("water", 0.16666667f32),
            consume_items(vec![stack("phase-fabric", 1)]),
        ],
        ..spec("neoplasia-reactor", BlockKind::HeaterGenerator)
    })?;
    let _ = sink;
    Ok(())
}
