// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `config_spec` and the config read/write helpers.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    // ---- world queries ----

    /// Block kind at a tile (`None` out of bounds/unloaded).
    pub fn block_kind_at(&self, x: i32, y: i32) -> Option<BlockKind> {
        let host = self.host.clone()?;
        let guard = host.bind();
        let content = guard.content_registry()?;
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = guard.grid().block_at(TilePos::new(tx, ty))?;
        content.block(block).map(|def| def.kind)
    }

    /// Builds the config-spec JSON for a configurable block.
    pub fn config_spec(&self, x: i32, y: i32) -> Option<String> {
        let host = self.host.clone()?;
        // Fetched before the sim-host bind: `MindCampaign` owns the campaign
        // tech state (`content_unlocked`) that `unlockedNow()` reads.
        let campaign = self.campaign.clone();
        let guard = host.bind();
        let content = guard.content_registry()?;
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = guard.grid().block_at(TilePos::new(tx, ty))?;
        let def = content.block(block)?;
        let rules = guard.live_rules();
        // `CoreBlock.configurable` is set from `rules.coreBuildAndConfig`
        // (`CoreBlock.java:588,648`), not the static content flag.
        let core_configurable = def.kind == BlockKind::CoreBlock
            && rules
                .as_ref()
                .is_some_and(|rules| rules.core_build_and_config);
        if !def.configurable && !core_configurable {
            return None;
        }
        // `SwitchBlock.configTapped` toggles on tap; it never opens a panel.
        if def.kind == BlockKind::SwitchBlock {
            return None;
        }
        // `CanvasBlock.buildConfiguration` (`CanvasBlock.java:314-317`): one
        // pencil button opening `CanvasEditDialog`.
        if def.kind == BlockKind::CanvasBlock {
            let spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "canvas": true,
            });
            return Some(spec.to_string());
        }
        // `LogicBlock.buildConfiguration` (`LogicBlock.java:736-739`): one
        // pencil button opening the logic editor; `shouldShowConfigure` is
        // `accessible()` (`LogicBlock.java:734`), so an inaccessible world
        // processor opens no panel at all.
        if def.kind == BlockKind::LogicBlock {
            let state = guard.logic_get_state(i64::from(x), i64::from(y));
            let accessible = state
                .get("accessible")
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            if !accessible {
                return None;
            }
            let spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "logic": true,
            });
            return Some(spec.to_string());
        }
        // H02 T6: link blocks have no option list — the panel shows only the
        // title, the current link and the close button. The link itself is made
        // by tapping another block while this panel is open.
        if matches!(
            def.kind,
            BlockKind::ItemBridge
                | BlockKind::BufferedItemBridge
                | BlockKind::LiquidBridge
                | BlockKind::MassDriver
                | BlockKind::PayloadMassDriver
                | BlockKind::PowerNode
                | BlockKind::LongPowerNode
        ) {
            let mut spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "text": false,
                "multiline": false,
                "color": false,
                "options": [],
                "link": true,
            });
            let current = serde_json::from_str::<serde_json::Value>(
                guard.building_config_json(x, y).to_string().as_str(),
            )
            .ok()
            .filter(|value| value.get("type").and_then(|kind| kind.as_str()) == Some("point"));
            if let Some(current) = current {
                spec["current"] = current;
            }
            return Some(spec.to_string());
        }
        // `TargetDummy.buildConfiguration` (`TargetDummy.java:240-262`): the
        // enemy/flying checks and armor/reset/hit-size fields. The panel sends
        // the full `int[]` (`int_array` wire config).
        if def.kind == BlockKind::TargetDummy {
            let current = serde_json::from_str::<serde_json::Value>(
                guard.building_config_json(x, y).to_string().as_str(),
            )
            .ok()
            .filter(|value| value.get("type").and_then(|kind| kind.as_str()) == Some("int_array"));
            let mut spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "target_dummy": true,
            });
            if let Some(current) = current {
                spec["current"] = current;
            }
            return Some(spec.to_string());
        }
        // `CoreBlock.buildConfiguration` (`CoreBlock.java:289-319`): team
        // toggles only while `rules.coreBuildAndConfig` is on.
        if def.kind == BlockKind::CoreBlock
            && rules
                .as_ref()
                .is_some_and(|rules| rules.core_build_and_config)
        {
            let teams: Vec<serde_json::Value> = mind_core::game::team::Team::base_teams()
                .iter()
                .map(|team| {
                    serde_json::json!({
                        "id": team.id,
                        "name": team.name,
                        "color": format!(
                            "{:02x}{:02x}{:02x}ff",
                            (team.color.r * 255.0) as u8,
                            (team.color.g * 255.0) as u8,
                            (team.color.b * 255.0) as u8
                        ),
                    })
                })
                .collect();
            let spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "core_teams": teams,
                "current": { "team": guard.building_team_at(x, y) },
            });
            return Some(spec.to_string());
        }
        // `state.isCampaign()`/`state.getPlanet()`/`Rules.researched` feed the
        // `ItemSelection` filter (`ItemSelection.java:68`).
        let in_campaign = guard.in_campaign();
        let planet = guard.config_planet();
        // `Block.isHidden()` reads `state.rules` (`Block.java:1553-1555`,
        // `BuildVisibility.java:10-22`); without live rules the rules-less
        // custom/campaign context (`block_catalog_from`).
        let visibility = {
            let mut context = match rules.as_ref() {
                Some(rules) => {
                    mind_core::ui::campaign::BlockVisibilityContext::from_rules(rules, in_campaign)
                }
                None if in_campaign => mind_core::ui::campaign::BlockVisibilityContext::campaign(),
                None => mind_core::ui::campaign::BlockVisibilityContext::sandbox(),
            };
            // `indexer.isBlockPresent(Blocks.coreZone)` (`coreZoneOnly`).
            context.core_zone_present =
                content.block_id("core-zone").is_some_and(|zone| {
                    guard.grid().tiles.iter().any(|tile| {
                        tile.floor == zone || tile.overlay == zone || tile.block == zone
                    })
                });
            context
        };
        // `UnlockableContent.unlockedNow()` (`UnlockableContent.java:281-283`):
        // outside a campaign every option is unlocked. In a campaign the
        // client's tech-tree state lives on `MindCampaign` (the settings-backed
        // store); the sim's registry carries no campaign unlocks, so the
        // multiplayer `Rules.researched` set and the campaign query are the two
        // sources.
        let unlocked_now = |name: &str| -> bool {
            if !in_campaign {
                return true;
            }
            if rules
                .as_ref()
                .is_some_and(|rules| rules.researched.iter().any(|entry| entry.as_str() == name))
            {
                return true;
            }
            let Some(node) = campaign.clone() else {
                return false;
            };
            let mut node = node;
            node.call("content_unlocked", &[GString::from(name).to_variant()])
                .try_to::<bool>()
                .unwrap_or(false)
        };
        // `LaunchPad.buildConfiguration` (`LaunchPad.java:185-203`): in a
        // campaign (and not a net client) one 40 px `Styles.cleari` button with
        // `Icon.upOpen` opening the sector-destination picker; `deselect()`
        // otherwise (no panel at all).
        if def.kind == BlockKind::LaunchPad {
            if !in_campaign {
                return None;
            }
            let planet = planet
                .and_then(|planet| content.planet(planet))
                .map(|planet| planet.name.clone())
                .unwrap_or_default();
            let spec = serde_json::json!({
                "title": format!("block.{}.name", def.name),
                "block": def.name,
                "x": x,
                "y": y,
                "launch": true,
                "planet": planet,
            });
            return Some(spec.to_string());
        }
        // `Reconstructor.buildConfiguration` (`Reconstructor.java:215-240`):
        // with `showAllCommands = true` it lists every unit command (the held
        // payload's command subset needs a payload-model this panel cannot see).
        // The row is built below (`commands`), not as `ItemSelection` options.
        let reconstructor = def.kind == BlockKind::Reconstructor;
        let mut options: Vec<serde_json::Value> = Vec::new();
        // A behavior that declares its config kinds (`Block.config(...)`) owns
        // the picker; undeclared blocks keep the item/liquid-container fallback.
        let declared = guard.block_config_kinds(block);
        // `UnitFactory.buildConfiguration` (`UnitFactory.java:286-289`) and the
        // payload blocks (`PayloadSource.java:113-118`, `PayloadRouter.java:184`):
        // factory plans (or every unit), plus sortable blocks. The declared kind
        // order is Java's list order, so payload sources list blocks first.
        for kind in declared {
            match kind {
                ConfigKind::Unit => {
                    let push_unit = |options: &mut Vec<serde_json::Value>, name: &str| {
                        // `u.isBanned()` in the caller lists
                        // (`UnitFactory.java:287`, `PayloadSource.java:92-94`,
                        // `PayloadRouter.java:58-60`).
                        if rules
                            .as_ref()
                            .is_some_and(|rules| rules.is_banned_unit(name))
                        {
                            return;
                        }
                        options.push(content_option_json(
                            content,
                            ContentType::Unit,
                            name,
                            format!("unit.{name}.name"),
                            planet,
                            &unlocked_now,
                            &visibility,
                        ));
                    };
                    if def.unit_plans.is_empty() {
                        for entry in content.entries(ContentType::Unit) {
                            if let Some(name) = entry.name {
                                push_unit(&mut options, name);
                            }
                        }
                    } else {
                        for plan in &def.unit_plans {
                            if let Some(unit) = content.unit(plan.unit) {
                                let name = unit.name.clone();
                                push_unit(&mut options, &name);
                            }
                        }
                    }
                }
                ConfigKind::Block => {
                    // `Constructor.canProduce` size window (`Init` min/max block
                    // size); payload sources/routers sort any producible block.
                    let (min_size, max_size) = match def.name.as_str() {
                        "constructor" => (1, 2),
                        "large-constructor" => (3, 4),
                        _ => (1, i32::MAX),
                    };
                    for entry in content.entries(ContentType::Block) {
                        let Some(name) = entry.name else {
                            continue;
                        };
                        let Some(candidate) = content.block_by_name(name) else {
                            continue;
                        };
                        // `b.isVisible()` in `Constructor.canProduce`
                        // (`Constructor.java:55`) / `PayloadSource.canProduce`
                        // (`PayloadSource.java:89`): rules-dependent visibility
                        // (sandbox-only, editor-only, ...) plus `isHidden`.
                        if candidate.removed
                            || !visibility.block_visible(name, candidate.build_visibility)
                            || candidate.kind == BlockKind::CoreBlock
                            || candidate.size < min_size
                            || candidate.size > max_size
                            || rules
                                .as_ref()
                                .is_some_and(|rules| rules.is_banned_block(name))
                        {
                            continue;
                        }
                        options.push(content_option_json(
                            content,
                            ContentType::Block,
                            name,
                            format!("block.{name}.name"),
                            planet,
                            &unlocked_now,
                            &visibility,
                        ));
                    }
                }
                _ => {}
            }
        }
        let item_options = options.is_empty()
            && (declared.contains(&ConfigKind::Item) || (declared.is_empty() && def.has_items));
        if item_options {
            for entry in content.entries(ContentType::Item) {
                if let Some(name) = entry.name {
                    options.push(content_option_json(
                        content,
                        ContentType::Item,
                        name,
                        format!("item.{name}.name"),
                        planet,
                        &unlocked_now,
                        &visibility,
                    ));
                }
            }
        }
        let liquid_options =
            declared.contains(&ConfigKind::Liquid) || (declared.is_empty() && def.has_liquids);
        if options.is_empty() && liquid_options {
            for entry in content.entries(ContentType::Liquid) {
                if let Some(name) = entry.name {
                    options.push(content_option_json(
                        content,
                        ContentType::Liquid,
                        name,
                        format!("liquid.{name}.name"),
                        planet,
                        &unlocked_now,
                        &visibility,
                    ));
                }
            }
        }
        // `UnitFactory.buildConfiguration` (`UnitFactory.java:287`): the
        // command row's column count follows the *unlocked* plan count
        // (`Mathf.clamp(units.size, 2, selectionColumns)`).
        let unlocked_options = options
            .iter()
            .filter(|option| {
                option.get("unlocked").and_then(serde_json::Value::as_bool) == Some(true)
            })
            .count();
        let text = matches!(def.kind, BlockKind::LogicBlock | BlockKind::MessageBlock);
        let multiline = def.kind == BlockKind::MessageBlock;
        // `MessageBlock.maxTextLength`/`maxNewlines` (`MessageBlock.java:29-30`;
        // the port's `MessageDef` on the live block instance).
        let message = guard.message_limits(block);
        let color = def.kind == BlockKind::LightBlock;
        // Current value readbacks (`MessageBlock.message` / `LightBuild.color`).
        let current_text = if text {
            guard
                .building_config_string(x, y)
                .try_to::<String>()
                .ok()
                .filter(|text| !text.is_empty())
        } else {
            None
        };
        let current_color = if color {
            Some(guard.light_color(x, y).try_to::<i64>().unwrap_or_default())
        } else {
            None
        };
        // The checked option (`ItemSelection.java:75`: `holder.get() == item`)
        // and the Reconstructor's current command
        // (`Reconstructor.java:237-240`): the content config id resolved to a
        // name.
        let selection = item_selection_shape(def);
        let current_content = if reconstructor || selection.is_some() {
            serde_json::from_str::<serde_json::Value>(&guard.building_config_json(x, y).to_string())
                .ok()
                .and_then(|value| config_content_name(content, &value))
        } else {
            None
        };
        let mut spec = serde_json::json!({
            "title": format!("block.{}.name", def.name),
            "block": def.name,
            "x": x,
            "y": y,
            "text": text,
            "multiline": multiline,
            "color": color,
            "options": options,
        });
        if let Some((max_text, max_newlines)) = message {
            spec["max_text"] = serde_json::Value::from(max_text);
            spec["max_newlines"] = serde_json::Value::from(max_newlines);
        }
        if let Some((pick_kind, rows, columns)) = selection {
            // `MindItemSelection` route: the `ItemSelection.buildTable` panel
            // (no title, `Styles.black6`, 40 px icon cells, close on pick).
            spec["selection"] = serde_json::Value::Bool(true);
            spec["selection_rows"] = serde_json::Value::from(rows);
            spec["selection_columns"] = serde_json::Value::from(columns);
            spec["pick_kind"] = serde_json::Value::from(pick_kind);
            // `block.selectScroll` (`ItemSelection.java:103-108`): the pane's
            // remembered scroll offset, restored on open.
            spec["select_scroll"] =
                serde_json::Value::from(self.config_scroll.get(&def.name).copied().unwrap_or(0.0));
        }
        if let Some(current) = current_text {
            spec["current"] = serde_json::Value::from(current);
        }
        if let Some(current) = current_content {
            spec["current"] = serde_json::Value::from(current);
        }
        if let Some(current) = current_color {
            // The color picker replaces the option grid entirely.
            spec["current"] = serde_json::Value::from(current);
            spec["options"] = serde_json::Value::Array(Vec::new());
        }
        if def.kind == BlockKind::UnitFactory {
            // `UnitFactory.buildConfiguration` (`UnitFactory.java:292-340`):
            // under the plan grid, the selected plan's unit command row — 40 px
            // toggles, `clamp(units.size, 2, selectionColumns)` columns, a
            // `Pal.gray` separator, and a click configures without closing.
            let state = guard.unit_factory_config(x, y);
            let unit = state
                .and_then(|(plan, _)| usize::try_from(plan).ok())
                .and_then(|index| def.unit_plans.get(index))
                .and_then(|plan| content.unit(plan.unit));
            if let Some(unit) = unit
                && unit.commands.len() > 1
                && unit.allow_change_commands
            {
                let limit = selection.map(|(_, _, columns)| columns).unwrap_or(4);
                let columns = i64::try_from(unlocked_options)
                    .unwrap_or(2)
                    .clamp(2, limit.max(2));
                let current_command = state
                    .and_then(|(_, command)| command)
                    .and_then(|command| {
                        content.unit_command(mind_core::content::UnitCommandId::new(command))
                    })
                    .map(|def| def.name.as_str());
                spec["commands"] = serde_json::json!({
                    "options": command_options_json(
                        content,
                        &unit.commands,
                        current_command,
                        unit.default_command
                            .map(mind_core::content::UnitCommandId::raw),
                    ),
                    "columns": columns,
                    "size": 40.0,
                    "separator": true,
                    "background": true,
                    "close_select": false,
                });
            }
        }
        if reconstructor {
            // `Reconstructor.buildConfiguration` (`Reconstructor.java:215-240`):
            // all unit commands as 44 px toggles, 5 columns over
            // `Styles.black6`; a click configures and closes the panel
            // (`deselect()`, `:230`). The port has no payload unit, so the list
            // is never narrowed to `unit().commands`.
            let commands: Vec<mind_core::content::UnitCommandId> = content
                .entries(ContentType::UnitCommand)
                .into_iter()
                .map(|entry| mind_core::content::UnitCommandId::new(entry.id))
                .collect();
            let current_command = spec
                .get("current")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            spec["commands"] = serde_json::json!({
                "options": command_options_json(
                    content,
                    &commands,
                    current_command.as_deref(),
                    None,
                ),
                "columns": 5,
                "size": 44.0,
                "separator": false,
                "background": true,
                "close_select": true,
            });
        }
        Some(spec.to_string())
    }

    /// Records the `ItemSelection` pane's scroll offset for a block
    /// (`ItemSelection.java:105-107`: `block.selectScroll = pane.getScrollY()`).
    pub fn set_config_scroll(&mut self, block: &str, scroll: f32) {
        self.config_scroll.insert(block.to_owned(), scroll);
    }

    pub(super) fn enqueue_commands(&self, commands: &[SimCommand]) {
        if commands.is_empty() {
            return;
        }
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let mut guard = host.bind_mut();
        for command in commands {
            guard.submit_player_command(command.clone());
        }
    }
}
