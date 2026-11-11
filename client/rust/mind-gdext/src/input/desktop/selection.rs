// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Select plans, schematic copy/rebuild, last config and the placement view.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    /// `PlanBlockInfo` resolver for [`rotate_plans_about`]/[`flip_plans_about`]
    /// (G08 T6b): content size/offset, `Block.rotate` and the `electrolyzer`
    /// `invertFlip` (`Blocks.java:1363`).
    fn plan_info(host: &Gd<MindSimHost>) -> impl Fn(BlockId) -> PlanBlockInfo + '_ {
        move |block| {
            let guard = host.bind();
            let def = guard
                .content_registry()
                .and_then(|content| content.block(block));
            PlanBlockInfo {
                size: def.map(|def| def.size).unwrap_or(1),
                offset: def.map(|def| def.offset).unwrap_or(0.0),
                rotate: guard.block_rotates(block),
                invert_flip: def.is_some_and(|def| def.name == "electrolyzer"),
            }
        }
    }

    /// Select-plan preview (`[{x,y,rotation,block}]`, same shape as the
    /// placement line).
    pub(super) fn refresh_select_preview(&mut self) {
        self.preview_json = serde_json::Value::Array(
            self.controller
                .state
                .select_plans
                .iter()
                .map(|plan| {
                    serde_json::json!({
                        "x": plan.x,
                        "y": plan.y,
                        "rotation": plan.rotation,
                        "block": plan.block.raw(),
                    })
                })
                .collect(),
        )
        .to_string();
    }

    /// Adopts copied plans (`useSchematic`: dialog or F-drag) at the cursor and
    /// refreshes the preview; the block selection is cleared.
    pub fn adopt_select_plans(&mut self, plans: Vec<ClientPlan>, origin: (i32, i32)) {
        self.controller.state.select_block(None);
        self.controller.state.select_plans = plans;
        self.schematic_origin = origin;
        self.refresh_select_preview();
    }

    /// `pollInputPlayer` L843-858: wheel/rotate over the copied plans.
    pub(super) fn rotate_select_plans(&mut self, step: i32) {
        if self.controller.state.select_plans.is_empty() {
            return;
        }
        let Some((ox, oy)) = self.raw_cursor_tile() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let info = Self::plan_info(&host);
        rotate_plans_about(&mut self.controller.state.select_plans, step, ox, oy, &info);
        self.refresh_select_preview();
    }

    /// `pollInputPlayer` L641: Z/X flip the copied plans about the raw cursor.
    pub(super) fn flip_select_plans(&mut self, flip_x: bool) {
        if self.controller.state.select_plans.is_empty() {
            return;
        }
        let Some((ox, oy)) = self.raw_cursor_tile() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let info = Self::plan_info(&host);
        flip_plans_about(
            &mut self.controller.state.select_plans,
            flip_x,
            ox,
            oy,
            &info,
        );
        self.refresh_select_preview();
    }

    /// `keyRelease(schematicSelect)` (`DesktopInput.java:633-640`):
    /// `schematics.create` copies the F-selection into select plans, clears the
    /// block selection and keeps the plans selected for a paste.
    fn finish_schematic_copy(&mut self, bindings: &BindingState) {
        let Some(start) = self.schem_start else {
            return;
        };
        let Some(end) = self.raw_cursor_tile() else {
            return;
        };
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let plans = {
            let guard = host.bind();
            guard.copy_schematic_plans(start, end, end)
        };
        self.controller.state.select_block(None);
        self.controller.state.command_rect = None;
        {
            let mut guard = host.bind_mut();
            guard.select_block(GString::from("air"));
        }
        self.adopt_select_plans(plans, end);
        // `if(!keyDown(rebuildSelect)){ schemX = schemY = -1 }` (the B-release
        // half is T7's).
        if !self.key_down(bindings, ids::REBUILD_SELECT) {
            self.schem_start = None;
        }
    }

    /// Key-release bindings (`DesktopInput.pollInputPlayer` release half).
    pub(super) fn key_up_event(&mut self, bindings: &BindingState, code: &str) {
        // T9: a pointer binding rebound to a key releases through the same
        // handlers (`select` -> left release, `break_block`/`deselect` ->
        // right release).
        if bindings.name(ids::SELECT) == Some(code) {
            self.left_release(bindings);
        }
        if bindings.name(ids::BREAK_BLOCK) == Some(code)
            || bindings.name(ids::DESELECT) == Some(code)
        {
            self.right_release(bindings);
        }
        if bindings.name(ids::DIAGONAL_PLACEMENT) == Some(code) {
            // `HintsFragment.conveyorPathfind` = `keyRelease(diagonalPlacement)`.
            self.push_hint("diagonal_release");
        }
        if bindings.name(ids::SCHEMATIC_SELECT) == Some(code) {
            // `HintsFragment.schematicSelect` = `keyRelease(schematicSelect)`.
            self.push_hint("schematic_select_release");
        }
        let schematic = bindings.name(ids::SCHEMATIC_SELECT) == Some(code);
        let rebuild = bindings.name(ids::REBUILD_SELECT) == Some(code);
        if !(schematic || rebuild) || self.schem_start.is_none() || self.drag.is_some() {
            return;
        }
        if schematic {
            self.finish_schematic_copy(bindings);
        } else {
            self.finish_rebuild_select(bindings);
        }
    }

    /// `keyRelease(rebuildSelect)` (`InputHandler.java:2318-2330`): queue the
    /// team ghost plans overlapping the B-selection. (The `tryRepairDerelict`
    /// scan, `InputHandler.java:2331-2341`, is deferred: G02 T8.)
    fn finish_rebuild_select(&mut self, bindings: &BindingState) {
        let Some(start) = self.schem_start else {
            return;
        };
        let Some(end) = self.raw_cursor_tile() else {
            return;
        };
        let _ = self.with_host_mut(|host| host.queue_team_rebuild(start.0, start.1, end.0, end.1));
        // `if(!keyDown(schematicSelect)){ schemX = schemY = -1 }`.
        if !self.key_down(bindings, ids::SCHEMATIC_SELECT) {
            self.schem_start = None;
        }
    }

    /// `BuildingComp.configure` -> `Block.lastConfig` (`Block.java:827-832`):
    /// remembers the submitted config per saveConfig block for `nextConfig`.
    pub fn record_last_config(
        &mut self,
        x: i32,
        y: i32,
        value: mind_core::world::config::ConfigValue,
    ) {
        let Some((tx, ty)) = tile_i16(x, y) else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return;
        };
        let Some(block) = guard.grid().block_at(TilePos::new(tx, ty)) else {
            return;
        };
        let Some(def) = content.block(block) else {
            return;
        };
        if !def.save_config {
            return;
        }
        // The wire carries content names (`to_sim_config`); resolve them like
        // the sim's `Configure` arm so `nextConfig` stores the typed value.
        use mind_core::world::config::ConfigValue;
        let value = match value {
            ConfigValue::String(name) => {
                if let Some(item) = content.item_by_name(&name) {
                    ConfigValue::Item(item.id)
                } else if let Some(block) = content.block_by_name(&name) {
                    ConfigValue::Block(block.id)
                } else if let Some(unit) = content.unit_by_name(&name) {
                    ConfigValue::Unit(unit.id)
                } else if let Some(liquid) = content.liquid_by_name(&name) {
                    ConfigValue::Liquid(liquid.id)
                } else {
                    ConfigValue::String(name)
                }
            }
            other => other,
        };
        self.last_config.insert(block.raw(), value);
    }

    /// `Block.saveConfig` for `block` (`nextConfig` gate, `Block.java:827-832`).
    pub(super) fn block_save_config(&self, block: BlockId) -> bool {
        self.with_host(|host| {
            host.content_registry()
                .and_then(|content| content.block(block))
                .map(|def| def.save_config)
        })
        .flatten()
        .unwrap_or(false)
    }

    /// Snapshot for the placement overlay pass (G03 T2).
    pub fn placement_view(&self) -> PlacementView {
        let mode = self.controller.state.place_mode;
        // `DesktopInput.drawTop` gate (`DesktopInput.java:133-145`): breaking
        // mode and keyboard focus suppress both rects, F needs a latched start
        // and beats B. The latch is `schemX/schemY`; the rect end is the cursor.
        let show = selection_show(
            self.schem_start.is_some(),
            self.schematic_held,
            self.rebuild_held,
            mode == PlaceMode::Breaking,
            self.text_focus,
        );
        let selection = match (show, self.schem_start) {
            (SelectionShow::None, _) | (_, None) => None,
            (show, Some(start)) => Some((show, start)),
        };
        PlacementView {
            cursor: self.cursor_tile(),
            block: self.controller.state.block,
            rotation: self.controller.state.rotation,
            mode,
            line_plans: if mode == PlaceMode::Placing {
                self.controller.state.line_plans.clone()
            } else {
                Vec::new()
            },
            break_rect: match self.drag {
                Some(drag) if drag.kind == DragKind::Break => Some((drag.start, drag.last)),
                _ => None,
            },
            // `InputHandler.drawUnitSelection` (`IH:1354-1368`): the box only
            // exists while the select-rect drag is held (`commandRect`); the
            // post-release tile rect (`selectUnitsRect`, kept in the state for
            // the preview handoff) is not a live drag and must not draw.
            command_rect: match self.drag {
                Some(drag) if drag.kind == DragKind::SelectRect => {
                    self.controller.state.command_rect
                }
                _ => None,
            },
            selection,
            // `selectPlans` (`DesktopInput.java:175-185`): the copied plans
            // pasted under the cursor, drawn every frame while non-empty.
            select_plans: self.controller.state.select_plans.clone(),
        }
    }

    /// World pixel center of the cursor tile (Java `Tile.worldx()`).
    pub(super) fn cursor_world(&self) -> Option<(f32, f32)> {
        let (x, y) = self.cursor_tile()?;
        let unit = mind_core::config::TILESIZE as f32;
        Some((x as f32 * unit, y as f32 * unit))
    }

    /// `InputHandler.selectedUnit()` (`InputHandler.java:2148-2163`):
    /// `Units.closest(player.team(), mouseX, mouseY, 40, u -> u.isAI() &&
    /// u.playerControllable)`, accepted only when the hitbox grown by 6 on each
    /// side contains the cursor; else the ControlBlock branch
    /// (`InputHandler.java:2159-2161`): the own turret's `block` unit under the
    /// cursor (`MindSimHost::control_block_unit_id`; I01 T12).
    pub(super) fn selected_unit_for_control(&self) -> Option<i32> {
        let (wx, wy) = self.aim_world()?;
        let team = self.player_team();
        let best = self
            .selectable
            .iter()
            .filter(|unit| unit.team == team && unit.commandable)
            .filter_map(|unit| {
                let dx = unit.x - wx;
                let dy = unit.y - wy;
                let dst2 = dx * dx + dy * dy;
                (dst2 <= CONTROL_RADIUS * CONTROL_RADIUS).then_some((dst2, unit))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, unit)) = best {
            let half = unit.hit_size / 2.0 + CONTROL_HITBOX_GROW;
            if (unit.x - wx).abs() <= half && (unit.y - wy).abs() <= half {
                return Some(unit.id);
            }
        }
        // `build instanceof ControlBlock cont && cont.canControl() && build.team
        // == player.team() && cont.unit() != player.unit() && cont.unit().isAI()`
        // -> `cont.unit()`; the sim resolves the turret's proxy `block` unit.
        if let Some((x, y)) = self.cursor_tile()
            && let Some(id) = self
                .with_host_mut(|host| host.control_block_unit_id(x, y))
                .flatten()
        {
            return Some(id);
        }
        None
    }

    /// `InputHandler.selectedControlBuild()` (`InputHandler.java:2165-2171`):
    /// a same-team building with `canControlSelect(player.unit())` under the
    /// cursor — `CoreBlock` (core respawn) and the `PayloadConveyor`/
    /// `PayloadRouter` family (enter payload).
    pub(super) fn selected_control_build(&self) -> Option<(i32, i32)> {
        let (x, y) = self.cursor_tile()?;
        let kind = self.block_kind_at(x, y)?;
        let payload_build = matches!(kind, BlockKind::PayloadConveyor | BlockKind::PayloadRouter);
        if kind != BlockKind::CoreBlock && !payload_build {
            return None;
        }
        let team = self.with_host(|host| host.building_team(x, y)).flatten()?;
        if team != self.player_team() {
            return None;
        }
        if !payload_build {
            // `CoreBlock.canControlSelect(unit)` = `player.isPlayer()`.
            return Some((x, y));
        }
        // `PayloadConveyor.canControlSelect` (PayloadConveyor.java:85-87):
        // `unit.tileOn().build == this` (the `spawnedByCore`/payload gate is
        // approximated by the tile condition; the sim half of
        // `BuildingControlSelect` is H01's).
        let (px, py) = self
            .with_host(|host| host.player_unit_position())
            .flatten()?;
        let (tx, ty) = (WorldGrid::to_tile(px), WorldGrid::to_tile(py));
        (tx == x && ty == y).then_some((x, y))
    }
}
