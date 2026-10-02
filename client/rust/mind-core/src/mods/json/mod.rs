// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! JSON content parser (plan 20 §3.4): the `ContentParser` equivalent.
//!
//! M1 covers the `item` and `block` top-level kinds with their core fields;
//! M2 extends the same module with the remaining kinds and nested definables.
//! `serde_json` is built with `preserve_order`, so object key order is the
//! parse/patch traversal order (invariant 9).

use serde_json::{Map, Value};

use crate::content::bundle::MemoryBundle;
use crate::content::parser_hooks::ContentParseError;
use crate::content::registries::blocks::{StackSpec, stack};
use crate::content::settings_store::MemoryUnlockStore;
use crate::content::{
    BlockDef, BlockKind, BlockSpec, BuildVisibility, Category, ContentRef, ContentRegistry,
    ContentType, Item, Rgba,
};

/// One parser warning (unknown field, ignored value, deferred feature).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModWarning {
    /// Source file or content name.
    pub file: String,
    /// Warning text.
    pub message: String,
}

/// `ContentParser` equivalent. One instance per parser scope (mod-content load
/// or restricted patch-content load).
pub struct ContentJsonParser {
    /// Bundle entries registered from JSON `name`/`description` keys.
    pub bundle: MemoryBundle,
    /// Unlock store (headless memory store; plan 04 backs the real one).
    pub store: MemoryUnlockStore,
    /// Accumulated warnings (unknown fields, ignored values).
    pub warnings: Vec<ModWarning>,
    /// Whether type resolution through the ClassMap is allowed.
    pub allow_class_resolution: bool,
    /// Whether asset loading is allowed (restricted patch parser: false).
    pub allow_asset_loading: bool,
    /// Whether in-place patching of existing content is allowed.
    pub allow_patching: bool,
}

impl Default for ContentJsonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentJsonParser {
    /// Parser with default permissions (mod-content load).
    pub fn new() -> Self {
        Self {
            bundle: MemoryBundle::new(),
            store: MemoryUnlockStore::new(),
            warnings: Vec::new(),
            allow_class_resolution: true,
            allow_asset_loading: true,
            allow_patching: true,
        }
    }

    /// Parser restricted for data-patch content (`dp` identity).
    pub fn restricted() -> Self {
        Self {
            allow_class_resolution: false,
            allow_asset_loading: false,
            allow_patching: false,
            ..Self::new()
        }
    }

    /// Parses one content file. `stem` is the file stem (content name before
    /// prefixing).
    pub fn parse(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        json: &str,
        content_type: ContentType,
    ) -> Result<ContentRef, ContentParseError> {
        let value: Value = serde_json::from_str(json)
            .map_err(|error| ContentParseError::new(format!("{file}: invalid JSON: {error}")))?;
        let object = value.as_object().ok_or_else(|| {
            ContentParseError::new(format!("{file}: content must be a JSON object"))
        })?;
        match content_type {
            ContentType::Item => self.parse_item(registry, file, stem, object),
            ContentType::Block => self.parse_block(registry, file, stem, object),
            other => Err(ContentParseError::new(format!(
                "{file}: content type `{other}` is not supported yet (plan 20 M2)"
            ))),
        }
    }

    /// `readBundle`: registers `<type>.<name>.name|description`.
    fn read_bundle(&mut self, object: &Map<String, Value>, content_type: ContentType, name: &str) {
        let key = |suffix: &str| format!("{}.{}.{}", content_type.name(), name, suffix);
        if let Some(text) = object.get("name").and_then(Value::as_str) {
            self.bundle.insert(key("name"), text.to_owned());
        }
        if let Some(text) = object.get("description").and_then(Value::as_str) {
            self.bundle.insert(key("description"), text.to_owned());
        }
    }

    /// `locate`: returns the (prefixed) content name. Typeless files matching
    /// vanilla content are patched in place by M3; M1 surfaces a warning and
    /// creates a prefixed copy so names never collide.
    fn locate_name(
        &mut self,
        registry: &ContentRegistry,
        content_type: ContentType,
        stem: &str,
        object: &Map<String, Value>,
    ) -> String {
        let has_type = object.get("type").is_some();
        if registry.get_by_name(content_type, stem).is_some() {
            self.warn(
                stem,
                if has_type {
                    format!("content `{stem}` already exists; creating prefixed content")
                } else {
                    format!("content `{stem}` already exists; in-place patching lands in M3")
                },
            );
        }
        registry.transform_name(stem)
    }

    fn parse_item(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Item, stem, object);
        self.read_bundle(object, ContentType::Item, &name);

        let color = object
            .get("color")
            .and_then(parse_color)
            .unwrap_or(Rgba::WHITE);
        let mut item = Item::new(&name, color, &self.bundle, &self.store);
        if let Some(hardness) = object.get("hardness").and_then(Value::as_i64) {
            item.hardness = hardness as i32;
        }
        if let Some(cost) = object.get("cost").and_then(Value::as_f64) {
            item.cost = cost as f32;
        }
        for (field, target) in [
            ("explosiveness", &mut item.explosiveness),
            ("flammability", &mut item.flammability),
            ("radioactivity", &mut item.radioactivity),
            ("charge", &mut item.charge),
            ("healthScaling", &mut item.health_scaling),
            ("frameTime", &mut item.frame_time),
        ] {
            if let Some(value) = object.get(field).and_then(Value::as_f64) {
                *target = value as f32;
            }
        }
        for (field, target) in [
            ("lowPriority", &mut item.low_priority),
            ("buildable", &mut item.buildable),
            ("hidden", &mut item.hidden),
        ] {
            if let Some(value) = object.get(field).and_then(Value::as_bool) {
                *target = value;
            }
        }
        for (field, target) in [
            ("frames", &mut item.frames),
            ("transitionFrames", &mut item.transition_frames),
        ] {
            if let Some(value) = object.get(field).and_then(Value::as_i64) {
                *target = value as i32;
            }
        }
        registry
            .add_item(item)
            .map(ContentRef::item)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    #[allow(clippy::too_many_lines)]
    fn parse_block(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Block, stem, object);
        self.read_bundle(object, ContentType::Block, &name);
        let leaked: &'static str = Box::leak(name.into_boxed_str());

        let mut spec = BlockSpec {
            name: leaked,
            ..BlockSpec::default()
        };
        if let Some(kind) = object.get("type").and_then(Value::as_str) {
            spec.kind = resolve_block_kind(kind).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown block type `{kind}`"))
            })?;
        }
        if let Some(size) = object.get("size").and_then(Value::as_i64) {
            spec.size = Some(size as i32);
        }
        if let Some(health) = object.get("health").and_then(Value::as_i64) {
            spec.health = Some(health as i32);
        }
        if let Some(armor) = object.get("armor").and_then(Value::as_f64) {
            spec.armor = Some(armor as f32);
        }
        if let Some(value) = object.get("itemCapacity").and_then(Value::as_i64) {
            spec.item_capacity = Some(value as i32);
        }
        if let Some(value) = object.get("liquidCapacity").and_then(Value::as_f64) {
            spec.liquid_capacity = Some(value as f32);
        }
        for (field, target) in [
            ("solid", &mut spec.solid),
            ("floating", &mut spec.floating),
            ("update", &mut spec.update),
            ("destructible", &mut spec.destructible),
            ("saveData", &mut spec.save_data),
            ("saveConfig", &mut spec.save_config),
            ("configurable", &mut spec.configurable),
            ("inEditor", &mut spec.in_editor),
            ("hasItems", &mut spec.has_items),
            ("hasLiquids", &mut spec.has_liquids),
            ("hasPower", &mut spec.has_power),
            ("placeablePlayer", &mut spec.placeable_player),
            ("placeableLiquid", &mut spec.placeable_liquid),
            ("placeableOn", &mut spec.placeable_on),
        ] {
            if let Some(value) = object.get(field).and_then(Value::as_bool) {
                *target = Some(value);
            }
        }
        if let Some(visibility) = object.get("buildVisibility").and_then(Value::as_str) {
            spec.build_visibility = resolve_build_visibility(visibility);
        }
        if let Some(category) = object.get("category").and_then(Value::as_str) {
            spec.category = resolve_category(category);
        }
        if let Some(requirements) = object.get("requirements") {
            spec.requirements = self.parse_stacks(registry, file, requirements)?;
            if spec
                .build_visibility
                .is_none_or(|visibility| visibility == BuildVisibility::Hidden)
            {
                spec.build_visibility = Some(BuildVisibility::Shown);
            }
        }
        if object.contains_key("consumes") {
            self.warn(file, "`consumes` merging lands in M3; ignored for now");
        }

        let def = BlockDef::from_spec(spec, registry, &self.bundle, &self.store)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        registry
            .add_block(def)
            .map(ContentRef::block)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    fn parse_stacks(
        &self,
        registry: &ContentRegistry,
        file: &str,
        value: &Value,
    ) -> Result<Vec<StackSpec>, ContentParseError> {
        let mut out = Vec::new();
        let entries: Vec<&Value> = match value {
            Value::Array(items) => items.iter().collect(),
            Value::Object(map) => map.values().collect(),
            _ => {
                return Err(ContentParseError::new(format!(
                    "{file}: `requirements` must be an array or object"
                )));
            }
        };
        for entry in entries {
            let (raw_name, amount) = match entry {
                Value::String(text) => parse_stack(text).ok_or_else(|| {
                    ContentParseError::new(format!("{file}: invalid stack `{text}`"))
                })?,
                Value::Object(map) => {
                    let item = map.get("item").and_then(Value::as_str).ok_or_else(|| {
                        ContentParseError::new(format!("{file}: stack missing item"))
                    })?;
                    let amount = map.get("amount").and_then(Value::as_i64).unwrap_or(1);
                    (item.to_owned(), amount as i32)
                }
                _ => {
                    return Err(ContentParseError::new(format!(
                        "{file}: invalid requirement entry"
                    )));
                }
            };
            let resolved = resolve_name(registry, &raw_name).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown item `{raw_name}`"))
            })?;
            out.push(stack(Box::leak(resolved.into_boxed_str()), amount));
        }
        Ok(out)
    }

    /// Records a warning (unknown field/ignored value).
    pub fn warn(&mut self, file: &str, message: impl Into<String>) {
        self.warnings.push(ModWarning {
            file: file.to_owned(),
            message: message.into(),
        });
    }

    /// Bundle entries registered from JSON `name`/`description`.
    pub fn bundle_entries(&self) -> &MemoryBundle {
        &self.bundle
    }
}

/// Splits a `<name>/<amount>` item stack string (default amount 1).
pub fn parse_stack(raw: &str) -> Option<(String, i32)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match raw.split_once('/') {
        Some((name, amount)) => Some((name.trim().to_owned(), amount.trim().parse().ok()?)),
        None => Some((raw.to_owned(), 1)),
    }
}

/// Resolves a name against the registry, applying the current mod prefix.
pub fn resolve_name(registry: &ContentRegistry, raw: &str) -> Option<String> {
    if registry.item_id(raw).is_some()
        || registry.block_id(raw).is_some()
        || registry.liquid_id(raw).is_some()
        || registry.unit_id(raw).is_some()
    {
        return Some(raw.to_owned());
    }
    let prefixed = registry.transform_name(raw);
    if registry.item_id(&prefixed).is_some()
        || registry.block_id(&prefixed).is_some()
        || registry.liquid_id(&prefixed).is_some()
        || registry.unit_id(&prefixed).is_some()
    {
        return Some(prefixed);
    }
    None
}

/// Resolves a block `type` class name (M1: base `Block` + common aliases; the
/// full ClassMap lands in M2).
pub fn resolve_block_kind(name: &str) -> Option<BlockKind> {
    match name {
        "Block" | "GenericCrafter" | "Crafter" => Some(BlockKind::Block),
        "Wall" | "GenericWall" | "StaticWall" => Some(BlockKind::StaticWall),
        "Floor" => Some(BlockKind::Floor),
        "OreBlock" => Some(BlockKind::OreBlock),
        _ => None,
    }
}

/// Resolves a `BuildVisibility` name.
pub fn resolve_build_visibility(name: &str) -> Option<BuildVisibility> {
    match name {
        "hidden" => Some(BuildVisibility::Hidden),
        "shown" => Some(BuildVisibility::Shown),
        "debugOnly" => Some(BuildVisibility::DebugOnly),
        "editorOnly" => Some(BuildVisibility::EditorOnly),
        "sandboxOnly" => Some(BuildVisibility::SandboxOnly),
        _ => None,
    }
}

/// Resolves a `Category` name.
pub fn resolve_category(name: &str) -> Option<Category> {
    match name {
        "turret" => Some(Category::Turret),
        "production" => Some(Category::Production),
        "distribution" => Some(Category::Distribution),
        "liquid" => Some(Category::Liquid),
        "power" => Some(Category::Power),
        "defense" => Some(Category::Defense),
        "crafting" => Some(Category::Crafting),
        "units" => Some(Category::Units),
        "effect" => Some(Category::Effect),
        "logic" => Some(Category::Logic),
        _ => None,
    }
}

/// Parses a JSON color (hex string or `[r,g,b,a]`).
pub fn parse_color(value: &Value) -> Option<Rgba> {
    match value {
        Value::String(hex) => Rgba::from_hex(hex),
        Value::Array(components) => {
            let channel = |index: usize| {
                components
                    .get(index)
                    .and_then(Value::as_f64)
                    .map(|value| value as f32)
            };
            Some(Rgba::new(
                channel(0)?,
                channel(1)?,
                channel(2)?,
                channel(3).unwrap_or(1.0),
            ))
        }
        _ => None,
    }
}

/// Resolves a legacy/current content folder to its type.
pub fn content_type_for_folder(folder: &str) -> Option<ContentType> {
    match folder {
        "items" | "item" => Some(ContentType::Item),
        "blocks" | "block" => Some(ContentType::Block),
        "liquids" | "liquid" => Some(ContentType::Liquid),
        "statuses" | "status" => Some(ContentType::Status),
        "units" | "unit" => Some(ContentType::Unit),
        "weathers" | "weather" => Some(ContentType::Weather),
        "sectors" | "sector" => Some(ContentType::Sector),
        "planets" | "planet" => Some(ContentType::Planet),
        "teams" | "team" => Some(ContentType::Team),
        _ => None,
    }
}
