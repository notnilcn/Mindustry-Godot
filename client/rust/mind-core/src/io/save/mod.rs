// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save engine entry point (plan 04 §3.2/§3.3).
//!
//! Ported from `core/src/mindustry/io/SaveIO.java`: magic + format version +
//! named length-prefixed regions, write-to-temp with `<name>-backup.msav`
//! rotation and restore-on-error, backup fallback on load/meta reads, meta-only
//! reads for listing, and the exact unknown-version error text.
//!
//! Native framing (plan 04 §6.1, deviations 1–2): the 4-byte magic `MGRS` is
//! raw; everything after it (`u32` format version + regions) is zlib-deflated.
//! The legacy upstream magic `MSAV` is import-only behind the default-off
//! `msav-import` feature (OD2/NUD-02); the Godot client opts in so campaign
//! preset maps load. Without the feature, legacy files fail with the standard
//! unknown-version message.

pub mod chunk;
pub mod fixture;
pub mod meta;
pub mod options;
pub mod slot;
pub mod state;
pub mod version;
pub mod versions;

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

pub use chunk::{SaveReader, SaveWriter};
pub use meta::SaveMeta;
pub use options::SaveOptions;
pub use state::{SaveReadState, WorldContext};
pub use version::WriteContext;

use super::IoError;
use super::fs::{FileSystem, Paths, SAVE_EXTENSION, tmp_path_for};
use chunk::{InflateReader, MAX_DECOMPRESSED_BYTES};

/// Native save format magic (deviation 1: `MGRS`, not upstream `MSAV`).
pub const MAGIC: [u8; 4] = *b"MGRS";

/// Legacy upstream save magic (import-only boundary, OD2).
pub const LEGACY_MAGIC: [u8; 4] = *b"MSAV";

/// Save engine namespace (`SaveIO` statics).
pub struct SaveIo;

impl SaveIo {
    /// Writes one save to a byte sink: magic, then deflated version + regions
    /// (`SaveIO.write(OutputStream, SaveOptions)`).
    pub fn write<W: Write>(
        mut out: W,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> Result<(), IoError> {
        out.write_all(&MAGIC)?;
        let writer = version::current_writer();
        let mut encoder = ZlibEncoder::new(out, Compression::fast());
        encoder.write_all(&writer.version().to_be_bytes())?;
        {
            let mut save_writer = SaveWriter {
                out: &mut encoder,
                scratch: chunk::SaveScratch::new(),
                ctx,
            };
            writer.write(&mut save_writer, options)?;
        }
        encoder.finish()?;
        Ok(())
    }

    /// Serializes a save into a `Vec` (tests, buffers).
    pub fn write_to_vec(ctx: &WriteContext, options: &SaveOptions) -> Result<Vec<u8>, IoError> {
        let mut buf = Vec::new();
        Self::write(&mut buf, ctx, options)?;
        Ok(buf)
    }

    /// Atomic save with backup rotation (`SaveIO.save`):
    ///
    /// 1. an existing file is renamed to `<name>-backup.msav` first;
    /// 2. the new save is written to `<name>.msav.tmp`, flushed + synced, then
    ///    renamed over the target;
    /// 3. on any error the temp file is deleted and the backup restored.
    pub fn save(
        fs: &dyn FileSystem,
        file: &Path,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> Result<(), IoError> {
        if let Some(parent) = file.parent() {
            fs.mkdirs(parent)?;
        }
        let backup = Self::backup_file_for(file);
        let existed = fs.exists(file);
        if existed {
            fs.rename(file, &backup)?;
        }
        let tmp = tmp_path_for(file);
        let result = Self::write_to_vec(ctx, options)
            .and_then(|bytes| fs.write_sync(&tmp, &bytes))
            .and_then(|()| fs.rename(&tmp, file));
        if let Err(error) = result {
            let _ = fs.delete(&tmp);
            if existed && fs.exists(&backup) {
                let _ = fs.rename(&backup, file);
            }
            return Err(error);
        }
        Ok(())
    }

    /// Loads a save, falling back to `<name>-backup.msav` on any primary error
    /// (`SaveIO.load(Fi, WorldContext)` + `SaveException` semantics).
    pub fn load(
        fs: &dyn FileSystem,
        file: &Path,
        state: &mut SaveReadState,
    ) -> Result<(), IoError> {
        let primary = fs
            .read(file)
            .and_then(|bytes| Self::load_bytes(&bytes, state));
        match primary {
            Ok(()) => Ok(()),
            Err(error) => {
                log::error!("failed to load save `{}`: {error}", file.display());
                let backup = Self::backup_file_for(file);
                if fs.exists(&backup) {
                    log::warn!("falling back to backup `{}`", backup.display());
                    let bytes = fs.read(&backup)?;
                    Self::load_bytes(&bytes, state)
                } else {
                    Err(error)
                }
            }
        }
    }

    /// Loads from an in-memory (deflated, magic-prefixed) save.
    pub fn load_bytes(bytes: &[u8], state: &mut SaveReadState) -> Result<(), IoError> {
        // Legacy upstream `MSAV` import (opt-in `msav-import`): the whole
        // stream is deflated and the header is raw, so it is handled before
        // the native `MGRS` path.
        #[cfg(feature = "msav-import")]
        if super::legacy::sniffs_as_legacy(bytes) {
            return super::legacy::load_legacy(bytes, state);
        }
        let (format, mut reader) = Self::open_native(bytes)?;
        let writer = version::get_writer(format).ok_or(IoError::UnknownVersion(format))?;
        let result = writer.read(&mut reader, state);
        // Upstream `finally { content.setTemporaryMapper(null) }` (SaveIO.load):
        // the temporary mapper never escapes a load attempt.
        if let Some(content) = state.content.as_deref_mut() {
            content.set_temporary_mapper(None);
        }
        result
    }

    /// Meta-only read with backup fallback (`SaveIO.getMeta(Fi)`).
    pub fn get_meta(fs: &dyn FileSystem, file: &Path) -> Result<SaveMeta, IoError> {
        let primary = fs.read(file).and_then(|bytes| Self::get_meta_bytes(&bytes));
        match primary {
            Ok(meta) => Ok(meta),
            Err(error) => {
                log::error!("failed to read meta of `{}`: {error}", file.display());
                let backup = Self::backup_file_for(file);
                let bytes = fs.read(&backup)?;
                Self::get_meta_bytes(&bytes)
            }
        }
    }

    /// Meta-only read from memory (`SaveIO.getMeta(DataInputStream)`).
    pub fn get_meta_bytes(bytes: &[u8]) -> Result<SaveMeta, IoError> {
        // Legacy upstream `MSAV` import (opt-in `msav-import`): the whole
        // stream is deflated and the header is raw, so it is handled before
        // the native `MGRS` path (mirrors `load_bytes`).
        #[cfg(feature = "msav-import")]
        if super::legacy::sniffs_as_legacy(bytes) {
            return super::legacy::load_legacy_meta(bytes);
        }
        let (format, mut reader) = Self::open_native(bytes)?;
        let writer = version::get_writer(format).ok_or(IoError::UnknownVersion(format))?;
        writer.get_meta(&mut reader)
    }

    /// Whether the file (or its backup) parses far enough to read meta
    /// (`SaveIO.isSaveValid`).
    pub fn is_save_valid(fs: &dyn FileSystem, file: &Path) -> bool {
        let primary_ok = fs
            .read(file)
            .and_then(|bytes| Self::get_meta_bytes(&bytes))
            .is_ok();
        primary_ok || {
            let backup = Self::backup_file_for(file);
            fs.read(&backup)
                .and_then(|bytes| Self::get_meta_bytes(&bytes))
                .is_ok()
        }
    }

    /// The slot file path (`SaveIO.fileFor`): `saves/<n>.msav`.
    pub fn file_for(paths: &Paths, slot: u32) -> PathBuf {
        paths.save_slot(slot)
    }

    /// The backup sibling (`SaveIO.backupFileFor`): `<stem>-backup.<ext>`.
    pub fn backup_file_for(file: &Path) -> PathBuf {
        let stem = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = file
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned())
            .unwrap_or_else(|| SAVE_EXTENSION.to_owned());
        file.with_file_name(format!("{stem}-backup.{extension}"))
    }

    /// Opens a native (or legacy-sniffed) container: validates the magic,
    /// wraps the rest in an inflating reader, and reads the format version.
    ///
    /// Sniffing (plan 04 §2.3.1): raw `MGRS` → native. Anything else may be a
    /// legacy upstream file, which deflates the *whole* stream including its
    /// `MSAV` header — so the legacy sniff inflates first. Legacy input fails
    /// with the standard unknown-version message unless the `msav-import`
    /// feature is enabled (OD2 default).
    pub(crate) fn open_native(bytes: &[u8]) -> Result<(u32, SaveReader<'_>), IoError> {
        let magic: [u8; 4] = bytes
            .get(..4)
            .ok_or(IoError::UnexpectedEof)?
            .try_into()
            .map_err(|_| IoError::UnexpectedEof)?;
        if magic == MAGIC {
            // Native: magic is raw, the rest is deflated.
            let decoder = ZlibDecoder::new(Cursor::new(&bytes[4..]));
            let boxed: Box<dyn Read + '_> = Box::new(decoder);
            let mut input = InflateReader::new(boxed, MAX_DECOMPRESSED_BYTES);
            let format = input.u32()?;
            return Ok((format, SaveReader::new(input)));
        }
        if sniffs_as_legacy(bytes) {
            return super::legacy::open_legacy(bytes);
        }
        Err(IoError::Header {
            expected: MAGIC,
            actual: magic,
        })
    }
}

/// Whether the stream inflates to the legacy upstream `MSAV` header.
fn sniffs_as_legacy(bytes: &[u8]) -> bool {
    super::legacy::sniffs_as_legacy(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::fs::MockFs;
    use crate::io::save::versions::v1::base_meta_tags;

    fn test_ctx() -> WriteContext<'static> {
        WriteContext::meta_only(base_meta_tags(8, 8, 2, "test-map"))
    }

    #[test]
    fn save_writes_valid_file_and_meta_reads_back() {
        // Ported from `ApplicationTests.save`: the written file validates and
        // its meta is readable.
        let fs = MockFs::new();
        let file = PathBuf::from("/data/saves/0.msav");
        SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new()).unwrap();

        assert!(SaveIo::is_save_valid(&fs, &file));
        let meta = SaveIo::get_meta(&fs, &file).unwrap();
        assert_eq!(meta.map_name, "test-map");
        assert_eq!(meta.wave, 2);
        assert_eq!(meta.width(), 8);
        // First save: no backup exists yet.
        assert!(!fs.exists(&SaveIo::backup_file_for(&file)));
    }

    #[test]
    fn second_save_rotates_backup() {
        let fs = MockFs::new();
        let file = PathBuf::from("/data/saves/1.msav");
        SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new()).unwrap();
        let mut tags = base_meta_tags(8, 8, 9, "test-map-2");
        tags.insert("wave".to_owned(), "9".to_owned());
        let ctx = WriteContext::meta_only(tags);
        SaveIo::save(&fs, &file, &ctx, &SaveOptions::new()).unwrap();

        let backup = SaveIo::backup_file_for(&file);
        assert!(fs.exists(&backup));
        // Primary holds the new meta, backup the old.
        assert_eq!(SaveIo::get_meta(&fs, &file).unwrap().wave, 9);
        assert_eq!(
            SaveIo::get_meta_bytes(&fs.read(&backup).unwrap())
                .unwrap()
                .wave,
            2
        );
    }

    #[test]
    fn corrupt_primary_falls_back_to_backup_on_load_and_meta() {
        let fs = MockFs::new();
        let file = PathBuf::from("/data/saves/2.msav");
        SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new()).unwrap();
        let tags = base_meta_tags(8, 8, 4, "test-map-2");
        let ctx = WriteContext::meta_only(tags);
        SaveIo::save(&fs, &file, &ctx, &SaveOptions::new()).unwrap();

        // Corrupt the primary; backup (wave 2) must serve.
        fs.write(&file, b"garbage-not-a-save").unwrap();
        assert!(SaveIo::is_save_valid(&fs, &file));
        let meta = SaveIo::get_meta(&fs, &file).unwrap();
        assert_eq!(meta.wave, 2);

        let mut state = SaveReadState::default();
        SaveIo::load(&fs, &file, &mut state).unwrap();
        assert_eq!(state.tags.get("wave").unwrap(), "2");
    }

    #[test]
    fn failed_write_restores_backup() {
        let fs = MockFs::new();
        let file = PathBuf::from("/data/saves/3.msav");
        SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new()).unwrap();
        assert!(SaveIo::is_save_valid(&fs, &file));

        // Make the filesystem fail the second save; the original file must be
        // restored intact from the backup.
        fs.set_read_only(true);
        let result = SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new());
        assert!(result.is_err());
        fs.set_read_only(false);
        assert!(SaveIo::is_save_valid(&fs, &file));
        let meta = SaveIo::get_meta(&fs, &file).unwrap();
        assert_eq!(meta.map_name, "test-map");
    }

    #[test]
    fn unknown_version_message_matches_upstream() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        let mut encoder = ZlibEncoder::new(&mut bytes, Compression::fast());
        encoder.write_all(&99u32.to_be_bytes()).unwrap();
        encoder.finish().unwrap();

        let mut state = SaveReadState::default();
        let error = SaveIo::load_bytes(&bytes, &mut state).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Unknown save version: 99. Are you trying to load a save from a newer version?"
        );
    }

    #[cfg(not(feature = "msav-import"))]
    #[test]
    fn legacy_msav_fails_with_unknown_version_message() {
        // Hand-build a legacy stream: fully deflated `MSAV` + u32 version 13.
        let mut bytes = Vec::new();
        let mut encoder = ZlibEncoder::new(&mut bytes, Compression::fast());
        encoder.write_all(b"MSAV").unwrap();
        encoder.write_all(&13u32.to_be_bytes()).unwrap();
        encoder.finish().unwrap();

        let mut state = SaveReadState::default();
        let error = SaveIo::load_bytes(&bytes, &mut state).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Unknown save version: 13. Are you trying to load a save from a newer version?"
        );
    }

    #[cfg(feature = "msav-import")]
    #[test]
    fn legacy_msav_header_is_accepted_then_truncation_errors() {
        // Hand-build a legacy stream: fully deflated `MSAV` + u32 version 13.
        let mut bytes = Vec::new();
        let mut encoder = ZlibEncoder::new(&mut bytes, Compression::fast());
        encoder.write_all(b"MSAV").unwrap();
        encoder.write_all(&13u32.to_be_bytes()).unwrap();
        encoder.finish().unwrap();

        let mut state = SaveReadState::default();
        let error = SaveIo::load_bytes(&bytes, &mut state).unwrap_err();
        assert!(
            !error.to_string().contains("Unknown save version"),
            "MSAV header is imported, not rejected: {error}"
        );
    }

    #[test]
    fn incorrect_header_message() {
        let bytes = b"XXXXmore-bytes";
        let error = SaveIo::get_meta_bytes(bytes).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Incorrect header! Expecting: [77, 71, 82, 83]; Actual: [88, 88, 88, 88]"
        );
    }

    #[test]
    fn backup_file_for_naming() {
        assert_eq!(
            SaveIo::backup_file_for(Path::new("/data/saves/0.msav")),
            PathBuf::from("/data/saves/0-backup.msav")
        );
        assert_eq!(
            SaveIo::backup_file_for(Path::new("/data/saves/sector-serpulo-12.msav")),
            PathBuf::from("/data/saves/sector-serpulo-12-backup.msav")
        );
    }

    #[test]
    fn truncated_stream_is_an_error_not_a_panic() {
        let full = SaveIo::write_to_vec(&test_ctx(), &SaveOptions::new()).unwrap();
        // Header and mid-stream truncations must error (and never panic).
        for cut in [4, 8, full.len() / 2] {
            let mut state = SaveReadState::default();
            assert!(
                SaveIo::load_bytes(&full[..cut], &mut state).is_err(),
                "truncation at {cut} must fail"
            );
        }
        // A truncation inside the final zlib trailer (adler32) after the last
        // region was fully read is tolerated: every region payload was already
        // length-checked by then.
        let mut state = SaveReadState::default();
        let _ = SaveIo::load_bytes(&full[..full.len() - 1], &mut state);
    }

    #[test]
    fn save_writes_native_fs_fixture() {
        // M0 headless fixture: `mind-headless io dump-meta` reads this file.
        // Also exercises NativeFs end-to-end (tmp + sync + rename + backup).
        let dir = std::env::temp_dir().join("mind-io-fixtures");
        let fs = crate::io::fs::NativeFs;
        let file = dir.join("m0_empty.msav");
        SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new()).unwrap();
        // Second save rotates a backup, mirroring the MCP scenario layout.
        SaveIo::save(&fs, &file, &test_ctx(), &SaveOptions::new()).unwrap();
        assert!(SaveIo::is_save_valid(&fs, &file));
        assert!(fs.exists(&SaveIo::backup_file_for(&file)));
        let meta = SaveIo::get_meta(&fs, &file).unwrap();
        assert_eq!(meta.map_name, "test-map");
    }

    #[test]
    fn corrupted_midstream_is_an_error() {
        let full = SaveIo::write_to_vec(&test_ctx(), &SaveOptions::new()).unwrap();
        let mut bad = full.clone();
        // Flip a byte inside the deflate payload (past magic + zlib header).
        let at = 10;
        bad[at] ^= 0xFF;
        let mut state = SaveReadState::default();
        assert!(SaveIo::load_bytes(&bad, &mut state).is_err());
    }

    /// Imports the vendored designed Ground Zero map (`assets/maps/serpulo/`)
    /// through the MSAV reader: 128x128 and mostly open terrain, unlike the
    /// planet-generator fallback (EV-0014). Skips when the asset is absent.
    #[cfg(feature = "msav-import")]
    #[test]
    fn legacy_ground_zero_map_imports_designed_terrain() {
        use crate::content::{BlockId, MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::editor::context::EditorContext;
        use crate::world::WorldGrid;

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/maps/serpulo/groundZero.msav");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let result = {
            let mut context = EditorContext::new(&mut grid, &registry);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut registry),
                ..SaveReadState::default()
            };
            SaveIo::load_bytes(&bytes, &mut state)
        };
        result.expect("ground zero map imports");
        grid.end_map_load(&registry);
        // The committed 256x256 designed map: 56095 non-air block tiles (the
        // Java-launched save of the same map parses to 56218 after play).
        assert_eq!((grid.tiles.width, grid.tiles.height), (256, 256));
        let walls = grid
            .tiles
            .iter()
            .filter(|tile| tile.block != BlockId::AIR)
            .count();
        assert_eq!(walls, 56078, "designed Ground Zero composition");
    }

    /// EV-0038: every built-in default map is a legacy MSAV v4/v5/v7 file.
    /// `MapIo::create_map` (meta-only list reads) and the full editor load
    /// (`MapEditor::begin_edit_map`) must route them through the `MSAV`
    /// importer. Skips when the assets are absent.
    #[cfg(feature = "msav-import")]
    #[test]
    fn legacy_default_maps_read_meta_and_load() {
        use crate::content::{BlockId, MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::editor::context::EditorContext;
        use crate::io::map::MapIo;
        use crate::world::WorldGrid;

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/default");
        let mut checked = 0usize;
        for name in crate::maps::DEFAULT_MAP_NAMES {
            let path = dir.join(format!("{name}.msav"));
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let header = MapIo::create_map(&crate::io::fs::NativeFs, &path, true)
                .unwrap_or_else(|e| panic!("create_map {name}: {e}"));
            assert!(!header.name.trim().is_empty(), "meta name for {name}");
            assert!(header.width > 0 && header.height > 0, "{name} dimensions");

            let mut registry =
                create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
            let mut grid = WorldGrid::new(0, 0);
            grid.begin_map_load();
            let result = {
                let mut context = EditorContext::new(&mut grid, &registry);
                let mut state = SaveReadState {
                    context: Some(&mut context),
                    content: Some(&mut registry),
                    ..SaveReadState::default()
                };
                SaveIo::load_bytes(&bytes, &mut state)
            };
            result.unwrap_or_else(|e| panic!("load {name}: {e}"));
            grid.end_map_load(&registry);
            assert_eq!(
                (grid.tiles.width, grid.tiles.height),
                (header.width, header.height),
                "{name} grid size"
            );
            assert!(
                grid.tiles.iter().any(|tile| tile.block != BlockId::AIR),
                "{name} has terrain"
            );
            checked += 1;
        }
        // Either the assets are checked out (all 18) or the test is a no-op.
        assert!(
            checked == 0 || checked == crate::maps::DEFAULT_MAP_NAMES.len(),
            "checked {checked} default maps"
        );
    }
}
