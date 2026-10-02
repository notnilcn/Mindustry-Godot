// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! A discovered/loaded mod (`Mods.LoadedMod`) and its config path contract.

use std::path::PathBuf;

use crate::content::ContentRef;
use crate::io::SettingsStore;

use super::ModState;
use super::discovery::{ModRoot, ModSource, config_file, config_folder};
use super::meta::{MIN_JAVA_MOD_GAME_VERSION, MIN_MOD_GAME_VERSION, ModMeta};

/// Why a mod is not supported (`LoadedMod.isSupported` failure reasons).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// `getMinMajor() < minModGameVersion` (or `minJavaModGameVersion`).
    Outdated,
    /// Present in `Mods.blacklistedMods`.
    Blacklisted,
    /// `minGameVersion` is newer than this build.
    VersionTooNew,
    /// Script/Java code hooks are unavailable (OD1).
    ScriptsUnsupported,
    /// Java class mod code cannot run without a JVM (OD1; JSON/assets still load).
    JavaModUnsupported,
}

impl UnsupportedReason {
    /// Bundle-key-ish reason text for the dialog/error surface.
    pub fn message(&self) -> &'static str {
        match self {
            UnsupportedReason::Outdated => "mod is outdated and not compatible with this version",
            UnsupportedReason::Blacklisted => "mod is blacklisted",
            UnsupportedReason::VersionTooNew => "mod requires a newer game version",
            UnsupportedReason::ScriptsUnsupported => "script mods are not supported in this build",
            UnsupportedReason::JavaModUnsupported => {
                "Java class mod code is not supported; JSON content and assets still load"
            }
        }
    }
}

/// `Mods.LoadedMod`.
#[derive(Debug, Clone)]
pub struct LoadedMod {
    /// Stable per-run index into the owning [`Mods`](super::Mods) list.
    pub index: usize,
    /// Source file/folder on disk.
    pub file: PathBuf,
    /// Resolved mod file tree (zip-aware).
    pub root: ModRoot,
    /// Discovery source.
    pub source: ModSource,
    /// Internal name (`name` lowercased, spaces → hyphens).
    pub name: String,
    /// Parsed metadata.
    pub meta: ModMeta,
    /// Resolved state.
    pub state: ModState,
    /// Resolved required dependencies (internal names).
    pub dependencies: Vec<String>,
    /// Resolved soft dependencies (internal names).
    pub soft_dependencies: Vec<String>,
    /// Required dependency names that were not found.
    pub missing_dependencies: Vec<String>,
    /// Soft dependency names that were not found.
    pub missing_soft_dependencies: Vec<String>,
    /// Content records that failed to load (plan 20 M1/M2).
    pub errored_content: Vec<ContentRef>,
    /// Unsupported reason, if resolved as unsupported.
    pub unsupported_reason: Option<UnsupportedReason>,
    /// Explicitly marked as a Java mod.
    pub java: bool,
    /// The mod ships `scripts/` (OD1 script seam).
    pub has_scripts: bool,
}

impl LoadedMod {
    /// Builds a loaded-mod record.
    pub fn new(
        index: usize,
        file: PathBuf,
        root: ModRoot,
        source: ModSource,
        meta: ModMeta,
    ) -> Self {
        let name = meta.internal_name.clone();
        let java = meta.java || meta.main.is_some();
        Self {
            index,
            file,
            root,
            source,
            name,
            meta,
            state: ModState::Enabled,
            dependencies: Vec::new(),
            soft_dependencies: Vec::new(),
            missing_dependencies: Vec::new(),
            missing_soft_dependencies: Vec::new(),
            errored_content: Vec::new(),
            unsupported_reason: None,
            java,
            has_scripts: false,
        }
    }

    /// `LoadedMod.enabled()` — `enabled` or `contentErrors`.
    pub fn enabled(&self) -> bool {
        matches!(self.state, ModState::Enabled | ModState::ContentErrors)
    }

    /// `LoadedMod.shouldBeEnabled()` — `mod-<name>-enabled` (default true).
    pub fn should_be_enabled(&self, settings: &SettingsStore) -> bool {
        settings.get_bool(&format!("mod-{}-enabled", self.name), true)
    }

    /// `LoadedMod.failed()` — `mod-<name>-failed`.
    pub fn failed(&self, settings: &SettingsStore) -> bool {
        settings.get_bool(&format!("mod-{}-failed", self.name), false)
    }

    /// `mod-<name>-repo` setting with the metadata fallback.
    pub fn repo(&self, settings: &SettingsStore) -> Option<String> {
        let key = format!("mod-{}-repo", self.name);
        if settings.has(&key) {
            Some(settings.get_string(&key, ""))
        } else {
            self.meta.repo.clone()
        }
    }

    /// Persists `mod-<name>-repo`.
    pub fn set_repo(&self, settings: &mut SettingsStore, repo: &str) {
        settings.put_string(&format!("mod-{}-repo", self.name), repo);
    }

    /// `LoadedMod.isJava()`.
    pub fn is_java(&self) -> bool {
        self.meta.java || self.meta.main.is_some()
    }

    /// `LoadedMod.getMinMajor()`.
    pub fn min_major(&self) -> i32 {
        self.meta.min_major()
    }

    /// `LoadedMod.isOutdated()`.
    pub fn is_outdated(&self) -> bool {
        let minimum = if self.is_java() {
            MIN_JAVA_MOD_GAME_VERSION
        } else {
            MIN_MOD_GAME_VERSION
        };
        self.min_major() < minimum && !self.meta.legacy_compatible
    }

    /// `LoadedMod.isBlacklisted()`.
    pub fn is_blacklisted(&self) -> bool {
        self.meta.is_blacklisted()
    }

    /// `LoadedMod.isSupported()`; headless supports everything.
    pub fn is_supported(&self, headless: bool) -> bool {
        if headless {
            return true;
        }
        !self.is_outdated() && !self.is_blacklisted()
    }

    /// `LoadedMod.hasUnmetDependencies()`.
    pub fn has_unmet_dependencies(&self) -> bool {
        !self.missing_dependencies.is_empty()
    }

    /// `LoadedMod.hasContentErrors()`.
    pub fn has_content_errors(&self) -> bool {
        !self.errored_content.is_empty()
    }

    /// `Mods.getConfigFolder` path contract.
    pub fn config_folder(&self, mod_directory: &std::path::Path) -> PathBuf {
        config_folder(mod_directory, &self.name)
    }

    /// `Mods.getConfig` path contract.
    pub fn config_file(&self, mod_directory: &std::path::Path) -> PathBuf {
        config_file(mod_directory, &self.name)
    }

    /// `"name:version"` fingerprint used by the multiplayer handshake (§3.11).
    pub fn mod_string(&self) -> String {
        format!("{}:{}", self.name, self.meta.version())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::discovery::ModRoot;
    use std::collections::BTreeMap;

    fn loaded(name: &str, version: &str, min: &str, java: bool) -> LoadedMod {
        let meta = ModMeta {
            name: name.to_owned(),
            internal_name: name.to_owned(),
            version: Some(version.to_owned()),
            min_game_version: min.to_owned(),
            java,
            ..ModMeta::default()
        };
        LoadedMod::new(
            0,
            PathBuf::from("/mods/x"),
            ModRoot::Zip(BTreeMap::new()),
            ModSource::LocalZip,
            meta,
        )
    }

    #[test]
    fn support_and_fingerprint() {
        let mod_ = loaded("test-mod", "1.0", "146", false);
        assert!(mod_.is_supported(false));
        assert!(mod_.is_supported(true));
        assert_eq!(mod_.mod_string(), "test-mod:1.0");

        let old = loaded("old", "1.0", "100", false);
        assert!(old.is_outdated());
        assert!(!old.is_supported(false));
        assert!(old.is_supported(true), "headless supports everything");

        let java = loaded("javamod", "1.0", "150", true);
        assert!(java.is_outdated(), "Java min major is 154");
    }
}
