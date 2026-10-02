// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `defense`).
//
//! Generated block metadata (`Blocks.java` region `defense`).
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

/// Loads the `defense` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        health: Some(320),
        category: Some(Category::Defense),
        requirements: vec![stack("copper", 6)],
        research_cost_multiplier: Some(0.1f32),
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("copper-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(1280),
        category: Some(Category::Defense),
        requirements: vec![stack("copper", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("copper-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(440),
        category: Some(Category::Defense),
        requirements: vec![stack("titanium", 6)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("titanium-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(1760),
        category: Some(Category::Defense),
        requirements: vec![stack("titanium", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("titanium-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(500),
        category: Some(Category::Defense),
        requirements: vec![stack("plastanium", 5), stack("metaglass", 2)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        insulated: Some(true),
        absorb_lasers: Some(true),
        ..spec("plastanium-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(2000),
        category: Some(Category::Defense),
        requirements: vec![stack("plastanium", 20), stack("metaglass", 8)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        insulated: Some(true),
        absorb_lasers: Some(true),
        ..spec("plastanium-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(800),
        category: Some(Category::Defense),
        requirements: vec![stack("thorium", 6)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("thorium-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(3200),
        category: Some(Category::Defense),
        requirements: vec![stack("thorium", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("thorium-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(600),
        category: Some(Category::Defense),
        requirements: vec![stack("phase-fabric", 6)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("phase-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(2400),
        category: Some(Category::Defense),
        requirements: vec![stack("phase-fabric", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("phase-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(920),
        category: Some(Category::Defense),
        requirements: vec![stack("surge-alloy", 6)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("surge-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(3680),
        category: Some(Category::Defense),
        requirements: vec![stack("surge-alloy", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("surge-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(400),
        category: Some(Category::Defense),
        requirements: vec![stack("titanium", 6), stack("silicon", 4)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        destructible: Some(true),
        ..spec("door", BlockKind::Door)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(1600),
        category: Some(Category::Defense),
        requirements: vec![stack("titanium", 24), stack("silicon", 16)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        destructible: Some(true),
        ..spec("door-large", BlockKind::Door)
    })?;

    sink.push(BlockSpec {
        health: Some(240),
        category: Some(Category::Defense),
        requirements: vec![stack("scrap", 6)],
        build_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("scrap-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(960),
        category: Some(Category::Defense),
        requirements: vec![stack("scrap", 24)],
        build_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("scrap-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(2160),
        category: Some(Category::Defense),
        requirements: vec![stack("scrap", 54)],
        build_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("scrap-wall-huge", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        health: Some(3840),
        category: Some(Category::Defense),
        requirements: vec![stack("scrap", 96)],
        build_cost_multiplier: Some(4.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("scrap-wall-gigantic", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        health: Some(3520),
        category: Some(Category::Defense),
        requirements: vec![stack("scrap", 96)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("thruster", BlockKind::Thruster)
    })?;

    sink.push(BlockSpec {
        health: Some(520),
        armor: Some(2.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("beryllium", 6)],
        build_cost_multiplier: Some(8.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("beryllium-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(2080),
        armor: Some(2.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("beryllium", 24)],
        build_cost_multiplier: Some(5.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("beryllium-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(720),
        armor: Some(14.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("tungsten", 6)],
        build_cost_multiplier: Some(8.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("tungsten-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(2880),
        armor: Some(14.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("tungsten", 24)],
        build_cost_multiplier: Some(5.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("tungsten-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(2800),
        armor: Some(14.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("tungsten", 24), stack("silicon", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        update: Some(true),
        destructible: Some(true),
        ..spec("blast-door", BlockKind::AutoDoor)
    })?;

    sink.push(BlockSpec {
        health: Some(1000),
        armor: Some(2e+01f32),
        category: Some(Category::Defense),
        requirements: vec![stack("surge-alloy", 6), stack("tungsten", 2)],
        research_cost: Some(vec![stack("surge-alloy", 20), stack("tungsten", 100)]),
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("reinforced-surge-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(4000),
        armor: Some(2e+01f32),
        category: Some(Category::Defense),
        requirements: vec![stack("surge-alloy", 24), stack("tungsten", 8)],
        research_cost: Some(vec![stack("surge-alloy", 40), stack("tungsten", 200)]),
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("reinforced-surge-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        health: Some(1080),
        armor: Some(16.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("thorium", 6), stack("carbide", 6)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("carbide-wall", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(4320),
        armor: Some(16.0f32),
        category: Some(Category::Defense),
        requirements: vec![stack("thorium", 24), stack("carbide", 24)],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("carbide-wall-large", BlockKind::Wall)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(4160),
        armor: Some(15.0f32),
        category: Some(Category::Defense),
        requirements: vec![
            stack("phase-fabric", 20),
            stack("surge-alloy", 12),
            stack("beryllium", 12),
        ],
        build_cost_multiplier: Some(6.0f32),
        group: Some(BlockGroup::Walls),
        priority: Some(TARGET_PRIORITY_WALL),
        has_power: Some(true),
        conductive_power: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        consumes: vec![consume_power(0.05f32)],
        ..spec("shielded-wall", BlockKind::ShieldWall)
    })?;

    sink.push(BlockSpec {
        health: Some(80),
        category: Some(Category::Effect),
        requirements: vec![stack("lead", 30), stack("copper", 25)],
        group: Some(BlockGroup::Projectors),
        flags: vec![BlockFlag::BlockRepair],
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(0.3f32),
            consume_optional(consume_items(vec![stack("silicon", 1)])),
        ],
        ..spec("mender", BlockKind::MendProjector)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(8e+01f32),
        category: Some(Category::Effect),
        requirements: vec![
            stack("lead", 100),
            stack("titanium", 25),
            stack("silicon", 40),
            stack("copper", 50),
        ],
        group: Some(BlockGroup::Projectors),
        flags: vec![BlockFlag::BlockRepair],
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1.5f32),
            consume_optional(consume_items(vec![stack("phase-fabric", 1)])),
        ],
        ..spec("mend-projector", BlockKind::MendProjector)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Effect),
        requirements: vec![
            stack("lead", 100),
            stack("titanium", 75),
            stack("silicon", 75),
            stack("plastanium", 30),
        ],
        group: Some(BlockGroup::Projectors),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(3.5f32),
            consume_optional(consume_items(vec![stack("phase-fabric", 1)])),
        ],
        ..spec("overdrive-projector", BlockKind::OverdriveProjector)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        requirements: vec![
            stack("lead", 200),
            stack("titanium", 130),
            stack("silicon", 130),
            stack("plastanium", 80),
            stack("surge-alloy", 120),
        ],
        group: Some(BlockGroup::Projectors),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1e+01f32),
            consume_items(vec![stack("phase-fabric", 1), stack("silicon", 1)]),
        ],
        ..spec("overdrive-dome", BlockKind::OverdriveProjector)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        requirements: vec![
            stack("lead", 100),
            stack("titanium", 75),
            stack("silicon", 125),
        ],
        group: Some(BlockGroup::Projectors),
        flags: vec![BlockFlag::Shield],
        has_items: Some(true),
        has_liquids: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(4.0f32)],
        ..spec("force-projector", BlockKind::ForceProjector)
    })?;

    sink.push(BlockSpec {
        health: Some(50),
        category: Some(Category::Effect),
        requirements: vec![stack("lead", 25), stack("silicon", 12)],
        destructible: Some(true),
        ..spec("shock-mine", BlockKind::ShockMine)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Effect),
        requirements: vec![
            stack("silicon", 60),
            stack("graphite", 50),
            stack("beryllium", 10),
        ],
        research_cost: Some(vec![stack("silicon", 70), stack("graphite", 70)]),
        flags: vec![BlockFlag::HasFogRadius],
        build_visibility: Some(BuildVisibility::FogOnly),
        fog_radius: Some(34),
        consumes: vec![consume_power(0.6f32)],
        ..spec("radar", BlockKind::Radar)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        requirements: vec![
            stack("silicon", 150),
            stack("oxide", 40),
            stack("thorium", 60),
        ],
        group: Some(BlockGroup::Turrets),
        priority: Some(TARGET_PRIORITY_TURRET),
        flags: vec![BlockFlag::Turret],
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_power(3.0f32), consume_liquid("nitrogen", 0.05f32)],
        ..spec("build-tower", BlockKind::BuildTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        requirements: vec![
            stack("silicon", 80),
            stack("tungsten", 60),
            stack("oxide", 40),
            stack("beryllium", 80),
        ],
        group: Some(BlockGroup::Projectors),
        flags: vec![BlockFlag::BlockRepair],
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        consumes: vec![
            consume_power(1.0f32),
            consume_liquid("hydrogen", 0.016666668f32),
            consume_optional(consume_items(vec![stack("phase-fabric", 1)])),
        ],
        ..spec("regen-projector", BlockKind::RegenProjector)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        requirements: vec![
            stack("surge-alloy", 50),
            stack("silicon", 150),
            stack("oxide", 30),
            stack("tungsten", 100),
        ],
        solid: Some(true),
        update: Some(true),
        consumes: vec![consume_liquids(vec![]), consume_power(1.6666666f32)],
        ..spec("shockwave-tower", BlockKind::ShockwaveTower)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::EditorOnly),
        consumes: vec![consume_power(5.0f32)],
        ..spec("shield-projector", BlockKind::BaseShield)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        category: Some(Category::Effect),
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::EditorOnly),
        consumes: vec![consume_power(5.0f32)],
        ..spec("large-shield-projector", BlockKind::BaseShield)
    })?;
    let _ = sink;
    Ok(())
}
