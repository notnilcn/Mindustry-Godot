// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapLocales` editor half (plan 19 §3.9/§6.3; `type/MapLocales.java`).
//!
//! The persisted shape (`IndexMap<String, IndexMap<String, String>>`) already
//! lives in plan 04's [`crate::io::json::rules::MapLocales`]; 19 owns the editor
//! helpers: property status coloring, `.properties` bundle text read/write with
//! Java escaping, and the [`MapLocaleView`] adapter plan 12 consumes through
//! `ObjectiveLocale`/`LocaleView`.

use indexmap::IndexMap;

use crate::assets::bundle::parse_properties;
use crate::io::StringMap;

pub use crate::io::json::rules::MapLocales;

/// Status of one map-locale property relative to the base bundle + others
/// (`MapLocalesDialog` card coloring).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyStatus {
    /// Present only in the current locale (no base/other author).
    Unique,
    /// Differs from the base bundle.
    Modified,
    /// The same key exists with a different value in another map locale.
    Conflict,
    /// The current locale lacks the key that the base bundle has.
    Missing,
    /// Identical to the base bundle.
    Same,
}

/// Computes the status of `key` in `locale`.
///
/// `base` is the runtime bundle (plan 03 `BundleStack`) and `others` is every
/// other map-locale map; a disagreement is a `Conflict`.
pub fn property_status(
    locales: &MapLocales,
    locale: &str,
    key: &str,
    base: &StringMap,
    others: &[&StringMap],
) -> PropertyStatus {
    let value = locales.0.get(locale).and_then(|map| map.get(key));
    let other_conflict = others
        .iter()
        .any(|map| map.get(key).is_some_and(|other| Some(other) != value));
    match value {
        None => {
            if base.contains_key(key) {
                PropertyStatus::Missing
            } else if other_conflict {
                PropertyStatus::Conflict
            } else {
                PropertyStatus::Unique
            }
        }
        Some(value) => {
            if other_conflict {
                PropertyStatus::Conflict
            } else if base.get(key).is_some_and(|base| base == value) {
                PropertyStatus::Same
            } else {
                PropertyStatus::Modified
            }
        }
    }
}

/// Parses a `.properties` bundle text (`MapLocalesDialog` paste / `readLocale`).
pub fn read_locale(text: &str) -> StringMap {
    parse_properties(text)
}

/// Serializes a bundle map to `.properties` text with Java escaping
/// (`MapLocalesDialog` copy / `writeLocale`).
pub fn write_locale(map: &StringMap) -> String {
    let mut out = String::new();
    for (key, value) in map {
        out.push_str(&escape_property(key));
        out.push('=');
        out.push_str(&escape_property(value));
        out.push('\n');
    }
    out
}

/// Escapes a `.properties` key/value (`java.util.Properties.store` subset).
fn escape_property(input: &str) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '=' | ':' => {
                out.push('\\');
                out.push(ch);
            }
            '#' | '!' if index == 0 => {
                out.push('\\');
                out.push(ch);
            }
            ' ' if index == 0 => out.push_str("\\ "),
            _ => out.push(ch),
        }
    }
    out
}

/// Parses a `locales` map-tag JSON string; an empty/`{}` tag is an empty map.
pub fn parse_json(json: &str) -> Result<MapLocales, crate::io::IoError> {
    if json.trim().is_empty() {
        return Ok(MapLocales::new());
    }
    crate::io::json::JsonIo::read::<MapLocales>(json)
}

/// Serializes a `locales` map-tag JSON string.
pub fn write_json(locales: &MapLocales) -> Result<String, crate::io::IoError> {
    crate::io::json::JsonIo::write(locales)
}

/// The map-locale adapter plan 12 reads objective/marker text through
/// (`type/MapLocales.java` `getProperty`/`getFormatted`).
#[derive(Debug, Clone, Copy)]
pub struct MapLocaleView<'a> {
    /// The map locales being viewed.
    pub locales: &'a MapLocales,
    /// Active locale (bundle key).
    pub locale: &'a str,
}

impl<'a> MapLocaleView<'a> {
    /// Wraps map locales and a locale key.
    pub fn new(locales: &'a MapLocales, locale: &'a str) -> Self {
        Self { locales, locale }
    }

    /// `MapLocales.containsProperty`.
    pub fn contains(&self, key: &str) -> bool {
        self.locales.contains(self.locale, key) || self.locales.contains("en", key)
    }

    /// `MapLocales.getProperty(key)`.
    pub fn property(&self, key: &str) -> Option<String> {
        if self.contains(key) {
            Some(self.locales.get_property(self.locale, key))
        } else {
            None
        }
    }

    /// `MapLocales.getFormatted(key, args)`.
    pub fn formatted(&self, key: &str, args: &[&str]) -> String {
        self.locales.get_formatted(self.locale, key, args)
    }
}

impl crate::game::map_objectives::ObjectiveLocale for MapLocaleView<'_> {
    fn fetch_text(&self, text: &str) -> String {
        match text.strip_prefix('@') {
            Some(key) => self.property(key).unwrap_or_else(|| key.to_owned()),
            None => text.to_owned(),
        }
    }

    fn format(&self, key: &str, args: &[&str]) -> String {
        let key = key.strip_prefix('@').unwrap_or(key);
        if self.contains(key) {
            self.formatted(key, args)
        } else {
            crate::game::map_objectives::NoLocale.format(key, args)
        }
    }

    fn get(&self, key: &str) -> String {
        self.property(key).unwrap_or_else(|| key.to_owned())
    }

    fn map_locale(&self, key: &str) -> Option<String> {
        self.property(key)
    }
}

/// Adds a locale (defaulting from `SettingsStore.locale`) if absent.
pub fn ensure_locale(locales: &mut MapLocales, locale: &str) -> bool {
    if locales.0.contains_key(locale) {
        return false;
    }
    locales.0.insert(locale.to_owned(), IndexMap::new());
    true
}

/// Removes a locale; returns whether it existed.
pub fn remove_locale(locales: &mut MapLocales, locale: &str) -> bool {
    locales.0.shift_remove(locale).is_some()
}

/// Copies every key from `source` into `target` without overwriting
/// (`locales.applytoall`).
pub fn apply_to_all(locales: &mut MapLocales, source_locale: &str) -> usize {
    let Some(source) = locales.0.get(source_locale).cloned() else {
        return 0;
    };
    let mut applied = 0;
    let locales_count = locales.0.len();
    for index in 0..locales_count {
        let key = locales.0.get_index(index).map(|(key, _)| key.clone());
        let Some(key) = key else { continue };
        if key == source_locale {
            continue;
        }
        if let Some(map) = locales.0.get_mut(&key) {
            for (k, v) in &source {
                if !map.contains_key(k) {
                    map.insert(k.clone(), v.clone());
                    applied += 1;
                }
            }
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MapLocales {
        let mut locales = MapLocales::new();
        let mut en = IndexMap::new();
        en.insert("foo.name".to_owned(), "Foo".to_owned());
        en.insert("foo.desc".to_owned(), "A thing".to_owned());
        let mut ru = IndexMap::new();
        ru.insert("foo.name".to_owned(), "Фу".to_owned());
        locales.0.insert("en".to_owned(), en);
        locales.0.insert("ru".to_owned(), ru);
        locales
    }

    #[test]
    fn json_round_trip_and_statuses() {
        let locales = sample();
        let json = write_json(&locales).unwrap();
        let parsed = parse_json(&json).unwrap();
        assert_eq!(locales, parsed);

        let mut base = IndexMap::new();
        base.insert("foo.name".to_owned(), "Foo".to_owned());
        base.insert("bar.name".to_owned(), "Bar".to_owned());
        let en = &locales.0["en"];
        assert_eq!(
            property_status(&locales, "ru", "foo.name", &base, &[en]),
            PropertyStatus::Conflict
        );
        assert_eq!(
            property_status(&locales, "ru", "bar.name", &base, &[en]),
            PropertyStatus::Missing
        );
        // Identical to the base bundle and with no conflicting peer.
        assert_eq!(
            property_status(&locales, "en", "foo.name", &base, &[]),
            PropertyStatus::Same
        );
        // A value that differs from the base but has no peer conflict.
        assert_eq!(
            property_status(&locales, "ru", "foo.name", &base, &[]),
            PropertyStatus::Modified
        );
    }

    #[test]
    fn properties_read_write_and_escape() {
        let mut map = IndexMap::new();
        map.insert("a.key".to_owned(), "line1\nline2".to_owned());
        map.insert("hash".to_owned(), "#not-a-comment".to_owned());
        let text = write_locale(&map);
        let parsed = read_locale(&text);
        assert_eq!(parsed["a.key"], "line1\nline2");
        assert_eq!(parsed["hash"], "#not-a-comment");
    }

    #[test]
    fn apply_to_all_fills_missing_keys() {
        let mut locales = sample();
        let applied = apply_to_all(&mut locales, "en");
        assert_eq!(applied, 1, "only foo.desc missing in ru");
        assert_eq!(locales.0["ru"]["foo.desc"], "A thing");
        // Running again is a no-op.
        assert_eq!(apply_to_all(&mut locales, "en"), 0);
    }

    #[test]
    fn locale_view_resolves_objective_text() {
        use crate::game::map_objectives::ObjectiveLocale;
        let locales = sample();
        let view = MapLocaleView::new(&locales, "ru");
        assert_eq!(view.get("foo.name"), "Фу");
        assert_eq!(view.fetch_text("@foo.desc"), "A thing");
        assert_eq!(view.map_locale("foo.name").as_deref(), Some("Фу"));
        assert_eq!(
            view.format("objective.item", &["copper", "5"]),
            "objective.item copper 5"
        );
    }
}
