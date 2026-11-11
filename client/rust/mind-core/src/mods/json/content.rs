// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Item plus the plan, liquid, status, weather, team, sector and planet
//! content kinds of the JSON content parser.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    // ---- item ----

    pub(super) fn parse_item(
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
        let mut item = Item::new(&name, color, &self.bundle, self.store);
        if let Some(hardness) = field_i32(object, "hardness") {
            item.hardness = hardness;
        }
        if let Some(cost) = field_f32(object, "cost") {
            item.cost = cost;
        }
        set_f32s(
            object,
            &mut [
                ("explosiveness", &mut item.explosiveness),
                ("flammability", &mut item.flammability),
                ("radioactivity", &mut item.radioactivity),
                ("charge", &mut item.charge),
                ("healthScaling", &mut item.health_scaling),
                ("frameTime", &mut item.frame_time),
            ],
        );
        set_bools(
            object,
            &mut [
                ("lowPriority", &mut item.low_priority),
                ("buildable", &mut item.buildable),
                ("hidden", &mut item.hidden),
            ],
        );
        set_i32s(
            object,
            &mut [
                ("frames", &mut item.frames),
                ("transitionFrames", &mut item.transition_frames),
            ],
        );
        self.apply_unlock_fields(registry, file, object, &mut item.unlock);
        self.warn_unknown(file, "item", object, ITEM_FIELDS);
        registry
            .add_item(item)
            .map(ContentRef::item)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    /// `UnitFactory.plans` entries (`UnitPlan{unit, time, requirements}`,
    /// `UnitFactory.java:211-223`). `requirements` is an `ItemStack[]`; the
    /// plan's unit resolves against the registry in `BlockDef::from_spec`.
    pub(super) fn parse_unit_plans(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        entries: &[Value],
    ) -> Result<Vec<UnitPlanSpec>, ContentParseError> {
        let mut plans = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(plan) = entry.as_object() else {
                parse_err!("{file}: `plans` entries must be objects");
            };
            let unit = plan.get("unit").and_then(Value::as_str).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unit plan is missing `unit`"))
            })?;
            let requirements = match plan.get("requirements") {
                Some(value) => self.parse_stacks(registry, file, value)?,
                None => Vec::new(),
            };
            plans.push(UnitPlanSpec {
                unit: Box::leak(unit.to_owned().into_boxed_str()),
                time: field_f32(plan, "time").unwrap_or(0.0),
                requirements,
            });
        }
        Ok(plans)
    }

    /// `UnitAssembler.plans` entries (`AssemblerUnitPlan{unit, time,
    /// requirements}`, `UnitAssembler.java:280-294`). `requirements` is a
    /// `Seq<PayloadStack>`; each `item` is a unit or block content name
    /// (`PayloadStack.item` is an `UnlockableContent`).
    pub(super) fn parse_assembler_plans(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        entries: &[Value],
    ) -> Result<Vec<AssemblerUnitPlanSpec>, ContentParseError> {
        let mut plans = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(plan) = entry.as_object() else {
                parse_err!("{file}: `plans` entries must be objects");
            };
            let unit = plan.get("unit").and_then(Value::as_str).ok_or_else(|| {
                ContentParseError::new(format!("{file}: assembler plan is missing `unit`"))
            })?;
            let mut payloads = Vec::new();
            if let Some(Value::Array(stacks)) = plan.get("requirements") {
                for stack in stacks {
                    let Some(map) = stack.as_object() else {
                        parse_err!("{file}: assembler plan requirements must be objects");
                    };
                    let raw = map.get("item").and_then(Value::as_str).ok_or_else(|| {
                        ContentParseError::new(format!(
                            "{file}: assembler plan requirement is missing `item`"
                        ))
                    })?;
                    let amount = field_i32(map, "amount").unwrap_or(1);
                    // Payloads are units or blocks; resolve the unit table first
                    // (the common case) then the block table.
                    let payload = if let Some(reference) =
                        self.resolve_prefixed(registry, ContentType::Unit, raw)
                    {
                        let name = registry
                            .unit(crate::content::UnitTypeId::new(reference.id))
                            .map_or(raw, |record| record.name.as_str())
                            .to_owned();
                        PayloadStackSpec {
                            name: Box::leak(name.into_boxed_str()),
                            block: false,
                            amount,
                        }
                    } else if let Some(reference) =
                        self.resolve_prefixed(registry, ContentType::Block, raw)
                    {
                        let name = registry
                            .block(crate::content::BlockId::new(reference.id))
                            .map_or(raw, |record| record.name.as_str())
                            .to_owned();
                        PayloadStackSpec {
                            name: Box::leak(name.into_boxed_str()),
                            block: true,
                            amount,
                        }
                    } else {
                        parse_err!("{file}: unknown assembler payload `{raw}`");
                    };
                    payloads.push(payload);
                }
            }
            plans.push(AssemblerUnitPlanSpec {
                unit: Box::leak(unit.to_owned().into_boxed_str()),
                time: field_f32(plan, "time").unwrap_or(0.0),
                payloads,
            });
        }
        Ok(plans)
    }

    // ---- liquid ----

    pub(super) fn parse_liquid(
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
        let mut liquid = Liquid::new(&name, color, &self.bundle, self.store);
        set_f32s(
            object,
            &mut [
                ("temperature", &mut liquid.temperature),
                ("heatCapacity", &mut liquid.heat_capacity),
                ("viscosity", &mut liquid.viscosity),
                ("explosiveness", &mut liquid.explosiveness),
                ("flammability", &mut liquid.flammability),
                ("particleSpacing", &mut liquid.particle_spacing),
                ("boilPoint", &mut liquid.boil_point),
            ],
        );
        set_bools(
            object,
            &mut [
                ("gas", &mut liquid.gas),
                ("blockReactive", &mut liquid.block_reactive),
                ("coolant", &mut liquid.coolant),
                ("moveThroughBlocks", &mut liquid.move_through_blocks),
                ("incinerable", &mut liquid.incinerable),
                ("capPuddles", &mut liquid.cap_puddles),
                ("hidden", &mut liquid.hidden),
            ],
        );
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
        if let Some(effect) = object.get("particleEffect")
            && let Some(effect) = self.parse_effect_ref(file, effect)
        {
            liquid.particle_effect = effect;
        }
        if let Some(effect) = object.get("vaporEffect")
            && let Some(effect) = self.parse_effect_ref(file, effect)
        {
            liquid.vapor_effect = effect;
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
        self.apply_unlock_fields(registry, file, object, &mut liquid.unlock);
        self.warn_unknown(file, "liquid", object, LIQUID_FIELDS);
        registry
            .add_liquid(liquid)
            .map(ContentRef::liquid)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    // ---- status ----

    pub(super) fn parse_status(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Status, stem, object);
        self.read_bundle(object, ContentType::Status, &name);
        let mut status = StatusEffect::new(&name, &self.bundle, self.store);
        set_f32s(
            object,
            &mut [
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
            ],
        );
        set_bools(
            object,
            &mut [
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
            ],
        );
        if let Some(color) = object.get("color").and_then(parse_color) {
            status.color = color;
        }
        if let Some(color) = object.get("applyColor").and_then(parse_color) {
            status.apply_color = color;
        }
        if let Some(effect) = object.get("effect")
            && let Some(effect) = self.parse_effect_ref(file, effect)
        {
            status.effect = effect;
        }
        if let Some(effect) = object.get("applyEffect")
            && let Some(effect) = self.parse_effect_ref(file, effect)
        {
            status.apply_effect = effect;
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
        self.apply_unlock_fields(registry, file, object, &mut status.unlock);
        self.warn_unknown(file, "status", object, STATUS_FIELDS);
        registry
            .add_status(status)
            .map(ContentRef::status)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))
    }

    // ---- weather ----

    pub(super) fn parse_weather(
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
        let mut weather = WeatherDef::new(&name, kind, &self.bundle, self.store);
        set_f32s(
            object,
            &mut [
                ("duration", &mut weather.duration),
                ("opacityMultiplier", &mut weather.opacity_multiplier),
                ("soundVol", &mut weather.sound_vol),
                ("soundVolMin", &mut weather.sound_vol_min),
                ("soundVolOscMag", &mut weather.sound_vol_osc_mag),
                ("soundVolOscScl", &mut weather.sound_vol_osc_scl),
                ("statusDuration", &mut weather.status_duration),
            ],
        );
        set_bools(
            object,
            &mut [
                ("hidden", &mut weather.hidden),
                ("statusAir", &mut weather.status_air),
                ("statusGround", &mut weather.status_ground),
            ],
        );
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
            set_f32s(
                object,
                &mut [
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
                ],
            );
            if let Some(color) = object.get("color").and_then(parse_color) {
                particle.color = color;
            }
        }
        if let Some(rain) = weather.rain.as_mut() {
            set_f32s(
                object,
                &mut [
                    ("yspeed", &mut rain.yspeed),
                    ("xspeed", &mut rain.xspeed),
                    ("padding", &mut rain.padding),
                    ("density", &mut rain.density),
                    ("stroke", &mut rain.stroke),
                ],
            );
            if let Some(color) = object.get("color").and_then(parse_color) {
                rain.color = color;
            }
            if let Some(raw) = object.get("liquid").and_then(Value::as_str)
                && let Some(reference) = self.resolve_prefixed(registry, ContentType::Liquid, raw)
            {
                rain.liquid = crate::content::LiquidId::new(reference.id);
            }
        }
        self.apply_unlock_fields(registry, file, object, &mut weather.unlock);
        self.warn_unknown(file, "weather", object, WEATHER_FIELDS);
        let id = registry
            .add_weather(weather)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Weather, id.raw()))
    }

    // ---- team ----

    pub(super) fn parse_team(
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
        let mut entry = TeamEntry::new(&name, team, &self.bundle, self.store);
        self.apply_unlock_fields(registry, file, object, &mut entry.unlock);
        self.warn_unknown(file, "team", object, TEAM_FIELDS);
        let id = registry
            .add_team(entry)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Team, id.raw()))
    }

    // ---- sector ----

    pub(super) fn parse_sector(
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
        let mut sector = SectorPresetDef::new(&name, planet, index, &self.bundle, self.store);
        if let Some(value) = field_i32(object, "captureWave") {
            sector.capture_wave = value;
        }
        if let Some(value) = field_f32(object, "difficulty") {
            sector.difficulty = value;
        }
        if let Some(value) = field_f32(object, "startWaveTimeMultiplier") {
            sector.start_wave_time_multiplier = value;
        }
        set_bools(
            object,
            &mut [
                ("addStartingItems", &mut sector.add_starting_items),
                ("noLighting", &mut sector.no_lighting),
                ("isLastSector", &mut sector.is_last_sector),
                ("requireUnlock", &mut sector.require_unlock),
                ("showHidden", &mut sector.show_hidden),
                ("attackAfterWaves", &mut sector.attack_after_waves),
                ("showSectorLandInfo", &mut sector.show_sector_land_info),
                ("outline", &mut sector.outline),
            ],
        );
        if let Some(value) = field_i32(object, "outlineRadius") {
            sector.outline_radius = value;
        }
        self.apply_unlock_fields(registry, file, object, &mut sector.unlock);
        self.warn_unknown(file, "sector", object, SECTOR_FIELDS);
        let id = registry
            .add_sector(sector)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Sector, id.raw()))
    }

    // ---- planet ----

    pub(super) fn parse_planet(
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
        let mut planet = PlanetDef::new(&name, parent, radius, &self.bundle, self.store);
        // `allowCampaignRules = isVanilla()` (`Planet.java:221`): false for mod
        // planets unless the JSON sets it.
        planet.allow_campaign_rules = false;
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
        set_bools(
            object,
            &mut [
                ("bloom", &mut planet.bloom),
                ("accessible", &mut planet.accessible),
                ("visible", &mut planet.visible),
                ("hasAtmosphere", &mut planet.has_atmosphere),
                ("updateLighting", &mut planet.update_lighting),
                ("allowLaunchSchematics", &mut planet.allow_launch_schematics),
                ("allowLaunchLoadout", &mut planet.allow_launch_loadout),
                ("allowSectorInvasion", &mut planet.allow_sector_invasion),
                ("allowWaves", &mut planet.allow_waves),
                ("prebuildBase", &mut planet.prebuild_base),
                ("allowCampaignRules", &mut planet.allow_campaign_rules),
                ("drawOrbit", &mut planet.draw_orbit),
            ],
        );
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
        self.apply_unlock_fields(registry, file, object, &mut planet.unlock);
        self.warn_unknown(file, "planet", object, PLANET_FIELDS);
        let id = registry
            .add_planet(planet)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        Ok(ContentRef::new(ContentType::Planet, id.raw()))
    }
}
