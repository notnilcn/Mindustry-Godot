// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `sandbox`).
//
//! Generated block metadata (`Blocks.java` region `sandbox`).
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

/// Loads the `sandbox` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        category: Some(Category::Power),
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        outputs_power: Some(true),
        consumes_power: Some(false),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        destructible: Some(true),
        configurable: Some(true),
        ..spec("power-source", BlockKind::PowerSource)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Power),
        group: Some(BlockGroup::Power),
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        ..spec("power-void", BlockKind::PowerVoid)
    })?;

    sink.push(BlockSpec {
        group: Some(BlockGroup::Transportation),
        has_items: Some(true),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("item-source", BlockKind::ItemSource)
    })?;

    sink.push(BlockSpec {
        group: Some(BlockGroup::Transportation),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        ..spec("item-void", BlockKind::ItemVoid)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1e+04f32),
        has_liquids: Some(true),
        outputs_liquid: Some(true),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        ..spec("liquid-source", BlockKind::LiquidSource)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Liquid),
        group: Some(BlockGroup::Liquids),
        liquid_capacity: Some(1e+04f32),
        has_liquids: Some(true),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        env_enabled: Some(EnvMask::any()),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        ..spec("liquid-void", BlockKind::LiquidVoid)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        group: Some(BlockGroup::Payloads),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        update: Some(true),
        configurable: Some(true),
        ..spec("payload-source", BlockKind::PayloadSource)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        group: Some(BlockGroup::Payloads),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        update: Some(true),
        ..spec("payload-void", BlockKind::PayloadVoid)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Crafting),
        item_capacity: Some(0),
        has_items: Some(true),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        solid: Some(true),
        update: Some(true),
        ..spec("heat-source", BlockKind::HeatProducer)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Defense),
        build_visibility: Some(BuildVisibility::SandboxOnly),
        save_config: Some(true),
        ..spec("target-dummy", BlockKind::TargetDummy)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Effect),
        requirements: vec![stack("graphite", 12), stack("silicon", 8), stack("lead", 8)],
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::LightingOnly),
        update: Some(true),
        save_config: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(0.05f32)],
        ..spec("illuminator", BlockKind::LightBlock)
    })?;
    let _ = sink;
    Ok(())
}
