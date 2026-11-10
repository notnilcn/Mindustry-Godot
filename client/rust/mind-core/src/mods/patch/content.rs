// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Item, liquid, status, weather, planet, sector and team patching.
//!
//! Upstream: core/src/mindustry/mod/DataPatcher.java

use super::*;

impl DataPatcher {
    // ---- item ----

    pub(super) fn apply_item(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.item_id(name) else {
            self.warn(format!("unknown item `{name}`"));
            return;
        };
        let reference = ContentRef::item(id);
        match field {
            "hardness" => self.set_i32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.hardness),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.hardness = v;
                    }
                },
            ),
            "cost" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.cost),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.cost = v;
                    }
                },
            ),
            "explosiveness" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.explosiveness),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.explosiveness = v;
                    }
                },
            ),
            "flammability" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.flammability),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.flammability = v;
                    }
                },
            ),
            "radioactivity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.radioactivity),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.radioactivity = v;
                    }
                },
            ),
            "charge" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.charge),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.charge = v;
                    }
                },
            ),
            "buildable" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.buildable),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.buildable = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.hidden),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.hidden = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `item.{name}.{field}`")),
        }
    }

    // ---- liquid ----

    pub(super) fn apply_liquid(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.liquid_id(name) else {
            self.warn(format!("unknown liquid `{name}`"));
            return;
        };
        let reference = ContentRef::liquid(id);
        match field {
            "gas" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.gas),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.gas = v;
                    }
                },
            ),
            "coolant" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.coolant),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.coolant = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.hidden),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.hidden = v;
                    }
                },
            ),
            "temperature" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.temperature),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.temperature = v;
                    }
                },
            ),
            "heatCapacity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.heat_capacity),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.heat_capacity = v;
                    }
                },
            ),
            "viscosity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.viscosity),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.viscosity = v;
                    }
                },
            ),
            "flammability" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.flammability),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.flammability = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `liquid.{name}.{field}`")),
        }
    }

    // ---- status ----

    pub(super) fn apply_status(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.status_id(name) else {
            self.warn(format!("unknown status `{name}`"));
            return;
        };
        let reference = ContentRef::status(id);
        match field {
            "speedMultiplier" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.speed_multiplier),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.speed_multiplier = v;
                    }
                },
            ),
            "damageMultiplier" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.damage_multiplier),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.damage_multiplier = v;
                    }
                },
            ),
            "healthMultiplier" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.health_multiplier),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.health_multiplier = v;
                    }
                },
            ),
            "disarm" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.disarm),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.disarm = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `status.{name}.{field}`")),
        }
    }

    // ---- weather / planet / sector / team (scalar subset) ----

    pub(super) fn apply_weather(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.weather_by_name(name).map(|w| w.id) else {
            self.warn(format!("unknown weather `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Weather, id.raw());
        match field {
            "duration" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.weather(id).map(|w| w.duration),
                move |r, v| {
                    if let Some(w) = r.weather_mut(crate::content::WeatherId::new(id.raw())) {
                        w.duration = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.weather(id).map(|w| w.hidden),
                move |r, v| {
                    if let Some(w) = r.weather_mut(crate::content::WeatherId::new(id.raw())) {
                        w.hidden = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `weather.{name}.{field}`")),
        }
    }

    pub(super) fn apply_planet(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.planet_id(name) else {
            self.warn(format!("unknown planet `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Planet, id.raw());
        match field {
            "radius" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.planet(id).map(|p| p.radius),
                move |r, v| {
                    if let Some(p) = r.planet_mut(PlanetId::new(id.raw())) {
                        p.radius = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `planet.{name}.{field}`")),
        }
    }

    pub(super) fn apply_sector(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.sector_by_name(name).map(|s| s.id) else {
            self.warn(format!("unknown sector `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Sector, id.raw());
        match field {
            "difficulty" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.sector(id).map(|s| s.difficulty),
                move |r, v| {
                    if let Some(s) = r.sector_mut(crate::content::SectorId::new(id.raw())) {
                        s.difficulty = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `sector.{name}.{field}`")),
        }
    }

    pub(super) fn apply_team(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.team_by_name(name).map(|t| t.id) else {
            self.warn(format!("unknown team `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Team, id.raw());
        match field {
            "team" => {
                if let Some(new) = value.as_str() {
                    if mode != FieldMode::Set {
                        return;
                    }
                    let Some(original) = registry.team(id).map(|t| t.team.clone()) else {
                        return;
                    };
                    if self.mark_used(reference, "team") {
                        let raw = id.raw();
                        self.push_reset(move |r: &mut ContentRegistry| {
                            if let Some(t) = r.team_mut(crate::content::TeamEntryId::new(raw)) {
                                t.team = original.clone();
                            }
                        });
                    }
                    if let Some(t) = registry.team_mut(id) {
                        t.team = new.to_owned();
                    }
                }
            }
            _ => self.warn(format!("unknown field `team.{name}.{field}`")),
        }
    }
}
