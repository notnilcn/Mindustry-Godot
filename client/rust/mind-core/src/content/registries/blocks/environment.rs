// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `environment`).
//
//! Generated block metadata (`Blocks.java` region `environment`).
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

/// Loads the `environment` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        generate_icons: Some(false),
        ..spec("air", BlockKind::AirBlock)
    })?;

    sink.push(spec("spawn", BlockKind::SpawnBlock))?;

    sink.push(BlockSpec {
        in_editor: Some(false),
        placeable_liquid: Some(true),
        ..spec("remove-wall", BlockKind::RemoveWall)
    })?;

    sink.push(BlockSpec {
        in_editor: Some(false),
        placeable_liquid: Some(true),
        ..spec("remove-ore", BlockKind::RemoveOre)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        save_data: Some(true),
        in_editor: Some(false),
        ..spec("cliff", BlockKind::Cliff)
    })?;

    sink.push(BlockSpec {
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build1", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build2", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build3", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(4),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build4", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build5", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(6),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build6", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(7),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build7", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(8),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build8", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(9),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build9", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(10),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build10", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(11),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build11", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(12),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build12", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(13),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build13", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(14),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build14", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(15),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build15", BlockKind::ConstructBlock)
    })?;

    sink.push(BlockSpec {
        size: Some(16),
        health: Some(10),
        update: Some(true),
        in_editor: Some(false),
        generate_icons: Some(false),
        ..spec("build16", BlockKind::ConstructBlock)
    })?;

    // `liquidDrop` (plan 10 §3.10 append-only addendum; upstream
    // `content/Blocks.java` `liquidDrop = Liquids.<x>`).
    // Floor movement/liquid/drown metadata (`content/Blocks.java`; plan 11 §3.6).
    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.2),
        is_liquid: Some(true),
        drown_time: Some(200.0),
        liquid_multiplier: Some(1.5),
        ..spec("deep-water", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.5),
        is_liquid: Some(true),
        ..spec("shallow-water", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.5),
        is_liquid: Some(true),
        ..spec("tainted-water", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.18),
        is_liquid: Some(true),
        drown_time: Some(200.0),
        liquid_multiplier: Some(1.5),
        ..spec("deep-tainted-water", BlockKind::Floor)
    })?;

    // `ShallowLiquid` defaults `liquidDrop = Liquids.water`; `set()` sets
    // `isLiquid = true`, `shallow = true` and copies the liquid base.
    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.75),
        is_liquid: Some(true),
        ..spec("darksand-tainted-water", BlockKind::ShallowLiquid)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.8),
        is_liquid: Some(true),
        ..spec("sand-water", BlockKind::ShallowLiquid)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("water"),
        speed_multiplier: Some(0.8),
        is_liquid: Some(true),
        ..spec("darksand-water", BlockKind::ShallowLiquid)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("oil"),
        speed_multiplier: Some(0.19),
        is_liquid: Some(true),
        drown_time: Some(230.0),
        ..spec("tar", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("cryofluid"),
        speed_multiplier: Some(0.5),
        is_liquid: Some(true),
        drown_time: Some(150.0),
        liquid_multiplier: Some(0.5),
        ..spec("pooled-cryofluid", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("slag"),
        speed_multiplier: Some(0.19),
        is_liquid: Some(true),
        drown_time: Some(230.0),
        ..spec("molten-slag", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_on: Some(false),
        ..spec("space", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_on: Some(false),
        ..spec("empty", BlockKind::EmptyFloor)
    })?;

    sink.push(spec("stone", BlockKind::Floor))?;

    sink.push(spec("crater-stone", BlockKind::Floor))?;

    sink.push(spec("char", BlockKind::Floor))?;

    sink.push(spec("basalt", BlockKind::Floor))?;

    sink.push(spec("hotrock", BlockKind::Floor))?;

    sink.push(spec("magmarock", BlockKind::Floor))?;

    sink.push(BlockSpec {
        player_unmineable: Some(true),
        item_drop: Some("sand"),
        ..spec("sand-floor", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        player_unmineable: Some(true),
        item_drop: Some("sand"),
        ..spec("darksand", BlockKind::Floor)
    })?;

    sink.push(spec("dirt", BlockKind::Floor))?;

    sink.push(BlockSpec {
        speed_multiplier: Some(0.6),
        ..spec("mud", BlockKind::Floor)
    })?;

    sink.push(spec("dacite", BlockKind::Floor))?;

    sink.push(spec("rhyolite", BlockKind::Floor))?;

    sink.push(spec("rhyolite-crater", BlockKind::Floor))?;

    sink.push(spec("rough-rhyolite", BlockKind::Floor))?;

    sink.push(spec("regolith", BlockKind::Floor))?;

    sink.push(spec("yellow-stone", BlockKind::Floor))?;

    sink.push(spec("carbon-stone", BlockKind::Floor))?;

    sink.push(spec("ferric-stone", BlockKind::Floor))?;

    sink.push(spec("ferric-craters", BlockKind::Floor))?;

    sink.push(spec("beryllic-stone", BlockKind::Floor))?;

    sink.push(spec("crystalline-stone", BlockKind::Floor))?;

    sink.push(spec("crystal-floor", BlockKind::Floor))?;

    sink.push(spec("yellow-stone-plates", BlockKind::Floor))?;

    sink.push(spec("red-stone", BlockKind::Floor))?;

    sink.push(spec("dense-red-stone", BlockKind::Floor))?;

    sink.push(BlockSpec {
        speed_multiplier: Some(0.9),
        ..spec("red-ice", BlockKind::Floor)
    })?;

    sink.push(BlockSpec {
        liquid_drop: Some("arkycite"),
        speed_multiplier: Some(0.3),
        is_liquid: Some(true),
        drown_time: Some(200.0),
        ..spec("arkycite-floor", BlockKind::Floor)
    })?;

    sink.push(spec("arkyic-stone", BlockKind::Floor))?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("rhyolite-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("carbon-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("arkyic-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("yellow-stone-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("red-stone-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("crystalline-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("stone-vent", BlockKind::SteamVent)
    })?;

    sink.push(BlockSpec {
        attributes: vec![("steam", 1.0)],
        flags: vec![BlockFlag::SteamVent],
        ..spec("basalt-vent", BlockKind::SteamVent)
    })?;

    sink.push(spec("redmat", BlockKind::Floor))?;

    sink.push(spec("bluemat", BlockKind::Floor))?;

    sink.push(spec("grass", BlockKind::Floor))?;

    sink.push(spec("salt", BlockKind::Floor))?;

    sink.push(spec("snow", BlockKind::Floor))?;

    sink.push(BlockSpec {
        speed_multiplier: Some(0.9),
        ..spec("ice", BlockKind::Floor)
    })?;

    sink.push(spec("ice-snow", BlockKind::Floor))?;

    sink.push(spec("shale", BlockKind::Floor))?;

    sink.push(spec("moss", BlockKind::Floor))?;

    sink.push(BlockSpec {
        allow_core_placement: Some(true),
        ..spec("core-zone", BlockKind::Floor)
    })?;

    sink.push(spec("spore-moss", BlockKind::Floor))?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("stone-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("spore-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("dirt-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("dacite-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("ice-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("snow-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("dune-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("regolith-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("yellow-stone-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("rhyolite-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("carbon-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("ferric-stone-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("beryllic-stone-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("arkyic-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("crystalline-stone-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("red-ice-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("red-stone-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("red-diamond-wall", BlockKind::StaticTree)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("sand-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("salt-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("shrubs", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("shale-wall", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("spore-pine", BlockKind::StaticTree)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("snow-pine", BlockKind::StaticTree)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("pine", BlockKind::StaticTree)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        ..spec("white-tree-dead", BlockKind::TreeBlock)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        ..spec("white-tree", BlockKind::TreeBlock)
    })?;

    sink.push(spec("spore-cluster", BlockKind::Prop))?;

    sink.push(spec("redweed", BlockKind::Seaweed))?;

    sink.push(spec("pur-bush", BlockKind::SeaBush))?;

    sink.push(spec("yellowcoral", BlockKind::SeaBush))?;

    sink.push(spec("boulder", BlockKind::StaticProp))?;

    sink.push(spec("snow-boulder", BlockKind::StaticProp))?;

    sink.push(spec("shale-boulder", BlockKind::StaticProp))?;

    sink.push(spec("sand-boulder", BlockKind::StaticProp))?;

    sink.push(spec("dacite-boulder", BlockKind::StaticProp))?;

    sink.push(spec("basalt-boulder", BlockKind::StaticProp))?;

    sink.push(spec("carbon-boulder", BlockKind::StaticProp))?;

    sink.push(spec("ferric-boulder", BlockKind::StaticProp))?;

    sink.push(spec("beryllic-boulder", BlockKind::StaticProp))?;

    sink.push(spec("yellow-stone-boulder", BlockKind::StaticProp))?;

    sink.push(spec("arkyic-boulder", BlockKind::Prop))?;

    sink.push(BlockSpec {
        solid: Some(true),
        ..spec("crystal-cluster", BlockKind::TallBlock)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        ..spec("vibrant-crystal-cluster", BlockKind::TallBlock)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        ..spec("crystal-blocks", BlockKind::TallBlock)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        ..spec("crystal-orbs", BlockKind::TallBlock)
    })?;

    sink.push(spec("crystalline-boulder", BlockKind::StaticProp))?;

    sink.push(spec("red-ice-boulder", BlockKind::StaticProp))?;

    sink.push(spec("rhyolite-boulder", BlockKind::StaticProp))?;

    sink.push(spec("red-stone-boulder", BlockKind::StaticProp))?;

    sink.push(spec("metal-floor", BlockKind::Floor))?;

    sink.push(spec("metal-floor-damaged", BlockKind::Floor))?;

    sink.push(spec("metal-floor-2", BlockKind::Floor))?;

    sink.push(spec("metal-floor-3", BlockKind::Floor))?;

    sink.push(spec("metal-floor-4", BlockKind::Floor))?;

    sink.push(spec("metal-floor-5", BlockKind::Floor))?;

    sink.push(spec("dark-panel-1", BlockKind::Floor))?;

    sink.push(spec("dark-panel-2", BlockKind::Floor))?;

    sink.push(spec("dark-panel-3", BlockKind::Floor))?;

    sink.push(spec("dark-panel-4", BlockKind::Floor))?;

    sink.push(spec("dark-panel-5", BlockKind::Floor))?;

    sink.push(spec("dark-panel-6", BlockKind::Floor))?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("dark-metal", BlockKind::StaticWall)
    })?;

    sink.push(spec("metal-tiles-1", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-2", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-3", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-4", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-5", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-6", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-7", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-8", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-9", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-10", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-11", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-12", BlockKind::Floor))?;

    sink.push(spec("metal-tiles-13", BlockKind::Floor))?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("metal-wall-1", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("metal-wall-2", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        placeable_liquid: Some(true),
        ..spec("metal-wall-3", BlockKind::StaticWall)
    })?;

    sink.push(BlockSpec {
        save_data: Some(true),
        save_config: Some(true),
        ..spec("colored-floor", BlockKind::ColoredFloor)
    })?;

    sink.push(BlockSpec {
        solid: Some(true),
        save_data: Some(true),
        save_config: Some(true),
        placeable_liquid: Some(true),
        ..spec("colored-wall", BlockKind::ColoredWall)
    })?;

    sink.push(BlockSpec {
        save_data: Some(true),
        save_config: Some(true),
        ..spec("character-overlay", BlockKind::CharacterOverlay)
    })?;

    sink.push(BlockSpec {
        save_data: Some(true),
        save_config: Some(true),
        ..spec("character-overlay-white", BlockKind::CharacterOverlay)
    })?;

    sink.push(BlockSpec {
        save_data: Some(true),
        save_config: Some(true),
        ..spec("rune-overlay", BlockKind::RuneOverlay)
    })?;

    sink.push(BlockSpec {
        save_data: Some(true),
        save_config: Some(true),
        ..spec("rune-overlay-crux", BlockKind::RuneOverlay)
    })?;

    sink.push(spec("pebbles", BlockKind::OverlayFloor))?;

    sink.push(spec("tendrils", BlockKind::OverlayFloor))?;
    let _ = sink;
    Ok(())
}
