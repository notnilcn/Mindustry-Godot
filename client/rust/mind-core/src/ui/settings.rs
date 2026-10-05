// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/SettingsMenuDialog.java (`addSettings`,
//         `SettingsCategory`, `SettingsTable.CheckSetting`/`SliderSetting`; plan 14 §3.4).

//! Godot-free model of the settings dialog (plan 14 §3.4).
//!
//! `SettingsMenuDialog.addSettings()` registers the game/graphics/sound/dev
//! tables as ordered `Setting` rows; the category menu is
//! `game/graphics/sound/language/controls/data/dev` (desktop). This module
//! freezes that order, kind, default, range and value-formatting tag as pure
//! data so `MindUi` can project it to JSON and the GDScript dialog renders it
//! without hardcoding the row list. Values themselves live in the plan-04
//! `SettingsStore`; formatting happens in the widget layer.

/// How a slider value is rendered (the upstream `StringProcessor`s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingFormat {
    /// `i + ""`.
    Plain,
    /// `i + "%"`.
    Percent,
    /// `bundle.format("setting.seconds", i)`.
    Seconds,
    /// `(i / 4f) + "x"` (`screenshake`).
    Multiplier,
    /// `i + "x"` (`bloomblur`).
    X,
    /// `(int)(i / 4f * 100f) + "%"` (`bloomintensity`).
    BloomPercent,
    /// `i + "px"` (`uiEdgePadding`).
    Pixels,
    /// `setting.fpscap.none` when above 240, else `setting.fpscap.text`.
    FpsCap,
}

/// The widget kind of a settings row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    /// `CheckSetting`.
    Check,
    /// `SliderSetting` with its bounds.
    Slider { min: i32, max: i32, step: i32 },
}

/// The model default (`settings.defaults(name, def)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingDefault {
    /// Checkbox default.
    Check(bool),
    /// Slider default.
    Int(i32),
}

/// One `Setting` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingRow {
    /// Upstream setting key (`settings` KV name).
    pub key: &'static str,
    /// Widget kind and slider bounds.
    pub kind: SettingKind,
    /// Model default.
    pub default: SettingDefault,
    /// Slider value formatting.
    pub format: SettingFormat,
}

impl SettingRow {
    /// A `checkPref(name, def)` row.
    const fn check(key: &'static str, default: bool) -> Self {
        Self {
            key,
            kind: SettingKind::Check,
            default: SettingDefault::Check(default),
            format: SettingFormat::Plain,
        }
    }

    /// A `sliderPref(name, def, min, max, step, format)` row.
    const fn slider(
        key: &'static str,
        default: i32,
        min: i32,
        max: i32,
        step: i32,
        format: SettingFormat,
    ) -> Self {
        Self {
            key,
            kind: SettingKind::Slider { min, max, step },
            default: SettingDefault::Int(default),
            format,
        }
    }

    /// The slider default (`Check` rows return `0`).
    pub fn default_i32(&self) -> i32 {
        match self.default {
            SettingDefault::Check(_) => 0,
            SettingDefault::Int(value) => value,
        }
    }

    /// The checkbox default (`Slider` rows return `false`).
    pub fn default_bool(&self) -> bool {
        matches!(self.default, SettingDefault::Check(true))
    }
}

/// What a category rail button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// Selects the category's table (`visible(index)`).
    Table,
    /// Opens `LanguageDialog` (`ui.language::show`).
    Language,
    /// Opens `KeybindDialog` (`ui.controls::show`).
    Controls,
    /// Opens the data actions dialog (`dataDialog.show()`).
    Data,
}

/// One rail entry from `SettingsMenuDialog.rebuildMenu()` (desktop order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsCategory {
    /// Stable id used by the JSON bridge (`game`, `graphics`, …).
    pub id: &'static str,
    /// Bundle key (`settings.game`, …).
    pub key: &'static str,
    /// Icon alias resolved by `MindIcons`.
    pub icon: &'static str,
    /// Click behavior.
    pub action: SettingsAction,
}

/// Category rail, in upstream menu order (desktop, non-Steam).
pub const CATEGORIES: &[SettingsCategory] = &[
    SettingsCategory {
        id: "game",
        key: "settings.game",
        icon: "settings",
        action: SettingsAction::Table,
    },
    SettingsCategory {
        id: "graphics",
        key: "settings.graphics",
        icon: "image",
        action: SettingsAction::Table,
    },
    SettingsCategory {
        id: "sound",
        key: "settings.sound",
        icon: "effect",
        action: SettingsAction::Table,
    },
    SettingsCategory {
        id: "language",
        key: "settings.language",
        icon: "chat",
        action: SettingsAction::Language,
    },
    SettingsCategory {
        id: "controls",
        key: "settings.controls",
        icon: "pick",
        action: SettingsAction::Controls,
    },
    SettingsCategory {
        id: "data",
        key: "settings.data",
        icon: "save",
        action: SettingsAction::Data,
    },
    SettingsCategory {
        id: "dev",
        key: "settings.dev",
        icon: "file-text",
        action: SettingsAction::Table,
    },
];

/// `game` table rows (`SettingsMenuDialog.addSettings`, desktop).
pub const GAME_ROWS: &[SettingRow] = &[
    SettingRow::slider("saveinterval", 60, 10, 5 * 120, 10, SettingFormat::Seconds),
    SettingRow::check("communityservers", true),
    SettingRow::check("savecreate", true),
    SettingRow::check("blockreplace", true),
    SettingRow::check("conveyorpathfinding", true),
    SettingRow::check("hints", true),
    SettingRow::check("backgroundpause", true),
    SettingRow::check("buildautopause", false),
    SettingRow::check("distinctcontrolgroups", true),
    SettingRow::check("doubletapmine", false),
    SettingRow::check("commandmodehold", true),
];

/// `graphics` table rows (`SettingsMenuDialog.addSettings`, desktop).
pub const GRAPHICS_ROWS: &[SettingRow] = &[
    SettingRow::slider("uiEdgePadding", 0, 0, 100, 1, SettingFormat::Pixels),
    SettingRow::slider("uiscale", 100, 25, 300, 5, SettingFormat::Percent),
    SettingRow::slider("screenshake", 4, 0, 8, 1, SettingFormat::Multiplier),
    SettingRow::slider("bloomintensity", 6, 0, 16, 1, SettingFormat::BloomPercent),
    SettingRow::slider("bloomblur", 2, 1, 16, 1, SettingFormat::X),
    SettingRow::slider("fpscap", 240, 10, 245, 5, SettingFormat::FpsCap),
    SettingRow::slider("chatopacity", 100, 0, 100, 5, SettingFormat::Percent),
    SettingRow::slider("lasersopacity", 100, 0, 100, 5, SettingFormat::Percent),
    SettingRow::slider("unitlaseropacity", 100, 0, 100, 5, SettingFormat::Percent),
    SettingRow::slider("bridgeopacity", 100, 0, 100, 5, SettingFormat::Percent),
    SettingRow::slider(
        "maxmagnificationmultiplierpercent",
        100,
        100,
        200,
        25,
        SettingFormat::Percent,
    ),
    SettingRow::slider(
        "minmagnificationmultiplierpercent",
        100,
        100,
        300,
        25,
        SettingFormat::Percent,
    ),
    SettingRow::check("vsync", true),
    SettingRow::check("fullscreen", false),
    SettingRow::check("effects", true),
    SettingRow::check("atmosphere", true),
    SettingRow::check("drawlight", true),
    SettingRow::check("destroyedblocks", true),
    SettingRow::check("blockstatus", false),
    SettingRow::check("playerchat", true),
    SettingRow::check("coreitems", true),
    SettingRow::check("minimap", true),
    SettingRow::check("smoothcamera", true),
    SettingRow::check("detach-camera", false),
    SettingRow::check("position", false),
    SettingRow::check("mouseposition", false),
    SettingRow::check("fps", false),
    SettingRow::check("playerindicators", true),
    SettingRow::check("showpings", true),
    SettingRow::check("showotherbuildplans", true),
    SettingRow::check("indicators", true),
    SettingRow::check("showweather", true),
    SettingRow::check("animatedwater", true),
    SettingRow::check("bloom", true),
    SettingRow::check("pixelate", false),
    SettingRow::check("linear", true),
    SettingRow::check("skipcoreanimation", false),
    SettingRow::check("hidedisplays", false),
    SettingRow::check("logiclocalization", true),
];

/// `sound` table rows (`SettingsMenuDialog.addSettings`).
pub const SOUND_ROWS: &[SettingRow] = &[
    SettingRow::check("alwaysmusic", false),
    SettingRow::slider("musicvol", 100, 0, 100, 1, SettingFormat::Percent),
    SettingRow::slider("sfxvol", 100, 0, 100, 1, SettingFormat::Percent),
    SettingRow::slider("ambientvol", 100, 0, 100, 1, SettingFormat::Percent),
];

/// `dev` table rows (`SettingsMenuDialog.addSettings`, non-iOS).
pub const DEV_ROWS: &[SettingRow] = &[
    SettingRow::check("console", false),
    SettingRow::check("drawhitboxes", false),
    SettingRow::check("showperformance", false),
    SettingRow::check("modcrashdisable", true),
];

/// Table rows for a category id (`None` for action-only categories).
pub fn table_rows(id: &str) -> Option<&'static [SettingRow]> {
    match id {
        "game" => Some(GAME_ROWS),
        "graphics" => Some(GRAPHICS_ROWS),
        "sound" => Some(SOUND_ROWS),
        "dev" => Some(DEV_ROWS),
        _ => None,
    }
}

/// Ids of the table categories, in rail order.
pub fn table_ids() -> impl Iterator<Item = &'static str> {
    CATEGORIES
        .iter()
        .filter(|category| category.action == SettingsAction::Table)
        .map(|category| category.id)
}

/// Every row across all tables.
pub fn all_rows() -> impl Iterator<Item = &'static SettingRow> {
    table_ids().filter_map(table_rows).flatten()
}

/// The keys `SettingsTable.rebuild()`'s "Reset to Defaults" button removes.
pub fn reset_keys(id: &str) -> Option<Vec<&'static str>> {
    Some(table_rows(id)?.iter().map(|row| row.key).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_match_upstream_order() {
        let expected = [
            ("game", "settings.game", SettingsAction::Table),
            ("graphics", "settings.graphics", SettingsAction::Table),
            ("sound", "settings.sound", SettingsAction::Table),
            ("language", "settings.language", SettingsAction::Language),
            ("controls", "settings.controls", SettingsAction::Controls),
            ("data", "settings.data", SettingsAction::Data),
            ("dev", "settings.dev", SettingsAction::Table),
        ];
        assert_eq!(CATEGORIES.len(), expected.len());
        for (category, (id, key, action)) in CATEGORIES.iter().zip(expected) {
            assert_eq!(category.id, id);
            assert_eq!(category.key, key);
            assert_eq!(category.action, action);
        }
    }

    #[test]
    fn row_order_and_count_match_upstream() {
        assert_eq!(GAME_ROWS.len(), 11);
        assert_eq!(GRAPHICS_ROWS.len(), 39);
        assert_eq!(SOUND_ROWS.len(), 4);
        assert_eq!(DEV_ROWS.len(), 4);
        assert_eq!(all_rows().count(), 58);

        let game_keys: Vec<_> = GAME_ROWS.iter().map(|row| row.key).collect();
        assert_eq!(
            game_keys,
            [
                "saveinterval",
                "communityservers",
                "savecreate",
                "blockreplace",
                "conveyorpathfinding",
                "hints",
                "backgroundpause",
                "buildautopause",
                "distinctcontrolgroups",
                "doubletapmine",
                "commandmodehold",
            ]
        );

        let graphics_keys: Vec<_> = GRAPHICS_ROWS.iter().map(|row| row.key).collect();
        assert_eq!(
            graphics_keys,
            [
                "uiEdgePadding",
                "uiscale",
                "screenshake",
                "bloomintensity",
                "bloomblur",
                "fpscap",
                "chatopacity",
                "lasersopacity",
                "unitlaseropacity",
                "bridgeopacity",
                "maxmagnificationmultiplierpercent",
                "minmagnificationmultiplierpercent",
                "vsync",
                "fullscreen",
                "effects",
                "atmosphere",
                "drawlight",
                "destroyedblocks",
                "blockstatus",
                "playerchat",
                "coreitems",
                "minimap",
                "smoothcamera",
                "detach-camera",
                "position",
                "mouseposition",
                "fps",
                "playerindicators",
                "showpings",
                "showotherbuildplans",
                "indicators",
                "showweather",
                "animatedwater",
                "bloom",
                "pixelate",
                "linear",
                "skipcoreanimation",
                "hidedisplays",
                "logiclocalization",
            ]
        );

        let sound_keys: Vec<_> = SOUND_ROWS.iter().map(|row| row.key).collect();
        assert_eq!(
            sound_keys,
            ["alwaysmusic", "musicvol", "sfxvol", "ambientvol"]
        );

        let dev_keys: Vec<_> = DEV_ROWS.iter().map(|row| row.key).collect();
        assert_eq!(
            dev_keys,
            [
                "console",
                "drawhitboxes",
                "showperformance",
                "modcrashdisable"
            ]
        );
    }

    #[test]
    fn no_duplicate_keys_across_tables() {
        let mut keys: Vec<&str> = all_rows().map(|row| row.key).collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "settings keys must be unique");
    }

    #[test]
    fn slider_defaults_ranges_and_formats_match_upstream() {
        let ui_edge_padding = &GRAPHICS_ROWS[0];
        assert_eq!(ui_edge_padding.default_i32(), 0);
        assert_eq!(
            ui_edge_padding.kind,
            SettingKind::Slider {
                min: 0,
                max: 100,
                step: 1
            }
        );
        assert_eq!(ui_edge_padding.format, SettingFormat::Pixels);

        let uiscale = &GRAPHICS_ROWS[1];
        assert_eq!(uiscale.default_i32(), 100);
        assert_eq!(
            uiscale.kind,
            SettingKind::Slider {
                min: 25,
                max: 300,
                step: 5
            }
        );
        assert_eq!(uiscale.format, SettingFormat::Percent);

        let screenshake = &GRAPHICS_ROWS[2];
        assert_eq!(screenshake.default_i32(), 4);
        assert_eq!(screenshake.format, SettingFormat::Multiplier);

        let bloomintensity = &GRAPHICS_ROWS[3];
        assert_eq!(bloomintensity.default_i32(), 6);
        assert_eq!(bloomintensity.format, SettingFormat::BloomPercent);

        let bloomblur = &GRAPHICS_ROWS[4];
        assert_eq!(bloomblur.default_i32(), 2);
        assert_eq!(bloomblur.format, SettingFormat::X);

        let fpscap = &GRAPHICS_ROWS[5];
        assert_eq!(fpscap.default_i32(), 240);
        assert_eq!(
            fpscap.kind,
            SettingKind::Slider {
                min: 10,
                max: 245,
                step: 5
            }
        );
        assert_eq!(fpscap.format, SettingFormat::FpsCap);

        let saveinterval = &GAME_ROWS[0];
        assert_eq!(saveinterval.default_i32(), 60);
        assert_eq!(
            saveinterval.kind,
            SettingKind::Slider {
                min: 10,
                max: 600,
                step: 10
            }
        );
        assert_eq!(saveinterval.format, SettingFormat::Seconds);

        assert_eq!(SOUND_ROWS[1].default_i32(), 100);
        assert!(!SOUND_ROWS[0].default_bool());
        assert!(DEV_ROWS[3].default_bool());
    }

    #[test]
    fn table_lookup_and_reset_cover_every_row() {
        for id in table_ids() {
            let rows = table_rows(id).expect("table id resolves");
            let reset = reset_keys(id).expect("reset keys resolve");
            assert_eq!(reset.len(), rows.len());
            for (key, row) in reset.iter().zip(rows) {
                assert_eq!(*key, row.key);
            }
        }
        assert!(table_rows("language").is_none());
        assert!(table_rows("data").is_none());
    }
}
