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

/// Runs `f` on the shared store. Before [`init`] runs, the first call loads the
/// default data dir's file (`Vars.java:503 settings.load()`); a poisoned lock
/// (a panic while a facade held it) degrades to the inner store instead of
/// cascading panics.
pub fn with_settings<R>(f: impl FnOnce(&mut SettingsStore) -> R) -> R {
    let mutex =
        HUB.get_or_init(|| Mutex::new(SettingsStore::load(&NativeFs, &Paths::resolve(None))));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

/// `Core.settings.getBool(key)` with the default `SettingsMenuDialog` registered
/// through `settings.defaults(name, def)` (`SettingsTable.checkPref`); `false`
/// for a key that has no settings row. Per-frame readers (renderer, camera,
/// FX) call this the way Java reads `Core.settings` every frame.
pub fn row_bool(key: &str) -> bool {
    let default = mind_core::ui::settings::all_rows()
        .find(|row| row.key == key)
        .is_some_and(|row| row.default_bool());
    with_settings(|store| store.get_bool(key, default))
}

/// `Core.settings.getInt(key)` with the registered slider default
/// (`SettingsTable.sliderPref`); `0` for a key that has no settings row.
pub fn row_i32(key: &str) -> i32 {
    let default = mind_core::ui::settings::all_rows()
        .find(|row| row.key == key)
        .map_or(0, |row| row.default_i32());
    with_settings(|store| store.get_i32(key, default))
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

    /// Serializes the tests of this module: they share the one process-wide
    /// store and `reload` swaps it wholesale.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        match TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

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
        let _guard = serial();
        let root = temp_root();
        let paths = Paths::new(&root);
        init(&paths);
        // The hub may already be live from another path; force the test root.
        reload(&paths);

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

    /// `row_bool`/`row_i32` fall back to the `SettingsMenuDialog` defaults
    /// (`settings.defaults(name, def)`) and read live writes.
    #[test]
    fn row_readers_use_registered_defaults() {
        let _guard = serial();
        let root = temp_root();
        let paths = Paths::new(&root);
        init(&paths);
        reload(&paths);
        with_settings(|store| {
            store.remove("effects");
            store.remove("screenshake");
            store.remove("hidedisplays");
        });
        // `graphics.checkPref("effects", true)`, `checkPref("hidedisplays",
        // false)`, `sliderPref("screenshake", 4, ...)`.
        assert!(row_bool("effects"));
        assert!(!row_bool("hidedisplays"));
        assert_eq!(row_i32("screenshake"), 4);
        assert!(!row_bool("not-a-settings-row"));
        assert_eq!(row_i32("not-a-settings-row"), 0);
        with_settings(|store| {
            store.put_bool("effects", false);
            store.put_i32("screenshake", 8);
        });
        assert!(!row_bool("effects"));
        assert_eq!(row_i32("screenshake"), 8);
        with_settings(|store| {
            store.remove("effects");
            store.remove("screenshake");
        });
        let _ = std::fs::remove_dir_all(&root);
    }
}
