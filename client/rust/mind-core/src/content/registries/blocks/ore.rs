// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `ore`).
//
//! Generated block metadata (`Blocks.java` region `ore`).
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

/// Loads the `ore` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        item_drop: Some("copper"),
        ore_default: Some(true),
        ore_threshold: Some(0.81f32),
        ore_scale: Some(23.47619f32),
        map_color: Some(rgba_hex("d99d73")),
        ..spec("ore-copper", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("lead"),
        ore_default: Some(true),
        ore_scale: Some(23.952381f32),
        map_color: Some(rgba_hex("8c7fa9")),
        ..spec("ore-lead", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("scrap"),
        map_color: Some(rgba_hex("777777")),
        ..spec("ore-scrap", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("coal"),
        ore_default: Some(true),
        ore_threshold: Some(0.846f32),
        ore_scale: Some(24.428572f32),
        map_color: Some(rgba_hex("272727")),
        ..spec("ore-coal", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("titanium"),
        ore_default: Some(true),
        ore_threshold: Some(0.864f32),
        ore_scale: Some(24.904762f32),
        map_color: Some(rgba_hex("8da1e3")),
        ..spec("ore-titanium", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("thorium"),
        ore_default: Some(true),
        ore_threshold: Some(0.882f32),
        ore_scale: Some(25.380953f32),
        map_color: Some(rgba_hex("f9a3c7")),
        ..spec("ore-thorium", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("beryllium"),
        map_color: Some(rgba_hex("3a8f64")),
        ..spec("ore-beryllium", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("tungsten"),
        map_color: Some(rgba_hex("768a9a")),
        ..spec("ore-tungsten", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        item_drop: Some("thorium"),
        map_color: Some(rgba_hex("f9a3c7")),
        ..spec("ore-crystal-thorium", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        wall_ore: Some(true),
        item_drop: Some("thorium"),
        map_color: Some(rgba_hex("f9a3c7")),
        ..spec("ore-wall-thorium", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        wall_ore: Some(true),
        item_drop: Some("beryllium"),
        map_color: Some(rgba_hex("3a8f64")),
        ..spec("ore-wall-beryllium", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        item_drop: Some("graphite"),
        ..spec("graphitic-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        wall_ore: Some(true),
        item_drop: Some("graphite"),
        map_color: Some(rgba_hex("b2c6d2")),
        ..spec("ore-wall-graphite", BlockKind::OreBlock)
    })?;

    sink.push(BlockSpec {
        wall_ore: Some(true),
        item_drop: Some("tungsten"),
        map_color: Some(rgba_hex("768a9a")),
        ..spec("ore-wall-tungsten", BlockKind::OreBlock)
    })?;
    let _ = sink;
    Ok(())
}
