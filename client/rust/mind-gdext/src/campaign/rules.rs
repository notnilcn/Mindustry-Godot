// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Rules surface: planet/default environments and the campaign rule get/set pairs.
//! Upstream: ui/dialogs/CustomRulesDialog.java; ui/dialogs/CampaignRulesDialog.java; type/Planet.java (applyRules)

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// `Planet.applyRules(rules, true)` over a rules JSON (`CustomRulesDialog`'s
    /// planet buttons, `CustomRulesDialog.java:287-306` / `Planet.java:286-306`).
    /// Returns the updated rules JSON (the input unchanged on failure).
    #[func]
    pub fn apply_planet_rules(&mut self, planet: GString, rules_json: GString) -> GString {
        let text = rules_json.to_string();
        let Ok(mut rules) = serde_json::from_str::<Rules>(&text) else {
            return rules_json;
        };
        let Some(registry) = self.registry.as_ref() else {
            return rules_json;
        };
        let Some(def) = planet_def(registry, &planet.to_string()) else {
            return rules_json;
        };
        let record = mind_core::game::planet::Planet::from_def(registry, def, 0.0);
        let is_game = self.session.phase.is_game();
        record.apply_rules(registry, &mut rules, true, is_game);
        match serde_json::to_string(&rules) {
            Ok(json) => GString::from(json.as_str()),
            Err(_) => rules_json,
        }
    }

    /// `Planets.<name>.defaultEnv` as a bitmask (`CustomRulesDialog`'s Erekir
    /// checks). `0` for an unknown planet.
    #[func]
    pub fn planet_default_env(&self, planet: GString) -> i64 {
        let Some(registry) = self.registry.as_ref() else {
            return 0;
        };
        planet_def(registry, &planet.to_string())
            .map(|def| {
                i64::from(mind_core::game::planet::Planet::from_def(registry, def, 0.0).default_env)
            })
            .unwrap_or(0)
    }

    /// `Vars.defaultEnv` (`Vars.java:65`): `terrestrial | spores | groundOil |
    /// groundWater | oxygen` = 233. Used by `@rules.anyenv`.
    #[func]
    pub fn default_env(&self) -> i64 {
        use mind_core::content::EnvFlag;
        i64::from(
            mind_core::game::planet::env_bit(EnvFlag::Terrestrial)
                | mind_core::game::planet::env_bit(EnvFlag::Spores)
                | mind_core::game::planet::env_bit(EnvFlag::GroundOil)
                | mind_core::game::planet::env_bit(EnvFlag::GroundWater)
                | mind_core::game::planet::env_bit(EnvFlag::Oxygen),
        )
    }

    /// Planets the rules dialog may switch to (`content.planets().select(p ->
    /// p.accessible && p.visible && p.isLandable())`,
    /// `CustomRulesDialog.java:287`): `[{name}]` in content order.
    #[func]
    pub fn rules_planets(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        for def in registry.planets() {
            if !def.accessible || !def.visible || def.sectors.is_empty() {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set(&GString::from("name"), &def.name.as_str().to_variant());
            out.push(&row);
        }
        out
    }

    /// Current rules as JSON.
    #[func]
    pub fn get_rules_json(&self) -> GString {
        to_gstring(&self.session.rules, "{}")
    }

    /// Applies a rules JSON blob; campaign edits require `allowEditRules`.
    ///
    /// `PausedDialog`'s live edit (`PausedDialog.java:20-37`):
    /// `rulesDialog.hidden(() -> { Vars.state.rules = toEdit; Call.setRules(toEdit); })`.
    /// While a match runs the runtime session is authoritative (`pump_runtime`
    /// mirrors it back), so the accepted rules are written into the live
    /// `CampaignRuntime` and its world state is re-derived
    /// (`CampaignRuntime.rules_world_dirty`).
    #[func]
    pub fn set_rules_json(&mut self, json: GString) -> bool {
        // `JsonIo::read` coerces whole floats (`1.0` -> `1`), so a rules blob
        // round-tripped through GDScript's JSON parses like Arc's reader.
        let incoming = match mind_core::io::json::JsonIo::read::<Rules>(&json.to_string()) {
            Ok(rules) => rules,
            Err(error) => {
                log::warn!("set_rules_json: rules JSON did not parse: {error}");
                return false;
            }
        };
        let is_campaign = self.session.is_campaign();
        if let Err(error) = apply_set_rules(
            &mut self.session.rules,
            incoming.clone(),
            is_campaign,
            None,
            &mut self.epoch,
        ) {
            log::warn!("set_rules_json: rejected: {error}");
            return false;
        }
        // Mirror into the runtime copy (`Call.setRules`); the facade session is
        // refreshed by the next pump.
        let accepted = with_host_runtime(self.host(), |runtime| {
            let runtime_is_campaign = runtime.session.is_campaign();
            let accepted = match apply_set_rules(
                &mut runtime.session.rules,
                incoming,
                runtime_is_campaign,
                None,
                &mut self.epoch,
            ) {
                Ok(_) => true,
                Err(error) => {
                    log::warn!("set_rules_json: runtime rejected: {error}");
                    false
                }
            };
            if accepted {
                runtime.rules_world_dirty = true;
            }
            accepted
        });
        accepted.unwrap_or(true)
    }

    /// Writes one `CampaignRules` field for a planet and persists it
    /// (`campaign_rules_dialog.gd`'s `set_campaign_rule(planet, key, value)`;
    /// Java `PlanetDialog.rules`). Re-applies the planet rules on the live
    /// session when the edited planet is the one being played.
    #[func]
    pub fn set_campaign_rule(&mut self, planet: GString, key: GString, value: Variant) -> bool {
        let planet_name = planet.to_string();
        let key = key.to_string();
        let Some(campaign) = self.campaign.as_ref() else {
            return false;
        };
        let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
            return false;
        };
        let Some(record) = campaign.planet(planet_id) else {
            return false;
        };
        let Ok(mut json) = serde_json::to_value(&record.campaign_rules) else {
            return false;
        };
        let Some(object) = json.as_object_mut() else {
            return false;
        };
        let value = if let Ok(flag) = value.try_to::<bool>() {
            serde_json::Value::Bool(flag)
        } else if let Ok(number) = value.try_to::<f64>() {
            serde_json::Value::from(number)
        } else if let Ok(text) = value.try_to::<GString>() {
            serde_json::Value::String(text.to_string())
        } else {
            log::warn!("set_campaign_rule: unsupported value type for `{key}`");
            return false;
        };
        object.insert(key.clone(), value);
        let Ok(rules) = serde_json::from_value::<CampaignRules>(json) else {
            return false;
        };
        let Some(updated) = self.with_campaign_mut(|campaign, _settings| {
            let present = campaign.planet(planet_id).is_some();
            // `Campaign::set_campaign_rules` (H01): the planet-rules write half.
            campaign.set_campaign_rules(planet_id, rules);
            present
        }) else {
            return false;
        };
        if !updated {
            return false;
        }
        // Re-apply on the played planet (`Planet.applyRules(state.rules)`;
        // `play::reapply_planet_rules`, H02).
        if self
            .session
            .sector
            .is_some_and(|(played, _)| played == planet_id)
            && let Some(registry) = self.registry.take()
        {
            let applied = match self.campaign.as_ref() {
                Some(campaign) => {
                    play::reapply_planet_rules(&mut self.session, campaign, &registry)
                }
                None => None,
            };
            if let Some(applied) = applied {
                if applied.rts_swapped {
                    log::info!("campaign rule `{key}` triggered the RTS-AI swap");
                }
                self.push_session_rules_to_runtime();
            }
            self.registry = Some(registry);
        }
        self.persist();
        true
    }

    /// The active planet's `CampaignRules` as JSON (GAP-11).
    #[func]
    pub fn get_campaign_rules_json(&self) -> GString {
        let planet = self.session.sector.map(|(planet, _)| planet);
        let rules = planet
            .and_then(|planet| {
                self.campaign
                    .as_ref()
                    .and_then(|campaign| campaign.planet(planet))
            })
            .map(|planet| planet.campaign_rules.clone())
            .unwrap_or_default();
        to_gstring(&rules, "{}")
    }

    /// Applies a `CampaignRules` JSON blob to the active planet, folds it into
    /// the session rules and persists it (GAP-11 / C-9).
    ///
    /// T1: the planet record is written on the live campaign; the runtime's
    /// session rules are updated in a separate statement. The runtime campaign
    /// is never replaced by the facade snapshot.
    #[func]
    pub fn set_campaign_rules_json(&mut self, json: GString) -> bool {
        let Ok(rules) = serde_json::from_str::<CampaignRules>(&json.to_string()) else {
            return false;
        };
        let Some((planet_id, _)) = self.session.sector else {
            return false;
        };
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let Some(found) = self.with_campaign_mut(|campaign, _settings| {
            let present = campaign.planet(planet_id).is_some();
            // `Campaign::set_campaign_rules` (H01): the planet-rules write half.
            campaign.set_campaign_rules(planet_id, rules);
            present
        }) else {
            self.registry = Some(registry);
            return false;
        };
        if !found {
            self.registry = Some(registry);
            return false;
        }
        // `PlanetDialog.rules` -> `Planet.applyRules(state.rules)`
        // (`Planet.java:308-309`); `play::reapply_planet_rules` (H02).
        let applied = match self.campaign.as_ref() {
            Some(campaign) => play::reapply_planet_rules(&mut self.session, campaign, &registry),
            None => None,
        };
        let Some(applied) = applied else {
            self.registry = Some(registry);
            return false;
        };
        if applied.rts_swapped {
            log::info!("campaign rules RTS-AI swap (controller reset is plan-11)");
        }
        // Mirror into the live runtime session; `runtime.campaign` stays as-is.
        self.push_session_rules_to_runtime();
        self.registry = Some(registry);
        self.persist();
        true
    }
}
