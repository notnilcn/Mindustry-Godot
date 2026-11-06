// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Legacy block behavior (`world/blocks/legacy/*`).
//!
//! Legacy blocks are hidden content removed during `World.endMapLoad`: Java
//! iterates every tile and calls `LegacyBlock.removeSelf` (base = `tile.remove`,
//! `LegacyUnitFactory` = `tile.setBlock(replacement, team, rotation)`). The port
//! carries the marker behavior, the per-kind `read`/`write` byte skips and a
//! grid sweep ([`remove_legacy_blocks`]) for the load path.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::world::behavior::{BuildingBehavior, BuildingReader, BuildingWriter};
use crate::world::block::BlockTable;
use crate::world::{BlockKindData, WorldGrid};

/// Marker behavior for `LegacyBlock` + subclasses.
#[derive(Debug, Default, Clone, Copy)]
pub struct LegacyBehavior;

/// Block kind of a legacy building entity.
fn legacy_kind(world: &World, e: Entity) -> Option<BlockKind> {
    let block = world
        .get::<crate::entities::comp::Building>(e)
        .map(|building| building.block)?;
    world
        .get_resource::<BlockTable>()?
        .get(block)
        .map(|inst| inst.def.kind)
}

impl BuildingBehavior for LegacyBehavior {
    fn update_tile(&self, _world: &mut World, _e: Entity) {}

    /// `LegacyCommandCenter.LegacyCommandBuild.write` (`LegacyCommandCenter.java:20-24`):
    /// `super.write(write); write.b(0)`.
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if matches!(legacy_kind(world, e), Some(BlockKind::LegacyCommandCenter)) {
            w.b(0);
        }
    }

    /// The legacy `read` bodies discard their saved fields:
    ///
    /// * `LegacyCommandCenter.java:26-30`: one byte;
    /// * `LegacyMechPad.java:17-23`: three floats (pad data);
    /// * `LegacyUnitFactory.java:28-36`: one float (build time) and, on
    ///   revision 0, one int (spawn count).
    ///
    /// Without these skips an old save leaves the reader mid-record and every
    /// following building misparses.
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        match legacy_kind(world, e) {
            Some(BlockKind::LegacyCommandCenter) => {
                let _ = r.b();
            }
            Some(BlockKind::LegacyMechPad) => {
                let _ = r.f();
                let _ = r.f();
                let _ = r.f();
            }
            Some(BlockKind::LegacyUnitFactory) => {
                let _ = r.f();
                if revision == 0 {
                    let _ = r.i();
                }
            }
            _ => {}
        }
    }
}

/// Whether a block is a legacy placeholder.
pub fn is_legacy(data: &BlockKindData) -> bool {
    matches!(data, BlockKindData::Legacy(_))
}

/// Replacement block id for a legacy block, when defined.
pub fn legacy_replacement(data: &BlockKindData) -> Option<u16> {
    match data {
        BlockKindData::Legacy(def) => def.replacement,
        _ => None,
    }
}

/// Whether a [`crate::content::BlockKind`] is a legacy placeholder
/// (the load sweep runs before any ECS entity exists).
pub fn is_legacy_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::LegacyMechPad | BlockKind::LegacyUnitFactory | BlockKind::LegacyCommandCenter
    )
}

/// `World.endMapLoad` sweep hook (plan 06 calls `legacy_remove_self` per tile).
/// Returns whether the block should be removed from the world.
pub fn should_remove_on_load(data: &BlockKindData) -> bool {
    is_legacy(data)
}

/// `Block.legacyRemoveSelf` / `LegacyUnitFactory.replacement`
/// (`LegacyUnitFactory.java:9,20-24`, `Blocks.java:6824-6834`).
///
/// `None` = `tile.remove()` (air); `Some(id)` = `tile.setBlock(id, team, rot)`.
/// The three `legacy-unit-factory-air`/`-ground` variants replace with the
/// Serpulo air/ground factories; the base `legacy-unit-factory` replaces with
/// `Blocks.air`, exactly like the Java `replacement = Blocks.air` default.
pub fn replacement_block(
    content: &ContentRegistry,
    name: &str,
    kind: BlockKind,
) -> Option<BlockId> {
    if kind != BlockKind::LegacyUnitFactory {
        return None;
    }
    let target = match name {
        "legacy-unit-factory-air" => "air-factory",
        "legacy-unit-factory-ground" => "ground-factory",
        _ => return None,
    };
    content.block_id(target)
}

/// Applies the `World.endMapLoad` legacy pass to a freshly read grid:
/// `for(Tile tile : tiles) if(tile.block() instanceof LegacyBlock l)
///  l.removeSelf(tile);` (`World.java:220-230`).
///
/// Runs before buildings are materialized, so the replacement block occupies
/// the legacy tile's footprint and the normal spawn path creates its entity.
/// Returns the number of legacy tiles changed.
pub fn remove_legacy_blocks(grid: &mut WorldGrid, content: &ContentRegistry) -> usize {
    let width = grid.tiles.width;
    let height = grid.tiles.height;
    let mut changed = 0;
    for y in 0..height {
        for x in 0..width {
            let (tx, ty) = (x as i32, y as i32);
            let block = grid.tiles.get(tx, ty).block;
            if block == BlockId::AIR {
                continue;
            }
            let Some(def) = content.block(block) else {
                continue;
            };
            if !is_legacy_kind(def.kind) {
                continue;
            }
            match replacement_block(content, &def.name, def.kind) {
                Some(replacement) => {
                    let Some(inst) = content.block(replacement) else {
                        continue;
                    };
                    // `tile.setBlock(replacement, team, rot)`: the new block is
                    // centred on the legacy tile, clearing its footprint first.
                    let size = inst.size.max(1);
                    let corner = -((size - 1) / 2);
                    for dx in 0..size {
                        for dy in 0..size {
                            let (ox, oy) = (tx + corner + dx, ty + corner + dy);
                            if grid.tiles.in_bounds(ox, oy) {
                                let tile = grid.tiles.get_mut(ox, oy);
                                tile.block = replacement;
                                tile.build = None;
                            }
                        }
                    }
                    changed += 1;
                }
                None => {
                    // `LegacyBlock.removeSelf` = `tile.remove()`.
                    let tile = grid.tiles.get_mut(tx, ty);
                    tile.block = BlockId::AIR;
                    tile.build = None;
                    changed += 1;
                }
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::wire::{WireReader, WireWriter};
    use crate::world::TilePos;
    use crate::world::block::BlockTable;

    #[test]
    fn legacy_kinds_are_marked() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let mut legacy_seen = false;
        for inst in table.iter() {
            if is_legacy(&inst.kind_data) {
                legacy_seen = true;
                assert!(should_remove_on_load(&inst.kind_data));
            }
        }
        // The vanilla registry carries legacy blocks only on some branches; the
        // predicate must still be well-defined either way.
        let none = BlockKindData::None;
        assert!(!is_legacy(&none));
        assert_eq!(legacy_replacement(&none), None);
        let _ = legacy_seen;
    }

    fn legacy_world(name: &str) -> (World, Entity) {
        use crate::sim::runtime::load_vanilla_content;
        let content = load_vanilla_content().expect("vanilla content");
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named(name).expect(name).clone();
        let mut world = World::new();
        let entity = inst.spawn(
            &mut world,
            0,
            TilePos::new(5, 5),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, entity);
        world.insert_resource(table);
        (world, entity)
    }

    /// `LegacyCommandCenter.read` discards one byte and `write` emits it back.
    #[test]
    fn command_center_skips_and_writes_its_byte() {
        let (mut world, entity) = legacy_world("command-center");
        let bytes = [0x2a, 0x99];
        let mut reader = WireReader::new(&bytes);
        LegacyBehavior.read(&mut world, entity, &mut reader, 0);
        assert_eq!(reader.pos(), 1, "read.b() consumed one byte");
        assert_eq!(reader.remaining(), 1, "the trailing byte is untouched");

        let mut out = Vec::new();
        let mut writer = WireWriter::new(&mut out);
        LegacyBehavior.write(&world, entity, &mut writer);
        assert_eq!(out, vec![0], "write.b(0)");
    }

    /// `LegacyMechPad.read` discards three floats (no write override).
    #[test]
    fn legacy_mech_pad_skips_three_floats() {
        let (mut world, entity) = legacy_world("legacy-mech-pad");
        let bytes = [0u8; 14];
        let mut reader = WireReader::new(&bytes);
        LegacyBehavior.read(&mut world, entity, &mut reader, 0);
        assert_eq!(reader.pos(), 12, "three f32 fields");
    }

    /// `LegacyUnitFactory.read` discards a float and (revision 0) an int.
    #[test]
    fn legacy_unit_factory_skips_build_time_and_spawn_count() {
        let (mut world, entity) = legacy_world("legacy-unit-factory");
        let bytes = [0u8; 10];
        let mut reader = WireReader::new(&bytes);
        LegacyBehavior.read(&mut world, entity, &mut reader, 0);
        assert_eq!(reader.pos(), 8, "f build time + i spawn count");
        // Revision >= 1 has no spawn count (`LegacyUnitFactory.java:34`).
        let mut reader = WireReader::new(&bytes);
        LegacyBehavior.read(&mut world, entity, &mut reader, 1);
        assert_eq!(reader.pos(), 4, "revision 1: float only");
    }

    /// `Blocks.java:6824-6834` + `LegacyUnitFactory.removeSelf`: legacy tiles
    /// become air, except the air/ground factory variants, which become the
    /// 3x3 `air-factory`/`ground-factory` centered on the legacy tile.
    #[test]
    fn sweep_removes_and_replaces_legacy_blocks() {
        use crate::sim::runtime::load_vanilla_content;
        let content = load_vanilla_content().expect("vanilla content");
        let mut grid = WorldGrid::new(16, 16);
        let name = |n: &str| content.block_id(n).expect(n);
        grid.tiles.get_mut(5, 5).block = name("legacy-mech-pad");
        grid.tiles.get_mut(7, 7).block = name("legacy-unit-factory-air");
        grid.tiles.get_mut(10, 2).block = name("legacy-unit-factory-ground");
        grid.tiles.get_mut(12, 12).block = name("command-center");

        let changed = remove_legacy_blocks(&mut grid, &content);
        assert_eq!(changed, 4);
        assert_eq!(grid.tiles.get(5, 5).block, BlockId::AIR);
        assert_eq!(grid.tiles.get(12, 12).block, BlockId::AIR);
        // 3x3 factory centered on the legacy tile (Java `Tile.setBlock`).
        for dx in -1..=1 {
            for dy in -1..=1 {
                assert_eq!(
                    grid.tiles.get(7 + dx, 7 + dy).block,
                    name("air-factory"),
                    "air-factory footprint"
                );
                assert_eq!(
                    grid.tiles.get(10 + dx, 2 + dy).block,
                    name("ground-factory"),
                    "ground-factory footprint"
                );
            }
        }
    }
}
