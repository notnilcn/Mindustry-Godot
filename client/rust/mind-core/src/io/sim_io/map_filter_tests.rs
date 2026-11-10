// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map-filter application tests for loaded saves.
//! Upstream: none (port-only; io handler tests)

use super::*;
use crate::content::BlockKind;
use crate::content::test_support::test_registry;
use crate::io::fs::{FileSystem, MockFs};
use crate::io::save::fixture::FixtureWorld;
use crate::io::save::versions::v1::base_meta_tags;
use crate::io::save::{SaveIo, SaveOptions, WriteContext};
use crate::world::WorldGrid;

/// Explicit `Genfilters` ore stack: a low threshold over a smooth noise
/// field guarantees overlay tiles on a small fixture grid.
const ORE_TAG: &str = r#"[{"class":"ore","ore":"ore-copper","scl":8.0,"threshold":0.35}]"#;

/// `groundZero`-style scatter stack: `class:scatter` on the stone floor with
/// an explicit prop (`{"class":scatter,"chance":0.015,"flooronto":shale,
/// "block":shale-boulder}` in the shipped `.msav`; an entry without `block`
/// defaults to air and places nothing, as in `ScatterFilter.java:11`).
const SCATTER_TAG: &str =
    r#"[{"class":"scatter","chance":0.5,"flooronto":"stone","block":"boulder"}]"#;

/// Writes a 64x64 flat-stone map fixture carrying `extra` meta tags and
/// returns the encoded native container bytes.
fn map_bytes(
    registry: &crate::content::ContentRegistry,
    name: &str,
    extra: &[(&str, &str)],
) -> Vec<u8> {
    map_bytes_with_authored_block(registry, name, extra, None)
}

/// [`map_bytes`] with one authored block at `(4, 4)` so the filter pass can
/// be checked against authored tile data (`preset .msav` tiles survive).
fn map_bytes_with_authored_block(
    registry: &crate::content::ContentRegistry,
    name: &str,
    extra: &[(&str, &str)],
    authored: Option<crate::content::BlockId>,
) -> Vec<u8> {
    let fs = MockFs::new();
    let file = std::path::PathBuf::from(format!("/maps/{name}.msav"));
    let mut world = FixtureWorld::new(registry, 64, 64);
    let stone = registry.block_id("stone").expect("stone floor");
    for floor in world.floors.iter_mut() {
        *floor = stone;
    }
    if let Some(block) = authored {
        world.blocks[4 * 64 + 4] = block;
    }
    let mut tags = base_meta_tags(64, 64, 0, name);
    tags.insert("name".to_owned(), name.to_owned());
    for (key, value) in extra {
        tags.insert((*key).to_owned(), (*value).to_owned());
    }
    let mut ctx = WriteContext::meta_only(tags);
    ctx.content = Some(registry);
    ctx.map = Some(&world);
    ctx.entities = Some(&world);
    SaveIo::save(&fs, &file, &ctx, &SaveOptions::new()).expect("write fixture");
    fs.read(&file).expect("fixture bytes")
}

fn boulder_tiles(grid: &WorldGrid, registry: &crate::content::ContentRegistry) -> Vec<(i16, i16)> {
    let mut out = Vec::new();
    for (pos, index) in grid.iter_row_major() {
        let block = grid.tile_ref(index).block;
        if registry
            .block(block)
            .is_some_and(|def| def.kind == BlockKind::StaticProp && def.name.contains("boulder"))
        {
            out.push((pos.x(), pos.y()));
        }
    }
    out
}

fn ore_tiles(grid: &WorldGrid, registry: &crate::content::ContentRegistry) -> Vec<(i16, i16)> {
    let mut out = Vec::new();
    for (pos, index) in grid.iter_row_major() {
        let overlay = grid.tile_ref(index).overlay;
        if registry
            .block(overlay)
            .is_some_and(|def| def.kind == BlockKind::OreBlock)
        {
            out.push((pos.x(), pos.y()));
        }
    }
    out
}

fn tiles_equal(a: &WorldGrid, b: &WorldGrid) -> bool {
    a.tiles.array().len() == b.tiles.array().len()
        && a.tiles
            .array()
            .iter()
            .zip(b.tiles.array())
            .all(|(l, r)| l.floor == r.floor && l.overlay == r.overlay && l.block == r.block)
}

/// (a) A map with a `genfilters` ore filter gains overlay ore tiles on a
/// filtered map load; the recorded seed reproduces the placement and a new
/// seed re-rolls it (`GenerateFilter.randomize`, `GenerateFilter.java:91`).
#[test]
fn map_load_applies_genfilters_ore() {
    let mut registry = test_registry();
    let bytes = map_bytes(&registry, "OreMap", &[("genfilters", ORE_TAG)]);
    let plain = decode_save_bytes(&bytes, &mut registry).expect("plain decode");
    assert!(
        ore_tiles(&plain.grid, &registry).is_empty(),
        "raw decode has no generated ore"
    );

    let filtered = decode_map_bytes(&bytes, &mut registry, 7).expect("filtered decode");
    let filtered_ore = ore_tiles(&filtered.grid, &registry);
    assert!(
        !filtered_ore.is_empty(),
        "the genfilters ore stack adds overlay tiles"
    );
    assert_eq!(filtered.map_gen_seed, Some(7));

    let again = decode_map_bytes(&bytes, &mut registry, 7).expect("filtered decode");
    assert_eq!(
        ore_tiles(&again.grid, &registry),
        filtered_ore,
        "the same seed reproduces the placement"
    );

    let rerolled = decode_map_bytes(&bytes, &mut registry, 8).expect("filtered decode");
    assert_ne!(
        ore_tiles(&rerolled.grid, &registry),
        filtered_ore,
        "a fresh seed re-rolls the placement"
    );
}

/// (c) A save load never re-runs filters even when the container carries a
/// `genfilters` tag (`SaveIO.load(file)` uses the plain world context).
#[test]
fn save_load_is_untouched() {
    let mut registry = test_registry();
    let bytes = map_bytes(&registry, "OreSave", &[("genfilters", ORE_TAG)]);
    let save = decode_save_bytes(&bytes, &mut registry).expect("save decode");
    assert_eq!(save.map_gen_seed, None);
    assert!(
        ore_tiles(&save.grid, &registry).is_empty(),
        "a save decode never applies generation filters"
    );
}

/// (b) A `build < 83` map without the tag resolves an empty stack
/// (`Map.filters`, `Map.java:120-125`) and decodes untouched.
#[test]
fn build_lt_83_map_without_tag_is_untouched() {
    let mut registry = test_registry();
    let bytes = map_bytes(&registry, "Legacy", &[("build", "82")]);
    let plain = decode_save_bytes(&bytes, &mut registry).expect("plain decode");
    let filtered = decode_map_bytes(&bytes, &mut registry, 5).expect("filtered decode");
    assert!(ore_tiles(&filtered.grid, &registry).is_empty());
    assert!(tiles_equal(&plain.grid, &filtered.grid));
}

/// (d) A preset sector map (`SectorPreset.generator` ->
/// `FileMapGenerator.generate` -> `SaveIO.load(map.file, FilterContext)`,
/// `FileMapGenerator.java:71-82`) lands its `genfilters` scatter stack on a
/// map load, exactly like a custom-game map (`groundZero` carries five
/// `class:scatter` entries). Authored tile data survives the pass, and the
/// recorded seed reproduces the placement.
#[test]
fn preset_map_load_applies_genfilter_scatter() {
    let mut registry = test_registry();
    let boulder = registry.block_id("boulder").expect("boulder prop");
    let bytes = map_bytes_with_authored_block(
        &registry,
        "Preset",
        &[("genfilters", SCATTER_TAG)],
        Some(boulder),
    );
    // The plain (save/resume) reader keeps only the authored prop: the
    // sector-preset path must not use it.
    let plain = decode_save_bytes(&bytes, &mut registry).expect("plain decode");
    assert_eq!(
        boulder_tiles(&plain.grid, &registry),
        vec![(4, 4)],
        "a save decode keeps authored tiles and runs no scatter pass"
    );

    let filtered = decode_map_bytes(&bytes, &mut registry, 11).expect("filtered decode");
    let placed = boulder_tiles(&filtered.grid, &registry);
    assert!(
        placed.len() > 100,
        "the scatter stack lands boulders on the stone floor ({} placed)",
        placed.len()
    );
    assert!(
        placed.contains(&(4, 4)),
        "the authored prop survives the filter pass"
    );
    assert_eq!(filtered.map_gen_seed, Some(11));

    let again = decode_map_bytes(&bytes, &mut registry, 11).expect("filtered decode");
    assert_eq!(
        boulder_tiles(&again.grid, &registry),
        placed,
        "the recorded seed reproduces the scatter placement"
    );
    let rerolled = decode_map_bytes(&bytes, &mut registry, 12).expect("filtered decode");
    assert_ne!(
        boulder_tiles(&rerolled.grid, &registry),
        placed,
        "a fresh seed re-rolls the scatter placement"
    );
}

/// The decoded seed survives into the state dump (`map_gen_seed`) and is
/// absent for sims that never ran a map-load filter step.
#[test]
fn dump_records_map_gen_seed() {
    let mut sim = Sim::new(
        1,
        8,
        8,
        crate::content::BlockId::AIR,
        crate::content::BlockId::AIR,
    );
    let dump = serde_json::to_value(sim.dump()).expect("dump");
    assert!(dump.get("map_gen_seed").is_none());
    sim.set_map_gen_seed(42);
    let dump = serde_json::to_value(sim.dump()).expect("dump");
    assert_eq!(dump["map_gen_seed"], serde_json::json!(42));
}
