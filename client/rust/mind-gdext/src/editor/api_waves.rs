// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Wave, locale and spawn data plus the wave-graph series.
//! Upstream: core/src/mindustry/editor/WaveInfoDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    /// `WaveInfoDialog`: the `spawns` JSON.
    #[func]
    pub fn waves_json(&self) -> GString {
        match self.editor.tags.get("spawns") {
            Some(json) if !json.trim().is_empty() => GString::from(json.as_str()),
            _ => GString::from("[]"),
        }
    }

    /// Stores the `spawns` JSON after validating each group.
    #[func]
    pub fn set_waves_json(&mut self, json: GString) -> bool {
        let value: serde_json::Value = match serde_json::from_str(&json.to_string()) {
            Ok(value) => value,
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        let Some(items) = value.as_array() else {
            self.last_error_key = Some("@editor.errorload".to_owned());
            self.last_error_message = Some("spawns must be an array".to_owned());
            return false;
        };
        for item in items {
            if let Err(error) = mind_core::game::spawn_group::SpawnGroup::from_json(item) {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error);
                return false;
            }
        }
        self.editor
            .tags
            .insert("spawns".to_owned(), json.to_string());
        self.clear_error();
        true
    }

    /// `MapLocalesDialog`: the `locales` JSON.
    #[func]
    pub fn locales_json(&self) -> GString {
        match self.editor.tags.get("locales") {
            Some(json) if !json.trim().is_empty() => GString::from(json.as_str()),
            _ => GString::from("{}"),
        }
    }

    /// Stores the `locales` JSON after validating it parses.
    #[func]
    pub fn set_locales_json(&mut self, json: GString) -> bool {
        match mind_core::maps::locales::parse_json(&json.to_string()) {
            Ok(locales) => match mind_core::maps::locales::write_json(&locales) {
                Ok(canonical) => {
                    self.editor.tags.insert("locales".to_owned(), canonical);
                    self.clear_error();
                    true
                }
                Err(error) => {
                    self.last_error_key = Some("@editor.errorload".to_owned());
                    self.last_error_message = Some(error.to_string());
                    false
                }
            },
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                false
            }
        }
    }

    /// `WaveInfoDialog.showUnits`: unit rows (`{name, health}`) for the pickers
    /// and the health sort; icons are omitted.
    #[func]
    pub fn wave_units_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let rows: Vec<serde_json::Value> = content
            .units()
            .iter()
            .map(|unit| {
                serde_json::json!({
                    "name": unit.name,
                    "health": unit.health,
                })
            })
            .collect();
        GString::from(serde_json::to_string(&rows).unwrap_or_default().as_str())
    }

    /// `WaveInfoDialog.showEffects`: the status-effect names (`content.statusEffects()`).
    #[func]
    pub fn wave_effects_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let names: Vec<&str> = content
            .statuses()
            .iter()
            .map(|status| status.name.as_str())
            .collect();
        GString::from(serde_json::to_string(&names).unwrap_or_default().as_str())
    }

    /// `WaveInfoDialog`'s `@waves.random`: `groups = Waves.generate(1f / 10f)`.
    #[func]
    pub fn randomize_waves(&mut self) -> bool {
        let groups = mind_core::game::waves::Waves::generate(0.1);
        let values: Vec<serde_json::Value> = groups.iter().map(|group| group.to_json()).collect();
        let json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned());
        self.set_waves_json(GString::from(json.as_str()))
    }

    /// `WaveInfoDialog`'s spawn picker (`spawner.getSpawns()`): the map's
    /// spawn-overlay tiles packed `Point2.pack(x, y)`.
    #[func]
    pub fn spawn_points_json(&self) -> GString {
        let Some(spawn) = self
            .content
            .as_ref()
            .and_then(|content| content.block_id("spawn"))
        else {
            return GString::from("[]");
        };
        let mut packed: Vec<i32> = Vec::new();
        for y in 0..self.grid.height() {
            for x in 0..self.grid.width() {
                if self.grid.tile(x, y).overlay == spawn {
                    packed.push(mind_core::math::rand_arc::pack_point2(x, y) as i32);
                }
            }
        }
        GString::from(serde_json::to_string(&packed).unwrap_or_default().as_str())
    }

    /// `WaveInfoDialog`'s `@settings.reset`: the vanilla wave table
    /// (`JsonIO.copy(waves.get())`).
    #[func]
    pub fn default_waves_json(&self) -> GString {
        let mut waves = mind_core::game::waves::Waves::default();
        let values: Vec<serde_json::Value> =
            waves.get().iter().map(|group| group.to_json()).collect();
        GString::from(serde_json::to_string(&values).unwrap_or_default().as_str())
    }

    /// `WaveGraph.rebuild`: the counts/health series for the `spawns` tag.
    #[func]
    pub fn wave_graph(&self, from: i64, to: i64) -> VarDictionary {
        use mind_core::editor::wave_graph::{WaveGraphData, next_step};
        let Some(content) = self.content.as_ref() else {
            return dict(vec![("from", from.to_variant()), ("to", to.to_variant())]);
        };
        let groups = parse_spawn_groups(self.editor.tags.get("spawns").map(String::as_str));
        let resolve = |name: &str| {
            content
                .unit_by_name(name)
                .map(|def| (def.id.raw(), def.health))
        };
        let data = WaveGraphData::compute(&groups, from as i32, to as i32, resolve);

        let mut units: Array<VarDictionary> = Array::new();
        for unit in &data.units {
            units.push(&dict(vec![
                ("id", (unit.id as i64).to_variant()),
                ("name", GString::from(unit.name.as_str()).to_variant()),
                ("health", (unit.health as f64).to_variant()),
            ]));
        }
        let mut series: Array<Variant> = Array::new();
        for row in &data.series {
            let mut values = PackedInt64Array::new();
            for value in row {
                values.push(*value as i64);
            }
            series.push(&values.to_variant());
        }
        let mut totals = PackedInt64Array::new();
        for value in &data.totals {
            totals.push(*value as i64);
        }
        let mut health: Array<Variant> = Array::new();
        for value in &data.health {
            health.push(&(*value as f64).to_variant());
        }

        dict(vec![
            ("from", (data.from as i64).to_variant()),
            ("to", (data.to as i64).to_variant()),
            ("units", units.to_variant()),
            ("series", series.to_variant()),
            ("totals", totals.to_variant()),
            ("health", health.to_variant()),
            ("max", (data.max as i64).to_variant()),
            ("max_total", (data.max_total as i64).to_variant()),
            ("max_health", (data.max_health as f64).to_variant()),
            // `nextStep(max)`, `nextStep(maxTotal)`, `nextStep((int)maxHealth)`
            // (`WaveGraph.java:87-91`), indexed by mode.
            (
                "max_y",
                PackedInt64Array::from(
                    &[
                        next_step(data.max as f32) as i64,
                        next_step(data.max_total as f32) as i64,
                        next_step(data.max_health.trunc()) as i64,
                    ][..],
                )
                .to_variant(),
            ),
            // `content.units().size` for `color(type)` (`WaveGraph.java:339-341`).
            ("unit_count", (content.units().len() as i64).to_variant()),
        ])
    }
}
