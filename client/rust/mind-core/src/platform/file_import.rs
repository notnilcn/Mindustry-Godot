// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dropped/passed file import routing (plan 22 §3.4/§6.7).
//!
//! Port of `ClientLauncher.fileDropped`/`handleFileImport`: only `.msav`/`.msch`
//! files are accepted; schematics open the schematic import flow, a `.msav` is
//! routed by its save meta to the map import, campaign-save rejection, or save
//! load flow. Mobile drops are ignored upstream; the host applies that rule.

use std::path::Path;

/// Save extension (`Vars.saveExtension`).
pub const SAVE_EXTENSION: &str = "msav";
/// Schematic extension (`Vars.schematicExtension`).
pub const SCHEMATIC_EXTENSION: &str = "msch";

/// Where a dropped/passed file should be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `.msch` → schematic import (`Schematics.importAndShow`).
    Schematic,
    /// `.msav` save → `SaveSlot::import_file` + load flow.
    Save,
    /// `.msav` map → `Maps.try_import_map`.
    Map,
    /// `.msav` campaign save → `@save.nocampaign` error.
    CampaignSave,
    /// Any other extension: ignored (`fileDropped` returns without routing).
    Unsupported,
}

/// Classifies a path by extension alone (the `fileDropped` gate).
pub fn from_extension(path: &Path) -> ImportKind {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some(SCHEMATIC_EXTENSION) => ImportKind::Schematic,
        Some(SAVE_EXTENSION) => ImportKind::Save,
        _ => ImportKind::Unsupported,
    }
}

/// Whether a file may enter the import router (`.msav`/`.msch` only).
pub fn is_droppable(path: &Path) -> bool {
    !matches!(from_extension(path), ImportKind::Unsupported)
}

/// Refines a `.msav` classification once its meta has been read
/// (`handleFileImport`): maps go to the map importer, campaign saves are
/// rejected, and ordinary saves open the load flow.
pub fn route_save(is_map: bool, has_sector: bool) -> ImportKind {
    if is_map {
        ImportKind::Map
    } else if has_sector {
        ImportKind::CampaignSave
    } else {
        ImportKind::Save
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extension_gate_matches_upstream() {
        assert_eq!(
            from_extension(Path::new("/a/b/slot0.msav")),
            ImportKind::Save
        );
        assert_eq!(
            from_extension(Path::new("/a/b/factory.MSCH")),
            ImportKind::Schematic
        );
        assert_eq!(
            from_extension(Path::new("/a/b/image.png")),
            ImportKind::Unsupported
        );
        assert!(is_droppable(&PathBuf::from("x.msav")));
        assert!(is_droppable(&PathBuf::from("x.msch")));
        assert!(!is_droppable(&PathBuf::from("x.msav.bak")));
        assert!(!is_droppable(&PathBuf::from("x.txt")));
    }

    #[test]
    fn save_meta_routing() {
        assert_eq!(route_save(true, false), ImportKind::Map);
        assert_eq!(route_save(false, true), ImportKind::CampaignSave);
        assert_eq!(route_save(false, false), ImportKind::Save);
    }
}
