// SPDX-License-Identifier: GPL-3.0-only

//! Runtime asset location (plan 03 §3.2/§6.1; I07 T7).
//!
//! The packed `assets/` tree lives at the repo root (outside the Godot project
//! `client/`), so `res://` cannot address it directly. Exported builds ship it
//! in an `assets/` directory next to the executable (`tools/export.sh`), the
//! same layout `tools/pack.sh pack` produces at the repo root.
//!
//! [`resolve_assets_dir`] picks the first valid candidate in this order (a
//! candidate is "valid" when `sprites/sprites.atlas.json` exists):
//!
//! 1. `ProjectSettings` [`ASSETS_DIR_SETTING`] (explicit override);
//! 2. a user command-line argument `--assets=<dir>`;
//! 3. the `MINDUSTRY_GODOT_ASSETS` environment variable;
//! 4. `res://assets` (asset tree packaged inside the project);
//! 5. `<OS.get_executable_path() dir>/assets` (the export layout);
//! 6. `<res://>/../assets` (editor / dev layout; also the logged fallback
//!    when nothing is valid, so callers print a precise missing-manifest
//!    error instead of panicking).

use godot::classes::{FileAccess, Os, ProjectSettings};
use godot::obj::Singleton;
use godot::prelude::*;

use mind_core::io::fs::Paths;
use mind_core::io::settings::KEY_LOCALE;

/// `ProjectSettings` key overriding the asset root (absolute or `res://`).
pub const ASSETS_DIR_SETTING: &str = "mindustry/assets_dir";

/// `ProjectSettings` key selecting the locale (`en`, `pt_BR`, `default`).
pub const LOCALE_SETTING: &str = "mindustry/locale";

/// Resolves the active locale.
///
/// Ported from `Vars.java:536-555` + `LanguageDialog.java:91-128`: the stored
/// `locale` setting (`Core.settings.getString("locale")`) wins; the registered
/// default `"default"` resolves through the OS locale against the
/// `assets/locales` list (exact match, then same language, else `en`) and the
/// result is stored (`findClosestLocale`). The `mindustry/locale` project
/// setting stays as a test-only override.
pub fn resolve_locale() -> String {
    if let Some(value) = project_override() {
        return value;
    }

    let paths = Paths::resolve(None);
    let stored =
        crate::settings_hub::with_settings(|store| store.get_string(KEY_LOCALE, "default"));
    if stored != "default" && !stored.is_empty() {
        return stored;
    }

    let available = crate::assets::bundle::load_locales(&resolve_assets_dir());
    let os_locale = Os::singleton().get_locale().to_string();
    let closest = mind_core::assets::locale::find_closest_locale(&os_locale, &available);
    crate::settings_hub::with_settings(|store| store.put_string(KEY_LOCALE, &closest));
    if let Err(error) = crate::settings_hub::save(&paths) {
        log::warn!("[assets] locale flush failed: {error}");
    }
    closest
}

/// The test-only `mindustry/locale` override (empty/`default` = no override).
fn project_override() -> Option<String> {
    ProjectSettings::singleton()
        .get_setting(LOCALE_SETTING)
        .try_to::<GString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty() && value != "default")
}

/// Resolves the directory containing `sprites/sprites.atlas.json`.
pub fn resolve_assets_dir() -> String {
    let candidates = asset_candidates();
    let (source, dir) = pick_assets_dir(&candidates, atlas_exists);
    log_resolved_once(&dir, source);
    dir
}

/// Candidate roots in resolution order (`(label, dir)`; the label names the
/// source for the once-per-process INFO log).
fn asset_candidates() -> Vec<(&'static str, String)> {
    let mut candidates: Vec<(&'static str, String)> = Vec::new();

    let configured = ProjectSettings::singleton()
        .get_setting(ASSETS_DIR_SETTING)
        .try_to::<GString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty());
    if let Some(dir) = configured {
        candidates.push(("mindustry/assets_dir", dir));
    }

    for arg in Os::singleton().get_cmdline_user_args().as_slice() {
        if let Some(dir) = arg.to_string().strip_prefix("--assets=")
            && !dir.is_empty()
        {
            candidates.push(("--assets=", dir.to_owned()));
        }
    }

    let env = Os::singleton()
        .get_environment("MINDUSTRY_GODOT_ASSETS")
        .to_string();
    if !env.is_empty() {
        candidates.push(("MINDUSTRY_GODOT_ASSETS", env));
    }

    candidates.push(("res://assets", String::from("res://assets")));

    let exe = Os::singleton().get_executable_path().to_string();
    if let Some(base) = std::path::Path::new(&exe).parent() {
        candidates.push(("exe-dir assets", format!("{}/assets", base.display())));
    }

    let project = ProjectSettings::singleton()
        .globalize_path("res://")
        .to_string();
    candidates.push(("dev ../assets", format!("{project}../assets")));

    candidates
}

/// The first candidate `valid` accepts; when none is valid the last (dev)
/// candidate is returned so callers log a precise missing-manifest error.
fn pick_assets_dir<F: Fn(&str) -> bool>(
    candidates: &[(&'static str, String)],
    valid: F,
) -> (&'static str, String) {
    for (label, dir) in candidates {
        if valid(dir) {
            return (label, dir.clone());
        }
    }
    match candidates.last() {
        Some((label, dir)) => (label, dir.clone()),
        None => ("dev ../assets", String::new()),
    }
}

/// Logs the chosen root once (and again only when it changes, which happens
/// when tests re-resolve with a different override).
fn log_resolved_once(dir: &str, source: &str) {
    static LAST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut last = match LAST.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if last.as_deref() != Some(dir) {
        log::info!("[assets] assets root: `{dir}` (source: {source})");
        *last = Some(dir.to_owned());
    }
}

fn atlas_exists(dir: &str) -> bool {
    FileAccess::file_exists(&format!("{dir}/sprites/sprites.atlas.json"))
}

/// Resolves a `res://`/`user://` virtual path to an OS path (absolute strings
/// pass through `ProjectSettings.globalize_path` unchanged).
pub fn globalize_path(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(path)
            .to_string(),
    )
}

/// The built-in map directory (`<assets>/maps`) as an OS path
/// (`Maps.defaultMapDirectory`).
pub fn assets_maps_dir() -> std::path::PathBuf {
    globalize_path(&resolve_assets_dir()).join("maps")
}

/// The session data root (`user://` globalized) that holds `maps/`,
/// `previews/`, `saves/` and `config/`.
pub fn user_data_root() -> std::path::PathBuf {
    globalize_path("user://")
}

/// Loads the map registry from the built-in folder plus the data root's
/// `maps/` (`Maps.load`, `Maps.java:200-260`).
pub fn load_map_registry(root: &std::path::Path) -> mind_core::maps::Maps {
    use mind_core::io::fs::{FileSystem, NativeFs, Paths};

    let mut maps = mind_core::maps::Maps::new();
    let custom = Paths::new(root).maps();
    let sources = mind_core::maps::MapSources {
        builtin_dir: Some(assets_maps_dir()),
        custom_dir: NativeFs.exists(&custom).then_some(custom),
        use_default_folder: true,
    };
    maps.load(&NativeFs, &sources);
    maps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six candidate sources of `asset_candidates()` with fixed values,
    /// so the ordering can be exercised without Godot singletons.
    fn candidates() -> Vec<(&'static str, String)> {
        vec![
            ("mindustry/assets_dir", String::from("/setting")),
            ("--assets=", String::from("/arg")),
            ("MINDUSTRY_GODOT_ASSETS", String::from("/env")),
            ("res://assets", String::from("res://assets")),
            ("exe-dir assets", String::from("/exe/assets")),
            ("dev ../assets", String::from("/dev/assets")),
        ]
    }

    #[test]
    fn pick_assets_dir_follows_the_resolution_order() {
        let all = candidates();

        // Every candidate valid: the project setting wins.
        let (source, dir) = pick_assets_dir(&all, |_| true);
        assert_eq!(source, "mindustry/assets_dir");
        assert_eq!(dir, "/setting");

        // The env var beats `res://assets` and the export layout.
        let (source, dir) = pick_assets_dir(&all, |path| {
            path == "/env" || path == "res://assets" || path == "/exe/assets"
        });
        assert_eq!(source, "MINDUSTRY_GODOT_ASSETS");
        assert_eq!(dir, "/env");

        // The export layout beats the dev fallback.
        let (source, dir) =
            pick_assets_dir(&all, |path| path == "/exe/assets" || path == "/dev/assets");
        assert_eq!(source, "exe-dir assets");
        assert_eq!(dir, "/exe/assets");

        // Nothing valid: the dev fallback is returned for a precise log.
        let (source, dir) = pick_assets_dir(&all, |_| false);
        assert_eq!(source, "dev ../assets");
        assert_eq!(dir, "/dev/assets");
    }
}
