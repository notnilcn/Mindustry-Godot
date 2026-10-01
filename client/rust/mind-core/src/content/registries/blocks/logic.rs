// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `logic`).
//
//! Generated block metadata (`Blocks.java` region `logic`).
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

/// Loads the `logic` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        category: Some(Category::Logic),
        requirements: vec![stack("graphite", 5), stack("copper", 5)],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("message", BlockKind::MessageBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        requirements: vec![stack("graphite", 5), stack("copper", 5)],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        update: Some(true),
        configurable: Some(true),
        ..spec("switch", BlockKind::SwitchBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        requirements: vec![stack("copper", 90), stack("lead", 50), stack("silicon", 50)],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        ..spec("micro-processor", BlockKind::LogicBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Logic),
        requirements: vec![
            stack("lead", 320),
            stack("silicon", 80),
            stack("graphite", 60),
            stack("thorium", 50),
        ],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        ..spec("logic-processor", BlockKind::LogicBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Logic),
        requirements: vec![
            stack("lead", 450),
            stack("silicon", 150),
            stack("thorium", 75),
            stack("surge-alloy", 50),
        ],
        group: Some(BlockGroup::Logic),
        has_liquids: Some(true),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_liquid("cryofluid", 0.08f32)],
        ..spec("hyper-processor", BlockKind::LogicBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        requirements: vec![
            stack("graphite", 30),
            stack("silicon", 30),
            stack("copper", 30),
        ],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("memory-cell", BlockKind::MemoryBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Logic),
        requirements: vec![
            stack("graphite", 80),
            stack("silicon", 80),
            stack("phase-fabric", 30),
            stack("copper", 30),
        ],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("memory-bank", BlockKind::MemoryBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Logic),
        requirements: vec![
            stack("lead", 100),
            stack("silicon", 50),
            stack("metaglass", 50),
        ],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("logic-display", BlockKind::LogicDisplay)
    })?;

    sink.push(BlockSpec {
        size: Some(6),
        category: Some(Category::Logic),
        requirements: vec![
            stack("lead", 200),
            stack("silicon", 150),
            stack("metaglass", 100),
            stack("phase-fabric", 75),
        ],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("large-logic-display", BlockKind::LogicDisplay)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        requirements: vec![
            stack("lead", 8),
            stack("silicon", 8),
            stack("metaglass", 8),
            stack("phase-fabric", 3),
        ],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("tile-logic-display", BlockKind::TileableLogicDisplay)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Logic),
        requirements: vec![stack("silicon", 10), stack("beryllium", 10)],
        build_visibility: Some(BuildVisibility::Shown),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("canvas", BlockKind::CanvasBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Logic),
        requirements: vec![
            stack("silicon", 15),
            stack("beryllium", 15),
            stack("surge-alloy", 5),
        ],
        build_visibility: Some(BuildVisibility::Shown),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("large-canvas", BlockKind::CanvasBlock)
    })?;

    sink.push(BlockSpec {
        health: Some(100),
        category: Some(Category::Logic),
        requirements: vec![stack("graphite", 10), stack("beryllium", 5)],
        group: Some(BlockGroup::Logic),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("reinforced-message", BlockKind::MessageBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        group: Some(BlockGroup::Logic),
        build_visibility: Some(BuildVisibility::WorldProcessorOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        ..spec("world-processor", BlockKind::LogicBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        group: Some(BlockGroup::Logic),
        build_visibility: Some(BuildVisibility::WorldProcessorOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        ..spec("world-cell", BlockKind::MemoryBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        group: Some(BlockGroup::Logic),
        build_visibility: Some(BuildVisibility::WorldProcessorOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("world-message", BlockKind::MessageBlock)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Logic),
        group: Some(BlockGroup::Logic),
        build_visibility: Some(BuildVisibility::WorldProcessorOnly),
        env_enabled: Some(EnvMask::any()),
        update: Some(true),
        configurable: Some(true),
        ..spec("world-switch", BlockKind::SwitchBlock)
    })?;
    let _ = sink;
    Ok(())
}
