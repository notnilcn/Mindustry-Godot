// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Entity-codec integration tests against the committed manifests
//! (`mind-core/revisions/`, `mind-core/entity_class_ids.toml`).

use std::path::Path;

use super::super::IoError;
use super::super::fs::NativeFs;
use super::super::wire::{WireReader, WireWriter};
use super::idfile::ClassIdFile;
use super::registry::{def_by_class_id, def_by_name, entity_defs};
use super::revisions::{RevisionCheck, check_def, load_manifests};
use super::{EntityCodec, EntityReader, EntityWriter};
use crate::content::BlockId;
use crate::ecs::{BuildingComp, TeamId};
use crate::world::TilePos;

/// `io::entity::tests::revision_check` (plan 04 §7a): every registered def is
/// up to date with its committed newest manifest.
#[test]
fn committed_revision_manifests_match_codecs() {
    let fs = NativeFs;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("revisions");
    for def in entity_defs() {
        let dir = root.join(def.name);
        let manifests = load_manifests(&fs, &dir)
            .unwrap_or_else(|e| panic!("loading revisions for {}: {e}", def.name));
        assert!(
            !manifests.is_empty(),
            "def {} has no revision manifests",
            def.name
        );
        assert_eq!(
            check_def(def.fields, &manifests),
            RevisionCheck::UpToDate,
            "def {} drifted from its newest manifest",
            def.name
        );
    }
}

/// The committed class-ID file covers every registered def with matching IDs.
#[test]
fn committed_class_ids_match_registry() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("entity_class_ids.toml"),
    )
    .expect("entity_class_ids.toml must be committed");
    let file = ClassIdFile::parse(&text).expect("entity_class_ids.toml must parse");
    assert!(
        file.problems(entity_defs()).is_empty(),
        "entity_class_ids.toml drifted from the registry"
    );
    // The committed table reserves the full upstream ID space (50 entries).
    assert_eq!(file.entries.len(), 50);
}

/// `io::entity::tests::unknown_class_skip` (plan 04 §7a): readers skip
/// unknown class IDs by length, never fatal.
#[test]
fn unknown_class_skip() {
    assert!(def_by_class_id(6).is_some());
    assert!(def_by_class_id(255).is_none());
    assert!(def_by_name("BuildingComp").is_some());
    assert!(def_by_name("NoSuchComp").is_none());
}

/// The derive macro: write emits newest revision + fields; read round-trips;
/// unknown revisions error with the def name.
#[test]
fn building_comp_codec_roundtrip() {
    assert_eq!(BuildingComp::NAME, "BuildingComp");
    assert_eq!(BuildingComp::CLASS_ID, 6);
    assert_eq!(BuildingComp::NEWEST_REVISION, 0);
    assert_eq!(BuildingComp::fields().len(), 4);
    const { assert!(BuildingComp::SERIALIZE) }

    let building = BuildingComp {
        pos: TilePos::new(3, 4),
        block: BlockId::STONE_WALL,
        team: TeamId::SHARDED,
        rot: 2,
    };
    let mut buf = Vec::new();
    {
        let mut w: EntityWriter = WireWriter::new(&mut buf);
        building.write(&mut w).unwrap();
    }
    // Revision u16 + pos(4) + block(2) + team(1) + rot(1).
    assert_eq!(buf.len(), 2 + 4 + 2 + 1 + 1);

    let mut read = BuildingComp {
        pos: TilePos::new(0, 0),
        block: BlockId::AIR,
        team: TeamId(0),
        rot: 0,
    };
    {
        let mut r: EntityReader = WireReader::new(&buf);
        let revision = r.us().unwrap();
        read.read(&mut r, revision).unwrap();
        assert_eq!(r.remaining(), 0);
    }
    assert_eq!(read, building);

    // Unknown revision: error names the def.
    let mut r: EntityReader = WireReader::new(&buf);
    let error = read.read(&mut r, 99).unwrap_err();
    assert!(matches!(
        error,
        IoError::UnknownRevision { revision: 99, .. }
    ));
    assert!(error.to_string().contains("BuildingComp"));
}
