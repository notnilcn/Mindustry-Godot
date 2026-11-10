// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapProcessorsDialog.java,
//         core/src/mindustry/editor/MapLoadDialog.java,
//         core/src/mindustry/editor/SectorGenerateDialog.java,
//         core/src/mindustry/editor/BannedContentDialog.java,
//         core/src/mindustry/editor/data/MapAssetsDialog.java (+ `MapPatchesView`,
//         `MapContentView`, `MapBundlesView`, `MapImagesView`, `MapAudioView`,
//         `AssetView`), core/src/mindustry/ui/dialogs/SchematicsDialog.java
//         (`SchematicInfoDialog`, :774-834).

//! Moved to editor/ and schematics/schematic_info.rs. Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

/// The dialog controllers of the old batch; every id moved to a group leaf.
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}
