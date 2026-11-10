// SPDX-License-Identifier: GPL-3.0-only

//! Overlay-top and placement-range JSON seams.
//! Upstream: core/src/mindustry/input/InputHandler.java (overlay)

use super::*;

/// `PowerNode.laserRange` per name (`Blocks.java:2483-2504`), in tiles.
pub(super) fn power_laser_range(name: &str) -> f32 {
    match name {
        "power-node" => 6.0,
        "power-node-large" => 15.0,
        "surge-tower" => 40.0,
        _ => 0.0,
    }
}

/// `ItemBridge.range` for bridge kinds (`ItemBridge.java`), in tiles.
pub(super) fn bridge_link_range(def: &mind_core::content::BlockDef) -> Option<i32> {
    match def.name.as_str() {
        "bridge-conveyor" => Some(4),
        "phase-conveyor" => Some(12),
        _ => None,
    }
}

impl MindSimHost {
    // ---- H03 T5 overlay-top seams ----

    /// One frame of `OverlayRenderer.drawTop` inputs
    /// (`OverlayRenderer.java:91-226`): player position/team, non-player-team
    /// units, enemy cores and the ground spawn tiles.
    pub fn overlay_view(&self) -> OverlayView {
        use mind_core::entities::comp::{Pos, TeamComp};
        let player_team = self.player_team();
        let player = self.player_unit_position();
        let mut enemy_units = Vec::new();
        if let Some(runtime) = self.sim.unit_runtime() {
            for &entity in &runtime.units {
                let Some(pos) = self.world().get::<Pos>(entity) else {
                    continue;
                };
                let team = self
                    .world()
                    .get::<TeamComp>(entity)
                    .map(|comp| comp.team)
                    .unwrap_or(0);
                if team != player_team {
                    enemy_units.push((pos.x, pos.y, team));
                }
            }
        }
        let mut enemy_cores = Vec::new();
        if let Some(runtime) = self.campaign_runtime() {
            for team in runtime.session.teams.active.clone() {
                let Some(data) = runtime.session.teams.data(team) else {
                    continue;
                };
                if team.0 == player_team {
                    continue;
                }
                for &core in &data.cores {
                    if let Some(pos) = self.world().get::<Pos>(core) {
                        enemy_cores.push((pos.x, pos.y, team.0));
                    }
                }
            }
        }
        let spawns = self
            .sim
            .unit_runtime()
            .map(|runtime| {
                runtime
                    .wave_spawner
                    .spawns
                    .iter()
                    .map(|pos| {
                        (
                            (pos.x() as f32 + 0.5) * TILESIZE_F,
                            (pos.y() as f32 + 0.5) * TILESIZE_F,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        OverlayView {
            player,
            player_team,
            enemy_units,
            enemy_cores,
            spawns,
        }
    }

    /// `input.config.getSelected().drawConfigure()` dispatch
    /// (`BuildingComp.java:1732-1737`; `PowerNode.java:458-480`,
    /// `ItemBridge.java:282-296`). `Logic` is filled by H06 T12; everything
    /// else falls back to the generic accent square.
    pub(crate) fn config_overlay(&self, x: i32, y: i32) -> Option<ConfigOverlay> {
        use mind_core::world::modules::PowerModule;
        let content = self.content_snapshot.as_ref()?;
        let tile = self.tile_at(x, y)?;
        let entity = tile.build?;
        let def = content.block(tile.block)?;
        let (cx, cy) = tile_center(x, y, def.offset);
        if matches!(def.kind, BlockKind::PowerNode) {
            let laser_range = power_laser_range(&def.name) * TILESIZE_F;
            let mut links = Vec::new();
            if let Some(module) = self.world().get::<PowerModule>(entity) {
                for packed in &module.links {
                    let (lx, ly) = mind_core::world::pos::unpack(*packed);
                    let Some(other) = self.tile_at(lx as i32, ly as i32) else {
                        continue;
                    };
                    let size = self
                        .content_snapshot
                        .as_ref()
                        .and_then(|content| content.block(other.block))
                        .map(|def| def.size)
                        .unwrap_or(1);
                    links.push((
                        lx as f32 * TILESIZE_F + size as f32 * 4.0,
                        ly as f32 * TILESIZE_F + size as f32 * 4.0,
                        size,
                    ));
                }
            }
            return Some(ConfigOverlay::PowerNodeLinks {
                x: cx,
                y: cy,
                size: def.size,
                laser_range,
                links,
            });
        }
        let bridge_range = bridge_link_range(def);
        if let Some(range) = bridge_range {
            use mind_core::world::blocks::distribution::item_bridge::ItemBridgeBuild;
            let linked = self
                .world()
                .get::<ItemBridgeBuild>(entity)
                .map(|bridge| bridge.link)
                .unwrap_or(-1);
            let team = self
                .world()
                .get::<mind_core::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            let mut targets = Vec::new();
            let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            for (dx, dy) in dirs {
                for step in 1..=range {
                    let (tx, ty) = (x + dx * step, y + dy * step);
                    let Some(other) = self.tile_at(tx, ty) else {
                        continue;
                    };
                    if other.build.is_none() || other.block != tile.block {
                        continue;
                    }
                    let other_team = other
                        .build
                        .and_then(|entity| {
                            self.world()
                                .get::<mind_core::entities::comp::TeamComp>(entity)
                        })
                        .map(|comp| comp.team)
                        .unwrap_or(0);
                    if other_team != team {
                        continue;
                    }
                    targets.push((
                        tx as f32 * TILESIZE_F + 4.0,
                        ty as f32 * TILESIZE_F + 4.0,
                        linked == (tx << 16) | ty,
                    ));
                }
            }
            return Some(ConfigOverlay::BridgeLink {
                x: cx,
                y: cy,
                size: def.size,
                range,
                targets,
            });
        }
        if def.kind == BlockKind::LogicBlock {
            // `LogicBlock.drawConfigure` (`LogicBlock.java:661-683`).
            let privileged = def.privileged;
            // `LogicBlock.range` lives on the family knobs (`LogicBlockDef`).
            let range = (!privileged)
                .then(|| {
                    mind_core::logic::blocks::logic_def_of(self.world(), entity)
                        .map(|def| def.range)
                        .unwrap_or(0.0)
                })
                .filter(|range| *range > 0.0);
            let links = mind_core::logic::blocks::logic_block::overlay_links(self.world(), entity);
            return Some(ConfigOverlay::Logic {
                x: cx,
                y: cy,
                size: def.size,
                range,
                links,
            });
        }
        Some(ConfigOverlay::Generic {
            x: cx,
            y: cy,
            size: def.size,
        })
    }

    /// `Building.drawSelect` dispatch for the hovered player-team building
    /// (`OverlayRenderer.java:213-229`): turret/projector range circles,
    /// power-node `laserRange` circles (`PowerNode.java:445-453`).
    pub(crate) fn select_overlay(&self, x: i32, y: i32) -> SelectOverlay {
        let Some(tile) = self.tile_at(x, y) else {
            return SelectOverlay::None;
        };
        let Some(entity) = tile.build else {
            return SelectOverlay::None;
        };
        let team = self
            .world()
            .get::<mind_core::entities::comp::TeamComp>(entity)
            .map(|comp| comp.team)
            .unwrap_or(0);
        let content = self.content_snapshot.as_ref();
        let def = content.and_then(|content| content.block(tile.block));
        let (cx, cy) = def
            .map(|def| tile_center(x, y, def.offset))
            .unwrap_or((x as f32 * TILESIZE_F, y as f32 * TILESIZE_F));
        if team != self.player_team() {
            return SelectOverlay::None;
        }
        if def.is_some_and(|def| def.kind == BlockKind::MessageBlock) {
            // `MessageBlock.drawSelect`: the stored message above the block.
            let text = self
                .world()
                .get::<mind_core::logic::blocks::MessageBlockState>(entity)
                .map(|state| state.message.clone())
                .unwrap_or_default();
            if !text.is_empty() {
                return SelectOverlay::MessageText { x: cx, y: cy, text };
            }
        }
        if def.is_some_and(|def| def.kind == BlockKind::LogicBlock) {
            // `LogicBuild.drawSelect` (`LogicBlock.java:685-725`).
            let icon_assets = self
                .base()
                .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets");
            let (tag, icon) = self
                .world()
                .get::<mind_core::logic::blocks::LogicBlockState>(entity)
                .map(|state| {
                    let icon = (state.icon_tag != '\0')
                        .then(|| {
                            icon_assets.as_ref().map(|assets| {
                                let name = assets.bind().icon_name_for_code(state.icon_tag as i32);
                                let region = assets.bind().icon_region_for_name(name.clone());
                                (name.to_string(), region.to_string())
                            })
                        })
                        .flatten()
                        .filter(|(_, region)| !region.is_empty());
                    (state.tag.clone(), icon)
                })
                .unwrap_or((None, None));
            let mut units = Vec::new();
            for unit in self.world().iter_entities() {
                let Some(ai) = unit.get::<mind_core::ai::types::logic::LogicAi>() else {
                    continue;
                };
                if ai.controller != Some(entity) {
                    continue;
                }
                let Some(pos) = unit.get::<mind_core::entities::comp::Pos>() else {
                    continue;
                };
                let hit = unit
                    .get::<mind_core::entities::comp::unit::comp::HitboxComp>()
                    .map(|hitbox| hitbox.hit_size)
                    .unwrap_or(0.0);
                let rotation = unit
                    .get::<mind_core::entities::comp::unit::comp::UnitCore>()
                    .map(|core| core.rotation)
                    .unwrap_or(0.0);
                units.push((pos.x, pos.y, hit, rotation));
            }
            if tag.is_some() || icon.is_some() || !units.is_empty() {
                return SelectOverlay::LogicTag {
                    x: cx,
                    y: cy,
                    tag,
                    icon,
                    units,
                };
            }
        }
        if let Some((range, kind, color)) = self.block_place_range(tile.block) {
            return SelectOverlay::Range {
                x: cx,
                y: cy,
                range,
                color,
                circles: matches!(kind, PlaceRangeKind::PowerNode),
            };
        }
        SelectOverlay::None
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Debug probe for the logic overlays (I03 T11): the config overlay and
    /// the hover select overlay of the building at `(x, y)` as JSON.
    #[func]
    pub fn overlay_probe(&self, x: i32, y: i32) -> GString {
        let config = match self.config_overlay(x, y) {
            Some(ConfigOverlay::Logic {
                x, y, range, links, ..
            }) => serde_json::json!({
                "kind": "logic",
                "x": x,
                "y": y,
                "range": range,
                "links": links
                    .iter()
                    .map(|(lx, ly, size, name)| serde_json::json!({
                        "x": lx, "y": ly, "size": size, "name": name,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Some(ConfigOverlay::Generic { x, y, size }) => serde_json::json!({
                "kind": "generic", "x": x, "y": y, "size": size,
            }),
            Some(ConfigOverlay::PowerNodeLinks { x, y, .. }) => serde_json::json!({
                "kind": "power-node", "x": x, "y": y,
            }),
            Some(ConfigOverlay::BridgeLink { x, y, .. }) => serde_json::json!({
                "kind": "bridge", "x": x, "y": y,
            }),
            None => serde_json::Value::Null,
        };
        let select = match self.select_overlay(x, y) {
            SelectOverlay::LogicTag {
                x,
                y,
                tag,
                icon,
                units,
            } => serde_json::json!({
                "kind": "logic-tag",
                "x": x,
                "y": y,
                "tag": tag,
                "icon": icon.as_ref().map(|(name, _)| name.as_str()),
                "units": units
                    .iter()
                    .map(|(ux, uy, hit, rotation)| serde_json::json!({
                        "x": ux, "y": uy, "hit": hit, "rotation": rotation,
                    }))
                    .collect::<Vec<_>>(),
            }),
            SelectOverlay::MessageText { x, y, text } => serde_json::json!({
                "kind": "message", "x": x, "y": y, "text": text,
            }),
            SelectOverlay::Range { x, y, range, .. } => serde_json::json!({
                "kind": "range", "x": x, "y": y, "range": range,
            }),
            SelectOverlay::None => serde_json::Value::Null,
        };
        GString::from(
            serde_json::json!({"config": config, "select": select})
                .to_string()
                .as_str(),
        )
    }

    /// Debug probe for the placement range lookup (`Block.drawPlace` /
    /// `drawSelect`): `{"range","kind","color"}` or `"null"` for a block name.
    /// H03 T5 step 6 diagnosis seam.
    #[func]
    pub fn place_range_json(&self, name: GString) -> GString {
        let Some(content) = self.content_registry() else {
            return GString::from("null");
        };
        let Some(id) = content.block_id(&name.to_string()) else {
            return GString::from("null");
        };
        match self.block_place_range(id) {
            Some((range, kind, color)) => {
                let kind = match kind {
                    PlaceRangeKind::Turret => "turret",
                    PlaceRangeKind::Projector => "projector",
                    PlaceRangeKind::PowerNode => "power-node",
                };
                GString::from(
                    serde_json::json!({"range": range, "kind": kind, "color": color})
                        .to_string()
                        .as_str(),
                )
            }
            None => GString::from("null"),
        }
    }
}
