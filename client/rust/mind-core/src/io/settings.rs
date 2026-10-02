// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Settings persistence (plan 04 §3.7/§6.5).
//!
//! Ported from Arc `Settings` (`Core.settings`): a typed key/value store with
//! string coercion getters, JSON values, `defaults`, atomic flush and the
//! upstream key names used by `Saves.java` (`save-<n>-name`,
//! `save-<n>-autosave`, `saveinterval`, `last-sector-save`, ...). Disk format
//! is native (plan 04 §6.5): magic `MGST`, `u16` version, `u16` count, typed
//! entries; writes are atomic (tmp + sync + rename) and a corrupt file falls
//! back to an empty store (defaults) instead of failing.
//!
//! Settings are platform state, not sim state: the debounce clock
//! (`std::time::Instant`) never touches the deterministic sim (HLP §2.4).

use std::time::{Duration, Instant};

use indexmap::IndexMap;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::IoError;
use super::fs::{FileSystem, Paths};
use super::wire::{WireReader, WireWriter};

/// Settings file magic (plan 04 §6.5).
pub const SETTINGS_MAGIC: [u8; 4] = *b"MGST";

/// Settings file format version.
pub const SETTINGS_VERSION: u16 = 1;

/// Cap on one `bytes` value (untrusted-input guard).
pub const MAX_SETTINGS_BYTES: usize = 4 * 1024 * 1024;

/// Default debounce before a dirty store flushes itself (plan 04 §3.7).
pub const DEFAULT_FLUSH_DEBOUNCE: Duration = Duration::from_secs(1);

/// `Saves.SaveSlot.getName/setName` key for a slot index (`save-<n>-name`).
pub fn slot_name_key(index: &str) -> String {
    format!("save-{index}-name")
}

/// `Saves.SaveSlot.isAutosave/setAutosave` key (`save-<n>-autosave`).
pub fn slot_autosave_key(index: &str) -> String {
    format!("save-{index}-autosave")
}

/// Player color key for slot `n` (`color-<n>`, `InputHandler`/`Vars` usage).
pub fn color_key(n: u32) -> String {
    format!("color-{n}")
}

/// Autosave interval (minutes) key (`Saves.update`).
pub const KEY_SAVE_INTERVAL: &str = "saveinterval";
/// Last sector save name key (`Saves.saveSector` / `Saves.load`).
pub const KEY_LAST_SECTOR_SAVE: &str = "last-sector-save";
/// UI scale key (plan 14).
pub const KEY_UI_SCALE: &str = "uiscale";
/// Locale key.
pub const KEY_LOCALE: &str = "locale";
/// Player name key.
pub const KEY_PLAYER_NAME: &str = "name";

/// One typed settings value (plan 04 §6.5 type tags).
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    /// UTF-8 string (type 0).
    Str(String),
    /// `i32` (type 1).
    Int(i32),
    /// `i64` (type 2).
    Long(i64),
    /// `f32` (type 3).
    Float(f32),
    /// Boolean (type 4).
    Bool(bool),
    /// Raw bytes (type 5).
    Bytes(Vec<u8>),
    /// JSON-serialized value (type 6).
    Json(String),
}

impl SettingValue {
    const fn type_tag(&self) -> u8 {
        match self {
            SettingValue::Str(_) => 0,
            SettingValue::Int(_) => 1,
            SettingValue::Long(_) => 2,
            SettingValue::Float(_) => 3,
            SettingValue::Bool(_) => 4,
            SettingValue::Bytes(_) => 5,
            SettingValue::Json(_) => 6,
        }
    }

    /// `String.valueOf` coercion used by the Arc getters.
    fn as_string(&self) -> Option<String> {
        match self {
            SettingValue::Str(value) => Some(value.clone()),
            SettingValue::Int(value) => Some(value.to_string()),
            SettingValue::Long(value) => Some(value.to_string()),
            SettingValue::Float(value) => Some(value.to_string()),
            SettingValue::Bool(value) => Some(value.to_string()),
            SettingValue::Json(value) => Some(value.clone()),
            // Java would print the array identity; a default is strictly saner.
            SettingValue::Bytes(_) => None,
        }
    }
}

/// Arc-like typed key/value settings store (`Core.settings`).
///
/// No global statics (plan 04 §3.7): the store is owned by the plan-00 app
/// state and referenced by plans via a handle.
#[derive(Debug)]
pub struct SettingsStore {
    map: IndexMap<String, SettingValue>,
    dirty: bool,
    last_mutation: Option<Instant>,
    debounce: Duration,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsStore {
    /// Empty store.
    pub fn new() -> Self {
        Self {
            map: IndexMap::new(),
            dirty: false,
            last_mutation: None,
            debounce: DEFAULT_FLUSH_DEBOUNCE,
        }
    }

    /// Loads `config/settings.bin`; a missing file yields an empty store and a
    /// corrupt file logs a warning and falls back to defaults (never fatal,
    /// plan 04 §7b).
    pub fn load(fs: &dyn FileSystem, paths: &Paths) -> Self {
        let file = paths.settings_file();
        let mut store = Self::new();
        let Ok(bytes) = fs.read(&file) else {
            return store;
        };
        match read_settings(&bytes) {
            Ok(map) => {
                store.map = map;
            }
            Err(error) => {
                log::warn!(
                    "settings file `{}` is corrupt ({error}); falling back to defaults",
                    file.display()
                );
            }
        }
        store
    }

    /// Overrides the flush debounce (tests).
    pub fn set_debounce(&mut self, debounce: Duration) {
        self.debounce = debounce;
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Whether the key exists (`Settings.has`).
    pub fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Removes a key (`Settings.remove`), returning whether it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        let removed = self.map.shift_remove(key).is_some();
        if removed {
            self.mark_dirty();
        }
        removed
    }

    /// Removes everything (`Settings.clear`).
    pub fn clear(&mut self) {
        if !self.map.is_empty() {
            self.map.clear();
            self.mark_dirty();
        }
    }

    /// Ordered keys (deterministic iteration for tooling).
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }

    /// Raw access to one typed value.
    pub fn get(&self, key: &str) -> Option<&SettingValue> {
        self.map.get(key)
    }

    /// `Settings.getString`: string coercion of any stored type.
    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.map
            .get(key)
            .and_then(SettingValue::as_string)
            .unwrap_or_else(|| default.to_owned())
    }

    /// `Settings.getInt`: `Int`, else string-parse, else default.
    pub fn get_i32(&self, key: &str, default: i32) -> i32 {
        match self.map.get(key) {
            Some(SettingValue::Int(value)) => *value,
            other => other
                .and_then(SettingValue::as_string)
                .and_then(|text| text.parse().ok())
                .unwrap_or(default),
        }
    }

    /// `Settings.getLong`: `Long`, else string-parse, else default.
    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        match self.map.get(key) {
            Some(SettingValue::Long(value)) => *value,
            other => other
                .and_then(SettingValue::as_string)
                .and_then(|text| text.parse().ok())
                .unwrap_or(default),
        }
    }

    /// `Settings.getFloat`: `Float`, else string-parse, else default.
    pub fn get_f32(&self, key: &str, default: f32) -> f32 {
        match self.map.get(key) {
            Some(SettingValue::Float(value)) => *value,
            other => other
                .and_then(SettingValue::as_string)
                .and_then(|text| text.parse().ok())
                .unwrap_or(default),
        }
    }

    /// `Settings.getBool`: `Bool`, else string-equals-`true`, else default.
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        match self.map.get(key) {
            Some(SettingValue::Bool(value)) => *value,
            other => other
                .and_then(SettingValue::as_string)
                .map(|text| text == "true")
                .unwrap_or(default),
        }
    }

    /// `Settings.get(key, byte[])` equivalent.
    pub fn get_bytes(&self, key: &str) -> Option<&[u8]> {
        match self.map.get(key) {
            Some(SettingValue::Bytes(value)) => Some(value),
            _ => None,
        }
    }

    /// `Settings.getJson`: parsed JSON value, `None` when absent. A corrupt
    /// JSON payload is an error (untrusted-input rule), not a panic.
    pub fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, IoError> {
        match self.map.get(key) {
            Some(SettingValue::Json(raw)) => Ok(Some(serde_json::from_str(raw)?)),
            Some(other) => Err(IoError::corrupt(format!(
                "settings key `{key}` holds {:?}, not JSON",
                other.type_tag()
            ))),
            None => Ok(None),
        }
    }

    /// `Settings.getJson(key, type, supplier)`: parsed value or the supplied
    /// default when absent *or* corrupt (upstream swallows parse failures).
    pub fn get_json_or<T: DeserializeOwned>(&self, key: &str, default: impl FnOnce() -> T) -> T {
        self.get_json(key).ok().flatten().unwrap_or_else(default)
    }

    /// `Settings.put(String)`.
    pub fn put_string(&mut self, key: &str, value: &str) {
        self.put(key, SettingValue::Str(value.to_owned()));
    }

    /// `Settings.put(int)`.
    pub fn put_i32(&mut self, key: &str, value: i32) {
        self.put(key, SettingValue::Int(value));
    }

    /// `Settings.put(long)`.
    pub fn put_i64(&mut self, key: &str, value: i64) {
        self.put(key, SettingValue::Long(value));
    }

    /// `Settings.put(float)`.
    pub fn put_f32(&mut self, key: &str, value: f32) {
        self.put(key, SettingValue::Float(value));
    }

    /// `Settings.put(boolean)`.
    pub fn put_bool(&mut self, key: &str, value: bool) {
        self.put(key, SettingValue::Bool(value));
    }

    /// `Settings.put(byte[])`.
    pub fn put_bytes(&mut self, key: &str, value: &[u8]) {
        self.put(key, SettingValue::Bytes(value.to_vec()));
    }

    /// `Settings.putJson`.
    pub fn put_json<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<(), IoError> {
        self.put(key, SettingValue::Json(serde_json::to_string(value)?));
        Ok(())
    }

    /// Inserts a typed value and marks the store dirty.
    pub fn put(&mut self, key: &str, value: SettingValue) {
        self.map.insert(key.to_owned(), value);
        self.mark_dirty();
    }

    /// `Settings.toggle`: flips a boolean key, returning the new value.
    pub fn toggle(&mut self, key: &str, default: bool) -> bool {
        let next = !self.get_bool(key, default);
        self.put_bool(key, next);
        next
    }

    /// `Settings.defaults`: sets entries only for keys that are absent
    /// (never overwrites current values).
    pub fn defaults(&mut self, entries: &[(&str, SettingValue)]) {
        for (key, value) in entries {
            if !self.map.contains_key(*key) {
                self.map.insert((*key).to_owned(), value.clone());
            }
        }
    }

    /// Whether unsaved mutations exist.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Serializes and atomically writes `config/settings.bin`
    /// (`Settings.save`): tmp + sync + rename.
    pub fn force_save(&mut self, fs: &dyn FileSystem, paths: &Paths) -> Result<(), IoError> {
        let bytes = write_settings(&self.map)?;
        fs.write_atomic(&paths.settings_file(), &bytes)?;
        self.dirty = false;
        self.last_mutation = None;
        Ok(())
    }

    /// Debounced flush: saves when the store has been dirty for at least the
    /// debounce window (default 1 s, plan 04 §3.7). Returns whether it saved.
    pub fn flush_if_dirty(&mut self, fs: &dyn FileSystem, paths: &Paths) -> Result<bool, IoError> {
        if !self.dirty {
            return Ok(false);
        }
        let due = self
            .last_mutation
            .map(|at| at.elapsed() >= self.debounce)
            .unwrap_or(true);
        if due {
            self.force_save(fs, paths)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_mutation = Some(Instant::now());
    }
}

/// Serializes the store (plan 04 §6.5): magic, `u16` version, `u16` count,
/// then `(key, type, payload)` entries.
pub fn write_settings(map: &IndexMap<String, SettingValue>) -> Result<Vec<u8>, IoError> {
    if map.len() > u16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: u16::MAX as usize,
            actual: map.len(),
        });
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(&SETTINGS_MAGIC);
    buf.extend_from_slice(&SETTINGS_VERSION.to_be_bytes());
    buf.extend_from_slice(&(map.len() as u16).to_be_bytes());
    let mut wire = WireWriter::new(&mut buf);
    for (key, value) in map {
        wire.str(key)?;
        wire.ub(value.type_tag());
        match value {
            SettingValue::Str(text) | SettingValue::Json(text) => wire.str(text)?,
            SettingValue::Int(v) => wire.i(*v),
            SettingValue::Long(v) => wire.l(*v),
            SettingValue::Float(v) => wire.f(*v),
            SettingValue::Bool(v) => wire.bool(*v),
            SettingValue::Bytes(bytes) => {
                if bytes.len() > MAX_SETTINGS_BYTES {
                    return Err(IoError::TooLarge {
                        limit: MAX_SETTINGS_BYTES,
                        actual: bytes.len(),
                    });
                }
                wire.u(bytes.len() as u32);
                wire.bytes(bytes);
            }
        }
    }
    Ok(buf)
}

/// Parses a settings file; any structural error makes the whole file invalid
/// (the caller falls back to defaults).
pub fn read_settings(bytes: &[u8]) -> Result<IndexMap<String, SettingValue>, IoError> {
    if bytes.len() < 8 {
        return Err(IoError::UnexpectedEof);
    }
    if bytes[..4] != SETTINGS_MAGIC {
        return Err(IoError::corrupt("bad settings magic"));
    }
    let version = u16::from_be_bytes([bytes[4], bytes[5]]);
    if version != SETTINGS_VERSION {
        return Err(IoError::corrupt(format!(
            "unsupported settings version: {version}"
        )));
    }
    let count = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;
    let mut wire = WireReader::new(&bytes[8..]);
    let mut map = IndexMap::with_capacity(count);
    for _ in 0..count {
        let key = wire.str()?;
        let type_tag = wire.ub()?;
        let value = match type_tag {
            0 => SettingValue::Str(wire.str()?),
            1 => SettingValue::Int(wire.i()?),
            2 => SettingValue::Long(wire.l()?),
            3 => SettingValue::Float(wire.f()?),
            4 => SettingValue::Bool(wire.bool()?),
            5 => {
                let len = wire.u()? as usize;
                if len > MAX_SETTINGS_BYTES {
                    return Err(IoError::TooLarge {
                        limit: MAX_SETTINGS_BYTES,
                        actual: len,
                    });
                }
                SettingValue::Bytes(wire.bytes(len)?.to_vec())
            }
            6 => SettingValue::Json(wire.str()?),
            other => {
                return Err(IoError::corrupt(format!(
                    "unknown settings value type: {other}"
                )));
            }
        };
        map.insert(key, value);
    }
    if wire.remaining() != 0 {
        return Err(IoError::corrupt(format!(
            "trailing {} byte(s) after settings entries",
            wire.remaining()
        )));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::fs::MockFs;

    fn test_paths() -> Paths {
        Paths::new("/data")
    }

    #[test]
    fn typed_roundtrip_and_coercions() {
        let mut store = SettingsStore::new();
        store.put_string("name", "alice");
        store.put_i32("uiscale", 150);
        store.put_i64("big", 9_000_000_000);
        store.put_f32("volume", 0.5);
        store.put_bool("vsync", true);
        store.put_bytes("blob", &[1, 2, 3]);
        store.put_json("pos", &vec![1i32, 2, 3]).unwrap();

        assert_eq!(store.get_string("name", "?"), "alice");
        assert_eq!(store.get_i32("uiscale", 100), 150);
        assert_eq!(store.get_i64("big", 0), 9_000_000_000);
        assert_eq!(store.get_f32("volume", 1.0), 0.5);
        assert!(store.get_bool("vsync", false));
        assert_eq!(store.get_bytes("blob"), Some(&[1u8, 2, 3][..]));
        assert_eq!(
            store.get_json::<Vec<i32>>("pos").unwrap(),
            Some(vec![1, 2, 3])
        );

        // Arc-style coercions.
        store.put_string("number-as-string", "42");
        assert_eq!(store.get_i32("number-as-string", 0), 42);
        store.put_i32("string-as-number", 7);
        assert_eq!(store.get_string("string-as-number", "?"), "7");
        store.put_string("bool-as-string", "true");
        assert!(store.get_bool("bool-as-string", false));
        store.put_string("junk", "not-a-number");
        assert_eq!(store.get_i32("junk", -1), -1);

        // Missing keys → defaults.
        assert_eq!(store.get_string("missing", "d"), "d");
        assert_eq!(store.get_i32("missing", 3), 3);
        assert!(!store.get_bool("missing", false));

        // toggle + has + remove + defaults.
        assert!(store.toggle("feature", false));
        assert!(!store.toggle("feature", false));
        assert!(store.has("feature"));
        assert!(store.remove("feature"));
        assert!(!store.has("feature"));

        store.defaults(&[
            ("name", SettingValue::Str("default-name".to_owned())),
            ("fresh", SettingValue::Int(5)),
        ]);
        // defaults never overwrite.
        assert_eq!(store.get_string("name", "?"), "alice");
        assert_eq!(store.get_i32("fresh", 0), 5);
    }

    #[test]
    fn flush_reload_equality() {
        let fs = MockFs::new();
        let paths = test_paths();
        let mut store = SettingsStore::new();
        store.put_string(&slot_name_key("0"), "base");
        store.put_bool(&slot_autosave_key("0"), false);
        store.put_i32(KEY_SAVE_INTERVAL, 10);
        store.put_string(KEY_LAST_SECTOR_SAVE, "sector-serpulo-12");
        store
            .put_json("controlGroups", &vec![vec![1i32, 2], vec![3]])
            .unwrap();

        assert!(store.is_dirty());
        store.force_save(&fs, &paths).unwrap();
        assert!(!store.is_dirty());

        let reloaded = SettingsStore::load(&fs, &paths);
        assert_eq!(reloaded.get_string(&slot_name_key("0"), "?"), "base");
        assert!(!reloaded.get_bool(&slot_autosave_key("0"), true));
        assert_eq!(reloaded.get_i32(KEY_SAVE_INTERVAL, 2), 10);
        assert_eq!(
            reloaded.get_string(KEY_LAST_SECTOR_SAVE, "<none>"),
            "sector-serpulo-12"
        );
        assert_eq!(
            reloaded.get_json::<Vec<Vec<i32>>>("controlGroups").unwrap(),
            Some(vec![vec![1, 2], vec![3]])
        );
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let fs = MockFs::new();
        let paths = test_paths();
        fs.write(&paths.settings_file(), b"garbage!!").unwrap();
        let store = SettingsStore::load(&fs, &paths);
        assert!(store.is_empty());
        assert_eq!(store.get_i32(KEY_SAVE_INTERVAL, 2), 2);

        // Truncated valid file also falls back.
        let mut store = SettingsStore::new();
        store.put_i32("a", 1);
        store.put_string("b", "two");
        let bytes = write_settings(&store.map).unwrap();
        fs.write(&paths.settings_file(), &bytes[..bytes.len() - 3])
            .unwrap();
        let store = SettingsStore::load(&fs, &paths);
        assert!(store.is_empty());
    }

    #[test]
    fn debounced_flush() {
        let fs = MockFs::new();
        let paths = test_paths();
        let mut store = SettingsStore::new();
        store.set_debounce(Duration::from_millis(20));
        store.put_i32("x", 1);
        // Inside the window: no write yet.
        assert!(!store.flush_if_dirty(&fs, &paths).unwrap());
        assert!(!fs.exists(&paths.settings_file()));
        std::thread::sleep(Duration::from_millis(25));
        assert!(store.flush_if_dirty(&fs, &paths).unwrap());
        assert!(fs.exists(&paths.settings_file()));
        // Clean now: nothing to do.
        assert!(!store.flush_if_dirty(&fs, &paths).unwrap());
    }

    #[test]
    fn wire_format_header() {
        let mut store = SettingsStore::new();
        store.put_i32("k", 5);
        let bytes = write_settings(&store.map).unwrap();
        assert_eq!(&bytes[..4], b"MGST");
        assert_eq!(u16::from_be_bytes([bytes[4], bytes[5]]), 1);
        assert_eq!(u16::from_be_bytes([bytes[6], bytes[7]]), 1);
        // Unknown type byte is rejected.
        let mut bad = bytes.clone();
        let type_pos = bad.len() - 4 - 1;
        bad[type_pos] = 99;
        assert!(read_settings(&bad).is_err());
    }
}
