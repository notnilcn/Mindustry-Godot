// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The one settings store every facade shares (`Core.settings`, PAR-0164).
//!
//! Arc creates exactly one `Settings` instance at app boot
//! (`ARC/backends/backend-sdl/src/arc/backend/sdl/SdlApplication.java:45`
//! `Core.settings = new Settings();`) and exposes it as the single static
//! `Core.settings` (`ARC/arc-core/src/arc/Core.java:24`). Mindustry loads it
//! once (`J/Vars.java:503 settings.load()`, defaults registered at `:501`) and
//! every subsystem — UI, campaign, mods, input — reads and writes that one
//! object; a value written anywhere is immediately visible everywhere.
//!
//! Before I05 T1 the port had four independent `SettingsStore`s (MindUi,
//! MindCampaign, MindMods and the throwaway loads in the input bindings), each
//! a stale snapshot of the same `user://config/settings.bin`, plus read-through
//! workarounds on MindCampaign (`live_setting_flag`,
//! `live_setting_flag_default`, `live_save_interval_seconds`). A write through
//! one facade stayed invisible to the others until a reload (PAR-0164). This
//! module is the one store: the host calls [`init`] once at boot, every facade
//! reads/writes through [`with_settings`], and persistence goes through
//! [`save`] (Arc `Settings.forceSave`, `ARC/arc-core/src/arc/Settings.java:91`).
//!
//! `std::sync::Mutex` is not re-entrant: nothing inside a [`with_settings`]
//! closure may call Godot, another facade, or [`with_settings`] itself.

use std::sync::{Mutex, OnceLock};

use mind_core::io::fs::{NativeFs, Paths};
use mind_core::io::{IoError, SettingsStore};

/// The one process-wide store (`Core.settings`).
static HUB: OnceLock<Mutex<SettingsStore>> = OnceLock::new();

/// Loads `config/settings.bin` into the shared store. Idempotent: only the
/// first call per process reads the file (`Vars.java:503 settings.load()`).
pub fn init(paths: &Paths) {
    let _ = HUB.get_or_init(|| Mutex::new(SettingsStore::load(&NativeFs, paths)));
}

/// Runs `f` on the shared store. A poisoned lock (a panic while a facade held
/// it) degrades to the inner store instead of cascading panics.
pub fn with_settings<R>(f: impl FnOnce(&mut SettingsStore) -> R) -> R {
    let mutex = HUB.get_or_init(|| Mutex::new(SettingsStore::new()));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

/// Re-reads the file after an external writer replaced it (data import, clear
/// game data; Java `importData` reloads settings before exit).
pub fn reload(paths: &Paths) {
    with_settings(|store| *store = SettingsStore::load(&NativeFs, paths));
}

/// Flushes the shared store to `config/settings.bin` (`Settings.forceSave`).
pub fn save(paths: &Paths) -> Result<(), IoError> {
    with_settings(|store| store.force_save(&NativeFs, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A unique directory under the system temp dir (this suite only).
    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "mind-gdext-settings-hub-{}-{n}",
            std::process::id()
        ))
    }

    /// Two closures write through the one store; `save` makes both visible to a
    /// fresh `SettingsStore::load` (the PAR-0164 shape: one `Core.settings`).
    #[test]
    fn closures_share_writes_and_save_persists() {
        let root = temp_root();
        let paths = Paths::new(&root);
        init(&paths);

        with_settings(|store| store.put_bool("feature", true));
        // A second closure sees the first write without a reload.
        assert!(with_settings(|store| store.get_bool("feature", false)));
        with_settings(|store| store.put_string("locale", "de"));

        if let Err(error) = save(&paths) {
            panic!("save failed: {error}");
        }
        let fresh = SettingsStore::load(&NativeFs, &paths);
        assert!(fresh.get_bool("feature", false));
        assert_eq!(fresh.get_string("locale", ""), "de");

        // reload replaces the in-memory copy from disk.
        with_settings(|store| store.put_string("locale", "en"));
        reload(&paths);
        assert_eq!(with_settings(|store| store.get_string("locale", "")), "de");

        let _ = std::fs::remove_dir_all(&root);
    }
}
