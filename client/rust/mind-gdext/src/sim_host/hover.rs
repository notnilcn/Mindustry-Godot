// SPDX-License-Identifier: GPL-3.0-only

//! Hovered-building readouts for the HUD.
//! Upstream: core/src/mindustry/input/InputHandler.java (hover)

use super::*;

impl MindSimHost {
    /// Tile hover rows only (`hover_info` minus the unit test).
    pub(super) fn hover_tile(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        let Some(content) = self.content_registry() else {
            return out;
        };
        // `world.tileWorld` is null off the map: no hover info.
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return out;
        }
        let tile = self.sim.grid.tile(x, y);
        // `hoverTile.build.displayable()`: any live building.
        if let Some(entity) = tile.build {
            let build = self
                .world()
                .get::<mind_core::ecs::BuildingComp>(entity)
                .and_then(|comp| content.block(comp.block));
            if let Some(def) = build {
                out.set("kind", &"building".to_variant());
                out.set("name", &GString::from(def.name.as_str()).to_variant());
                out.set(
                    "icon",
                    &GString::from(format!("block-{}-ui", def.name).as_str()).to_variant(),
                );
                out.set(
                    "label",
                    &GString::from(format!("block.{}.name", def.name).as_str()).to_variant(),
                );
                return out;
            }
        }
        // `Tile.displayable()`: `(drop() != null && block == air) ||
        // wallDrop() != null || (floor().liquidDrop != null && block == air)`.
        let block = content.block(tile.block);
        let overlay = content.block(tile.overlay);
        let floor = content.block(tile.floor);
        let air = tile.block == BlockId::AIR;
        let drop = overlay
            .and_then(|def| def.item_drop)
            .or_else(|| floor.and_then(|def| def.item_drop));
        let wall_drop = block
            .filter(|def| def.solid)
            .and_then(|def| def.item_drop)
            .or_else(|| {
                overlay
                    .filter(|def| !air && def.wall_ore)
                    .and_then(|def| def.item_drop)
            });
        let liquid_drop = floor.is_some_and(|def| def.liquid_drop.is_some());
        if !((drop.is_some() && air) || wall_drop.is_some() || (liquid_drop && air)) {
            return out;
        }
        // `Tile.display`: block drop -> overlay drop/wall drop -> floor.
        let displayed = if block.is_some_and(|def| def.item_drop.is_some()) {
            block
        } else if overlay.is_some_and(|def| def.item_drop.is_some()) || wall_drop.is_some() {
            overlay
        } else {
            floor
        };
        let Some(displayed) = displayed else {
            return out;
        };
        out.set("kind", &"tile".to_variant());
        // `OreBlock.getDisplayName` is the drop item's name, but the icon stays
        // the ore block's own `uiIcon` (`Block.getDisplayIcon`).
        if displayed.kind == BlockKind::OreBlock
            && let Some(item_def) = displayed.item_drop.and_then(|item| content.item(item))
        {
            out.set("name", &GString::from(item_def.name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("block-{}-ui", displayed.name).as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("item.{}.name", item_def.name).as_str()).to_variant(),
            );
            return out;
        }
        // `Floor.getDisplayIcon/Name`: a liquid floor shows its liquid.
        if let Some(liquid) = displayed.liquid_drop.and_then(|id| content.liquid(id)) {
            out.set("name", &GString::from(liquid.name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("liquid-{}-ui", liquid.name).as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("liquid.{}.name", liquid.name).as_str()).to_variant(),
            );
            return out;
        }
        out.set("name", &GString::from(displayed.name.as_str()).to_variant());
        out.set(
            "icon",
            &GString::from(format!("block-{}-ui", displayed.name).as_str()).to_variant(),
        );
        out.set(
            "label",
            &GString::from(format!("block.{}.name", displayed.name).as_str()).to_variant(),
        );
        out
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Cursor hover at world pixels (`PlacementFragment.hovered`):
    /// `Units.closestOverlap(player.team(), Core.input.mouseWorldX/Y, 5f,
    /// u -> !u.isLocal() && u.displayable())` — the distance test adds half
    /// the hit size, and the cursor position (not the tile centre) is used.
    #[func]
    pub fn hover_info_world(&self, wx: f32, wy: f32) -> Dictionary<GString, Variant> {
        let size = mind_core::config::TILESIZE as f32;
        let team = self.player_team();
        let own = self.sim.player_unit_position();
        let unit = self
            .unit_draws_full()
            .into_iter()
            .filter(|unit| unit.team == team)
            .filter(|unit| own != Some((unit.x, unit.y)))
            .find(|unit| (unit.x - wx).hypot(unit.y - wy) <= 5.0 + unit.hit_size / 2.0)
            .map(|unit| unit.name);
        if let Some(name) = unit {
            let mut out = Dictionary::new();
            out.set("kind", &"unit".to_variant());
            out.set("name", &GString::from(name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("unit-{name}-ui").as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("unit.{name}.name").as_str()).to_variant(),
            );
            return out;
        }
        // `world.tileWorld`/`screenToTile` fallback (Java recomputes the tile
        // from the mouse world position).
        self.hover_tile(
            ((wx + size / 2.0) / size).floor() as i32,
            ((wy + size / 2.0) / size).floor() as i32,
        )
    }

    /// Hover target at tile `(x, y)` (`PlacementFragment.hovered`): a unit
    /// within 5 world px of the tile centre, else the tile's building, else the
    /// tile's displayable drop (`Tile.displayable`/`Tile.display`).
    /// `{kind, name, icon, label, team}`; empty when nothing is hovered.
    #[func]
    pub fn hover_info(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        let size = mind_core::config::TILESIZE as f32;
        let cx = x as f32 * size;
        let cy = y as f32 * size;
        // `Units.closestOverlap(player.team(), mouseWorld, 5f, ...)`: a unit
        // under the cursor wins over the tile.
        // Only the player's team, never the player's own unit
        // (`u -> !u.isLocal() && u.displayable()`).
        let team = self.player_team();
        let own = self.sim.player_unit_position();
        let unit = self
            .unit_draws_full()
            .into_iter()
            .filter(|unit| unit.team == team)
            .filter(|unit| own != Some((unit.x, unit.y)))
            .find(|unit| (unit.x - cx).hypot(unit.y - cy) <= 5.0)
            .map(|unit| unit.name);
        if let Some(name) = unit {
            out.set("kind", &"unit".to_variant());
            out.set("name", &GString::from(name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("unit-{name}-ui").as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("unit.{name}.name").as_str()).to_variant(),
            );
            return out;
        }
        // `world.tileWorld` is null off the map handled by `hover_tile`.
        self.hover_tile(x, y)
    }

    /// Build requirements of block `name` for the placement info table
    /// (`PlacementFragment` requirement rows): JSON
    /// `[{item, amount, have, infinite, label}]` — amount scaled by
    /// `Rules.buildCostMultiplier` (`Math.round(stack.amount * multiplier)`),
    /// `have` from the player's first core (`player.core().items`), and
    /// `label` the Java-markup amount text (`*/90` under infinite resources
    /// or without a core; `PlacementFragment.java:407-416`).
    #[func]
    pub fn block_requirements_json(&self, name: GString) -> GString {
        let empty = crate::gdutil::json::empty_array();
        let Some(content) = self.content_registry() else {
            return empty;
        };
        let Some(def) = content.block_id(&name.to_string()) else {
            return empty;
        };
        let Some(def) = content.block(def) else {
            return empty;
        };
        let multiplier = self
            .campaign_runtime()
            .map(|runtime| runtime.session.rules.build_cost_multiplier)
            .filter(|value| *value > 0.0)
            .unwrap_or(1.0);
        let team = self
            .campaign_runtime()
            .map(|runtime| runtime.session.rules.default_team)
            .unwrap_or(0);
        let core_items: std::collections::HashMap<String, i32> =
            self.player_core_items(team).into_iter().collect();
        // `core == null || state.rules.isInfiniteResources(player.team())`
        // (`PlacementFragment.java:410`): Java's team check is
        // `infiniteResources || teams.get(team).infiniteResources`.
        let infinite = self.campaign_runtime().is_some_and(|runtime| {
            let rules = &runtime.session.rules;
            rules.infinite_resources
                || rules
                    .teams
                    .get(team)
                    .is_some_and(|rule| rule.infinite_resources)
        });
        let has_core = self.player_core_blocks(team) > 0;
        let rows: Vec<serde_json::Value> = def
            .requirements
            .iter()
            .filter_map(|stack| {
                let item = content.item(stack.item)?;
                let amount = (stack.amount as f32 * multiplier + 0.5).floor() as i32;
                let have = core_items.get(&item.name).copied().unwrap_or(0);
                let label = mind_core::ui::display::requirement_label(
                    has_core.then_some(have),
                    amount,
                    infinite,
                );
                Some(serde_json::json!({
                    "item": item.name,
                    "amount": amount,
                    "have": have,
                    "infinite": infinite || !has_core,
                    "label": label,
                }))
            })
            .collect();
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(rows))
    }

    /// `PlacementFragment` block-info header state: `unlocked(block)` plus the
    /// `getUnplaceableReason` row (`PlacementFragment.java:380-432,753-761,
    /// 779-782`). JSON `{unlocked, reason, limit}`:
    ///
    /// - `unlocked` — `block.unlockedNowHost() && block.placeablePlayer &&
    ///   block.environmentBuildable() && block.supportsEnv(state.rules.env)`;
    ///   false swaps the header to `block.unknown` and hides `?`.
    /// - `reason` — `null`, or the bundle key `@unit.nobuild`,
    ///   `@unsupported.environment`, `@banned` or `block.limit`.
    /// - `limit` — the `{0}` argument of `block.limit` (0 otherwise).
    #[func]
    pub fn block_placement_info_json(&self, name: GString) -> GString {
        let fallback = GString::from(r#"{"unlocked":true,"reason":null,"limit":0}"#);
        let Some(content) = self.content_registry() else {
            return fallback;
        };
        let name = name.to_string();
        let Some(def) = content.block_id(&name).and_then(|id| content.block(id)) else {
            return fallback;
        };
        let campaign = self.campaign_runtime();
        let is_campaign = campaign.is_some_and(|runtime| runtime.session.is_campaign());
        // `state.getPlanet()`: the launched sector's planet (campaign) or the
        // session `rules.planet` (custom game, default serpulo).
        let planet_name = self
            .last_sector_rules()
            .map(|rules| rules.planet.clone())
            .or_else(|| campaign.map(|runtime| runtime.session.rules.planet.clone()))
            .unwrap_or_default();
        let planet = content.planet_id(&planet_name);
        let sun = content.planet_id("sun");
        let build_rules = self
            .world()
            .get_resource::<mind_core::world::limits::BuildRules>();
        // `state.rules.env`; an absent `BuildRules` keeps the upstream default.
        let env = build_rules.map_or(mind_core::game::rules::DEFAULT_ENV, |rules| rules.env);
        let unlocked =
            mind_core::world::build::placement_unlocked(def, is_campaign, planet, sun, env);
        let team = self.player_team();
        // `Block.isOverPlacementLimit` skips AI teams (`Block.java:1044-1048`).
        let team_is_ai = campaign
            .and_then(|runtime| {
                mind_core::game::team::Team::all()
                    .get(team as usize)
                    .map(|team| team.is_ai(&runtime.session.rules, runtime.session.is_campaign()))
            })
            .unwrap_or(false);
        let counter = self
            .world()
            .get_resource::<mind_core::world::limits::BlockCounter>();
        let reason = mind_core::world::build::unplaceable_reason(
            def,
            &mind_core::world::build::PlacementReasonContext {
                builder: self.sim.player_can_build(),
                editor: build_rules.is_some_and(|rules| rules.editor),
                env,
                team,
                team_is_ai,
                rules: build_rules,
                counter,
            },
        );
        GString::from(
            serde_json::json!({
                "unlocked": unlocked,
                "reason": reason.map(|reason| reason.key()),
                "limit": reason.and_then(|reason| reason.limit()).unwrap_or(0),
            })
            .to_string()
            .as_str(),
        )
    }

    /// `PlacementFragment.java:313-323`: the palette cell tint for every content
    /// block, keyed by block name — `0` white (placeable, affordable and the
    /// player unit is a builder), `1` gray (unaffordable or not a builder),
    /// `2` darkGray (`Block.isPlaceable()` false: the block does not support
    /// the current env). One call per palette refresh instead of a per-cell
    /// seam, mirroring Java's per-frame `button.update`.
    #[func]
    pub fn palette_tints_json(&self) -> GString {
        let Some(content) = self.content_registry() else {
            return crate::gdutil::json::empty_object();
        };
        let rules = self
            .campaign_runtime()
            .map(|runtime| runtime.session.rules.clone());
        let team = rules.as_ref().map_or(0, |rules| rules.default_team);
        let core_items: std::collections::HashMap<String, i32> =
            self.player_core_items(team).into_iter().collect();
        let multiplier = rules
            .as_ref()
            .map(|rules| rules.build_cost_multiplier)
            .filter(|value| *value > 0.0)
            .unwrap_or(1.0);
        // `state.rules.isInfiniteResources(player.team())`
        // (`PlacementFragment.java:316`): the rule field or the team rule.
        let infinite = rules.as_ref().is_some_and(|rules| {
            rules.infinite_resources
                || rules
                    .teams
                    .get(team)
                    .is_some_and(|rule| rule.infinite_resources)
        });
        let builder = self.sim.player_can_build();
        let env = rules.as_ref().map_or(0, |rules| rules.env);
        let mut out = serde_json::Map::new();
        for def in content.blocks() {
            let placeable = mind_core::world::build::supports_env_bits(def, env);
            let affordable = infinite
                || def.requirements.iter().all(|stack| {
                    let amount = (stack.amount as f32 * multiplier + 0.5).floor() as i32;
                    content.item(stack.item).is_some_and(|item| {
                        core_items.get(&item.name).copied().unwrap_or(0) >= amount
                    })
                });
            let tint = mind_core::world::build::palette_tint(placeable, affordable, builder);
            out.insert(def.name.clone(), serde_json::Value::from(tint));
        }
        crate::gdutil::json::value_gstring(&serde_json::Value::Object(out))
    }
}
