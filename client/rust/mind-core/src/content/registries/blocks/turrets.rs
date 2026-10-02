// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `turrets`).
//
//! Generated block metadata (`Blocks.java` region `turrets`).
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

/// Loads the `turrets` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        health: Some(250),
        category: Some(Category::Turret),
        requirements: vec![stack("copper", 35)],
        research_cost_multiplier: Some(0.05f32),
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("duo", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(2e+02f32),
        category: Some(Category::Turret),
        requirements: vec![stack("copper", 85), stack("lead", 45)],
        research_cost_multiplier: Some(0.05f32),
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("scatter", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        health: Some(400),
        category: Some(Category::Turret),
        requirements: vec![stack("copper", 25), stack("graphite", 22)],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("scorch", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        health: Some(260),
        category: Some(Category::Turret),
        requirements: vec![stack("copper", 40), stack("graphite", 17)],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("hail", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(2.5e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("metaglass", 45),
            stack("lead", 75),
            stack("copper", 25),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret, BlockFlag::Extinguisher],
        liquid_capacity: Some(1e+01f32),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("wave", BlockKind::LiquidTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(2.8e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 60),
            stack("lead", 70),
            stack("silicon", 60),
            stack("titanium", 30),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(6.0f32)],
        ..spec("lancer", BlockKind::PowerTurret)
    })?;

    sink.push(BlockSpec {
        health: Some(260),
        category: Some(Category::Turret),
        requirements: vec![stack("copper", 50), stack("lead", 50)],
        research_cost_multiplier: Some(0.33333334f32),
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(3.3f32)],
        ..spec("arc", BlockKind::PowerTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(1.6e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("silicon", 160),
            stack("titanium", 110),
            stack("graphite", 50),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(3.3f32)],
        ..spec("parallax", BlockKind::TractorBeamTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(3e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("graphite", 35),
            stack("titanium", 35),
            stack("plastanium", 45),
            stack("silicon", 30),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        ..spec("swarmer", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(2.4e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 100),
            stack("graphite", 80),
            stack("titanium", 50),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("salvo", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(2.5e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("silicon", 130),
            stack("thorium", 80),
            stack("phase-fabric", 40),
            stack("titanium", 40),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(8.0f32)],
        ..spec("segment", BlockKind::PointDefenseTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(2.5e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("metaglass", 100),
            stack("lead", 400),
            stack("titanium", 250),
            stack("thorium", 100),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret, BlockFlag::Extinguisher],
        liquid_capacity: Some(4e+01f32),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("tsunami", BlockKind::LiquidTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(2.2e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 225),
            stack("graphite", 225),
            stack("thorium", 100),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        ..spec("fuse", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(1.3e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 150),
            stack("graphite", 135),
            stack("titanium", 60),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("ripple", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(145.0f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 200),
            stack("titanium", 125),
            stack("plastanium", 80),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("cyclone", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(1.5e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 1000),
            stack("metaglass", 600),
            stack("surge-alloy", 300),
            stack("plastanium", 200),
            stack("silicon", 600),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(6e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(1e+01f32)],
        ..spec("foreshadow", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(1.6e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 900),
            stack("graphite", 300),
            stack("surge-alloy", 250),
            stack("plastanium", 175),
            stack("thorium", 250),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(1.2e+02f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("spectre", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(2e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("copper", 1200),
            stack("lead", 350),
            stack("graphite", 300),
            stack("surge-alloy", 325),
            stack("silicon", 325),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(6e+01f32),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(17.0f32)],
        ..spec("meltdown", BlockKind::LaserTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(1.8e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("beryllium", 150),
            stack("silicon", 150),
            stack("graphite", 125),
        ],
        research_cost_multiplier: Some(0.05f32),
        build_time: Some(5.4e+02f32),
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        ..spec("breach", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(2.1e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("beryllium", 150),
            stack("silicon", 200),
            stack("graphite", 200),
            stack("tungsten", 50),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        ..spec("diffuse", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(2.1e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("tungsten", 150),
            stack("silicon", 200),
            stack("oxide", 40),
            stack("beryllium", 400),
        ],
        research_cost: Some(vec![
            stack("tungsten", 400),
            stack("silicon", 400),
            stack("oxide", 80),
            stack("beryllium", 800),
        ]),
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(5e+01f32),
        has_liquids: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("sublimate", BlockKind::ContinuousLiquidTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(2.5e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("tungsten", 250),
            stack("silicon", 300),
            stack("thorium", 400),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquid("hydrogen", 0.083333336f32)],
        ..spec("titan", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(2.8e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("thorium", 50),
            stack("oxide", 50),
            stack("silicon", 200),
            stack("beryllium", 350),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("disperse", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(2.2e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("surge-alloy", 100),
            stack("silicon", 200),
            stack("graphite", 250),
            stack("oxide", 40),
        ],
        research_cost_multiplier: Some(0.04f32),
        build_cost_multiplier: Some(1.5f32),
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(5.0f32)],
        ..spec("afflict", BlockKind::PowerTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(2.1e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("silicon", 250),
            stack("graphite", 200),
            stack("oxide", 50),
            stack("carbide", 90),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_liquid("nitrogen", 0.1f32),
            consume_power(3.3333333f32),
        ],
        ..spec("lustre", BlockKind::ContinuousTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        scaled_health: Some(2.2e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("silicon", 450),
            stack("graphite", 400),
            stack("tungsten", 500),
            stack("oxide", 100),
            stack("carbide", 200),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        ..spec("scathe", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        scaled_health: Some(3.5e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("oxide", 200),
            stack("surge-alloy", 400),
            stack("silicon", 800),
            stack("carbide", 500),
            stack("phase-fabric", 300),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_items: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        ..spec("smite", BlockKind::ItemTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        scaled_health: Some(3.7e+02f32),
        category: Some(Category::Turret),
        requirements: vec![
            stack("carbide", 200),
            stack("beryllium", 1000),
            stack("silicon", 500),
            stack("graphite", 500),
            stack("phase-fabric", 200),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        liquid_capacity: Some(2e+01f32),
        has_power: Some(true),
        env_enabled: Some(EnvMask::of(vec![EnvFlag::Space, EnvFlag::Terrestrial])),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(4e+01f32)],
        ..spec("malign", BlockKind::PowerTurret)
    })?;
    let _ = sink;
    Ok(())
}
