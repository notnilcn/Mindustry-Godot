// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Styles.java (field inventory),
//         core/src/mindustry/core/UI.java (dialog registry + pause flags).

//! UI manifest loaders/validators (plan 14 §6.4/§6.5).
//!
//! `dialogs_manifest.json` and `styles_manifest.json` are the parity ABI that
//! binds the GDScript scene tree to this plan. Both are validated here so the
//! `mind-headless ui manifest` scenario and `cargo test` share one code path.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// `dialogs_manifest.json` schema version.
pub const DIALOGS_FORMAT: u32 = 1;
/// `styles_manifest.json` schema version.
pub const STYLES_FORMAT: u32 = 1;

/// Manifest validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// Wrong/unsupported `format` field.
    BadFormat(u32),
    /// A name appears more than once.
    Duplicate(String),
    /// A scene path does not exist on disk.
    MissingScene(String),
    /// Unknown fragment group.
    BadGroup(String),
    /// A pause flag disagrees with the source table.
    PauseMismatch(String),
    /// A `Styles.*` field is missing from the manifest.
    MissingStyle(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::BadFormat(value) => write!(f, "unsupported manifest format {value}"),
            ManifestError::Duplicate(name) => write!(f, "duplicate name '{name}'"),
            ManifestError::MissingScene(path) => write!(f, "missing scene '{path}'"),
            ManifestError::BadGroup(group) => write!(f, "unknown fragment group '{group}'"),
            ManifestError::PauseMismatch(name) => {
                write!(f, "pause flag mismatch for dialog '{name}'")
            }
            ManifestError::MissingStyle(name) => write!(f, "missing style '{name}'"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// One dialog registry entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogEntry {
    /// Registry key (`about`, `settings`, …).
    pub name: String,
    /// `res://` scene path.
    pub scene: String,
    /// Whether showing it pauses the game.
    #[serde(default)]
    pub pause: bool,
    /// Reachable from the menu fragment.
    #[serde(default)]
    pub menu_openable: bool,
    /// Context argument descriptors for the MCP sweep.
    #[serde(default)]
    pub args: Vec<String>,
}

/// One fragment registry entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentEntry {
    /// Registry key.
    pub name: String,
    /// `res://` scene path.
    pub scene: String,
    /// Group (`menu`/`hud`/`overlay`).
    pub group: String,
}

/// `dialogs_manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogsManifest {
    /// Schema version.
    pub format: u32,
    /// Dialog entries.
    #[serde(default)]
    pub dialogs: Vec<DialogEntry>,
    /// Fragment entries.
    #[serde(default)]
    pub fragments: Vec<FragmentEntry>,
    /// Prompt helper names.
    #[serde(default)]
    pub prompts: Vec<String>,
}

impl DialogsManifest {
    /// Parses the manifest JSON.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Looks up a dialog entry.
    pub fn dialog(&self, name: &str) -> Option<&DialogEntry> {
        self.dialogs.iter().find(|entry| entry.name == name)
    }

    /// Validates uniqueness, pause flags and (when `root` is set) scene paths.
    pub fn validate(&self, root: Option<&Path>) -> Result<(), ManifestError> {
        if self.format != DIALOGS_FORMAT {
            return Err(ManifestError::BadFormat(self.format));
        }
        let mut seen = HashSet::new();
        for entry in &self.dialogs {
            if !seen.insert(entry.name.as_str()) {
                return Err(ManifestError::Duplicate(entry.name.clone()));
            }
            if let Some(expected) = expected_pause(&entry.name)
                && expected != entry.pause
            {
                return Err(ManifestError::PauseMismatch(entry.name.clone()));
            }
            if let Some(root) = root {
                check_scene(root, &entry.scene)?;
            }
        }
        for entry in &self.fragments {
            if !seen.insert(entry.name.as_str()) {
                return Err(ManifestError::Duplicate(entry.name.clone()));
            }
            if !matches!(entry.group.as_str(), "menu" | "hud" | "overlay" | "loading") {
                return Err(ManifestError::BadGroup(entry.group.clone()));
            }
            if let Some(root) = root {
                check_scene(root, &entry.scene)?;
            }
        }
        Ok(())
    }
}

/// `styles_manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StylesManifest {
    /// Schema version.
    pub format: u32,
    /// Ninepatch/drawable styles.
    #[serde(default)]
    pub drawables: Vec<String>,
    /// Text-button styles.
    #[serde(default)]
    pub text_buttons: Vec<String>,
    /// Button styles.
    #[serde(default)]
    pub buttons: Vec<String>,
    /// Image-button styles.
    #[serde(default)]
    pub image_buttons: Vec<String>,
    /// Pane styles.
    #[serde(default)]
    pub panes: Vec<String>,
    /// Slider styles.
    #[serde(default)]
    pub sliders: Vec<String>,
    /// Label styles.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Field styles.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Check styles.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Dialog styles.
    #[serde(default)]
    pub dialogs: Vec<String>,
    /// Tree styles.
    #[serde(default)]
    pub trees: Vec<String>,
}

impl StylesManifest {
    /// Parses the manifest JSON.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Validates the format, name uniqueness, and that every Java `Styles.*`
    /// field is present exactly once.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.format != STYLES_FORMAT {
            return Err(ManifestError::BadFormat(self.format));
        }
        let mut seen = HashSet::new();
        for group in self.groups() {
            for name in group {
                if !seen.insert(name.as_str()) {
                    return Err(ManifestError::Duplicate(name.clone()));
                }
            }
        }
        for required in EXPECTED_STYLES {
            if !seen.contains(required) {
                return Err(ManifestError::MissingStyle((*required).to_owned()));
            }
        }
        Ok(())
    }

    /// All style names as `(kind, name)` pairs.
    pub fn groups(&self) -> [&[String]; 11] {
        [
            &self.drawables,
            &self.text_buttons,
            &self.buttons,
            &self.image_buttons,
            &self.panes,
            &self.sliders,
            &self.labels,
            &self.fields,
            &self.checks,
            &self.dialogs,
            &self.trees,
        ]
    }

    /// Whether `name` appears anywhere in the manifest.
    pub fn contains(&self, name: &str) -> bool {
        self.groups()
            .iter()
            .any(|group| group.iter().any(|n| n == name))
    }
}

/// The complete `Styles.*` field list from `Styles.java` (plan §6.5).
pub const EXPECTED_STYLES: &[&str] = &[
    // drawables
    "black",
    "black9",
    "black8",
    "black6",
    "black5",
    "black3",
    "grayPanel",
    "grayPanelDark",
    "none",
    "flatDown",
    "flatOver",
    "accentDrawable",
    // text buttons
    "defaultt",
    "flatt",
    "grayt",
    "flatTogglet",
    "logicTogglet",
    "flatToggleMenut",
    "togglet",
    "cleart",
    "clearTogglet",
    "fullTogglet",
    "squareTogglet",
    "logict",
    "flatBordert",
    "nonet",
    // buttons
    "defaultb",
    "underlineb",
    // image buttons
    "defaulti",
    "nodei",
    "emptyi",
    "emptyTogglei",
    "selecti",
    "logici",
    "geni",
    "grayi",
    "graySquarei",
    "flati",
    "squarei",
    "squareTogglei",
    "grayTogglei",
    "clearNonei",
    "cleari",
    "clearTogglei",
    "clearNoneTogglei",
    // panes
    "defaultPane",
    "horizontalPane",
    "smallPane",
    "noBarPane",
    // sliders
    "defaultSlider",
    // labels
    "defaultLabel",
    "outlineLabel",
    "techLabel",
    "monoLabel",
    // fields
    "defaultField",
    "nodeField",
    "areaField",
    "nodeArea",
    // checks
    "defaultCheck",
    // dialogs
    "defaultDialog",
    "fullDialog",
    // trees
    "defaultTree",
];

/// Dialog names required by plan 14 M3 (menus + standalone dialogs). Later
/// milestones append to this list; a name missing from
/// `client/ui/dialogs_manifest.json` fails [`tests::repo_dialogs_manifest_complete`].
pub const EXPECTED_M3_DIALOGS: &[&str] = &[
    "about",
    "settings",
    "language",
    "controls",
    "database",
    "content",
    "icon_select",
    "palette",
    "picker",
    "full_text",
    "discord",
    "mods",
    "mod_browser",
    "join",
    "host",
    "load",
    "save",
    "restart",
    "custom_rules",
    "campaign_rules",
    "campaign_complete",
    "admins",
    "bans",
    "traces",
];

/// Pause flag for a dialog from `UI.init()` (§3.4).
pub fn expected_pause(name: &str) -> Option<bool> {
    Some(match name {
        "settings" | "database" | "planet" | "research" | "schematics" | "paused" | "logic"
        | "campaign_complete" | "full_text" => true,
        "about" | "admins" | "bans" | "campaign_rules" | "canvas_edit" | "picker" | "content"
        | "custom" | "custom_rules" | "discord" | "editor_maps" | "effects" | "file_chooser"
        | "restart" | "host" | "icon_select" | "join" | "controls" | "language"
        | "launch_loadout" | "load" | "loadout" | "map_play" | "mod_browser" | "mods"
        | "palette" | "save" | "sector_select" | "traces" | "editor" => false,
        _ => return None,
    })
}

fn check_scene(root: &Path, scene: &str) -> Result<(), ManifestError> {
    let relative = scene.strip_prefix("res://").unwrap_or(scene);
    if root.join(relative).is_file() {
        Ok(())
    } else {
        Err(ManifestError::MissingScene(scene.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALOGS: &str = r#"{
      "format": 1,
      "dialogs": [
        {"name":"about","scene":"res://scenes/ui/dialogs/about_dialog.tscn","pause":false,"menu_openable":true,"args":[]},
        {"name":"settings","scene":"res://scenes/ui/dialogs/settings_menu_dialog.tscn","pause":true,"menu_openable":true,"args":[]}
      ],
      "fragments": [
        {"name":"menu","scene":"res://scenes/ui/fragments/menu_fragment.tscn","group":"menu"}
      ],
      "prompts": ["show_info"]
    }"#;

    #[test]
    fn dialogs_manifest_valid() {
        let manifest = DialogsManifest::from_json(DIALOGS).unwrap();
        assert_eq!(manifest.format, 1);
        assert!(manifest.dialog("about").is_some());
        assert!(manifest.validate(None).is_ok());
    }

    #[test]
    fn every_pause_dialog_flagged() {
        assert_eq!(expected_pause("settings"), Some(true));
        assert_eq!(expected_pause("about"), Some(false));
        assert_eq!(expected_pause("unknown_dialog"), None);

        // A pause flag that disagrees with the source table fails validation.
        let bad = DialogsManifest::from_json(
            r#"{"format":1,"dialogs":[{"name":"settings","scene":"res://x.tscn","pause":false}]}"#,
        )
        .unwrap();
        assert_eq!(
            bad.validate(None),
            Err(ManifestError::PauseMismatch("settings".into()))
        );
    }

    /// Validates the committed `client/ui/dialogs_manifest.json` against the M3
    /// catalogue and on-disk scenes (plan 14 §7a/§7b `ui manifest`).
    #[test]
    fn repo_dialogs_manifest_complete() {
        let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = client.join("ui/dialogs_manifest.json");
        let text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display()));
        let manifest = DialogsManifest::from_json(&text).expect("parse dialogs_manifest.json");
        manifest
            .validate(Some(&client))
            .expect("validate dialogs_manifest.json (scene paths + pause flags)");
        for name in EXPECTED_M3_DIALOGS {
            assert!(
                manifest.dialog(name).is_some(),
                "M3 dialog '{name}' missing from dialogs_manifest.json"
            );
        }
        assert!(
            manifest
                .fragments
                .iter()
                .any(|entry| entry.name == "menu" && entry.group == "menu"),
            "menu fragment missing"
        );
    }

    #[test]
    fn styles_manifest_matches_java() {
        let mut manifest =
            StylesManifest::from_json(r#"{"format":1,"drawables":["black","none"]}"#).unwrap();
        assert!(manifest.validate().is_err(), "missing styles must fail");

        manifest.drawables = EXPECTED_STYLES.iter().map(|s| (*s).to_owned()).collect();
        assert!(manifest.validate().is_ok());
        assert!(manifest.contains("black"));
        assert!(!manifest.contains("not-a-style"));

        // Duplicate names across groups fail.
        manifest.drawables.push("black".to_owned());
        assert!(manifest.validate().is_err());
    }
}
