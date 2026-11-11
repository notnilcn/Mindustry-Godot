// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tech tree: the research flow, the research dialog payload and the objective text.
//! Upstream: ui/dialogs/ResearchDialog.java; game/Objectives.java

use super::events::event_content_of;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Researches a content name, spending the active sector's real stored items
    /// (`ResearchDialog.spend`), gated by dependencies/objectives and persisted
    /// through the settings-backed unlock store.
    #[func]
    pub fn research(&mut self, content: GString) -> bool {
        let target = content.to_string();
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        // `ResearchDialog`'s node lookup: content name -> content + node
        // (`tech_tree::node_for_content_name`, H02).
        let Some((content_ref, node_ref)) = tech_tree::node_for_content_name(&registry, &target)
        else {
            self.registry = Some(registry);
            return false;
        };
        if tech_tree::content_unlocked_with_rules(&registry, content_ref, &self.session.rules) {
            self.registry = Some(registry);
            return true;
        }
        // `ResearchDialog.rebuildItems` inventory + `ResearchDialog.spend`.
        let Some((result, inv)) = self.spend_research(&mut registry, node_ref) else {
            self.registry = Some(registry);
            return false;
        };
        let changed = result.complete || !result.spent.is_empty();

        // `ResearchDialog.complete` (`ResearchDialog.java:624`):
        // `Events.fire(new ResearchEvent(node.content))` once the node is
        // finished (host-owned fire site; PAR-0577 residual).
        if result.complete
            && let Some(payload) = event_content_of(content_ref)
        {
            self.events
                .dispatch_game(event::GameEvent::ResearchEvent(event::ResearchEvent {
                    content: payload,
                }));
        }

        // Deduct the spent stacks exactly like `ResearchDialog.rebuildItems`'s
        // `ItemSeq.add(item, -amount)` (`tech_tree::plan_research_removal`, H02):
        // percentage removal from the other sectors in order, then the
        // remainder from the active sector last (its live core module while
        // playing; `Sector.removeItem`).
        let (record_updates, core_removals) =
            tech_tree::plan_research_removal(&inv, &result.spent, &registry);
        let default_team = self.session.default_team();
        for (name, amount) in &core_removals {
            if let Some(mut host) = self.host() {
                host.bind_mut()
                    .remove_player_core_items(default_team, name, *amount);
            }
        }
        self.apply_research_record_updates(&record_updates);

        // Newly unlocked content may auto-unlock dependent zero-cost nodes.
        let mut unlocked_content: Vec<ContentRef> = result.unlocked.clone();
        if !result.unlocked.is_empty()
            && let Some(campaign_ref) = self.campaign.as_ref()
        {
            let unlocked = settings_hub::with_settings(|settings| {
                let mut store = SettingsUnlockStore::new(settings);
                // `tech_tree::check_campaign_auto_unlocks` (H02).
                tech_tree::check_campaign_auto_unlocks(&mut registry, &mut store, campaign_ref)
            });
            unlocked_content.extend(unlocked.iter().copied());
            log::info!("research auto-unlocked {} content", unlocked.len());
        }
        self.registry = Some(registry);
        // Refresh the runtime copy's unlocked-content cache (the campaign
        // mutations above already went through `with_campaign_mut`).
        self.push_campaign_to_runtime();
        if changed {
            self.persist();
            self.invalidate_ui_block_catalog();
        }
        if let Some(registry) = self.registry.as_ref() {
            // Each unlock fires an `UnlockEvent` (`Control.java:160-165`):
            // toast for `showUnlock()` content, palette rebuild for blocks.
            // The `self.registry` borrow ends before the `&mut self` emit, and
            // no host bind is held here.
            self.push_unlock_toasts(registry, &unlocked_content);
            self.emit_block_unlocked_if_any(&unlocked_content);
        }
        changed
    }

    /// `ResearchDialog` tree data (`treeLayout` positions + `checkNodes` state
    /// + the hover popup rows): the selected tree's nodes in depth-first order
    /// with Java y-up positions, lock/selectable state, requirement rows, the
    /// incomplete objectives and the global item pool.
    ///
    /// `root` selects the tree by name; empty picks the played sector's tree,
    /// else serpulo. Cheap: no world access (`campaign_views_json` is the
    /// heavy one). The builder is `mind_core::ui::campaign::research_tree_view`
    /// (T01: the typed `research_tree` producer).
    #[func]
    pub fn research_tree_json(&self, root: GString) -> GString {
        match self.research_tree(&root.to_string()) {
            Some(view) => to_gstring(&view, "{}"),
            None => empty_object(),
        }
    }

    /// `UnlockableContent.unlockedHost()` by content name (G05 T4b hint gates);
    /// `false` for unknown names or before content boot.
    #[func]
    pub fn content_unlocked(&self, name: GString) -> bool {
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        let name = name.to_string();
        let Some(content) = registry.by_name(&name) else {
            return false;
        };
        tech_tree::content_unlocked(registry, content)
    }

    /// Content names unlocked since the last call (`Control.java:160-174`:
    /// `hudfrag.showUnlock` popup; H05 consumes). Drained here.
    #[func]
    pub fn take_unlocked_content(&mut self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in self.unlocked_since_last_take.drain(..) {
            out.push(&GString::from(name.as_str()));
        }
        out
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
}
impl MindCampaign {
    /// `research`'s inventory + spend step (`ResearchDialog.rebuildItems` ->
    /// `ResearchDialog.spend`): the tree root's planets (`tech_tree::
    /// root_planets`), the live item pool (`tech_tree::research_inventory`) and
    /// the spend result. `None` when the node cannot be spent (the caller
    /// restores the registry and returns false).
    fn spend_research(
        &mut self,
        registry: &mut ContentRegistry,
        node_ref: TechNodeRef,
    ) -> Option<(tech_tree::SpendResult, tech_tree::ResearchInventory)> {
        let root_ref = registry
            .tech()
            .node(node_ref)
            .and_then(|node| node.root_node)
            .unwrap_or(node_ref);
        let planets = tech_tree::root_planets(registry, root_ref);
        // T1: reads (`research_inventory`/`ctx`) must see the live campaign.
        self.mirror_runtime_campaign();
        let played_items = self.played_core_items(self.session.default_team());
        let campaign_snapshot = self.campaign.clone();
        let campaign_ref = campaign_snapshot.as_ref()?;
        // Real inventory: `rebuildItems` aggregates the stored items of the
        // tree root's planets; the sector being played reads its live core
        // module instead (`Sector.items` -> default-team items), so it is not
        // double-counted.
        let inv = tech_tree::research_inventory(
            registry,
            campaign_ref,
            &planets,
            self.session.sector,
            &played_items,
        );
        // `spend` mutates the pool; the planner only reads `inv.sectors` /
        // `inv.played_items`, so hand it a copy and keep the inventory intact.
        let mut pool = inv.pool.clone();
        let ctx = CampaignObjectiveContext::new(registry, campaign_ref);
        if !tech_tree::can_spend(registry, node_ref, &pool, &ctx) {
            return None;
        }
        let outcome = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            tech_tree::spend(registry, node_ref, &mut pool, &mut store, &ctx, false)
        });
        let result = outcome.ok()?;
        Some((result, inv))
    }

    /// The default team's live core rows (`Sector.items` while playing), clamped
    /// at 0 like the `ResearchDialog.rebuildItems` pool read; empty outside a
    /// live campaign match.
    pub(super) fn played_core_items(&self, default_team: u8) -> Vec<(String, i32)> {
        if !self.session.is_campaign() {
            return Vec::new();
        }
        let Some(host) = self.host() else {
            return Vec::new();
        };
        host.bind()
            .player_core_items(default_team)
            .into_iter()
            .map(|(name, amount)| (name, amount.max(0)))
            .collect()
    }

    /// `research`'s sector-record half: apply `plan_research_removal`'s
    /// `(planet, sector, item, amount)` updates on the live campaign in one
    /// `with_campaign_mut` (`Sector.removeItems` -> `saveInfo`).
    fn apply_research_record_updates(&mut self, updates: &[(PlanetId, u16, String, i32)]) {
        if updates.is_empty() {
            return;
        }
        self.with_campaign_mut(|campaign, settings| {
            for (planet, sector_id, name, updated) in updates {
                let planet_name = campaign.planet_name(*planet).unwrap_or_default().to_owned();
                campaign.edit_sector(settings, &planet_name, *sector_id, |record| {
                    record.info.info.items.insert(name.clone(), *updated);
                });
            }
        });
    }
}
