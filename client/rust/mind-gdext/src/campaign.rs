// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindCampaign` autoload (plan 12 M8/§3.9): the GDScript-callable campaign
//! facade.
//!
//! Campaign state, play flows, objectives, tech and schematics all live in
//! `mind_core::game`; this node is the thin Godot shell the MCP oracle and the
//! plan-14 dialogs drive. It is declared in `res://scenes/game.tscn` (HLP §6.6,
//! tscn-first) rather than registered as a project autoload singleton.
//!
//! In-engine MCP verification (plan 12 §7c) is deferred to the orchestrator's
//! single-editor mutex; this class compiles and exposes the API surface today.

use godot::classes::{INode, Node};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::{
    ContentRef, MemoryBundle, MemoryUnlockStore, PlanetId, SectorId, create_base_content,
};
use mind_core::game::objectives::{ObjectiveContext, SectorStatus};
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::play::{
    PlaySession, play_map, play_new_sector, run_wave_campaign, sector_capture,
};
use mind_core::game::rules_event::{RulesEpoch, apply_set_rules};
use mind_core::game::schematics::Schematics;
use mind_core::game::tech_tree;
use mind_core::game::universe::{Campaign, TurnContext};
use mind_core::game::world_reloader::HostReloader;
use mind_core::io::settings::SettingsStore;
use mind_core::random::JavaRandom;
use mind_core::world::modules::ItemModule;

/// Objective context with every tech objective met (single-player research path).
struct AllObjectivesMet;

impl ObjectiveContext for AllObjectivesMet {
    fn is_unlocked(&self, _content: ContentRef) -> bool {
        true
    }
    fn sector_status(&self, _sector: SectorId) -> SectorStatus {
        SectorStatus::default()
    }
    fn planet_has_base(&self, _planet: PlanetId) -> bool {
        false
    }
}

/// `MindCampaign` — campaign/sector/rules/tech/schematic facade for MCP + UI.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindCampaign {
    base: Base<Node>,
    session: PlaySession,
    campaign: Option<Campaign>,
    registry: Option<mind_core::content::ContentRegistry>,
    store: MemoryUnlockStore,
    settings: SettingsStore,
    epoch: RulesEpoch,
    reloader: HostReloader,
    content_error: Option<String>,
}

#[godot_api]
impl INode for MindCampaign {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            session: PlaySession::default(),
            campaign: None,
            registry: None,
            store: MemoryUnlockStore::new(),
            settings: SettingsStore::new(),
            epoch: RulesEpoch::new(),
            reloader: HostReloader::default(),
            content_error: None,
        }
    }

    fn ready(&mut self) {
        let bundle = MemoryBundle::new();
        match create_base_content(&bundle, &self.store, true) {
            Ok(registry) => {
                let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
                self.registry = Some(registry);
                self.campaign = Some(campaign);
                log::info!("MindCampaign ready");
            }
            Err(error) => {
                self.content_error = Some(error.to_string());
                log::error!("MindCampaign content boot failed: {error}");
            }
        }
    }
}

#[godot_api]
impl MindCampaign {
    /// Starts a campaign sector (host-side launch), returning success.
    #[func]
    pub fn start_sector(&mut self, planet: GString, sector: i32) -> bool {
        let planet_name = planet.to_string();
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let Some(mut campaign) = self.campaign.take() else {
            self.registry = Some(registry);
            return false;
        };
        let planet_id = campaign
            .planet_id_by_name(&planet_name)
            .unwrap_or(PlanetId::new(0));
        let sector_id = sector as u16;

        if let Some(sector) = campaign.sector_mut(planet_id, sector_id) {
            sector.save = Some(format!("sector-{planet_name}-{sector_id}"));
            sector.info.info.has_core = true;
        }
        let events = play_new_sector(
            &mut self.session,
            &mut campaign,
            &registry,
            planet_id,
            sector_id,
            None,
            &mut self.epoch,
            &mut self.reloader,
        );
        self.registry = Some(registry);
        self.campaign = Some(campaign);
        !events.is_empty()
    }

    /// Starts a custom map by name (`Logic.playMap`).
    #[func]
    pub fn start_map(&mut self, name: GString) -> bool {
        let mut rules = self.session.rules.clone();
        rules.mode_name = Some(name.to_string());
        rules.sector = None;
        let events = play_map(&mut self.session, rules, None, |_| false, &mut self.epoch);
        !events.is_empty()
    }

    /// Forces sector capture (`Logic.sectorCapture`).
    #[func]
    pub fn capture_sector(&mut self) -> bool {
        let Some(mut campaign) = self.campaign.take() else {
            return false;
        };
        let events = sector_capture(&mut self.session, &mut campaign);
        self.campaign = Some(campaign);
        !events.is_empty()
    }

    /// Runs one production turn; returns success.
    #[func]
    pub fn run_turn(&mut self) -> bool {
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let Some(mut campaign) = self.campaign.take() else {
            self.registry = Some(registry);
            return false;
        };
        let turn = campaign.universe.turn;
        let mut rand = JavaRandom::new(turn as u64 + 1);
        let report = campaign.run_turn(
            &registry,
            &mut self.settings,
            &TurnContext::default(),
            &mut rand,
        );
        self.registry = Some(registry);
        self.campaign = Some(campaign);
        report.turn > 0
    }

    /// Advances the wave counter (`Logic.runWave`).
    #[func]
    pub fn run_wave(&mut self) -> bool {
        let _ = run_wave_campaign(&mut self.session);
        true
    }

    /// Current rules as JSON.
    #[func]
    pub fn get_rules_json(&self) -> GString {
        GString::from(
            serde_json::to_string(&self.session.rules)
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// Applies a rules JSON blob; campaign edits require `allowEditRules`.
    #[func]
    pub fn set_rules_json(&mut self, json: GString) -> bool {
        let Ok(incoming) = serde_json::from_str::<mind_core::game::rules::Rules>(&json.to_string())
        else {
            return false;
        };
        let is_campaign = self.session.is_campaign();
        apply_set_rules(
            &mut self.session.rules,
            incoming,
            is_campaign,
            None,
            &mut self.epoch,
        )
        .is_ok()
    }

    /// Researches a content name, spending the active sector's items.
    #[func]
    pub fn research(&mut self, content: GString) -> bool {
        let target = content.to_string();
        let Some(registry) = self.registry.as_mut() else {
            return false;
        };
        let Some(content_ref) = registry.tech().nodes.iter().find_map(|node| {
            let content = node.content?;
            (tech_tree::content_name(registry, content).as_deref() == Some(target.as_str()))
                .then_some(content)
        }) else {
            return false;
        };
        if tech_tree::content_unlocked_with_rules(registry, content_ref, &self.session.rules) {
            return true;
        }
        let Some(node_ref) = registry
            .tech()
            .nodes
            .iter()
            .position(|node| node.content == Some(content_ref))
            .map(|index| mind_core::content::tech::TechNodeRef(index as u32))
        else {
            return false;
        };
        let mut items = ItemModule::with_items(registry.items().len());
        tech_tree::spend(
            registry,
            node_ref,
            &mut items,
            &mut self.store,
            &AllObjectivesMet,
            false,
        )
        .is_ok()
    }

    /// Tech-tree summary (`nodes`, `unlocked`, error).
    #[func]
    pub fn get_tech_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        match &self.registry {
            Some(registry) => {
                let nodes = registry.tech().nodes.len() as i64;
                let unlocked = registry
                    .tech()
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.content.is_some_and(|content| {
                            tech_tree::content_unlocked_with_rules(
                                registry,
                                content,
                                &self.session.rules,
                            )
                        })
                    })
                    .count() as i64;
                dict.set(&key("nodes"), &nodes.to_variant());
                dict.set(&key("unlocked"), &unlocked.to_variant());
            }
            None => {
                dict.set(&key("nodes"), &0i64.to_variant());
                dict.set(&key("unlocked"), &0i64.to_variant());
            }
        }
        if let Some(error) = &self.content_error {
            dict.set(&key("error"), &error.as_str().to_variant());
        }
        dict
    }

    /// Completes an objective by index (`complete_objective` relay target).
    #[func]
    pub fn complete_objective(&mut self, index: i64) -> bool {
        if index < 0 {
            return false;
        }
        self.session
            .objectives
            .complete(index as usize, &mut self.session.rules)
    }

    /// Per-objective state rows (`index`, `type`, `completed`).
    #[func]
    pub fn get_objective_state(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for index in 0..self.session.objectives.len() {
            let mut row = VarDictionary::new();
            row.set(&GString::from("index"), &(index as i64).to_variant());
            if let Some(kind) = self.session.objectives.type_name(index) {
                row.set(&GString::from("type"), &kind.as_str().to_variant());
            }
            row.set(
                &GString::from("completed"),
                &self.session.objectives.is_completed(index).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Current sector/rules summary.
    #[func]
    pub fn get_sector_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(
            &GString::from("campaign"),
            &self.session.is_campaign().to_variant(),
        );
        dict.set(&GString::from("wave"), &self.session.wave.to_variant());
        dict.set(
            &GString::from("waves"),
            &self.session.rules.waves.to_variant(),
        );
        dict.set(
            &GString::from("attack"),
            &self.session.rules.attack_mode.to_variant(),
        );
        dict.set(
            &GString::from("hasCore"),
            &(self.session.player_core_count() > 0).to_variant(),
        );
        if let Some((planet, sector)) = self.session.sector {
            let name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(planet))
                .map(|planet| planet.name.clone())
                .unwrap_or_default();
            dict.set(&key("planet"), &name.as_str().to_variant());
            dict.set(&key("sector"), &(sector as i64).to_variant());
            dict.set(
                &key("wasCaptured"),
                &self
                    .campaign
                    .as_ref()
                    .and_then(|campaign| campaign.sector(planet, sector))
                    .map(|sector| sector.info.info.was_captured)
                    .unwrap_or(false)
                    .to_variant(),
            );
        }
        dict
    }

    /// Fog query placeholder (fog state is plan 12 M7 `FogControl`, not yet
    /// bound to this facade); reports the rules fog flags.
    #[func]
    pub fn fog_query(&self, team: i64, x: i32, y: i32) -> VarDictionary {
        let mut dict = VarDictionary::new();
        dict.set(&GString::from("team"), &team.to_variant());
        dict.set(&GString::from("x"), &x.to_variant());
        dict.set(&GString::from("y"), &y.to_variant());
        dict.set(&GString::from("fog"), &self.session.rules.fog.to_variant());
        dict.set(
            &GString::from("staticFog"),
            &self.session.rules.static_fog.to_variant(),
        );
        dict.set(&GString::from("discovered"), &false.to_variant());
        dict
    }

    /// Lists the decoded vanilla schematics.
    #[func]
    pub fn list_schematics(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = &self.registry else {
            return out;
        };
        let mut schematics = Schematics::new();
        schematics.load_loadouts(registry);
        for schematic in &schematics.all {
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &schematic.name().as_str().to_variant(),
            );
            row.set(&GString::from("width"), &schematic.width.to_variant());
            row.set(&GString::from("height"), &schematic.height.to_variant());
            row.set(
                &GString::from("tiles"),
                &(schematic.tiles.len() as i64).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Schematic selection capture is plan-19/15 (world selection); stub.
    #[func]
    pub fn write_schematic_selection(&self, _x0: i32, _y0: i32, _x1: i32, _y1: i32) -> GString {
        GString::from("")
    }

    /// Schematic placement is plan-06/07 tile ops (deferred from plan 12 M5).
    #[func]
    pub fn place_schematic_base64(&mut self, _b64: GString, _x: i32, _y: i32) -> bool {
        false
    }

    /// Saves a named slot marker into the settings store.
    #[func]
    pub fn save_slot(&mut self, name: GString) -> bool {
        let key = format!("mcp-save-{name}");
        self.settings.put_string(&key, "1");
        self.settings.has(&key)
    }

    /// Loads a named slot marker from the settings store.
    #[func]
    pub fn load_slot(&mut self, name: GString) -> bool {
        self.settings.has(&format!("mcp-save-{name}"))
    }

    /// Content boot error, if any (diagnostics).
    #[func]
    pub fn content_error(&self) -> GString {
        GString::from(self.content_error.as_deref().unwrap_or(""))
    }
}
