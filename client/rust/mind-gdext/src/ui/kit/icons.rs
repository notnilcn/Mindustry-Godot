// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Fonts.java (registerIcon/glyph injection),
//         core/src/mindustry/core/UI.java (formatIcons),
//         core/assets-raw/fontgen/config.json (`assets/icons/icon_codes.json`).

//! Icon-font glyph lookup (`MindIcons`).
//!
//! The `icon.ttf` fontello font is keyed by the code points in
//! `assets/icons/icon_codes.json`; a dialog renders a glyph by assigning the
//! character to a label drawn with `MindAssets.icon_font()`. Names are the
//! lowercase aliases used across the UI layer, matching the former
//! `client/ui/theme/mind_icons.gd` table (which this module replaces 1:1).

use godot::classes::{Control, FontFile, Label, RichTextLabel};
use godot::prelude::*;

use super::assets;

/// Lowercase icon alias -> `icon_codes.json` code point.
///
/// The table is the parity ABI: it is the same mapping the GDScript port
/// carried, and `client/tools/` verifies every glyph resolves.
const CODES: &[(&str, u32)] = &[
    ("add", 59411),
    ("admin", 59436),
    ("android", 59461),
    ("arrow-note", 59444),
    ("book", 59483),
    ("book-open", 59393),
    ("box", 59422),
    ("cancel", 59413),
    ("chart-bar", 59417),
    ("chat", 59504),
    ("command-attack", 59502),
    ("command-rally", 59500),
    ("copy", 59508),
    ("crafting", 59440),
    ("defense", 59469),
    ("discord", 59405),
    ("distribution", 59412),
    ("down", 59397),
    ("down-open", 59428),
    ("download", 59513),
    ("edit", 59414),
    ("effect", 59475),
    ("eraser", 61741),
    ("exit", 59487),
    ("export", 59512),
    ("eye", 59534),
    ("eye-off", 59535),
    ("file-code", 61897),
    ("file-text", 61686),
    ("fill", 59468),
    ("filter", 61616),
    ("filters", 59454),
    ("folder", 59421),
    ("github", 62216),
    ("github-square", 62208),
    ("googleplay", 59453),
    ("grid", 61481),
    ("hammer", 59415),
    ("home", 59399),
    ("host", 59485),
    ("image", 59400),
    ("info", 61737),
    ("itchio", 59434),
    ("layers", 59455),
    ("left", 59394),
    ("left-open", 59429),
    ("line", 59435),
    ("link", 59420),
    ("liquid", 59484),
    ("list", 59409),
    ("lock", 59533),
    ("lock-open", 59510),
    ("logic", 59406),
    ("map", 59431),
    ("menu", 59532),
    ("mode-attack", 59493),
    ("mode-pvp", 59489),
    ("mode-survival", 59499),
    ("move", 59416),
    ("music", 59402),
    ("none", 59496),
    ("ok", 59392),
    ("paste", 59474),
    ("pause", 59398),
    ("pencil", 59497),
    ("pick", 59511),
    ("planet", 59443),
    ("play", 59433),
    ("players", 59506),
    ("power", 59408),
    ("production", 59486),
    ("reddit-alien", 62081),
    ("redo", 59446),
    ("refresh", 59498),
    ("resize", 59403),
    ("right", 59395),
    ("rotate", 59427),
    ("right-open", 59450),
    ("save", 59419),
    ("settings", 59516),
    ("spray", 59517),
    ("star", 59401),
    ("steam", 59426),
    ("tag", 59425),
    ("tags", 59432),
    ("terrain", 59492),
    ("trash", 59503),
    ("trello", 61825),
    ("tree", 59509),
    ("turret", 59505),
    ("undo", 59445),
    ("units", 59501),
    ("up", 59396),
    ("up-open", 59430),
    ("upload", 59515),
    ("volume-up", 59424),
    ("warn", 9888),
    ("waves", 59451),
    ("wrench", 59407),
    ("zoom", 59530),
];

/// Java `Iconc` field names stored in `SectorInfo.icon`/schematics tags ->
/// the aliases in [`CODES`].
const JAVA_ALIASES: &[(&str, &str)] = &[
    ("commandAttack", "command-attack"),
    ("commandRally", "command-rally"),
    ("eyeSmall", "eye"),
    ("modePvp", "mode-pvp"),
    ("modeSurvival", "mode-survival"),
    ("warning", "warn"),
];

fn code_for(name: &str) -> Option<u32> {
    if let Some((_, code)) = CODES.iter().find(|(key, _)| *key == name) {
        return Some(*code);
    }
    let alias = JAVA_ALIASES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, alias)| *alias)?;
    CODES
        .iter()
        .find(|(key, _)| *key == alias)
        .map(|(_, code)| *code)
}

/// Glyph string for an icon alias (empty when unknown). Java `Iconc` field
/// names resolve through [`JAVA_ALIASES`].
pub fn glyph(name: &str) -> GString {
    match code_for(name).and_then(char::from_u32) {
        Some(character) => {
            let mut buffer = [0u8; 4];
            GString::from(&*character.encode_utf8(&mut buffer))
        }
        None => GString::new(),
    }
}

/// The glyph code point for an alias (used by headless checks).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn glyph_code(name: &str) -> Option<u32> {
    code_for(name)
}

/// Applies the `MindAssets.icon_font()` to a label (no-op when assets are
/// absent). `Control` so both `Label` (`font`) and `RichTextLabel`
/// (`normal_font`) accept it.
pub fn apply_icon_font(target: &mut Gd<Control>) {
    let Some(assets) = assets() else {
        return;
    };
    let Some(font) = assets.bind().icon_font() else {
        return;
    };
    apply_font(target, &font);
}

/// Applies a font to a control, choosing the `Label` or `RichTextLabel` theme
/// item by node type.
pub fn apply_font(target: &mut Gd<Control>, font: &Gd<FontFile>) {
    if target.clone().try_cast::<RichTextLabel>().is_ok() {
        target.add_theme_font_override("normal_font", font);
    } else if target.clone().try_cast::<Label>().is_ok() {
        target.add_theme_font_override("font", font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_alias_resolves_to_a_scalar() {
        for (name, code) in CODES {
            assert!(
                char::from_u32(*code).is_some(),
                "icon {name} has invalid code point {code}"
            );
            assert_eq!(glyph_code(name), Some(*code));
        }
    }

    #[test]
    fn java_aliases_resolve() {
        assert_eq!(glyph_code("commandAttack"), glyph_code("command-attack"));
        assert_eq!(glyph_code("warning"), glyph_code("warn"));
        assert_eq!(glyph_code("eyeSmall"), glyph_code("eye"));
    }

    #[test]
    fn unknown_alias_is_empty() {
        assert_eq!(glyph("definitely-not-an-icon"), GString::new());
        assert_eq!(glyph_code("definitely-not-an-icon"), None);
    }
}
