// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! JSON content parser (plan 20 §3.4): the `ContentParser` equivalent.
//!
//! M1 covered the `item` and `block` top-level kinds with their core fields.
//! M2 extends the same module with `liquid`, `status`, `unit`, `weather`,
//! `sector`, `planet` and `team`, the committed [`ClassMap`](classmap)
//! replacement, nested bullet/weapon creation for units, and null-field
//! validation. `serde_json` is built with `preserve_order`, so object key order
//! is the parse/patch traversal order (invariant 9).
//!
//! Reflection-free field parsing: unknown fields warn and are ignored exactly
//! like upstream `ignoreUnknownFields = true`; the accepted vocabulary per kind
//! is tracked in `parity/ledgers/mod_fields.md`.

pub mod classmap;
pub mod classmap_gen;

use serde_json::{Map, Value};

use crate::content::bundle::MemoryBundle;
use crate::content::parser_hooks::ContentParseError;
use crate::content::registries::blocks::{StackSpec, stack};
use crate::content::registries::bullets::{BulletDef, BulletKind, BulletSpec};
use crate::content::registries::fx_meta::effect_by_name;
use crate::content::registries::planets::{CloudMeshKind, GeneratorKind, MeshKind, PlanetDef};
use crate::content::registries::sectors::SectorPresetDef;
use crate::content::registries::units::weapon::{BulletRef, WeaponDef, WeaponSpec};
use crate::content::registries::units::{
    AiControllerKind, ControllerKind, EntityDefSpec, ResolvedBullet, UnitKind, UnitSpec,
    UnitTypeDef,
};
use crate::content::registries::weathers::{Attribute, WeatherDef, WeatherKind};
use crate::content::settings_store::MemoryUnlockStore;
use crate::content::{
    BlockDef, BlockKind, BlockSpec, BuildVisibility, Category, ContentRef, ContentRegistry,
    ContentType, Item, Liquid, PlanetId, Rgba, StatusEffect, StatusId, TeamEntry,
};

use classmap::{ClassScope, ClassTagMap};

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
    /// Committed ClassMap replacement.
    classmap: ClassTagMap,
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
            classmap: ClassTagMap::committed(),
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

    /// The committed ClassMap replacement.
    pub fn classmap(&self) -> &ClassTagMap {
        &self.classmap
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
        self.check_null_fields(file, content_type, object)?;
        match content_type {
            ContentType::Item => self.parse_item(registry, file, stem, object),
            ContentType::Block => self.parse_block(registry, file, stem, object),
            ContentType::Liquid => self.parse_liquid(registry, file, stem, object),
            ContentType::Status => self.parse_status(registry, file, stem, object),
            ContentType::Unit => self.parse_unit(registry, file, stem, object),
            ContentType::Weather => self.parse_weather(registry, file, stem, object),
            ContentType::Sector => self.parse_sector(registry, file, stem, object),
            ContentType::Planet => self.parse_planet(registry, file, stem, object),
            ContentType::Team => self.parse_team(registry, file, stem, object),
            other => Err(ContentParseError::new(format!(
                "{file}: content type `{other}` is not supported yet (plan 20 M2)"
            ))),
        }
    }

    /// `checkNullFields`: refuses explicit `null` on non-nullable fields.
    ///
    /// Upstream reflection inspects declared field types; the port keeps a
    /// per-kind deny list of fields that are never nullable. Region fields
    /// (`TextureRegion`-equivalent) are implicitly nullable and skipped.
    fn check_null_fields(
        &self,
        file: &str,
        content_type: ContentType,
        object: &Map<String, Value>,
    ) -> Result<(), ContentParseError> {
        let non_nullable: &[&str] = match content_type {
            ContentType::Item => &["name"],
            ContentType::Block => &["name"],
            ContentType::Liquid => &["name"],
            ContentType::Status => &["name"],
            ContentType::Unit => &["name"],
            ContentType::Weather => &["name"],
            ContentType::Sector => &["name"],
            ContentType::Planet => &["name"],
            ContentType::Team => &["name"],
            _ => &[],
        };
        for field in non_nullable {
            if matches!(object.get(*field), Some(Value::Null)) {
                return Err(ContentParseError::new(format!(
                    "{file}: Field '{field}' cannot be null."
                )));
            }
        }
        Ok(())
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
    /// vanilla content are patched in place by M3; M2 surfaces a warning and
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

    /// Resolves a `<mod>-`-prefixed (or raw) mappable reference by type.
    fn resolve_prefixed(
        &self,
        registry: &ContentRegistry,
        content_type: ContentType,
        raw: &str,
    ) -> Option<ContentRef> {
        if let Some(reference) = registry.get_by_name(content_type, raw) {
            return Some(reference);
        }
        registry.get_by_name(content_type, &registry.transform_name(raw))
    }

    // ---- item ----

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
        if let Some(hardness) = field_i32(object, "hardness") {
            item.hardness = hardness;
        }
        if let Some(cost) = field_f32(object, "cost") {
            item.cost = cost;
        }
        for (field, target) in [
            ("explosiveness", &mut item.explosiveness),
            ("flammability", &mut item.flammability),
            ("radioactivity", &mut item.radioactivity),
            ("charge", &mut item.charge),
            ("healthScaling", &mut item.health_scaling),
            ("frameTime", &mut item.frame_time),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("lowPriority", &mut item.low_priority),
            ("buildable", &mut item.buildable),
            ("hidden", &mut item.hidden),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("frames", &mut item.frames),
            ("transitionFrames", &mut item.transition_frames),
        ] {
            if let Some(value) = field_i32(object, field) {
                *target = value;
            }
        }
        self.warn_unknown(file, "item", object, ITEM_FIELDS);
        registry
            .add_item(item)
            .map(ContentRef::item)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    // ---- block ----

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
            spec.kind = self.resolve_block_kind(file, kind)?;
        }
        if let Some(size) = field_i32(object, "size") {
            spec.size = Some(size);
        }
        if let Some(health) = field_i32(object, "health") {
            spec.health = Some(health);
        }
        if let Some(armor) = field_f32(object, "armor") {
            spec.armor = Some(armor);
        }
        if let Some(value) = field_i32(object, "itemCapacity") {
            spec.item_capacity = Some(value);
        }
        if let Some(value) = field_f32(object, "liquidCapacity") {
            spec.liquid_capacity = Some(value);
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
            if let Some(value) = field_bool(object, field) {
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

    // ---- liquid ----

    fn parse_liquid(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Liquid, stem, object);
        self.read_bundle(object, ContentType::Liquid, &name);
        let color = object
            .get("color")
            .and_then(parse_color)
            .unwrap_or(Rgba::WHITE);
        let mut liquid = Liquid::new(&name, color, &self.bundle, &self.store);
        for (field, target) in [
            ("temperature", &mut liquid.temperature),
            ("heatCapacity", &mut liquid.heat_capacity),
            ("viscosity", &mut liquid.viscosity),
            ("explosiveness", &mut liquid.explosiveness),
            ("flammability", &mut liquid.flammability),
            ("particleSpacing", &mut liquid.particle_spacing),
            ("boilPoint", &mut liquid.boil_point),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("gas", &mut liquid.gas),
            ("blockReactive", &mut liquid.block_reactive),
            ("coolant", &mut liquid.coolant),
            ("moveThroughBlocks", &mut liquid.move_through_blocks),
            ("incinerable", &mut liquid.incinerable),
            ("capPuddles", &mut liquid.cap_puddles),
            ("hidden", &mut liquid.hidden),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        if let Some(color) = object.get("gasColor").and_then(parse_color) {
            liquid.gas_color = color;
        }
        if let Some(color) = object.get("lightColor").and_then(parse_color) {
            liquid.light_color = color;
        }
        if let Some(color) = object.get("barColor").and_then(parse_color) {
            liquid.bar_color = Some(color);
        }
        if let Some(status) = object.get("effect").and_then(Value::as_str) {
            liquid.effect = self
                .resolve_status(registry, status)
                .unwrap_or(StatusId::NONE);
        }
        if let Some(effect) = object.get("particleEffect").and_then(Value::as_str) {
            liquid.particle_effect = effect_by_name(effect).unwrap_or(liquid.particle_effect);
        }
        if let Some(effect) = object.get("vaporEffect").and_then(Value::as_str) {
            liquid.vapor_effect = effect_by_name(effect).unwrap_or(liquid.vapor_effect);
        }
        if let Some(Value::Array(entries)) = object.get("canStayOn") {
            for entry in entries {
                if let Some(raw) = entry.as_str()
                    && let Some(reference) =
                        self.resolve_prefixed(registry, ContentType::Liquid, raw)
                {
                    liquid.add_can_stay_on(crate::content::LiquidId::new(reference.id));
                }
            }
        }
        if object.get("type").and_then(Value::as_str) == Some("CellLiquid") {
            liquid.cell = Some(crate::content::CellLiquidFields::default());
        }
        self.warn_unknown(file, "liquid", object, LIQUID_FIELDS);
        registry
            .add_liquid(liquid)
            .map(ContentRef::liquid)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    // ---- status ----

    fn parse_status(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Status, stem, object);
        self.read_bundle(object, ContentType::Status, &name);
        let mut status = StatusEffect::new(&name, &self.bundle, &self.store);
        for (field, target) in [
            ("damageMultiplier", &mut status.damage_multiplier),
            ("healthMultiplier", &mut status.health_multiplier),
            ("speedMultiplier", &mut status.speed_multiplier),
            ("reloadMultiplier", &mut status.reload_multiplier),
            ("buildSpeedMultiplier", &mut status.build_speed_multiplier),
            ("dragMultiplier", &mut status.drag_multiplier),
            ("transitionDamage", &mut status.transition_damage),
            ("damage", &mut status.damage),
            ("intervalDamageTime", &mut status.interval_damage_time),
            ("intervalDamage", &mut status.interval_damage),
            ("effectChance", &mut status.effect_chance),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("disarm", &mut status.disarm),
            ("intervalDamagePierce", &mut status.interval_damage_pierce),
            ("parentizeEffect", &mut status.parentize_effect),
            ("permanent", &mut status.permanent),
            ("reactive", &mut status.reactive),
            ("dynamic", &mut status.dynamic),
            ("show", &mut status.show),
            ("applyExtend", &mut status.apply_extend),
            ("parentizeApplyEffect", &mut status.parentize_apply_effect),
            ("outline", &mut status.outline),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        if let Some(color) = object.get("color").and_then(parse_color) {
            status.color = color;
        }
        if let Some(color) = object.get("applyColor").and_then(parse_color) {
            status.apply_color = color;
        }
        if let Some(effect) = object.get("effect").and_then(Value::as_str) {
            status.effect = effect_by_name(effect).unwrap_or(status.effect);
        }
        if let Some(effect) = object.get("applyEffect").and_then(Value::as_str) {
            status.apply_effect = effect_by_name(effect).unwrap_or(status.apply_effect);
        }
        if let Some(Value::Array(entries)) = object.get("opposites") {
            for entry in entries {
                if let Some(raw) = entry.as_str()
                    && let Some(other) = self.resolve_status(registry, raw)
                {
                    status.declare_opposite(other);
                    status.add_opposite(other);
                }
            }
        }
        if let Some(Value::Array(entries)) = object.get("affinities") {
            for entry in entries {
                if let Some(raw) = entry.as_str()
                    && let Some(other) = self.resolve_status(registry, raw)
                {
                    status.declare_affinity(other, crate::content::AffinityTransition::none());
                    status.add_affinity(other);
                }
            }
        }
        self.warn_unknown(file, "status", object, STATUS_FIELDS);
        registry
            .add_status(status)
            .map(ContentRef::status)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    // ---- weather ----

    fn parse_weather(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Weather, stem, object);
        self.read_bundle(object, ContentType::Weather, &name);
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .map(|class| {
                self.classmap
                    .resolve(class, ClassScope::Weather)
                    .and_then(resolve_weather_kind)
                    .unwrap_or(WeatherKind::Plain)
            })
            .unwrap_or(WeatherKind::Plain);
        let mut weather = WeatherDef::new(&name, kind, &self.bundle, &self.store);
        for (field, target) in [
            ("duration", &mut weather.duration),
            ("opacityMultiplier", &mut weather.opacity_multiplier),
            ("soundVol", &mut weather.sound_vol),
            ("soundVolMin", &mut weather.sound_vol_min),
            ("soundVolOscMag", &mut weather.sound_vol_osc_mag),
            ("soundVolOscScl", &mut weather.sound_vol_osc_scl),
            ("statusDuration", &mut weather.status_duration),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("hidden", &mut weather.hidden),
            ("statusAir", &mut weather.status_air),
            ("statusGround", &mut weather.status_ground),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        if let Some(sound) = object.get("sound").and_then(Value::as_str) {
            weather.sound = Some(sound.to_owned());
        }
        if let Some(status) = object.get("status").and_then(Value::as_str) {
            weather.status = self
                .resolve_status(registry, status)
                .unwrap_or(StatusId::NONE);
        }
        if let Some(map) = object.get("attrs").and_then(Value::as_object) {
            for (key, value) in map {
                if let (Some(attribute), Some(value)) = (resolve_attribute(key), value.as_f64()) {
                    weather.set_attr(attribute, value as f32);
                }
            }
        }
        if let Some(particle) = weather.particle.as_mut() {
            // Parse only the ported `ParticleWeatherFields` subset.
            for (field, target) in [
                ("yspeed", &mut particle.yspeed),
                ("xspeed", &mut particle.xspeed),
                ("padding", &mut particle.padding),
                ("sizeMin", &mut particle.size_min),
                ("sizeMax", &mut particle.size_max),
                ("density", &mut particle.density),
                ("minAlpha", &mut particle.min_alpha),
                ("maxAlpha", &mut particle.max_alpha),
                ("force", &mut particle.force),
                ("noiseScale", &mut particle.noise_scale),
            ] {
                if let Some(value) = field_f32(object, field) {
                    *target = value;
                }
            }
            if let Some(color) = object.get("color").and_then(parse_color) {
                particle.color = color;
            }
        }
        if let Some(rain) = weather.rain.as_mut() {
            for (field, target) in [
                ("yspeed", &mut rain.yspeed),
                ("xspeed", &mut rain.xspeed),
                ("padding", &mut rain.padding),
                ("density", &mut rain.density),
                ("stroke", &mut rain.stroke),
            ] {
                if let Some(value) = field_f32(object, field) {
                    *target = value;
                }
            }
            if let Some(color) = object.get("color").and_then(parse_color) {
                rain.color = color;
            }
            if let Some(raw) = object.get("liquid").and_then(Value::as_str)
                && let Some(reference) = self.resolve_prefixed(registry, ContentType::Liquid, raw)
            {
                rain.liquid = crate::content::LiquidId::new(reference.id);
            }
        }
        self.warn_unknown(file, "weather", object, WEATHER_FIELDS);
        let id = registry
            .add_weather(weather)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Weather, id.raw()))
    }

    // ---- team ----

    fn parse_team(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Team, stem, object);
        self.read_bundle(object, ContentType::Team, &name);
        let team = object
            .get("team")
            .and_then(Value::as_str)
            .unwrap_or(name.as_str());
        let entry = TeamEntry::new(&name, team, &self.bundle, &self.store);
        self.warn_unknown(file, "team", object, TEAM_FIELDS);
        let id = registry
            .add_team(entry)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Team, id.raw()))
    }

    // ---- sector ----

    fn parse_sector(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Sector, stem, object);
        self.read_bundle(object, ContentType::Sector, &name);
        let planet_raw = object
            .get("planet")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ContentParseError::new(format!("{file}: sector `{stem}` needs `planet`"))
            })?;
        let planet = self.resolve_planet(registry, planet_raw).ok_or_else(|| {
            ContentParseError::new(format!("{file}: unknown planet `{planet_raw}`"))
        })?;
        let index = field_i32(object, "sector").unwrap_or(0);
        let mut sector = SectorPresetDef::new(&name, planet, index, &self.bundle, &self.store);
        if let Some(value) = field_i32(object, "captureWave") {
            sector.capture_wave = value;
        }
        if let Some(value) = field_f32(object, "difficulty") {
            sector.difficulty = value;
        }
        if let Some(value) = field_f32(object, "startWaveTimeMultiplier") {
            sector.start_wave_time_multiplier = value;
        }
        for (field, target) in [
            ("addStartingItems", &mut sector.add_starting_items),
            ("noLighting", &mut sector.no_lighting),
            ("isLastSector", &mut sector.is_last_sector),
            ("requireUnlock", &mut sector.require_unlock),
            ("showHidden", &mut sector.show_hidden),
            ("attackAfterWaves", &mut sector.attack_after_waves),
            ("outline", &mut sector.outline),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        if let Some(value) = field_i32(object, "outlineRadius") {
            sector.outline_radius = value;
        }
        self.warn_unknown(file, "sector", object, SECTOR_FIELDS);
        let id = registry
            .add_sector(sector)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Sector, id.raw()))
    }

    // ---- planet ----

    fn parse_planet(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Planet, stem, object);
        self.read_bundle(object, ContentType::Planet, &name);
        let parent = object
            .get("parent")
            .and_then(Value::as_str)
            .and_then(|raw| self.resolve_planet(registry, raw));
        let radius = field_f32(object, "radius").unwrap_or(1.0);
        let mut planet = PlanetDef::new(&name, parent, radius, &self.bundle, &self.store);
        if let Some(size) = field_i32(object, "sectorSize") {
            planet = planet.with_sector_grid(size.clamp(0, 255) as u8);
        }
        if let Some(mesh) = object.get("mesh").and_then(Value::as_str)
            && let Some(kind) = resolve_mesh_kind(mesh)
        {
            planet.mesh = kind;
        }
        if let Some(mesh) = object.get("cloudMesh").and_then(Value::as_str)
            && let Some(kind) = resolve_cloud_mesh_kind(mesh)
        {
            planet.cloud_mesh = Some(kind);
        }
        for (field, target) in [
            ("bloom", &mut planet.bloom),
            ("accessible", &mut planet.accessible),
            ("visible", &mut planet.visible),
            ("hasAtmosphere", &mut planet.has_atmosphere),
            ("updateLighting", &mut planet.update_lighting),
            ("allowLaunchSchematics", &mut planet.allow_launch_schematics),
            ("allowLaunchLoadout", &mut planet.allow_launch_loadout),
            ("allowSectorInvasion", &mut planet.allow_sector_invasion),
            ("allowWaves", &mut planet.allow_waves),
            ("drawOrbit", &mut planet.draw_orbit),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        if let Some(value) = field_i32(object, "startSector") {
            planet.start_sector = value.max(0) as u32;
        }
        if let Some(color) = object.get("lightColor").and_then(parse_color) {
            planet.light_color = color;
        }
        if let Some(color) = object.get("atmosphereColor").and_then(parse_color) {
            planet.atmosphere_color = color;
        }
        if let Some(color) = object.get("iconColor").and_then(parse_color) {
            planet.icon_color = color;
        }
        if let Some(generator) = object.get("generator").and_then(Value::as_str)
            && let Some(kind) = resolve_generator_kind(generator)
        {
            planet.generator = kind;
        }
        self.warn_unknown(file, "planet", object, PLANET_FIELDS);
        let id = registry
            .add_planet(planet)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Planet, id.raw()))
    }

    // ---- unit ----

    #[allow(clippy::too_many_lines)]
    fn parse_unit(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Unit, stem, object);
        self.read_bundle(object, ContentType::Unit, &name);
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());

        let template = object.get("template").and_then(Value::as_str);
        if let Some(class) = template
            && self.classmap.resolve(class, ClassScope::UnitType).is_none()
        {
            return Err(ContentParseError::new(format!(
                "{file}: unknown unit template `{class}`"
            )));
        }
        let kind = template
            .and_then(resolve_unit_class_kind)
            .unwrap_or(UnitKind::UnitType);
        let entity = object
            .get("type")
            .and_then(Value::as_str)
            .and_then(resolve_entity_def)
            .unwrap_or_default();
        let mut spec = UnitSpec::for_kind(leaked, kind, entity);
        for (field, target) in [
            ("health", &mut spec.health),
            ("speed", &mut spec.speed),
            ("armor", &mut spec.armor),
            ("hitSize", &mut spec.hit_size),
            ("drag", &mut spec.drag),
            ("accel", &mut spec.accel),
            ("rotateSpeed", &mut spec.rotate_speed),
            ("range", &mut spec.range),
            ("mineRange", &mut spec.mine_range),
            ("buildRange", &mut spec.build_range),
            ("buildSpeed", &mut spec.build_speed),
            ("mineSpeed", &mut spec.mine_speed),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = Some(value);
            }
        }
        if let Some(value) = field_i32(object, "itemCapacity") {
            spec.item_capacity = Some(value);
        }
        if let Some(value) = field_i32(object, "mineTier") {
            spec.mine_tier = Some(value);
        }
        if let Some(value) = field_bool(object, "flying") {
            spec.flying = Some(value);
        }
        for (field, target) in [
            ("targetAir", &mut spec.target_air),
            ("targetGround", &mut spec.target_ground),
            ("useUnitCap", &mut spec.use_unit_cap),
            ("playerControllable", &mut spec.player_controllable),
            ("logicControllable", &mut spec.logic_controllable),
            ("hidden", &mut spec.hidden),
            ("isEnemy", &mut spec.is_enemy),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = Some(value);
            }
        }
        if let Some(class) = object.get("controller").and_then(Value::as_str) {
            spec.controller = Some(self.resolve_controller(class).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown controller `{class}`"))
            })?);
        }
        if let Some(class) = object.get("aiController").and_then(Value::as_str) {
            spec.ai_controller = Some(self.resolve_ai_controller(class).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown aiController `{class}`"))
            })?);
        }
        if let Some(Value::Array(entries)) = object.get("immunities") {
            let mut names = Vec::new();
            for entry in entries {
                if let Some(raw) = entry.as_str()
                    && let Some(status) = self.resolve_status(registry, raw)
                    && let Some(record) = registry.status(status)
                {
                    names.push(record.name.clone());
                }
            }
            // `UnitSpec.immunities` holds `&'static str` names; leak the resolved set.
            let leaked: &'static [&'static str] = Box::leak(
                names
                    .into_iter()
                    .map(|name| Box::leak(name.into_boxed_str()) as &'static str)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            );
            spec.immunities = leaked.to_vec();
        }

        let def = UnitTypeDef::from_spec(&spec, registry, &self.bundle, &self.store)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        let unit_id = registry
            .add_unit(def)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;

        let weapons = self.parse_weapons(registry, file, object)?;
        if !weapons.is_empty() {
            registry
                .unit_mut(unit_id)
                .ok_or_else(|| {
                    ContentParseError::new(format!("{file}: unit lost after registration"))
                })?
                .weapons = weapons;
        }
        self.warn_unknown(file, "unit", object, UNIT_FIELDS);
        Ok(ContentRef::new(ContentType::Unit, unit_id.raw()))
    }

    /// Parses the `weapons` array into [`WeaponDef`]s, creating inline bullets.
    fn parse_weapons(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        object: &Map<String, Value>,
    ) -> Result<Vec<WeaponDef>, ContentParseError> {
        let Some(Value::Array(entries)) = object.get("weapons") else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let Some(weapon) = entry.as_object() else {
                self.warn(file, "weapon entries must be objects; skipped");
                continue;
            };
            out.push(self.parse_weapon_object(registry, file, weapon, index)?);
        }
        Ok(out)
    }

    /// Parses a single weapon object (`Weapon{}`), registering its inline bullet
    /// and running the created-object `init()` callback (plan 20 M3b).
    ///
    /// Shared by unit-content parsing and the `DataPatcher` created-object
    /// callback path (`PatcherTests.unitWeapons`/`addWeapon`).
    pub fn parse_weapon_object(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        weapon: &Map<String, Value>,
        index: usize,
    ) -> Result<WeaponDef, ContentParseError> {
        let weapon_name: &'static str = weapon
            .get("name")
            .and_then(Value::as_str)
            .map(|name| Box::leak(name.to_owned().into_boxed_str()) as &'static str)
            .unwrap_or_else(|| Box::leak(format!("weapon{index}").into_boxed_str()));
        let mut spec = WeaponSpec {
            name: weapon_name,
            ..WeaponSpec::default()
        };
        if let Some(value) = field_bool(weapon, "mirror") {
            spec.mirror = Some(value);
        }
        if let Some(value) = field_f32(weapon, "reload") {
            spec.reload = Some(value);
        }
        if let Some(value) = field_f32(weapon, "x") {
            spec.x = Some(value);
        }
        if let Some(value) = field_f32(weapon, "y") {
            spec.y = Some(value);
        }
        if let Some(value) = field_bool(weapon, "alternate") {
            spec.alternate = Some(value);
        }
        if let Some(value) = field_bool(weapon, "rotate") {
            spec.rotate = Some(value);
        }
        if let Some(value) = field_bool(weapon, "shootOnDeath") {
            spec.shoot_on_death = Some(value);
        }
        match weapon.get("bullet") {
            Some(Value::Object(bullet)) => {
                let bullet_spec = self.parse_bullet(file, bullet)?;
                spec.bullet = BulletRef::Inline(Box::new(bullet_spec));
            }
            Some(Value::String(_)) => {
                self.warn(file, "named bullet references are not supported yet (M2b)");
            }
            _ => {}
        }
        let bullet_resolved = match &spec.bullet {
            BulletRef::Inline(bullet_spec) => {
                let def = BulletDef::from_spec(bullet_spec, registry)
                    .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
                let id = registry
                    .add_bullet(def)
                    .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
                let record = registry
                    .bullet(id)
                    .ok_or_else(|| ContentParseError::new(format!("{file}: bullet lost")))?;
                ResolvedBullet {
                    id,
                    range: record.compute_range(),
                    heals: record.heals(),
                    kill_shooter: record.kill_shooter,
                    dps: 0.0,
                }
            }
            _ => ResolvedBullet {
                id: crate::content::BulletId::new(0),
                range: 0.0,
                heals: false,
                kill_shooter: false,
                dps: 0.0,
            },
        };
        let mut def = WeaponDef::from_spec(spec, bullet_resolved, registry)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        // Created-object lifecycle: upstream calls `init()`/`postInit()` on the
        // freshly constructed weapon before attaching it.
        def.init();
        Ok(def)
    }

    /// Parses an inline `bullet` object into a [`BulletSpec`] (M2 subset).
    fn parse_bullet(
        &mut self,
        file: &str,
        object: &Map<String, Value>,
    ) -> Result<BulletSpec, ContentParseError> {
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .map(|class| {
                self.classmap
                    .resolve(class, ClassScope::BulletType)
                    .and_then(resolve_bullet_kind)
                    .ok_or_else(|| {
                        ContentParseError::new(format!("{file}: unknown bullet type `{class}`"))
                    })
            })
            .transpose()?
            .unwrap_or(BulletKind::Plain);
        let mut spec = BulletSpec {
            kind,
            ..BulletSpec::default()
        };
        for (field, target) in [
            ("damage", &mut spec.damage),
            ("speed", &mut spec.speed),
            ("lifetime", &mut spec.lifetime),
            ("hitSize", &mut spec.hit_size),
            ("drawSize", &mut spec.draw_size),
            ("splashDamage", &mut spec.splash_damage),
            ("splashDamageRadius", &mut spec.splash_damage_radius),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = Some(value);
            }
        }
        for (field, target) in [
            ("lightning", &mut spec.lightning),
            ("lightningLength", &mut spec.lightning_length),
            ("lightningLengthRand", &mut spec.lightning_length_rand),
        ] {
            if let Some(value) = field_i32(object, field) {
                *target = Some(value);
            }
        }
        for (field, target) in [
            ("pierce", &mut spec.pierce),
            ("pierceBuilding", &mut spec.pierce_building),
            ("keepVelocity", &mut spec.keep_velocity),
            ("collides", &mut spec.collides),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = Some(value);
            }
        }
        if let Some(status) = object.get("status").and_then(Value::as_str) {
            // Bullet status is a name resolved during the unit load pass; keep raw.
            spec.status = Some(Box::leak(status.to_owned().into_boxed_str()));
        }
        Ok(spec)
    }

    // ---- shared helpers ----

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

    fn resolve_block_kind(
        &mut self,
        file: &str,
        class: &str,
    ) -> Result<BlockKind, ContentParseError> {
        if let Some(tag) = self.classmap.resolve(class, ClassScope::Block)
            && let Some(kind) = resolve_block_kind_tag(tag)
        {
            return Ok(kind);
        }
        if !self.allow_class_resolution && !self.classmap.is_known(class) {
            self.warn(
                file,
                format!("type `{class}` could not be resolved (class resolution disabled)"),
            );
        }
        Err(ContentParseError::new(format!(
            "{file}: unknown block type `{class}`"
        )))
    }

    fn resolve_status(&self, registry: &ContentRegistry, raw: &str) -> Option<StatusId> {
        self.resolve_prefixed(registry, ContentType::Status, raw)
            .map(|reference| StatusId::new(reference.id))
    }

    fn resolve_planet(&self, registry: &ContentRegistry, raw: &str) -> Option<PlanetId> {
        self.resolve_prefixed(registry, ContentType::Planet, raw)
            .map(|reference| PlanetId::new(reference.id))
    }

    fn resolve_controller(&self, class: &str) -> Option<ControllerKind> {
        let tag = self.classmap.resolve(class, ClassScope::UnitController)?;
        match tag {
            "AssemblerAI" => Some(ControllerKind::Assembler),
            "BuilderAI" => Some(ControllerKind::Builder {
                core_flee_range: 400.0,
            }),
            "CargoAI" => Some(ControllerKind::Cargo),
            "NoAI" => Some(ControllerKind::No),
            "MissileAI" => Some(ControllerKind::Missile),
            _ => None,
        }
    }

    fn resolve_ai_controller(&self, class: &str) -> Option<AiControllerKind> {
        match self.classmap.resolve(class, ClassScope::UnitController)? {
            "DefenderAI" => Some(AiControllerKind::Defender),
            "FlyingFollowAI" => Some(AiControllerKind::FlyingFollow),
            "HugAI" => Some(AiControllerKind::Hug),
            "SuicideAI" => Some(AiControllerKind::Suicide),
            _ => None,
        }
    }

    /// Records a warning (unknown field/ignored value).
    pub fn warn(&mut self, file: &str, message: impl Into<String>) {
        self.warnings.push(ModWarning {
            file: file.to_owned(),
            message: message.into(),
        });
    }

    fn warn_unknown(
        &mut self,
        file: &str,
        kind: &str,
        object: &Map<String, Value>,
        known: &[&str],
    ) {
        for key in object.keys() {
            if !known.contains(&key.as_str()) {
                self.warn(file, format!("unknown {kind} field `{key}`"));
            }
        }
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

/// Resolves a block class tag to a [`BlockKind`].
pub fn resolve_block_kind_tag(tag: &str) -> Option<BlockKind> {
    crate::content::BlockKind::ALL
        .iter()
        .copied()
        .find(|kind| kind.name() == tag)
}

/// Resolves a block `type` class name through the committed ClassMap.
pub fn resolve_block_kind(name: &str) -> Option<BlockKind> {
    let map = ClassTagMap::committed();
    map.resolve(name, ClassScope::Block)
        .and_then(resolve_block_kind_tag)
}

/// Resolves a bullet class tag to a [`BulletKind`].
pub fn resolve_bullet_kind(tag: &str) -> Option<BulletKind> {
    Some(match tag {
        "BulletType" => BulletKind::Plain,
        "BasicBulletType" => BulletKind::Basic,
        "FireBulletType" => BulletKind::Fire,
        "SpaceLiquidBulletType" => BulletKind::SpaceLiquid,
        "ArtilleryBulletType" => BulletKind::Artillery,
        "MissileBulletType" => BulletKind::Missile,
        "LaserBoltBulletType" => BulletKind::LaserBolt,
        "SapBulletType" => BulletKind::Sap,
        "LightningBulletType" => BulletKind::Lightning,
        "LaserBulletType" => BulletKind::Laser,
        "FlakBulletType" => BulletKind::Flak,
        "ExplosionBulletType" => BulletKind::Explosion,
        "RailBulletType" => BulletKind::Rail,
        "ContinuousLaserBulletType" => BulletKind::ContinuousLaser,
        "ShrapnelBulletType" => BulletKind::Shrapnel,
        "LiquidBulletType" => BulletKind::Liquid,
        "EmpBulletType" => BulletKind::Emp,
        "BombBulletType" => BulletKind::Bomb,
        "ContinuousBulletType" => BulletKind::Continuous,
        "MultiBulletType" => BulletKind::Multi,
        "PointBulletType" => BulletKind::Point,
        "PointLaserBulletType" => BulletKind::PointLaser,
        "ContinuousFlameBulletType" => BulletKind::ContinuousFlame,
        "InterceptorBulletType" => BulletKind::Interceptor,
        "MassDriverBolt" => BulletKind::MassDriver,
        "EmptyBulletType" => BulletKind::Empty,
        _ => return None,
    })
}

/// Resolves a weather class tag to a [`WeatherKind`].
pub fn resolve_weather_kind(tag: &str) -> Option<WeatherKind> {
    Some(match tag {
        "Weather" => WeatherKind::Plain,
        "ParticleWeather" => WeatherKind::Particle,
        "RainWeather" => WeatherKind::Rain,
        "MagneticStorm" => WeatherKind::MagneticStorm,
        "SolarFlare" => WeatherKind::SolarFlare,
        _ => return None,
    })
}

/// Resolves a unit class tag to a [`UnitKind`].
pub fn resolve_unit_class_kind(tag: &str) -> Option<UnitKind> {
    Some(match tag {
        "UnitType" => UnitKind::UnitType,
        "ErekirUnitType" => UnitKind::ErekirUnitType,
        "TankUnitType" => UnitKind::TankUnitType,
        "MissileUnitType" => UnitKind::MissileUnitType,
        "NeoplasmUnitType" => UnitKind::NeoplasmUnitType,
        _ => return None,
    })
}

/// Resolves a unit `type`/entity keyword to an [`EntityDefSpec`].
pub fn resolve_entity_def(name: &str) -> Option<EntityDefSpec> {
    use crate::content::registries::units::entity;
    Some(match name {
        "mech" => entity::MECH,
        "legs" | "leg" => entity::LEGS,
        "flyer" | "flying" | "air" => entity::AIR,
        "hover" => entity::HOVER,
        "naval" | "water" | "ship" => entity::NAVAL,
        "payload" => entity::AIR_PAYLOAD,
        "tank" => entity::TANK,
        "missile" => entity::MISSILE,
        "crawl" => entity::CRAWL,
        "block" => entity::BLOCK,
        _ => return None,
    })
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

/// Resolves an environment [`Attribute`] name.
pub fn resolve_attribute(name: &str) -> Option<Attribute> {
    match name {
        "heat" => Some(Attribute::Heat),
        "spores" => Some(Attribute::Spores),
        "water" => Some(Attribute::Water),
        "light" => Some(Attribute::Light),
        _ => None,
    }
}

/// Resolves a `MeshKind` name.
pub fn resolve_mesh_kind(name: &str) -> Option<MeshKind> {
    Some(match name {
        "shaderSphere" | "ShaderSphereMesh" => MeshKind::ShaderSphere,
        "sun" | "SunMesh" => MeshKind::Sun,
        "hexSphere" | "HexMesh" => MeshKind::Hex { detail: 5 },
        "asteroid" | "NoiseMesh" => MeshKind::Asteroid,
        _ => return None,
    })
}

/// Resolves a `CloudMeshKind` name.
pub fn resolve_cloud_mesh_kind(name: &str) -> Option<CloudMeshKind> {
    Some(match name {
        "cloudMesh" | "multiHex" | "MultiMesh" => CloudMeshKind::MultiHex,
        _ => return None,
    })
}

/// Resolves a `GeneratorKind` name.
pub fn resolve_generator_kind(name: &str) -> Option<GeneratorKind> {
    Some(match name {
        "serpulo" | "SerpuloPlanetGenerator" => GeneratorKind::Serpulo,
        "erekir" | "ErekirPlanetGenerator" => GeneratorKind::Erekir,
        "tantros" | "TantrosPlanetGenerator" => GeneratorKind::Tantros,
        "asteroid" | "AsteroidGenerator" => GeneratorKind::Asteroid,
        "fileMap" | "FileMapGenerator" => GeneratorKind::FileMap,
        "none" | "None" => GeneratorKind::None,
        _ => return None,
    })
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

/// Reads a number field as `f32` (accepts both int and float JSON values).
fn field_f32(object: &Map<String, Value>, key: &str) -> Option<f32> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .map(|value| value as f32)
}

/// Reads a number field as `i32`.
fn field_i32(object: &Map<String, Value>, key: &str) -> Option<i32> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .map(|value| value as i32)
}

/// Reads a boolean field.
fn field_bool(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(Value::as_bool)
}

/// Known JSON fields per kind (ledger-backed; unknown keys warn).
const ITEM_FIELDS: &[&str] = &[
    "name",
    "description",
    "color",
    "hardness",
    "cost",
    "explosiveness",
    "flammability",
    "radioactivity",
    "charge",
    "healthScaling",
    "lowPriority",
    "frames",
    "transitionFrames",
    "frameTime",
    "buildable",
    "hidden",
];

const LIQUID_FIELDS: &[&str] = &[
    "name",
    "description",
    "type",
    "color",
    "gas",
    "gasColor",
    "barColor",
    "lightColor",
    "flammability",
    "temperature",
    "heatCapacity",
    "viscosity",
    "explosiveness",
    "blockReactive",
    "coolant",
    "moveThroughBlocks",
    "incinerable",
    "effect",
    "particleEffect",
    "particleSpacing",
    "boilPoint",
    "capPuddles",
    "vaporEffect",
    "hidden",
    "canStayOn",
];

const STATUS_FIELDS: &[&str] = &[
    "name",
    "description",
    "damageMultiplier",
    "healthMultiplier",
    "speedMultiplier",
    "reloadMultiplier",
    "buildSpeedMultiplier",
    "dragMultiplier",
    "transitionDamage",
    "disarm",
    "damage",
    "intervalDamageTime",
    "intervalDamage",
    "intervalDamagePierce",
    "effectChance",
    "parentizeEffect",
    "permanent",
    "reactive",
    "dynamic",
    "show",
    "color",
    "effect",
    "applyEffect",
    "applyExtend",
    "applyColor",
    "parentizeApplyEffect",
    "affinities",
    "opposites",
    "outline",
];

const WEATHER_FIELDS: &[&str] = &[
    "name",
    "description",
    "type",
    "duration",
    "opacityMultiplier",
    "attrs",
    "sound",
    "soundVol",
    "soundVolMin",
    "soundVolOscMag",
    "soundVolOscScl",
    "hidden",
    "status",
    "statusDuration",
    "statusAir",
    "statusGround",
    "color",
    "yspeed",
    "xspeed",
    "padding",
    "sizeMin",
    "sizeMax",
    "density",
    "minAlpha",
    "maxAlpha",
    "force",
    "noiseScale",
    "stroke",
    "liquid",
];

const SECTOR_FIELDS: &[&str] = &[
    "name",
    "description",
    "planet",
    "sector",
    "captureWave",
    "difficulty",
    "startWaveTimeMultiplier",
    "addStartingItems",
    "noLighting",
    "isLastSector",
    "requireUnlock",
    "showHidden",
    "attackAfterWaves",
    "outline",
    "outlineRadius",
];

const PLANET_FIELDS: &[&str] = &[
    "name",
    "description",
    "parent",
    "radius",
    "sectorSize",
    "mesh",
    "cloudMesh",
    "bloom",
    "accessible",
    "visible",
    "hasAtmosphere",
    "updateLighting",
    "lightColor",
    "atmosphereColor",
    "iconColor",
    "defaultEnv",
    "generator",
    "startSector",
    "allowLaunchSchematics",
    "allowLaunchLoadout",
    "allowSectorInvasion",
    "allowWaves",
    "drawOrbit",
];

const TEAM_FIELDS: &[&str] = &["name", "description", "team"];

const UNIT_FIELDS: &[&str] = &[
    "name",
    "description",
    "type",
    "template",
    "controller",
    "aiController",
    "health",
    "speed",
    "armor",
    "hitSize",
    "drag",
    "accel",
    "rotateSpeed",
    "range",
    "mineRange",
    "buildRange",
    "buildSpeed",
    "mineSpeed",
    "itemCapacity",
    "mineTier",
    "flying",
    "targetAir",
    "targetGround",
    "useUnitCap",
    "playerControllable",
    "logicControllable",
    "hidden",
    "isEnemy",
    "immunities",
    "weapons",
    "requirements",
    "abilities",
    "commands",
    "stances",
    "defaultCommand",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::create_base_content;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn registry() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry = create_base_content(&bundle, &store, true).expect("base content");
        registry.init().expect("init");
        registry.post_init().expect("post init");
        registry
    }

    fn parse(
        reg: &mut ContentRegistry,
        folder: ContentType,
        stem: &str,
        json: &str,
    ) -> (ContentJsonParser, Result<ContentRef, ContentParseError>) {
        reg.set_current_mod(Some(crate::content::ModId(String::from("mods"))));
        let mut parser = ContentJsonParser::new();
        let result = parser.parse(reg, "test.json", stem, json, folder);
        (parser, result)
    }

    #[test]
    fn liquid_parity() {
        let mut reg = registry();
        let (_, result) = parse(
            &mut reg,
            ContentType::Liquid,
            "test-fluid",
            r#"{"name":"Test Fluid","color":"ff0000","temperature":0.2,"gas":true,"effect":"wet"}"#,
        );
        assert!(result.is_ok(), "{result:?}");
        let liquid = reg.liquid_by_name("mods-test-fluid").expect("liquid");
        assert_eq!(liquid.name, "mods-test-fluid");
        assert!(liquid.effect != StatusId::NONE, "status resolved");
    }

    #[test]
    fn status_transitions() {
        let mut reg = registry();
        let (_, result) = parse(
            &mut reg,
            ContentType::Status,
            "test-aura",
            r#"{"name":"Test Aura","speedMultiplier":1.5,"opposites":["wet"],"color":"00ff00"}"#,
        );
        assert!(result.is_ok(), "{result:?}");
        let status = reg.status_by_name("mods-test-aura").expect("status");
        assert!((status.speed_multiplier - 1.5).abs() < 1e-6);
        assert_eq!(status.opposites.len(), 1);
    }

    #[test]
    fn weather_parity() {
        let mut reg = registry();
        let (_, result) = parse(
            &mut reg,
            ContentType::Weather,
            "test-storm",
            r#"{"name":"Test Storm","type":"ParticleWeather","duration":100,"hidden":true}"#,
        );
        assert!(result.is_ok(), "{result:?}");
        let weather = reg.weather_by_name("mods-test-storm").expect("weather");
        assert!(matches!(weather.kind, WeatherKind::Particle));
        assert!(weather.hidden);
    }

    #[test]
    fn team_parity() {
        let mut reg = registry();
        let (_, result) = parse(
            &mut reg,
            ContentType::Team,
            "test-crew",
            r#"{"name":"Test Crew","team":"test-crew"}"#,
        );
        assert!(result.is_ok(), "{result:?}");
        assert!(reg.team_by_name("mods-test-crew").is_some());
    }

    #[test]
    fn sector_planet_team_parity() {
        let mut reg = registry();
        let (_, result) = parse(
            &mut reg,
            ContentType::Planet,
            "test-world",
            r#"{"name":"Test World","radius":2.0,"sectorSize":2,"mesh":"hexSphere","accessible":true}"#,
        );
        assert!(result.is_ok(), "{result:?}");
        let planet = reg.planet_by_name("mods-test-world").expect("planet");
        assert_eq!(planet.radius, 2.0);
        assert_eq!(planet.sector_tiles, 2);
        assert_eq!(planet.sectors.len(), planet.sector_count);

        let planet_id = reg.planet_id("mods-test-world").expect("planet id");
        let json = format!(
            r#"{{"name":"Test Landing","planet":"mods-test-world","sector":{},"difficulty":1.5}}"#,
            planet_id.raw()
        );
        let (_, result) = parse(&mut reg, ContentType::Sector, "test-landing", &json);
        assert!(result.is_ok(), "{result:?}");
        let sector = reg.sector_by_name("mods-test-landing").expect("sector");
        assert!((sector.difficulty - 1.5).abs() < 1e-6);
    }

    #[test]
    fn unit_weapons_nested() {
        let mut reg = registry();
        let (_, result) = parse(
            &mut reg,
            ContentType::Unit,
            "test-mech",
            r#"{"name":"Test Mech","type":"mech","health":500,"speed":0.6,
                "weapons":[{"name":"test-gun","mirror":true,"reload":30,
                  "bullet":{"type":"LaserBulletType","damage":10,"speed":5}}]}"#,
        );
        assert!(result.is_ok(), "{result:?}");
        let unit = reg.unit_by_name("mods-test-mech").expect("unit");
        assert_eq!(unit.health, 500.0);
        assert_eq!(unit.weapons.len(), 1);
        assert_eq!(unit.weapons[0].name, "test-gun");
        assert!(unit.weapons[0].bullet.id.raw() >= 112);
    }

    #[test]
    fn null_field_refused() {
        let mut reg = registry();
        for folder in [
            ContentType::Item,
            ContentType::Block,
            ContentType::Liquid,
            ContentType::Status,
            ContentType::Unit,
            ContentType::Weather,
            ContentType::Sector,
            ContentType::Planet,
            ContentType::Team,
        ] {
            let (_, result) = parse(&mut reg, folder, "bad-null", r#"{"name":null}"#);
            let error = result.expect_err("null name refused");
            assert!(error.message.contains("cannot be null"), "{error}");
        }
    }
}
