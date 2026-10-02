// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `legacy`).
//
//! Generated block metadata (`Blocks.java` region `legacy`).
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

/// Loads the `legacy` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        has_power: Some(true),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("legacy-mech-pad", BlockKind::LegacyMechPad)
    })?;

    sink.push(BlockSpec {
        has_items: Some(true),
        has_power: Some(true),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("legacy-unit-factory", BlockKind::LegacyUnitFactory)
    })?;

    sink.push(BlockSpec {
        has_items: Some(true),
        has_power: Some(true),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("legacy-unit-factory-air", BlockKind::LegacyUnitFactory)
    })?;

    sink.push(BlockSpec {
        has_items: Some(true),
        has_power: Some(true),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("legacy-unit-factory-ground", BlockKind::LegacyUnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("command-center", BlockKind::LegacyCommandCenter)
    })?;
    let _ = sink;
    Ok(())
}
