// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Native save format version 2 (plan G04 T1).
//!
//! v2 changes only the tile-entity chunk layout of the `map` region: a chunk
//! is `[per-kind revision][writeBase with the embedded base version byte]
//! [per-kind body]`, byte-identical to Java's `SaveVersion.writeMap`/`readMap`
//! (`SaveVersion.java:257-269, 364-379`) and `BuildingComp.writeAll/readAll`
//! (`BuildingComp.java:284-292`). The v1 reader kept the base version byte in
//! the chunk header and dropped the body.
//!
//! Read-side, the chunk layout matches the legacy `MSAV` layout
//! (`read_base_legacy` embeds the base version after the team byte), so the
//! reader selects `set_legacy_entities(true)` on the world context and reuses
//! [`super::v1::read_regions`].

use super::super::chunk::{SaveReader, SaveWriter};
use super::super::meta::SaveMeta;
use super::super::options::SaveOptions;
use super::super::state::SaveReadState;
use super::super::version::SaveVersion;
use super::v1::{read_regions, write_regions};
use crate::io::IoError;

/// Native save format version 2 (Java tile-entity chunk layout).
pub struct SaveV2;

impl SaveVersion for SaveV2 {
    fn version(&self) -> u32 {
        2
    }

    fn write(&self, w: &mut SaveWriter, options: &SaveOptions) -> Result<(), IoError> {
        // Identical writer; the chunk layout difference is produced by the
        // `MapSource` (`EcsMapSource::write_building` writes the Java chunk).
        write_regions(w, options)
    }

    fn read(&self, r: &mut SaveReader, state: &mut SaveReadState) -> Result<(), IoError> {
        // v2 tile-entity chunks equal the legacy `MSAV` layout: the per-kind
        // revision byte is the chunk header (`readMap`), the base version byte
        // is embedded after the team byte (`read_base_legacy`).
        if let Some(context) = state.context.as_deref_mut() {
            context.set_legacy_entities(true);
        }
        read_regions(r, state)
    }

    fn get_meta(&self, r: &mut SaveReader) -> Result<SaveMeta, IoError> {
        // Identical meta region; the returned `SaveMeta.version` is this
        // entry's version (`SaveIO.getMeta` resolves the writer by header).
        let Some(name) = r.next_region()? else {
            return Err(IoError::UnexpectedEof);
        };
        if name != super::super::chunk::REGION_META {
            return Err(IoError::corrupt(format!(
                "first save region must be \"meta\", found \"{name}\""
            )));
        }
        let mut wire = r.wire();
        let tags = wire.string_map()?;
        SaveMeta::from_tags(self.version() as i32, tags)
    }
}
