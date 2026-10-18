// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Data-asset audio decorator (PAR-0394).
//!
//! `DataAudioLoader` registers its `dp-<name>` sounds/music in the runtime
//! `Core.assets` map, so callers (`LExecutor` `@sfx-*` vars, the map assets
//! dialog) can play them by name. The port's `mind_core::mods::assets::AudioApplier`
//! holds the records; this module copies the embedded bytes into the live
//! `StreamCache` under the same names.

use mind_core::mods::assets::{DataAssetType, ModDataManager};

use crate::assets::decorator::data_blob;

use super::MindAudio;

impl MindAudio {
    /// Replace-only registration of the manager's data-asset sounds/music
    /// (`DataManager.reloadAudio` → `DataAudioLoader.load`).
    pub fn apply_data_audio(&mut self, manager: &ModDataManager) {
        self.streams.clear_data_streams();
        for entry in manager.audio_applier().entries() {
            let type_ = if entry.music {
                DataAssetType::Music
            } else {
                DataAssetType::Sound
            };
            let Some(bytes) = data_blob(manager, type_, &entry.path) else {
                log::warn!(
                    "[audio] data audio `{}` has no embedded payload (cache not mounted)",
                    entry.path
                );
                continue;
            };
            self.streams
                .add_data_stream(&entry.name, bytes, entry.music);
        }
    }
}
