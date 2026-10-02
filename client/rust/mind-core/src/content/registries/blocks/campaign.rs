// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `campaign`).
//
//! Generated block metadata (`Blocks.java` region `campaign`).
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

/// Loads the `campaign` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Effect),
        requirements: vec![
            stack("copper", 350),
            stack("silicon", 140),
            stack("lead", 200),
            stack("titanium", 150),
        ],
        flags: vec![BlockFlag::LaunchPad],
        item_capacity: Some(100),
        has_items: Some(true),
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::LegacyLaunchPadOnly),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(4.0f32)],
        ..spec("launch-pad", BlockKind::LaunchPad)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        category: Some(Category::Effect),
        requirements: vec![
            stack("copper", 350),
            stack("silicon", 250),
            stack("lead", 300),
            stack("titanium", 200),
        ],
        flags: vec![BlockFlag::LaunchPad],
        item_capacity: Some(100),
        liquid_capacity: Some(4e+01f32),
        has_items: Some(true),
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::NotLegacyLaunchPadOnly),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_liquid("oil", 0.15f32), consume_power(8.0f32)],
        ..spec("advanced-launch-pad", BlockKind::LaunchPad)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        category: Some(Category::Effect),
        requirements: vec![
            stack("copper", 200),
            stack("graphite", 100),
            stack("titanium", 100),
        ],
        item_capacity: Some(100),
        liquid_capacity: Some(3e+03f32),
        has_items: Some(true),
        has_liquids: Some(true),
        build_visibility: Some(BuildVisibility::NotLegacyLaunchPadOnly),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        ..spec("landing-pad", BlockKind::LandingPad)
    })?;

    sink.push(BlockSpec {
        size: Some(7),
        scaled_health: Some(8e+01f32),
        category: Some(Category::Effect),
        requirements: vec![
            stack("copper", 16000),
            stack("silicon", 11000),
            stack("thorium", 13000),
            stack("titanium", 12000),
            stack("surge-alloy", 6000),
            stack("phase-fabric", 5000),
        ],
        research_cost_multiplier: Some(0.1f32),
        build_cost_multiplier: Some(0.5f32),
        item_capacity: Some(8000),
        has_items: Some(true),
        has_power: Some(true),
        build_visibility: Some(BuildVisibility::CampaignOnly),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        consumes: vec![consume_power(1e+01f32)],
        ..spec("interplanetary-accelerator", BlockKind::Accelerator)
    })?;
    let _ = sink;
    Ok(())
}
