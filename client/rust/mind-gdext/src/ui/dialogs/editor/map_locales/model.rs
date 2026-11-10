// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Locale bundle state, status and serialization helpers of the map locales
//! dialog.
//! Upstream: editor/MapLocalesDialog.java

use godot::classes::Button;
use godot::classes::Control;
use godot::classes::Node;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit::styles;
use mind_core::maps::locales::MapLocales;
use mind_core::maps::locales::parse_json;
use mind_core::maps::locales::read_locale;
use mind_core::maps::locales::remove_locale;
use mind_core::maps::locales::write_json;
use mind_core::maps::locales::write_locale;

use super::super::util_e::*;

// ---------------------------------------------------------------------------
// MapLocalesDialog (`map_locales`, `MapLocalesDialog.java`)
// ---------------------------------------------------------------------------

/// Width of one UI property card (`MapLocalesDialog.cardWidth`,
/// `MapLocalesDialog.java:33`).
pub(super) const LOCALE_CARD_WIDTH: f32 = 400.0;
/// `Pal.gray` (`#454545`): the "correct" card frame and the pane rules
/// (`MapLocalesDialog.java:320-327`).
pub(super) const LOCALE_GRAY: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);
/// `Pal.techBlue` (`#8ca9e8`): another locale already has this value.
pub(super) const LOCALE_TECH_BLUE: Color = Color::from_rgba8(0x8c, 0xa9, 0xe8, 0xff);
/// `Color.valueOf("f25555")`: a key rename colliding with an existing key
/// (`MapLocalesDialog.java:239`).
pub(super) const LOCALE_DUPLICATE_COLOR: Color = Color::from_rgba8(0xf2, 0x55, 0x55, 0xff);
/// Value a missing property's `@add` writes before its value area appears
/// (`MapLocalesDialog.java:559`).
pub(super) const LOCALE_PLACEHOLDER_VALUE: &str = "moai";

/// Working bundles, canonical `locales` map-tag JSON
/// (`MapLocalesDialog.locales`).
const LOCALE_STATE_META: &str = "mind_locales_state";
/// The last applied bundles (`MapLocalesDialog.lastSaved`).
const LOCALE_SAVED_STATE_META: &str = "mind_locales_last_saved";
/// `saved`: no edits since the last apply.
pub(super) const LOCALE_SAVED_META: &str = "mind_locales_saved";
/// `selectedLocale`.
pub(super) const LOCALE_SELECTED_META: &str = "mind_locales_selected";
/// `applytoall` (default `true`).
pub(super) const LOCALE_APPLY_ALL_META: &str = "mind_locales_apply_all";
/// `collapsed`: the collapse-all default for newly built cards.
pub(super) const LOCALE_COLLAPSED_META: &str = "mind_locales_collapsed";
/// `searchString`.
pub(super) const LOCALE_SEARCH_META: &str = "mind_locales_search";
/// `searchByValue`.
pub(super) const LOCALE_SEARCH_VALUE_META: &str = "mind_locales_search_value";
/// `showCorrect` / `showMissing` / `showSame` (property filters).
pub(super) const LOCALE_SHOW_CORRECT_META: &str = "mind_locales_show_correct";
pub(super) const LOCALE_SHOW_MISSING_META: &str = "mind_locales_show_missing";
pub(super) const LOCALE_SHOW_SAME_META: &str = "mind_locales_show_same";
/// A pending `@editor.savechanges` answer (`Dialog.closeOnBack`).
pub(super) const LOCALE_PENDING_SAVE_META: &str = "mind_locales_pending_save";
/// The locale a pending `@locales.deletelocale` confirmation targets.
pub(super) const LOCALE_PENDING_DELETE_META: &str = "mind_locales_pending_delete";
/// Signal-binding guards.
pub(super) const LOCALE_CONFIRM_BOUND_META: &str = "mind_locales_confirm_bound";
pub(super) const LOCALE_RESIZE_BOUND_META: &str = "mind_locales_resize_bound";
/// Last column count the resize hook rebuilt for (the `resized(this::buildMain)`
/// equivalent; skipping unchanged counts avoids a rebuild storm).
pub(super) const LOCALE_RESIZE_COLUMNS_META: &str = "mind_locales_resize_columns";
/// The `@editor.apply` button (its disabled state tracks `saved`).
pub(super) const LOCALE_APPLY_NODE: &str = "LocaleApplyButton";
/// One sub-dialog overlay slot (property edit / view / add icon / add locale /
/// locale edit / edit menu). Opening a new one replaces the old, mirroring the
/// Java dialogs that hide the opener before showing the next.
pub(super) const LOCALE_SUB_OVERLAY: &str = "MapLocalesSubOverlay";
/// The filter overlay; stacks over the property view like the Java dialog.
pub(super) const LOCALE_FILTER_OVERLAY: &str = "MapLocalesFilterOverlay";
/// The key field's last committed property key
/// (`MapLocalesDialog.buildMain`'s `propKey[0]`).
pub(super) const LOCALE_FIELD_KEY_META: &str = "mind_locales_field_key";

/// Rebuilds only the card pane (`buildMain`).
pub(super) const LOCALE_REQUEST_MAIN: &str = "main";
/// Rebuilds the locale sidebar and the card pane (`buildTables`).
pub(super) const LOCALE_REQUEST_TABLES: &str = "tables";

/// `PropertyStatus` (`MapLocalesDialog.java:769-773`): status of one property
/// relative to every other locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocaleStatus {
    /// Present in every other locale with a distinct value everywhere.
    Correct,
    /// At least one other locale is missing the key.
    Missing,
    /// At least one other locale holds this exact value.
    Same,
}

/// `MapLocalesDialog.getPropertyStatus` (`MapLocalesDialog.java:329-348`).
///
/// Main mode (`for_view == false`) skips the selected locale and reports
/// `Missing` as soon as another bundle lacks the key; view mode skips the
/// card's locale and only reports `Same`. The first decisive bundle in map
/// order wins, exactly like the Java early returns.
pub(super) fn locale_property_status(
    locales: &MapLocales,
    key: &str,
    value: Option<&str>,
    locale: &str,
    for_view: bool,
) -> LocaleStatus {
    if for_view && value.is_none() {
        return LocaleStatus::Missing;
    }
    for (name, props) in &locales.0 {
        if name == locale {
            continue;
        }
        match props.get(key) {
            None => {
                if !for_view {
                    return LocaleStatus::Missing;
                }
            }
            Some(other) => {
                if Some(other.as_str()) == value {
                    return LocaleStatus::Same;
                }
            }
        }
    }
    LocaleStatus::Correct
}

/// `updateCard`'s status switch (`MapLocalesDialog.java:320-327`).
pub(super) fn locale_status_color(status: LocaleStatus) -> Color {
    match status {
        LocaleStatus::Missing => styles::ACCENT,
        LocaleStatus::Same => LOCALE_TECH_BLUE,
        LocaleStatus::Correct => LOCALE_GRAY,
    }
}

/// `if(status == correct && !showCorrect) continue; ...`
/// (`MapLocalesDialog.java:225-228`).
pub(super) fn locale_status_visible(
    status: LocaleStatus,
    show_correct: bool,
    show_missing: bool,
    show_same: bool,
) -> bool {
    match status {
        LocaleStatus::Correct => show_correct,
        LocaleStatus::Missing => show_missing,
        LocaleStatus::Same => show_same,
    }
}

/// The search comparison (`MapLocalesDialog.java:220-221`): a case-insensitive
/// `contains` over the property name or value.
pub(super) fn locale_matches_search(search: &str, candidate: &str) -> bool {
    search.is_empty() || candidate.to_lowercase().contains(&search.to_lowercase())
}

/// `max(1, (int)((width / Scl.scl() - offset) / cardWidth) - subtract)`.
///
/// `Scl.scl()` is 1 in the port (the UI scale setting is applied by the root
/// viewport), so the viewport width is used directly.
fn locale_columns(width: f32, offset: f32, subtract: i32) -> usize {
    let columns = ((width - offset) / LOCALE_CARD_WIDTH) as i32 - subtract;
    columns.max(1) as usize
}

/// The main pane's column count (`MapLocalesDialog.java:213`).
pub(super) fn locale_main_columns(width: f32) -> usize {
    locale_columns(width, 410.0, 1)
}

/// The property-view column count (`MapLocalesDialog.java:528`).
pub(super) fn locale_view_columns(width: f32) -> usize {
    locale_columns(width, 100.0, 0)
}

/// The icon gallery's column count
/// (`min(20, Core.graphics.getWidth() / Scl.scl(52f))`,
/// `MapLocalesDialog.java:655`). Clamped to at least one (Java would divide by
/// zero on a sub-52 px window).
pub(super) fn locale_icon_columns(width: f32) -> usize {
    ((width / 52.0) as usize).clamp(1, 20)
}

/// The key field's per-character filter (`MapLocalesDialog.java:232`): `=` and
/// `:` are rejected so a key stays a valid `.properties` name.
pub(super) fn locale_key_valid(key: &str) -> bool {
    !key.contains('=') && !key.contains(':')
}

/// `@locales.addtoother` (`MapLocalesDialog.java:384-392`): fill `key` in every
/// bundle that does not have it, never overwriting an existing value.
pub(super) fn locale_add_to_other(locales: &mut MapLocales, key: &str, value: &str) -> usize {
    let mut added = 0;
    for props in locales.0.values_mut() {
        if props.contains_key(key) {
            continue;
        }
        props.insert(key.to_string(), value.to_string());
        added += 1;
    }
    added
}

/// The add-property panel's `@add` (`MapLocalesDialog.java:139-147`): with
/// apply-to-all the value overwrites every locale; otherwise only the selected
/// one is written.
pub(super) fn locale_add_property(
    locales: &mut MapLocales,
    locale: &str,
    key: &str,
    value: &str,
    apply_all: bool,
) {
    if apply_all {
        for props in locales.0.values_mut() {
            props.insert(key.to_string(), value.to_string());
        }
        return;
    }
    if let Some(props) = locales.0.get_mut(locale) {
        props.insert(key.to_string(), value.to_string());
    }
}

/// The card remove button (`MapLocalesDialog.java:249-259`).
pub(super) fn locale_remove_property(
    locales: &mut MapLocales,
    locale: &str,
    key: &str,
    apply_all: bool,
) {
    if apply_all {
        for props in locales.0.values_mut() {
            props.shift_remove(key);
        }
        return;
    }
    if let Some(props) = locales.0.get_mut(locale) {
        props.shift_remove(key);
    }
}

/// The card key rename (`MapLocalesDialog.java:240-249`): apply-to-all renames
/// the key in every bundle that has the old key and lacks the new one;
/// otherwise only the selected bundle is touched.
pub(super) fn locale_rename_key(
    locales: &mut MapLocales,
    locale: &str,
    from: &str,
    to: &str,
    value: &str,
    apply_all: bool,
) -> bool {
    let mut renamed = false;
    if apply_all {
        for props in locales.0.values_mut() {
            if props.contains_key(to) {
                continue;
            }
            let Some(previous) = props.get(from).cloned() else {
                continue;
            };
            props.shift_remove(from);
            props.insert(to.to_string(), previous);
            renamed = true;
        }
        return renamed;
    }
    if let Some(props) = locales.0.get_mut(locale)
        && !props.contains_key(to)
    {
        let previous = props
            .get(from)
            .cloned()
            .unwrap_or_else(|| value.to_string());
        props.shift_remove(from);
        props.insert(to.to_string(), previous);
        renamed = true;
    }
    renamed
}

/// `locales.remove(name)` + the next selection
/// (`MapLocalesDialog.java:190-195`); `None` when no bundle remains.
pub(super) fn locale_remove_locale(locales: &mut MapLocales, locale: &str) -> Option<String> {
    remove_locale(locales, locale);
    locales.0.keys().next().cloned()
}

/// Parses a `locales` tag payload; an empty/invalid payload is an empty map
/// (`MapInfoDialog.java:105` falls back to `new MapLocales()` too).
pub(super) fn locale_parse_or_default(text: &str) -> MapLocales {
    parse_json(text).unwrap_or_default()
}

/// Canonical `locales` tag JSON (`JsonIO.write(locales)`).
fn locale_json(locales: &MapLocales) -> String {
    write_json(locales).unwrap_or_else(|_| String::from("{}"))
}

/// `MapLocalesDialog.java:732-753`'s line splitter (`\r?\n|\r`).
fn locale_lines(text: &str) -> Vec<&str> {
    text.split(['\r', '\n']).collect()
}

/// `readBundles` (`MapLocalesDialog.java:732-753`): `locale:` headers followed
/// by `.properties` lines. The per-bundle text uses the port's canonical
/// `.properties` codec ([`read_locale`]) instead of the Java dialog's
/// `key = value` writer; both are read by the port.
pub(super) fn locale_read_bundles(text: &str) -> MapLocales {
    let mut bundles = MapLocales::new();
    let mut current = String::new();
    let mut body = String::new();
    for line in locale_lines(text) {
        if line.ends_with(':') && !line.contains('=') {
            if !current.is_empty() {
                bundles.0.insert(current.clone(), read_locale(&body));
            }
            current = line[..line.len() - 1].to_string();
            body.clear();
            continue;
        }
        if current.is_empty() {
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    if !current.is_empty() {
        bundles.0.insert(current, read_locale(&body));
    }
    bundles
}

/// `writeBundles` (`MapLocalesDialog.java:708-716`).
pub(super) fn locale_write_bundles(locales: &MapLocales) -> String {
    let mut out = String::new();
    for (locale, props) in &locales.0 {
        out.push_str(locale);
        out.push_str(":\n");
        out.push_str(&write_locale(props));
    }
    out
}

/// `Locale.getDisplayName(Core.bundle.getLocale())`: the port has no locale
/// display-name table reachable from this module (`LanguageDialog`'s table is
/// private to its own batch), so the code itself is shown.
pub(super) fn locale_display_name(locale: &str) -> String {
    locale.to_string()
}

// -- dialog state -----------------------------------------------------------

pub(super) fn locale_state(node: &Gd<MindDialog>) -> MapLocales {
    locale_parse_or_default(&meta_text(node, LOCALE_STATE_META))
}

pub(super) fn locale_saved_state(node: &Gd<MindDialog>) -> MapLocales {
    locale_parse_or_default(&meta_text(node, LOCALE_SAVED_STATE_META))
}

pub(super) fn locale_set_state(node: &Gd<MindDialog>, state: &MapLocales) {
    set_meta_text(&mut node.clone(), LOCALE_STATE_META, &locale_json(state));
}

pub(super) fn locale_set_saved_state(node: &Gd<MindDialog>, state: &MapLocales) {
    set_meta_text(
        &mut node.clone(),
        LOCALE_SAVED_STATE_META,
        &locale_json(state),
    );
}

/// `MapLocales.currentLocale()` when no locale has been selected yet; the live
/// resolved bundle locale is the port's equivalent.
pub(super) fn locale_default_locale() -> String {
    let live = live_locale();
    if live.is_empty() {
        String::from("en")
    } else {
        live
    }
}

pub(super) fn locale_selected(node: &Gd<MindDialog>) -> String {
    let selected = meta_text(node, LOCALE_SELECTED_META);
    if selected.is_empty() {
        locale_default_locale()
    } else {
        selected
    }
}

pub(super) fn locale_show_flags(node: &Gd<MindDialog>) -> (bool, bool, bool) {
    (
        meta_bool(node, LOCALE_SHOW_CORRECT_META),
        meta_bool(node, LOCALE_SHOW_MISSING_META),
        meta_bool(node, LOCALE_SHOW_SAME_META),
    )
}

/// The viewport width the wide layouts are measured against
/// (`Core.graphics.getWidth()`, scale 1).
pub(super) fn locale_viewport_width(node: &Gd<MindDialog>) -> f32 {
    let control: Gd<Control> = node.clone().upcast();
    control.get_viewport_rect().size.x
}

/// Marks the working bundles dirty and disables `@editor.apply` (`saved = false`
/// + the `.disabled(b -> saved)` supplier).
pub(super) fn locale_mark_dirty(node: &Gd<MindDialog>) {
    set_meta_bool(&mut node.clone(), LOCALE_SAVED_META, false);
    locale_sync_apply(node, false);
}

/// `editor.tags.put("locales", JsonIO.write(locales))`
/// (`MapLocalesDialog.java:67,76`), through the `MindEditor` facade.
pub(super) fn locale_write_tags(node: &Gd<MindDialog>) {
    let state = locale_state(node);
    let json = locale_json(&state);
    let _ = editor_write("set_locales_json", &GString::from(json.as_str()));
}

/// `@editor.apply` (`MapLocalesDialog.java:75-80`): write the tag, remember the
/// applied bundles and re-enable the button. `state.mapLocales = locales` has
/// no editor-facade seam in the port (the live world state owns it), so only
/// the map tag is written.
pub(super) fn locale_apply(node: &Gd<MindDialog>) {
    locale_write_tags(node);
    let state = locale_state(node);
    locale_set_saved_state(node, &state);
    set_meta_bool(&mut node.clone(), LOCALE_SAVED_META, true);
    locale_sync_apply(node, true);
}

/// `saved` -> the shared button row's `@editor.apply` disabled flag.
pub(super) fn locale_sync_apply(node: &Gd<MindDialog>, saved: bool) {
    let root: Gd<Node> = node.clone().upcast();
    let path = format!("Center/Panel/Layout/Buttons/{LOCALE_APPLY_NODE}");
    let Some(button) = root.get_node_or_null(&path) else {
        return;
    };
    if let Ok(mut button) = button.try_cast::<Button>() {
        button.set_disabled(saved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::maps::locales::ensure_locale;

    // -- MapLocalesDialog ---------------------------------------------------

    use mind_core::io::StringMap;

    /// `en` has two properties, `ru` only one: the fixture exercises every
    /// status branch (`MapLocalesDialog.getPropertyStatus`).
    fn locale_test_map() -> MapLocales {
        let mut locales = MapLocales::new();
        let mut en = StringMap::new();
        en.insert("foo.name".to_string(), "Foo".to_string());
        en.insert("foo.desc".to_string(), "A thing".to_string());
        let mut ru = StringMap::new();
        ru.insert("foo.name".to_string(), "Фу".to_string());
        locales.0.insert("en".to_string(), en);
        locales.0.insert("ru".to_string(), ru);
        locales
    }

    #[test]
    fn locale_property_status_covers_missing_same_and_correct() {
        let locales = locale_test_map();
        // Present in both bundles with different values -> correct.
        assert_eq!(
            locale_property_status(&locales, "foo.name", Some("Foo"), "en", false),
            LocaleStatus::Correct
        );
        // `foo.desc` is missing from `ru` -> missing (even though `en` has it).
        assert_eq!(
            locale_property_status(&locales, "foo.desc", Some("A thing"), "en", false),
            LocaleStatus::Missing
        );
        let mut same = locale_test_map();
        if let Some(ru) = same.0.get_mut("ru") {
            ru.insert("foo.name".to_string(), "Foo".to_string());
        }
        assert_eq!(
            locale_property_status(&same, "foo.name", Some("Foo"), "en", false),
            LocaleStatus::Same
        );
        // View mode: a null value is missing; an equal peer is same.
        assert_eq!(
            locale_property_status(&locales, "foo.name", None, "ru", true),
            LocaleStatus::Missing
        );
        assert_eq!(
            locale_property_status(&same, "foo.name", Some("Foo"), "ru", true),
            LocaleStatus::Same
        );
        // The only bundle has no peer to disagree with -> correct.
        let mut single = MapLocales::new();
        if let Some(en) = same.0.get("en") {
            single.0.insert("en".to_string(), en.clone());
        }
        assert_eq!(
            locale_property_status(&single, "foo.name", Some("Foo"), "en", false),
            LocaleStatus::Correct
        );
    }

    #[test]
    fn status_filters_control_which_cards_are_shown() {
        assert!(locale_status_visible(
            LocaleStatus::Correct,
            true,
            false,
            false
        ));
        assert!(!locale_status_visible(
            LocaleStatus::Correct,
            false,
            true,
            true
        ));
        assert!(locale_status_visible(
            LocaleStatus::Missing,
            false,
            true,
            false
        ));
        assert!(!locale_status_visible(
            LocaleStatus::Missing,
            true,
            false,
            true
        ));
        assert!(locale_status_visible(
            LocaleStatus::Same,
            false,
            false,
            true
        ));
        assert!(!locale_status_visible(
            LocaleStatus::Same,
            true,
            true,
            false
        ));
    }

    #[test]
    fn locale_payloads_parse_or_fall_back_to_empty() {
        for payload in ["", "{}", "[]", "null", "not json"] {
            assert!(
                locale_parse_or_default(payload).0.is_empty(),
                "payload {payload:?}"
            );
        }
        let parsed = locale_parse_or_default(r#"{"en":{"a":"b"}}"#);
        assert_eq!(parsed.0.len(), 1);
        // `buildTables` inserts the selected locale when the tag omitted it.
        let mut empty = locale_parse_or_default("[]");
        assert!(ensure_locale(&mut empty, "en"));
        let Some(props) = empty.0.get("en") else {
            panic!("the selected locale must be inserted");
        };
        assert!(
            props.is_empty(),
            "the inserted bundle is empty, so the card pane shows @empty"
        );
        assert!(!ensure_locale(&mut empty, "en"));
    }

    #[test]
    fn locale_property_keys_reject_separators_and_detect_duplicates() {
        assert!(locale_key_valid("timer.name"));
        assert!(locale_key_valid("locales.property.name"));
        assert!(!locale_key_valid("a=b"));
        assert!(!locale_key_valid("a:b"));
        let locales = locale_test_map();
        let props = locales.0.get("en");
        assert!(props.is_some_and(|props| props.contains_key("foo.name")));
        assert!(!props.is_some_and(|props| props.contains_key("missing.name")));
    }

    #[test]
    fn apply_to_all_only_fills_missing_keys() {
        let mut locales = locale_test_map();
        // `foo.name` exists in both bundles: nothing is filled.
        assert_eq!(locale_add_to_other(&mut locales, "foo.name", "Other"), 0);
        assert_eq!(locales.0["en"]["foo.name"], "Foo");
        assert_eq!(locales.0["ru"]["foo.name"], "Фу");
        // `bar.name` is absent from both: both get it.
        assert_eq!(locale_add_to_other(&mut locales, "bar.name", "Bar"), 2);
        assert_eq!(locales.0["en"]["bar.name"], "Bar");
        assert_eq!(locales.0["ru"]["bar.name"], "Bar");
        // Running it again is a no-op.
        assert_eq!(locale_add_to_other(&mut locales, "bar.name", "Bar"), 0);
    }

    #[test]
    fn add_property_apply_all_overwrites_every_locale() {
        let mut all = locale_test_map();
        locale_add_property(&mut all, "en", "foo.name", "Changed", true);
        assert_eq!(all.0["en"]["foo.name"], "Changed");
        assert_eq!(all.0["ru"]["foo.name"], "Changed");
        // Without apply-to-all only the selected bundle changes.
        let mut single = locale_test_map();
        locale_add_property(&mut single, "ru", "foo.name", "Только", false);
        assert_eq!(single.0["ru"]["foo.name"], "Только");
        assert_eq!(single.0["en"]["foo.name"], "Foo");
    }

    #[test]
    fn search_matches_names_or_values_case_insensitively() {
        assert!(locale_matches_search("", "anything"));
        assert!(locale_matches_search("TIMER", "timer.name"));
        assert!(locale_matches_search("timer", "TIMER.NAME"));
        assert!(!locale_matches_search("timer", "foo.name"));
        // `searchByValue` picks the value string instead of the key.
        assert!(locale_matches_search("left", "Time left: 10"));
        assert!(!locale_matches_search("left", "timer.name"));
    }

    #[test]
    fn card_column_counts_never_drop_below_one() {
        for width in [0.0, 100.0, 410.0, 800.0] {
            assert_eq!(locale_main_columns(width), 1, "main card pane at {width}");
            assert_eq!(locale_view_columns(width), 1, "property view at {width}");
        }
        // `max(1, (int)((width - 410) / 400) - 1)` / `max(1, (int)((width -
        // 100) / 400))` (`MapLocalesDialog.java:213,528`).
        assert_eq!(locale_main_columns(1152.0), 1);
        assert_eq!(locale_view_columns(1152.0), 2);
        assert_eq!(locale_main_columns(1920.0), 2);
        assert_eq!(locale_view_columns(1920.0), 4);
        for width in [0.0, 52.0, 1000.0, 4000.0] {
            assert!((1..=20).contains(&locale_icon_columns(width)), "{width}");
        }
    }

    #[test]
    fn bundle_text_round_trips_through_the_clipboard_format() {
        let locales = locale_test_map();
        let text = locale_write_bundles(&locales);
        assert!(text.contains("en:\n"), "{text}");
        assert!(text.contains("ru:\n"), "{text}");
        assert_eq!(locale_read_bundles(&text), locales);
        // Java's `key = value` text and CRLF line endings are still read by
        // the shared codec.
        let java_style = "en:\nfoo.name = Foo\n";
        let parsed = locale_read_bundles(java_style);
        assert_eq!(parsed.0["en"]["foo.name"], "Foo");
        let crlf = "en:\r\nfoo.name = Foo\r\n";
        let parsed = locale_read_bundles(crlf);
        assert_eq!(parsed.0["en"]["foo.name"], "Foo");
        // Newlines survive the `.properties` escape round trip.
        let mut tricky = MapLocales::new();
        let mut props = StringMap::new();
        props.insert("multi.line".to_string(), "line1\nline2".to_string());
        tricky.0.insert("en".to_string(), props);
        assert_eq!(locale_read_bundles(&locale_write_bundles(&tricky)), tricky);
    }

    #[test]
    fn rename_and_remove_follow_apply_to_all() {
        let mut locales = locale_test_map();
        assert!(locale_rename_key(
            &mut locales,
            "en",
            "foo.name",
            "foo.title",
            "Foo",
            true
        ));
        assert_eq!(locales.0["en"].get("foo.title"), Some(&"Foo".to_string()));
        // Apply-to-all keeps each bundle's own value under the new name.
        assert_eq!(locales.0["ru"].get("foo.title"), Some(&"Фу".to_string()));
        assert!(!locales.0["ru"].contains_key("foo.name"));
        // Without apply-to-all only the selected bundle renames.
        let mut single = locale_test_map();
        assert!(locale_rename_key(
            &mut single,
            "en",
            "foo.name",
            "foo.title",
            "Foo",
            false
        ));
        assert_eq!(single.0["en"].get("foo.title"), Some(&"Foo".to_string()));
        assert!(single.0["ru"].contains_key("foo.name"));
        // The current bundle already has the target key -> rejected.
        let mut blocked = locale_test_map();
        assert!(!locale_rename_key(
            &mut blocked,
            "en",
            "foo.name",
            "foo.desc",
            "Foo",
            false
        ));
        assert!(blocked.0["en"].contains_key("foo.name"));
        // Remove with apply-to-all drops the key from every bundle.
        let mut removed = locale_test_map();
        locale_remove_property(&mut removed, "en", "foo.name", true);
        assert!(!removed.0["en"].contains_key("foo.name"));
        assert!(!removed.0["ru"].contains_key("foo.name"));
    }

    #[test]
    fn deleting_a_locale_selects_the_first_remaining_bundle() {
        let mut locales = locale_test_map();
        assert_eq!(
            locale_remove_locale(&mut locales, "en").as_deref(),
            Some("ru")
        );
        assert!(!locales.0.contains_key("en"));
        assert_eq!(locale_remove_locale(&mut locales, "ru"), None);
        assert!(locales.0.is_empty());
    }
}
