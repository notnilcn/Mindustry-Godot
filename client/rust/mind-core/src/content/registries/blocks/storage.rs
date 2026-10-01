// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `storage`).
//
//! Generated block metadata (`Blocks.java` region `storage`).
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

/// Loads the `storage` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(3),
        health: Some(1100),
        category: Some(Category::Effect),
        requirements: vec![stack("copper", 1000), stack("lead", 800)],
        build_cost_multiplier: Some(2.0f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_CORE),
        unit_cap_modifier: Some(8),
        flags: vec![BlockFlag::Core],
        item_capacity: Some(4000),
        has_items: Some(true),
        build_visibility: Some(BuildVisibility::CoreZoneOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        ..spec("core-shard", BlockKind::CoreBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        health: Some(3500),
        category: Some(Category::Effect),
        requirements: vec![
            stack("copper", 3000),
            stack("lead", 3000),
            stack("silicon", 2000),
        ],
        research_cost_multiplier: Some(0.07f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_CORE),
        unit_cap_modifier: Some(16),
        flags: vec![BlockFlag::Core],
        item_capacity: Some(9000),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        ..spec("core-foundation", BlockKind::CoreBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        health: Some(6000),
        category: Some(Category::Effect),
        requirements: vec![
            stack("copper", 8000),
            stack("lead", 8000),
            stack("silicon", 5000),
            stack("thorium", 4000),
        ],
        research_cost_multiplier: Some(0.11f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_CORE),
        unit_cap_modifier: Some(24),
        flags: vec![BlockFlag::Core],
        item_capacity: Some(13000),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        ..spec("core-nucleus", BlockKind::CoreBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        health: Some(4500),
        armor: Some(5.0f32),
        category: Some(Category::Effect),
        requirements: vec![
            stack("graphite", 1000),
            stack("silicon", 1000),
            stack("beryllium", 800),
        ],
        research_cost_multiplier: Some(0.07f32),
        build_cost_multiplier: Some(0.7f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_CORE),
        unit_cap_modifier: Some(15),
        flags: vec![BlockFlag::Core],
        item_capacity: Some(2000),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        ..spec("core-bastion", BlockKind::CoreBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        health: Some(16000),
        armor: Some(1e+01f32),
        category: Some(Category::Effect),
        requirements: vec![
            stack("silicon", 4000),
            stack("beryllium", 4000),
            stack("tungsten", 3000),
            stack("oxide", 1000),
        ],
        research_cost_multiplier: Some(0.17f32),
        research_cost_multipliers: vec![("silicon", 0.5f32)],
        build_cost_multiplier: Some(0.7f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_CORE),
        unit_cap_modifier: Some(15),
        flags: vec![BlockFlag::Core],
        item_capacity: Some(3000),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        ..spec("core-citadel", BlockKind::CoreBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(6),
        health: Some(30000),
        armor: Some(15.0f32),
        category: Some(Category::Effect),
        requirements: vec![
            stack("beryllium", 6000),
            stack("silicon", 5000),
            stack("tungsten", 5000),
            stack("carbide", 3000),
            stack("oxide", 3000),
        ],
        research_cost_multiplier: Some(0.1f32),
        research_cost_multipliers: vec![("silicon", 0.4f32)],
        build_cost_multiplier: Some(0.7f32),
        group: Some(BlockGroup::Transportation),
        priority: Some(TARGET_PRIORITY_CORE),
        unit_cap_modifier: Some(15),
        flags: vec![BlockFlag::Core],
        item_capacity: Some(4000),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        destructible: Some(true),
        ..spec("core-acropolis", BlockKind::CoreBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(55.0f32),
        category: Some(Category::Effect),
        requirements: vec![stack("titanium", 100)],
        group: Some(BlockGroup::Transportation),
        flags: vec![BlockFlag::Storage],
        item_capacity: Some(300),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("container", BlockKind::StorageBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(55.0f32),
        category: Some(Category::Effect),
        requirements: vec![stack("titanium", 250), stack("thorium", 125)],
        group: Some(BlockGroup::Transportation),
        flags: vec![BlockFlag::Storage],
        item_capacity: Some(1000),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("vault", BlockKind::StorageBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        scaled_health: Some(1.2e+02f32),
        category: Some(Category::Effect),
        requirements: vec![stack("tungsten", 30), stack("graphite", 40)],
        group: Some(BlockGroup::Transportation),
        flags: vec![BlockFlag::Storage],
        item_capacity: Some(160),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("reinforced-container", BlockKind::StorageBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        scaled_health: Some(1.2e+02f32),
        category: Some(Category::Effect),
        requirements: vec![
            stack("tungsten", 125),
            stack("thorium", 70),
            stack("beryllium", 100),
        ],
        group: Some(BlockGroup::Transportation),
        flags: vec![BlockFlag::Storage],
        item_capacity: Some(900),
        has_items: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("reinforced-vault", BlockKind::StorageBlock)
    })?;
    let _ = sink;
    Ok(())
}
