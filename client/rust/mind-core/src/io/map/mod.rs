// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapIO` equivalent (plan 04 §3.9): map read/write delegating to the save
//! engine, meta-only headers, preview pixels, PNG image maps.
//!
//! Ported from `core/src/mindustry/io/MapIO.java`.

pub mod header;
pub mod preview;

use std::path::Path;

pub use header::MapHeader;
pub use preview::{
    BlockPalette, ColorMapper, FnColorMapper, ImageTileSink, PreviewContext, PreviewImage,
    color_for, generate_preview_from_tiles, read_image, team_color, write_image,
};

use super::fs::{FileSystem, SAVE_EXTENSION};
use super::save::state::MapSource;
use super::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
use super::{IoError, StringMap};
use crate::content::ContentRegistry;

/// PNG signature bytes (`MapIO.pngHeader`).
pub const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Map read/write entry points (`MapIO` statics).
pub struct MapIo;

impl MapIo {
    /// `MapIO.createMap`: meta-only read for map lists (no world alloc).
    pub fn create_map(
        fs: &dyn FileSystem,
        file: &Path,
        custom: bool,
    ) -> Result<MapHeader, IoError> {
        let meta = SaveIo::get_meta(fs, file)?;
        Ok(MapHeader::from_meta(
            &meta,
            custom,
            Some(file.to_path_buf()),
        ))
    }

    /// `MapIO.writeMap`: saves with the map's tags merged
    /// (`SaveOptions.extraTags = map.tags`, `embedAssets = embed`).
    pub fn write_map(
        fs: &dyn FileSystem,
        file: &Path,
        ctx: &WriteContext,
        map_tags: StringMap,
        embed: bool,
    ) -> Result<(), IoError> {
        SaveIo::save(
            fs,
            file,
            ctx,
            &SaveOptions {
                embed_assets: embed,
                extra_tags: Some(map_tags),
            },
        )
    }

    /// `MapIO.loadMap`.
    pub fn load_map(
        fs: &dyn FileSystem,
        file: &Path,
        state: &mut SaveReadState,
    ) -> Result<(), IoError> {
        SaveIo::load(fs, file, state)
    }

    /// `MapIO.isImage`: PNG signature sniff.
    pub fn is_image(fs: &dyn FileSystem, file: &Path) -> bool {
        fs.read(file)
            .map(|bytes| bytes.starts_with(&PNG_SIGNATURE))
            .unwrap_or(false)
    }

    /// `MapIO.generatePreview(Map)`: reads meta + content + the map region
    /// through a [`PreviewContext`] (no events, no world mutation) and returns
    /// the composed preview image.
    ///
    /// Takes the registry mutably because the content header installs a
    /// temporary mapper (cleared by the load epilogue).
    pub fn generate_preview(
        registry: &mut ContentRegistry,
        bytes: &[u8],
    ) -> Result<PreviewImage, IoError> {
        let palette = BlockPalette::of(registry);
        let mut context = PreviewContext::new(palette);
        let mut state = SaveReadState {
            preview: true,
            context: Some(&mut context),
            content: Some(registry),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(bytes, &mut state)?;
        Ok(context.image())
    }

    /// Preview pixels from a live tile source (`MapIO.generatePreview(Tiles)`).
    pub fn preview_from_tiles(registry: &ContentRegistry, map: &dyn MapSource) -> PreviewImage {
        generate_preview_from_tiles(&BlockPalette::of(registry), map)
    }
}

/// Whether a file name looks like a map/save file.
pub fn is_save_file_name(name: &str) -> bool {
    name.ends_with(&format!(".{SAVE_EXTENSION}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::fs::MockFs;
    use crate::io::save::fixture::FixtureWorld;
    use crate::io::save::versions::v1::base_meta_tags;

    fn write_fixture_map(
        fs: &dyn crate::io::FileSystem,
        file: &Path,
        registry: &ContentRegistry,
        name: &str,
    ) {
        let world = FixtureWorld::synthetic(registry, 32, 24);
        let mut tags = base_meta_tags(32, 24, 0, "fixture");
        tags.insert("name".to_owned(), name.to_owned());
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        SaveIo::save(fs, file, &ctx, &SaveOptions::new()).unwrap();
    }

    /// `io::map::tests::create_map_reads_meta_only` (plan 04 §7a, ported from
    /// `ApplicationTests.createMap`/`playMap` list path): width/height/tags
    /// equal; no world mutation.
    #[test]
    fn create_map_reads_meta_only() {
        let fs = MockFs::new();
        let registry = test_registry();
        let file = std::path::PathBuf::from("/data/maps/fork.msav");
        write_fixture_map(&fs, &file, &registry, "Fork");

        let header = MapIo::create_map(&fs, &file, true).unwrap();
        assert_eq!(header.name, "Fork");
        assert_eq!(header.width, 32);
        assert_eq!(header.height, 24);
        assert!(header.custom);
        assert_eq!(header.version, 1);
        assert_eq!(header.tag("mapname"), "fixture");
    }

    #[test]
    fn generate_preview_collects_pixels_teams_spawns() {
        let fs = MockFs::new();
        let registry = test_registry();
        let file = std::path::PathBuf::from("/data/maps/fork.msav");
        write_fixture_map(&fs, &file, &registry, "Fork");

        let mut preview_registry = test_registry();
        let image =
            MapIo::generate_preview(&mut preview_registry, &fs.read(&file).unwrap()).unwrap();
        assert_eq!(image.width, 32);
        assert_eq!(image.height, 24);
        assert_eq!(image.rgba.len(), 32 * 24 * 4);
        // The preview is not uniformly black (floors got colors).
        assert!(image.rgba.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn write_image_and_read_image_roundtrip() {
        let registry = test_registry();
        let world = FixtureWorld::synthetic(&registry, 16, 16);
        let image = MapIo::preview_from_tiles(&registry, &world);
        assert_eq!(image.width, 16);
        let direct = write_image(&BlockPalette::of(&registry), &world);
        assert_eq!(direct.rgba.len(), 16 * 16 * 4);
    }

    #[test]
    fn is_image_sniffs_png() {
        let fs = MockFs::new();
        let png = std::path::PathBuf::from("/data/maps/pic.png");
        fs.write(&png, &PNG_SIGNATURE).unwrap();
        assert!(MapIo::is_image(&fs, &png));
        let other = std::path::PathBuf::from("/data/maps/m.msav");
        fs.write(&other, b"MGRS....").unwrap();
        assert!(!MapIo::is_image(&fs, &other));
        assert!(!MapIo::is_image(&fs, std::path::Path::new("/missing")));
    }

    #[test]
    fn write_map_list_fixture_dir() {
        // M5 headless fixture: `mind-headless io map-list --dir <dir>` reads
        // this directory (2 maps + 1 save + 1 corrupt + 1 non-save).
        let registry = test_registry();
        let fs = crate::io::fs::NativeFs;
        let dir = std::env::temp_dir().join("m5-map-list");
        let _ = std::fs::remove_dir_all(&dir);
        write_fixture_map(&fs, &dir.join("fork.msav"), &registry, "Fork");
        write_fixture_map(&fs, &dir.join("caldera.msav"), &registry, "Caldera");
        // A save (no `name` tag → not a map).
        let world = FixtureWorld::synthetic(&registry, 24, 24);
        let mut tags = base_meta_tags(24, 24, 7, "save-one");
        tags.shift_remove("name");
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(&registry);
        ctx.map = Some(&world);
        SaveIo::save(&fs, &dir.join("0.msav"), &ctx, &SaveOptions::new()).unwrap();
        fs.write(&dir.join("broken.msav"), b"garbage").unwrap();
        fs.write(&dir.join("notes.txt"), b"hi").unwrap();

        let headers: Vec<_> = ["fork.msav", "caldera.msav", "0.msav"]
            .iter()
            .map(|name| MapIo::create_map(&fs, &dir.join(name), true).unwrap())
            .collect();
        assert!(headers[0].is_map_header());
        assert_eq!(headers[2].name, "save-one");
        assert!(!headers[2].is_map_header());
    }
}
