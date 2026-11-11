// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tech tree: the research flow, the research dialog payload and the objective text.
//! Upstream: ui/dialogs/ResearchDialog.java; game/Objectives.java

use super::events::event_content_of;
use super::text::content_icon_name;
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
    /// heavy one).
    #[func]
    pub fn research_tree_json(&self, root: GString) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return empty_object();
        };
        let Some(campaign) = self.campaign.as_ref() else {
            return empty_object();
        };
        let tech = registry.tech();
        let requested = root.to_string();
        let tree = if requested.is_empty() {
            self.session
                .sector
                .map(|(planet, _)| planet)
                .or_else(|| registry.planet_id("serpulo"))
                .and_then(|id| registry.planet(id))
                .and_then(|def| def.tech_tree)
                .and_then(|tree_id| tech.tree(tree_id))
        } else {
            tech.trees.iter().find(|tree| tree.name == requested)
        };
        let Some(tree) = tree else {
            return empty_object();
        };

        let ctx = CampaignObjectiveContext::new(registry, campaign);
        let unlocked_with_rules = |content: ContentRef| {
            tech_tree::content_unlocked_with_rules(registry, content, &self.session.rules)
        };

        // Depth-first pre-order from the tree root (`TechNode.each`); indices
        // in this array are the JSON `index`/`parent`/`children` values.
        let mut order: Vec<TechNodeRef> = Vec::new();
        tech.each(tree.root, &mut |node_ref| order.push(node_ref));
        if order.is_empty() {
            return empty_object();
        }
        let mut indices: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for (index, node_ref) in order.iter().enumerate() {
            indices.insert(node_ref.0, index);
        }

        // `checkNodes` (ResearchDialog.java:391-401): `visible` starts true for
        // every node; a locked parent hides its subtree, an unlocked parent
        // shows its direct children even when they are locked.
        let mut visible = vec![true; order.len()];
        let mut selectable = vec![false; order.len()];
        for (index, node_ref) in order.iter().enumerate() {
            let Some(node) = tech.node(*node_ref) else {
                continue;
            };
            let node_locked = node
                .content
                .is_none_or(|content| !unlocked_with_rules(content));
            let parent_visible = node
                .parent
                .and_then(|parent| indices.get(&parent.0).copied())
                .map(|parent_index| visible[parent_index])
                .unwrap_or(true);
            if !node_locked && (node.parent.is_none() || parent_visible) {
                visible[index] = true;
            }
            selectable[index] = node
                .content
                .is_some_and(|content| tech_tree::content_unlocked(registry, content))
                || tech_tree::objectives_complete(registry, &node.objectives, &ctx);
            for child in &node.children {
                if let Some(child_index) = indices.get(&child.0).copied() {
                    visible[child_index] = !node_locked && visible[index];
                }
            }
        }

        // `treeLayout`: positions come from the Java BranchTreeLayout walk.
        let children_lists: Vec<Vec<usize>> = order
            .iter()
            .map(|node_ref| {
                tech.node(*node_ref)
                    .map(|node| {
                        node.children
                            .iter()
                            .filter_map(|child| indices.get(&child.0).copied())
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        let positions = mind_core::ui::tree_layout::research_tree_layout(&children_lists, 0, 60.0);

        // Bounds over visible nodes (ResearchDialog.java:360-371): node width
        // goes into x, +`nodeSize * 1.5` into y.
        const NODE_SIZE: f32 = 60.0;
        let (mut minx, mut miny, mut maxx, mut maxy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for (index, &(x, y)) in positions.iter().enumerate() {
            if !visible.get(index).copied().unwrap_or(false) {
                continue;
            }
            minx = minx.min(x - NODE_SIZE / 2.0);
            maxx = maxx.max(x + NODE_SIZE / 2.0);
            miny = miny.min(y - NODE_SIZE / 2.0);
            maxy = maxy.max(y + NODE_SIZE / 2.0);
        }
        let bounds = [minx, miny + NODE_SIZE * 1.5, maxx - minx, maxy - miny];

        // The global item pool (`rebuildItems` + `ItemsDisplay`):
        // `tech_tree::research_inventory` over the root's planets.
        let planets = tech_tree::root_planets(registry, tree.root);
        let played_items = self.played_core_items(self.session.default_team());
        let inv = tech_tree::research_inventory(
            registry,
            campaign,
            &planets,
            self.session.sector,
            &played_items,
        );
        let items = &inv.pool;
        let mut items_json: Vec<serde_json::Value> = Vec::new();
        for index in 0..registry.items().len() {
            let id = ItemId::new(index as u16);
            let amount = items.get(id);
            if amount <= 0 {
                continue;
            }
            let item_name = registry
                .item(id)
                .map(|item| item.name.clone())
                .unwrap_or_default();
            items_json.push(serde_json::json!({
                "item": item_name,
                "amount": amount,
                "text": mind_core::ui::text::format_amount_default(amount as i64),
                "name": self.localized_content_name("item", &item_name),
            }));
        }

        // Root selector rows (`TechTree.roots` minus locked non-preferred ones).
        let mut roots_json: Vec<serde_json::Value> = Vec::new();
        for root_ref in &tech.roots {
            let Some(node) = tech.node(*root_ref) else {
                continue;
            };
            let selected = *root_ref == tree.root;
            let unlocked = node
                .content
                .is_some_and(|content| tech_tree::content_unlocked(registry, content));
            if node.requires_unlock && !unlocked && !selected {
                continue;
            }
            let name = node.name.clone().unwrap_or_default();
            roots_json.push(serde_json::json!({
                "name": name,
                "title": self.tree_title(&name),
                "type": node
                    .content
                    .map(|content| content.type_.name())
                    .unwrap_or_default(),
                "icon": node
                    .content
                    .map(|content| content_icon_name(content.type_.name(), &node.content_name))
                    .unwrap_or_default(),
                "selected": selected,
            }));
        }
        let root_node = tech.node(tree.root);
        let root_name = root_node
            .and_then(|node| node.name.clone())
            .unwrap_or_else(|| tree.name.clone());
        let root_type = root_node
            .and_then(|node| node.content)
            .map(|content| content.type_.name())
            .unwrap_or("");
        let root_icon = root_node
            .map(|node| {
                node.content
                    .map(|content| content_icon_name(content.type_.name(), &node.content_name))
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Node rows (`View.rebuildAll` buttons + the hover popup table).
        let mut nodes_json: Vec<serde_json::Value> = Vec::with_capacity(order.len());
        for (index, node_ref) in order.iter().enumerate() {
            let Some(node) = tech.node(*node_ref) else {
                continue;
            };
            let content = node.content;
            let type_name = content.map(|content| content.type_.name()).unwrap_or("");
            let icon = content
                .map(|_| content_icon_name(type_name, &node.content_name))
                .unwrap_or_default();
            let name = content
                .map(|_| self.localized_content_name(type_name, &node.content_name))
                .unwrap_or_default();
            let description = content
                .map(|_| self.localized_content_description(type_name, &node.content_name))
                .unwrap_or_default();
            let inline_description = content
                .and_then(|content| tech_tree::unlock_fields(registry, content))
                .is_some_and(|fields| fields.inline_description);

            // Progress line (`ResearchDialog.java:685-705`).
            let requirements = tech_tree::effective_requirements(registry, *node_ref);
            let (mut sum, mut used) = (0.0f32, 0.0f32);
            let mut has_progress = false;
            for (requirement_index, requirement) in requirements.iter().enumerate() {
                let finished = node
                    .finished_requirements
                    .get(requirement_index)
                    .map(|stack| stack.amount)
                    .unwrap_or(0);
                if finished > 0 {
                    has_progress = true;
                }
                let cost = registry
                    .item(requirement.item)
                    .map(|item| item.cost)
                    .unwrap_or(0.0);
                sum += cost * requirement.amount as f32;
                used += cost * finished as f32;
            }
            let progress = if has_progress {
                ((used / sum * 100.0) as i32).min(99)
            } else {
                -1
            };

            // Unfinished requirement rows (`ResearchDialog.java:707-736`).
            let mut reqs_json: Vec<serde_json::Value> = Vec::new();
            for (requirement_index, requirement) in requirements.iter().enumerate() {
                let finished = node
                    .finished_requirements
                    .get(requirement_index)
                    .map(|stack| stack.amount)
                    .unwrap_or(0);
                if requirement.amount <= finished {
                    continue;
                }
                let remaining = requirement.amount - finished;
                let pool = items.get(requirement.item);
                let item_name = registry
                    .item(requirement.item)
                    .map(|item| item.name.clone())
                    .unwrap_or_default();
                reqs_json.push(serde_json::json!({
                    "item": item_name,
                    "name": self.localized_content_name("item", &item_name),
                    "have": mind_core::ui::text::format_amount_default(pool.min(remaining) as i64),
                    "need": mind_core::ui::text::format_amount_default(remaining as i64),
                    "has": pool > 0,
                }));
            }

            // Incomplete objectives (`ResearchDialog.java:737-749`).
            let mut objectives_json: Vec<String> = Vec::new();
            for spec in &node.objectives {
                let (complete, text) = self.objective_display(registry, &ctx, *spec);
                if !complete {
                    objectives_json.push(text);
                }
            }

            nodes_json.push(serde_json::json!({
                "index": index,
                "parent": node
                    .parent
                    .and_then(|parent| indices.get(&parent.0).copied())
                    .map(|parent_index| parent_index as i64)
                    .unwrap_or(-1),
                "children": node
                    .children
                    .iter()
                    .filter_map(|child| indices.get(&child.0).copied())
                    .collect::<Vec<usize>>(),
                "content": node.content_name,
                "type": type_name,
                "icon": icon,
                "name": name,
                "x": positions[index].0,
                "y": positions[index].1,
                "visible": visible[index],
                "locked": content.is_none_or(|content| !unlocked_with_rules(content)),
                "selectable": selectable[index],
                "can_spend": tech_tree::can_spend(registry, *node_ref, &items, &ctx),
                "description": description,
                "inline_description": inline_description,
                "progress": progress,
                "reqs": reqs_json,
                "objectives": objectives_json,
            }));
        }

        let view = serde_json::json!({
            "root": tree.name,
            "root_title": self.tree_title(&root_name),
            "root_type": root_type,
            "root_icon": root_icon,
            "roots": roots_json,
            "items": items_json,
            "bounds": bounds,
            "nodes": nodes_json,
        });
        value_gstring(&view)
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
    fn played_core_items(&self, default_team: u8) -> Vec<(String, i32)> {
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
                let planet_name = campaign
                    .planet_name(*planet)
                    .unwrap_or_default()
                    .to_owned();
                campaign.edit_sector(settings, &planet_name, *sector_id, |record| {
                    record.info.info.items.insert(name.clone(), *updated);
                });
            }
        });
    }

    /// `bundle("techtree." + name, name)` for a tree/root name.
    fn tree_title(&self, name: &str) -> String {
        let key = format!("techtree.{name}");
        match self.bundle_lookup(&key) {
            Some(value) => value.to_owned(),
            None => name.to_owned(),
        }
    }

    /// `Objectives.Research.display()` half: `content.techNode == null ||
    /// parent == null || parent.content.unlockedHost()`. The parent comes from
    /// `UnlockFields.tech_node` (the last-created node), matching Java's
    /// `UnlockableContent.techNode` (PAR-0500).
    fn research_parent_unlocked(&self, registry: &ContentRegistry, content: ContentRef) -> bool {
        tech_tree::research_parent_unlocked(registry, content)
    }

    /// `Objective.display()` and `complete()` for the hover popup
    /// (Objectives.java:26-131).
    fn objective_display(
        &self,
        registry: &ContentRegistry,
        ctx: &dyn ObjectiveContext,
        spec: ObjectiveSpec,
    ) -> (bool, String) {
        let complete = Objective::from_spec(spec).complete(ctx);
        let (key, argument) = match spec {
            ObjectiveSpec::SectorComplete(sector) => (
                "requirement.capture",
                self.sector_localized_name(registry, sector),
            ),
            ObjectiveSpec::Research(content) => {
                let value = if self.research_parent_unlocked(registry, content) {
                    self.emoji_content_name(registry, content)
                } else {
                    String::from("???")
                };
                ("requirement.research", value)
            }
            ObjectiveSpec::Produce(content) => {
                let value = if tech_tree::content_unlocked(registry, content) {
                    self.emoji_content_name(registry, content)
                } else {
                    String::from("???")
                };
                ("requirement.produce", value)
            }
            ObjectiveSpec::OnSector(sector) => (
                "requirement.onsector",
                self.sector_localized_name(registry, sector),
            ),
            ObjectiveSpec::OnPlanet(planet) => (
                "requirement.onplanet",
                self.planet_localized_name(registry, planet),
            ),
        };
        (complete, self.bundle.format(key, &[argument.as_str()]))
    }
}
