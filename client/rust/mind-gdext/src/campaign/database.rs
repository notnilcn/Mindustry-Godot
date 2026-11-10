// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Database/content-info payloads and the build-visibility context they filter by.
//! Upstream: ui/dialogs/ContentInfoDialog.java; ui/dialogs/DatabaseDialog.java; ui/fragments/PlacementFragment.java

use super::*;
use super::text::content_icon_name;

#[godot_api(secondary)]
impl MindCampaign {
    /// `ContentInfoDialog.show(content)`: name/description/details plus the
    /// computed stat categories for the `?` dialog (`UnlockableContent
    /// .computeStats` -> `world/meta/Stats`). Unknown names return `{}` so the
    /// caller can keep its bundle-only fallback.
    #[func]
    pub fn content_info_json(&self, name: GString) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let name = name.to_string();
        let Some(content) = registry.by_name(&name) else {
            return GString::from("{}");
        };
        let type_name = content.type_.name();
        // The family stats read the runtime block table (kind data + behavior
        // knobs). It is rebuilt per call — cheap next to dialog construction and
        // never stale across a registry reload (H05 T10b).
        let block_table = mind_core::world::block::BlockTable::build_default(registry).ok();
        let ctx = StatCtx {
            registry,
            bundle: &self.bundle,
            blocks: block_table.as_ref(),
            in_game: self.session.phase.is_game(),
            planet: Some(self.session.rules.planet.as_str()),
            campaign: self.session.is_campaign(),
        };
        let stats = compute_content_stats(&ctx, content);
        let categories: Vec<serde_json::Value> = stats
            .to_map()
            .into_iter()
            .filter_map(|(cat, rows)| {
                let stat_rows: Vec<serde_json::Value> = rows
                    .into_iter()
                    .map(|(stat, values)| {
                        let rendered: Vec<serde_json::Value> = values
                            .iter()
                            .map(|cells| render_stat_cells(cells))
                            .collect();
                        serde_json::json!({
                            "name": stat.name(),
                            "label": ctx.stat_label(stat),
                            "tooltip": ctx.stat_tooltip(stat),
                            "values": rendered,
                        })
                    })
                    .collect();
                (!stat_rows.is_empty()).then(|| {
                    serde_json::json!({
                        "name": cat.name(),
                        "label": ctx.category_label(cat),
                        "stats": stat_rows,
                    })
                })
            })
            .collect();
        let hide_details = match content.type_ {
            ContentType::Block => registry
                .block(BlockId::new(content.id))
                .is_some_and(|def| def.unlock.hide_details),
            ContentType::Item => registry
                .item(ItemId::new(content.id))
                .is_some_and(|def| def.unlock.hide_details),
            ContentType::Liquid => registry
                .liquid(LiquidId::new(content.id))
                .is_some_and(|def| def.unlock.hide_details),
            _ => false,
        };
        let view = serde_json::json!({
            "content": name,
            "type": type_name,
            "localized": self.localized_content_name(type_name, &name),
            "icon": content_icon_name(type_name, &name),
            "description": self.localized_content_description(type_name, &name),
            "details": self.localized_content_field(type_name, &name, "details"),
            "credit": self.localized_content_field(type_name, &name, "credit"),
            "use_categories": stats.use_categories,
            "locked": !tech_tree::content_unlocked_with_rules(
                registry,
                content,
                &self.session.rules
            ),
            "hide_details": hide_details,
            "categories": categories,
        });
        GString::from(view.to_string().as_str())
    }

    /// Placement-palette catalog JSON (`PlacementFragment.getUnlockedByCategory`).
    ///
    /// Built over the live session registry — the one content boot filled,
    /// mods included — because Java reads the global `Vars.content.blocks()`
    /// (`PlacementFragment.java:771-781`); the base-content helpers are only
    /// the no-session fallback. Campaign games filter through the
    /// settings-backed unlock store: only blocks whose `<name>-unlocked` bit is
    /// set (or that `check_auto_unlocks` grants at boot) and whose
    /// `BuildVisibility` is buildable survive, empty categories are dropped,
    /// and other-planet content is hidden (`Block.environmentBuildable` reads
    /// the active planet's `shownPlanets`). Custom games expose the full
    /// build-menu inventory, mirroring `unlockedNowHost`'s
    /// `!state.is_campaign()` branch, with the same planet filter.
    ///
    /// The campaign planet comes from the LOADED world's rules when one exists:
    /// Java's `state.rules` is the loaded world's from `World.loadSector` on,
    /// while the facade session only flips in `start_sector` — which the UI can
    /// call a frame after the world load, so `WorldLoadEvent`'s posted rebuild
    /// (`PlacementFragment.java:78-84`) would otherwise read the old planet.
    #[func]
    pub fn block_catalog_json(&mut self) -> GString {
        // Read the loaded rules into one owned string; never hold the host
        // bind across the catalog build below.
        let loaded_planet = self.host().and_then(|host| {
            host.bind()
                .last_sector_rules()
                .map(|rules| rules.planet.clone())
        });
        let planet = loaded_planet.unwrap_or_else(|| self.session.rules.planet.clone());
        let is_campaign = self.session.is_campaign();
        let visibility = self.block_visibility_context(is_campaign);
        let catalog = match self.registry.as_ref() {
            Some(registry) => {
                if is_campaign {
                    settings_hub::with_settings(|settings| {
                        let store = SettingsUnlockStore::new(settings);
                        mind_core::ui::campaign::block_catalog_visible(
                            registry,
                            Some(&store),
                            Some(planet.as_str()),
                            &visibility,
                        )
                    })
                } else {
                    mind_core::ui::campaign::block_catalog_visible(
                        registry,
                        None,
                        Some(planet.as_str()),
                        &visibility,
                    )
                }
            }
            None => {
                if is_campaign {
                    settings_hub::with_settings(|settings| {
                        let store = SettingsUnlockStore::new(settings);
                        mind_core::ui::campaign::block_catalog_unlocked_for(&store, planet.as_str())
                    })
                } else {
                    mind_core::ui::campaign::block_catalog_for(planet.as_str())
                }
            }
        };
        GString::from(
            serde_json::to_string(&catalog)
                .unwrap_or_else(|_| String::from("{}"))
                .as_str(),
        )
    }

    /// Core Database view JSON (`DatabaseDialog` planet tabs + content grid).
    ///
    /// Built from the live campaign registry so the menu and campaign show the
    /// settings-backed locked/unlocked entries; a custom (non-campaign,
    /// non-menu) game forces everything unlocked (`DatabaseDialog.unlocked`).
    #[func]
    pub fn database_json(&self) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let unlocked_override = mind_core::ui::database::custom_game_unlock_override(&self.session);
        // `Block.isBanned()`: `state.isGame() && rules.bannedBlocks.contains`
        // (`DatabaseDialog.rebuild` draws the cross and sorts banned last).
        let banned = self
            .session
            .phase
            .is_game()
            .then_some(&self.session.rules.banned_blocks);
        let view = mind_core::ui::database::database_view(registry, unlocked_override, banned);
        GString::from(
            serde_json::to_string(&view)
                .unwrap_or_else(|_| String::from("{}"))
                .as_str(),
        )
    }
}

impl MindCampaign {
    /// `Block.isVisible()` inputs for the palette (`BuildVisibility.java`):
    /// the live match rules (the sim's session rules, else the facade
    /// session), `indexer.isBlockPresent(Blocks.coreZone)`, the active planet's
    /// `campaignRules.legacyLaunchPads` and `advancedLaunchPad.unlocked()`.
    fn block_visibility_context(
        &self,
        is_campaign: bool,
    ) -> mind_core::ui::campaign::BlockVisibilityContext {
        let host = self.host();
        let rules = host
            .as_ref()
            .and_then(|host| host.bind().live_rules())
            .unwrap_or_else(|| self.session.rules.clone());
        let mut context =
            mind_core::ui::campaign::BlockVisibilityContext::from_rules(&rules, is_campaign);
        // `BlockIndexer.blocksPresent` covers floors, overlays and blocks.
        context.core_zone_present = host.as_ref().is_some_and(|host| {
            let guard = host.bind();
            let Some(core_zone) = guard
                .content_registry()
                .and_then(|content| content.block_id("core-zone"))
            else {
                return false;
            };
            guard.grid().tiles.iter().any(|tile| {
                tile.floor == core_zone || tile.overlay == core_zone || tile.block == core_zone
            })
        });
        context.legacy_launch_pads = self
            .session
            .sector
            .and_then(|(planet, _)| {
                self.campaign
                    .as_ref()
                    .and_then(|campaign| campaign.planet(planet))
            })
            .is_some_and(|planet| planet.campaign_rules.legacy_launch_pads);
        context.advanced_launch_pad_unlocked = self
            .registry
            .as_ref()
            .and_then(|registry| {
                registry
                    .block_id("advanced-launch-pad")
                    .and_then(|id| registry.block(id))
            })
            .is_some_and(|block| block.unlock.unlocked())
            || settings_hub::with_settings(|settings| {
                settings.get_bool("advanced-launch-pad-unlocked", false)
            });
        context
    }
}

/// Renders one stat value's cells for the `content_info_json` payload: Text
/// cells pass through `UI.formatIcons` (`render_markup`), every other cell
/// serializes as its tagged JSON form (`Cell::k`).
fn render_stat_cells(cells: &[Cell]) -> serde_json::Value {
    let rendered: Vec<serde_json::Value> = cells
        .iter()
        .map(|cell| match cell {
            Cell::Text { t } => serde_json::json!({ "k": "text", "t": t }),
            other => serde_json::to_value(other).unwrap_or(serde_json::Value::Null),
        })
        .collect();
    serde_json::Value::Array(rendered)
}
