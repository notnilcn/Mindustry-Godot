// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Revision manifests: load/check/write (`EntityIO.java` revision discipline,
//! plan 04 §3.5/§6.4).
//!
//! Manifests live in `client/rust/mind-core/revisions/<NAME>/<N>.json` and are
//! **append-only**: schema `{"version":N,"fields":[{"name","type","size",
//! "flags"}]}`, old files are never edited or deleted. The check compares the
//! codec's `fields()` against the newest manifest strictly — field names AND
//! types, not just count+size (plan 04 deviation 3: upstream `Revision.equal`
//! misses renames; here a rename fails until a bump + `aliases` entry).

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::super::IoError;
use super::super::fs::FileSystem;
use super::{EntityDefMeta, FieldDesc, FieldFlag};

/// One manifest field entry (owned form of [`FieldDesc`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionField {
    /// Field name.
    pub name: String,
    /// Wire type name.
    #[serde(rename = "type")]
    pub type_: String,
    /// Byte size, `-1` for variable/complex.
    pub size: i32,
    /// Flags (manifest strings, §6.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
}

impl RevisionField {
    /// Converts a codec field descriptor.
    pub fn of(field: &FieldDesc) -> Self {
        Self {
            name: field.name.to_owned(),
            type_: field.type_.to_owned(),
            size: field.size,
            flags: field
                .flags
                .iter()
                .map(|flag| flag.as_str().to_owned())
                .collect(),
        }
    }
}

/// One `revisions/<NAME>/<N>.json` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionManifest {
    /// Revision number (matches the file name `<N>.json`).
    pub version: u32,
    /// Serialized fields in write order.
    pub fields: Vec<RevisionField>,
    /// Renamed-field aliases (`"old" -> "new"`); used only by importer paths
    /// (plan 04 §6.4), appended when a rename is bumped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<(String, String)>,
}

impl RevisionManifest {
    /// The manifest for a codec's current fields.
    pub fn of_fields(fields: &[FieldDesc], version: u32) -> Self {
        Self {
            version,
            fields: fields.iter().map(RevisionField::of).collect(),
            aliases: Vec::new(),
        }
    }

    /// Deterministic JSON rendering (trailing newline).
    pub fn render(&self) -> Result<String, IoError> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    /// Parses one manifest file.
    pub fn parse(text: &str) -> Result<Self, IoError> {
        Ok(serde_json::from_str(text)?)
    }
}

/// Loads `<dir>/<N>.json` for one def, sorted by `N`; enforces the dense
/// append-only chain (`0.json .. N.json`, no gaps, version keys matching).
pub fn load_manifests(fs: &dyn FileSystem, dir: &Path) -> Result<Vec<RevisionManifest>, IoError> {
    let mut entries: Vec<(u32, std::path::PathBuf)> = Vec::new();
    for path in fs.ls(dir)? {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(number) = stem.parse::<u32>() else {
            continue;
        };
        entries.push((number, path));
    }
    entries.sort_by_key(|(number, _)| *number);
    // Dense chain: entries must be exactly 0..=N.
    for (expected, (number, path)) in entries.iter().enumerate() {
        if *number != expected as u32 {
            return Err(IoError::corrupt(format!(
                "revision chain for `{}` is not append-only: expected {expected}.json, found {}",
                dir.display(),
                path.display()
            )));
        }
    }
    let mut out = Vec::with_capacity(entries.len());
    for (number, path) in entries {
        let text = String::from_utf8(fs.read(&path)?).map_err(|_| {
            IoError::corrupt(format!("revision file `{}` is not UTF-8", path.display()))
        })?;
        let manifest = RevisionManifest::parse(&text)
            .map_err(|e| IoError::corrupt(format!("revision file `{}`: {e}", path.display())))?;
        if manifest.version != number {
            return Err(IoError::corrupt(format!(
                "revision file `{}` declares version {} but is named {number}.json",
                path.display(),
                manifest.version
            )));
        }
        out.push(manifest);
    }
    Ok(out)
}

/// Result of comparing a codec against its newest manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevisionCheck {
    /// Codec fields match the newest manifest exactly.
    UpToDate,
    /// No manifests exist yet (first write pending).
    Missing,
    /// Fields drifted; details list every mismatch (strict name+type compare,
    /// deviation 3).
    Drift {
        /// The version the next manifest should get.
        next_version: u32,
        /// Human-readable mismatch list.
        details: Vec<String>,
    },
}

/// Strict compare of codec fields against the newest manifest.
///
/// Unlike upstream `Revision.equal` (count + types only), names are compared
/// positionally, so a silent rename is flagged as drift (plan 04 deviation 3).
pub fn check_def(fields: &[FieldDesc], manifests: &[RevisionManifest]) -> RevisionCheck {
    let Some(newest) = manifests.last() else {
        return RevisionCheck::Missing;
    };
    let actual: Vec<RevisionField> = fields.iter().map(RevisionField::of).collect();
    let mut details = Vec::new();
    if newest.fields.len() != actual.len() {
        details.push(format!(
            "field count differs: manifest {} vs codec {}",
            newest.fields.len(),
            actual.len()
        ));
    }
    for (index, actual_field) in actual.iter().enumerate() {
        match newest.fields.get(index) {
            None => details.push(format!(
                "field `{}` added at index {index}",
                actual_field.name
            )),
            Some(expected) => {
                if expected.name != actual_field.name {
                    details.push(format!(
                        "field {index} renamed: `{}` -> `{}` (renames require a revision bump + aliases entry)",
                        expected.name, actual_field.name
                    ));
                }
                if expected.type_ != actual_field.type_ {
                    details.push(format!(
                        "field `{}` type changed: `{}` -> `{}`",
                        actual_field.name, expected.type_, actual_field.type_
                    ));
                }
                if expected.size != actual_field.size {
                    details.push(format!(
                        "field `{}` size changed: {} -> {}",
                        actual_field.name, expected.size, actual_field.size
                    ));
                }
                if expected.flags != actual_field.flags {
                    details.push(format!(
                        "field `{}` flags changed: {:?} -> {:?}",
                        actual_field.name, expected.flags, actual_field.flags
                    ));
                }
            }
        }
    }
    for (index, expected) in newest.fields.iter().enumerate().skip(actual.len()) {
        details.push(format!(
            "field `{}` removed at index {index}",
            expected.name
        ));
    }
    if details.is_empty() {
        RevisionCheck::UpToDate
    } else {
        RevisionCheck::Drift {
            next_version: newest.version + 1,
            details,
        }
    }
}

/// Per-def check report for `mind-headless io check-revisions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefRevisionReport {
    /// Def name.
    pub name: String,
    /// Outcome.
    pub outcome: RevisionCheck,
    /// Version written by `--update`, when applied.
    pub updated_to: Option<u32>,
}

/// Checks every def against `revisions_root/<NAME>/`, optionally appending the
/// next manifest on drift (`EntityIO.java` behavior: write the next `<N>.json`
/// and warn; old files are never touched).
pub fn check_all(
    fs: &dyn FileSystem,
    revisions_root: &Path,
    defs: &[EntityDefMeta],
    update: bool,
) -> Result<Vec<DefRevisionReport>, IoError> {
    let mut reports = Vec::new();
    for def in defs {
        let dir = revisions_root.join(def.name);
        let manifests = if fs.exists(&dir) {
            load_manifests(fs, &dir)?
        } else {
            Vec::new()
        };
        let outcome = check_def(def.fields, &manifests);
        let mut updated_to = None;
        if update {
            let write = match &outcome {
                RevisionCheck::Missing => Some(0),
                RevisionCheck::Drift { next_version, .. } => Some(*next_version),
                RevisionCheck::UpToDate => None,
            };
            if let Some(version) = write {
                let manifest = RevisionManifest::of_fields(def.fields, version);
                let text = manifest.render()?;
                fs.write(&dir.join(format!("{version}.json")), text.as_bytes())?;
                updated_to = Some(version);
            }
        }
        reports.push(DefRevisionReport {
            name: def.name.to_owned(),
            outcome,
            updated_to,
        });
    }
    Ok(reports)
}

/// Validates manifest flags parse (used by the checker for strictness).
pub fn validate_flags(manifest: &RevisionManifest) -> Result<(), IoError> {
    for field in &manifest.fields {
        for flag in &field.flags {
            if FieldFlag::parse(flag).is_none() {
                return Err(IoError::corrupt(format!(
                    "field `{}` has unknown flag `{flag}`",
                    field.name
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::entity::SyncFieldMeta;
    use crate::io::fs::MockFs;

    static FIELDS_V0: &[FieldDesc] = &[
        FieldDesc {
            name: "pos",
            type_: "TilePos",
            size: 4,
            flags: &[FieldFlag::Save, FieldFlag::Sync],
        },
        FieldDesc {
            name: "health",
            type_: "f32",
            size: 4,
            flags: &[FieldFlag::Save],
        },
    ];

    fn meta(name: &'static str, fields: &'static [FieldDesc]) -> EntityDefMeta {
        EntityDefMeta {
            name,
            class_id: 1,
            serialize: true,
            sync: true,
            fields,
            sync_fields: &[] as &[SyncFieldMeta],
        }
    }

    /// `io::entity::tests::revision_check` (plan 04 §7a): write v0, check ok,
    /// add a field → drift → `--update` appends v1 and leaves v0 untouched.
    #[test]
    fn revision_check_add_field() {
        let fs = MockFs::new();
        let root = Path::new("/revisions");

        // Missing manifest: --update writes 0.json.
        let reports = check_all(&fs, root, &[meta("TestComp", FIELDS_V0)], true).unwrap();
        assert_eq!(reports[0].outcome, RevisionCheck::Missing);
        assert_eq!(reports[0].updated_to, Some(0));
        let v0_text = fs.read(Path::new("/revisions/TestComp/0.json")).unwrap();

        // Now up to date.
        let reports = check_all(&fs, root, &[meta("TestComp", FIELDS_V0)], false).unwrap();
        assert_eq!(reports[0].outcome, RevisionCheck::UpToDate);

        // Add a field → drift detected in check mode.
        static FIELDS_V1: &[FieldDesc] = &[
            FIELDS_V0[0],
            FIELDS_V0[1],
            FieldDesc {
                name: "items",
                type_: "ItemModule",
                size: -1,
                flags: &[FieldFlag::Save],
            },
        ];
        let reports = check_all(&fs, root, &[meta("TestComp", FIELDS_V1)], false).unwrap();
        match &reports[0].outcome {
            RevisionCheck::Drift {
                next_version,
                details,
            } => {
                assert_eq!(*next_version, 1);
                assert!(details.iter().any(|d| d.contains("count differs")));
            }
            other => panic!("expected drift, got {other:?}"),
        }

        // --update appends 1.json; 0.json stays byte-identical.
        let reports = check_all(&fs, root, &[meta("TestComp", FIELDS_V1)], true).unwrap();
        assert_eq!(reports[0].updated_to, Some(1));
        assert_eq!(
            fs.read(Path::new("/revisions/TestComp/0.json")).unwrap(),
            v0_text
        );
        let manifests = load_manifests(&fs, Path::new("/revisions/TestComp")).unwrap();
        assert_eq!(manifests.len(), 2);
        assert_eq!(manifests[1].fields.len(), 3);
    }

    /// `io::entity::tests::rename_requires_bump` (deviation 3): a rename with
    /// the same count/types fails the strict name check.
    #[test]
    fn rename_requires_bump() {
        let fs = MockFs::new();
        let root = Path::new("/revisions");
        check_all(&fs, root, &[meta("TestComp", FIELDS_V0)], true).unwrap();

        static RENAMED: &[FieldDesc] = &[
            FIELDS_V0[0],
            FieldDesc {
                name: "hp", // renamed from `health`, same type/size
                type_: "f32",
                size: 4,
                flags: &[FieldFlag::Save],
            },
        ];
        let reports = check_all(&fs, root, &[meta("TestComp", RENAMED)], false).unwrap();
        match &reports[0].outcome {
            RevisionCheck::Drift { details, .. } => {
                assert!(details.iter().any(|d| d.contains("renamed")));
            }
            other => panic!("expected rename drift, got {other:?}"),
        }
    }

    #[test]
    fn manifest_chain_must_be_dense() {
        let fs = MockFs::new();
        fs.write(
            Path::new("/revisions/TestComp/1.json"),
            br#"{"version":1,"fields":[]}"#,
        )
        .unwrap();
        assert!(load_manifests(&fs, Path::new("/revisions/TestComp")).is_err());
    }

    #[test]
    fn manifest_render_parse_roundtrip() {
        let manifest = RevisionManifest::of_fields(FIELDS_V0, 3);
        let text = manifest.render().unwrap();
        assert!(text.ends_with('\n'));
        let parsed = RevisionManifest::parse(&text).unwrap();
        assert_eq!(parsed, manifest);
        assert_eq!(parsed.fields[0].flags, vec!["save", "sync"]);
        validate_flags(&parsed).unwrap();
    }
}
