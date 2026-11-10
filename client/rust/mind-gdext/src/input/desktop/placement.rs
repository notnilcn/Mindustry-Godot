// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pointer move, place line, place/select commit and scroll.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    pub(super) fn mouse_move(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        // `pollInputPlayer` L630: copied plans follow the raw cursor.
        if !self.controller.state.select_plans.is_empty() {
            let (sx, sy) = self.schematic_origin;
            let (dx, dy) = (x - sx, y - sy);
            if dx != 0 || dy != 0 {
                for plan in &mut self.controller.state.select_plans {
                    plan.x += dx;
                    plan.y += dy;
                }
                self.schematic_origin = (x, y);
                self.refresh_select_preview();
            }
        }
        // `tileX/tileY`: an active placement drag tracks the offset cursor tile.
        let position = if self
            .drag
            .as_ref()
            .is_some_and(|drag| drag.kind == DragKind::Place)
        {
            self.placement_cursor_tile().unwrap_or((x, y))
        } else {
            (x, y)
        };
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if drag.last != position || position != drag.start {
            drag.last = position;
            drag.moved = true;
        }
        let drag = *drag;
        if drag.kind == DragKind::Place {
            self.update_place_line(bindings, drag.start, position);
        }
    }

    /// Rebuilds the placement line from `start` to `end` (`updateLine`).
    pub(super) fn update_place_line(
        &mut self,
        bindings: &BindingState,
        start: (i32, i32),
        end: (i32, i32),
    ) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let diagonal = self.key_down(bindings, ids::DIAGONAL_PLACEMENT);
        let params = LineParams {
            diagonal,
            rotation: self.controller.state.rotation,
            override_line_rotation: self.controller.state.override_line_rotation,
            ..LineParams::default()
        };
        let block = self.controller.state.block;
        let block_replace = self.block_replace;
        {
            let guard = host.bind();
            let world = HostWorld {
                grid: guard.grid(),
                content: guard.content_registry(),
                host: &guard,
            };
            let block_meta = self.line_block_for(&world);
            let rules = block.and_then(|block| {
                world
                    .content
                    .map(|content| mind_core::input::line_rules::line_rules_for(content, block))
            });
            // `changePlacementPath` runs inside `iterateLine` regardless of
            // `blockreplace`; the rules half (`getReplacement` /
            // `handlePlacementLine`) is the gated part (IH:1952-1961).
            let active = rules.as_ref().filter(|_| block_replace);
            match rules.as_ref().and_then(|rules| rules.node) {
                Some(rule) => {
                    let hooks = mind_core::input::line_rules::NodeHooks {
                        rule,
                        world: &world,
                        block: block.unwrap_or(BlockId::AIR),
                    };
                    self.controller.state.update_line_with(
                        &world,
                        block_meta.as_ref(),
                        TilePos::new(start.0 as i16, start.1 as i16),
                        TilePos::new(end.0 as i16, end.1 as i16),
                        &params,
                        &hooks,
                        active,
                    );
                }
                None => {
                    self.controller.state.update_line_with(
                        &world,
                        block_meta.as_ref(),
                        TilePos::new(start.0 as i16, start.1 as i16),
                        TilePos::new(end.0 as i16, end.1 as i16),
                        &params,
                        &mind_core::input::line::NoLineHooks,
                        active,
                    );
                }
            }
        }
        self.controller.state.refresh_preview();
        self.preview_json = serde_json::Value::Array(
            self.controller
                .state
                .line_plans
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

    /// Block metadata for `iterateLine` (flags from the Java block class;
    /// `None` when unknown).
    fn line_block_for(&self, world: &HostWorld<'_>) -> Option<LineBlock> {
        let block = self.controller.state.block?;
        let content = world.content?;
        let def = content.block(block)?;
        Some(mind_core::input::line::line_block_from_def(block, def))
    }

    /// Commits a placement drag into queued `SimCommand::Place`s.
    pub(super) fn commit_place(&mut self, drag: Drag) {
        let Some(block) = self.controller.state.block else {
            return;
        };
        let mut plans: Vec<ClientPlan> = self.controller.state.line_plans.clone();
        if !drag.moved || plans.is_empty() {
            plans = vec![ClientPlan::place(
                drag.last.0,
                drag.last.1,
                self.controller.state.rotation,
                block,
            )];
        }
        // Java `InputHandler.placeBlock` places on `player.team()` — the
        // campaign's default team (`Team.sharded`), not the derelict id.
        let team = self
            .host
            .clone()
            .map(|host| host.bind().player_team())
            .unwrap_or(0);
        // `Block.nextConfig` (`Block.java:827-832`): a block with a remembered
        // config (`saveConfig`) queues through the config-carrying path; line
        // plans may carry their own config (T6b/T8).
        let remembered = self
            .last_config
            .get(&block.raw())
            .cloned()
            .filter(|_| self.block_save_config(block));
        let mut commands: Vec<SimCommand> = Vec::new();
        let mut configured: Vec<(i16, i16, BlockId, u8, mind_core::world::config::ConfigValue)> =
            Vec::new();
        for plan in plans.iter().take(DRAG_TILE_CAP) {
            if plan.breaking || plan.block == BlockId::AIR {
                continue;
            }
            let config = if !matches!(plan.config, mind_core::world::config::ConfigValue::None) {
                plan.config.clone()
            } else if plan.block == block {
                remembered
                    .clone()
                    .unwrap_or(mind_core::world::config::ConfigValue::None)
            } else {
                mind_core::world::config::ConfigValue::None
            };
            if matches!(config, mind_core::world::config::ConfigValue::None) {
                commands.push(SimCommand::Place {
                    x: plan.x as i16,
                    y: plan.y as i16,
                    block: plan.block.raw(),
                    rotation: plan.rotation as i8,
                    team,
                    // Player input queues on the player unit (`flushPlans` ->
                    // `BuilderComp.addBuild`); the builder constructs it (PAR-0098).
                    player: Some(0),
                });
            } else {
                configured.push((
                    plan.x as i16,
                    plan.y as i16,
                    plan.block,
                    plan.rotation,
                    config,
                ));
            }
        }
        for (x, y, plan_block, rotation, config) in configured {
            let _ = self.with_host_mut(|host| {
                host.queue_player_place_config(x, y, plan_block, rotation, config)
            });
        }
        self.enqueue_commands(&commands);
        self.controller.state.line_plans.clear();
        self.controller.state.line.clear();
        self.controller.state.place_mode = PlaceMode::None;
        // `pollInputPlayer` L762-768: the latch clears once the drag is over.
        self.controller.state.override_line_rotation = false;
        self.controller.state.preview.clear();
    }

    /// Commits a command-mode selection drag (`selectUnitsRect`).
    pub(super) fn commit_select(&mut self, drag: Drag) {
        let unit = mind_core::config::TILESIZE as f32;
        let (x1, x2) = (drag.start.0.min(drag.last.0), drag.start.0.max(drag.last.0));
        let (y1, y2) = (drag.start.1.min(drag.last.1), drag.start.1.max(drag.last.1));
        if !drag.moved {
            if let Some((x, y)) = self.cursor_world() {
                match select_unit_tap(&self.selectable, self.player_team(), x, y, UNIT_TAP_RADIUS) {
                    Some(id) => {
                        let now_ms = Time::singleton().get_ticks_msec();
                        let type_id = self
                            .selectable
                            .iter()
                            .find(|unit| unit.id == id)
                            .map(|unit| unit.type_id);
                        let double_tap = type_id.is_some()
                            && type_id == self.last_tap_type
                            && now_ms.saturating_sub(self.last_tap_ms) < UNIT_TAP_INTERVAL_MS;
                        self.controller.state.selected_units.clear();
                        if double_tap {
                            // Double tap selects every unit of the tapped type.
                            let mut out = SmallVec::new();
                            select_typed_units(
                                &self.selectable,
                                self.player_team(),
                                type_id.unwrap_or(0),
                                &mut out,
                            );
                            self.controller.state.selected_units = out;
                        } else {
                            self.controller.state.selected_units.push(id);
                        }
                        self.controller.state.tapped_one = true;
                        self.last_tap_ms = now_ms;
                        self.last_tap_type = type_id;
                    }
                    None => {
                        self.controller.state.selected_units.clear();
                    }
                }
            }
            // `InputHandler.selectUnitsRect` (`IH:1114`): `commandRect = false`
            // — a tap never leaves a drag rect behind (the live rect set by
            // the press is dropped here).
            self.controller.state.command_rect = None;
        } else {
            let rect = SelectRect {
                x: x1 as f32 * unit,
                y: y1 as f32 * unit,
                w: (x2 - x1 + 1) as f32 * unit,
                h: (y2 - y1 + 1) as f32 * unit,
            };
            let mut out = SmallVec::new();
            select_units_rect(&self.selectable, self.player_team(), rect, &mut out);
            self.controller.state.selected_units = out;
            self.controller.state.command_rect = Some((rect.x, rect.y, rect.w, rect.h));
            self.controller.state.tapped_one = self.controller.state.selected_units.len() == 1;
        }
        self.controller.state.command_buildings.clear();
        self.emit_selection();
    }

    /// `update()` scroll gating: rotate the placement, else zoom the camera.
    /// Honours the `rotate`/`zoom` bindings (both default to `scroll`).
    pub(super) fn scroll(&mut self, bindings: &BindingState, delta: f32) {
        if delta == 0.0 || self.ui_dialog {
            return;
        }
        let rotate_on_scroll = Self::axis_is_scroll(bindings, ids::ROTATE);
        let zoom_on_scroll = Self::axis_is_scroll(bindings, ids::ZOOM);
        if !rotate_on_scroll && !zoom_on_scroll {
            return;
        }
        let diagonal_held = self.key_down(bindings, ids::DIAGONAL_PLACEMENT);
        let block = self.controller.state.block;
        let placing = self.controller.state.is_building && block.is_some();
        let ctx = mind_core::input::desktop::ScrollCtx {
            diagonal_held,
            rotate_placed_held: self.key_down(bindings, ids::ROTATE_PLACED),
            zoom_is_rotate: bindings.value(ids::ROTATE) == bindings.value(ids::ZOOM),
            placing,
            block_rotates: block.is_some_and(|block| self.block_rotates(block)),
            select_plans: !self.controller.state.select_plans.is_empty(),
            cursor_build_quick_rotates: self.cursor_build_quick_rotates(),
        };
        let route = mind_core::input::desktop::route_scroll(&ctx);
        let step = if delta > 0.0 { 1 } else { -1 };
        if route.rotate_selection {
            // `pollInputPlayer` L762-768 (snap to the drag angle) runs BEFORE
            // L836-849 (`rotation = mod(rotation + axisTap, 4)`): a wheel during
            // a fresh drag snaps to the drag angle, adds the step and latches
            // `overrideLineRotation`, so every line step gets `input.rotation`.
            let drag = self.drag.filter(|drag| drag.kind == DragKind::Place);
            if placing
                && let Some(drag) = drag
                && !self.controller.state.override_line_rotation
                && !diagonal_held
                && drag.start != drag.last
            {
                self.controller.state.rotation = mind_core::input::placement::line_angle_rotation(
                    drag.start.0,
                    drag.start.1,
                    drag.last.0,
                    drag.last.1,
                );
                self.controller.state.override_line_rotation = true;
            }
            self.controller.state.rotation =
                (self.controller.state.rotation as i32 + step).rem_euclid(4) as u8;
            if placing && let Some(drag) = drag {
                self.update_place_line(bindings, drag.start, drag.last);
            } else if !placing && !self.controller.state.select_plans.is_empty() {
                self.rotate_select_plans(step);
            }
        }
        if route.rotate_placed
            && let Some((x, y)) = self.cursor_tile()
        {
            // `Call.rotateBlock(player, cursor.build, axisTap(rotate) > 0)`.
            self.emit_action(RemoteAction::Rotate {
                x: x as i16,
                y: y as i16,
                direction: delta > 0.0,
            });
        }
        if route.zoom {
            self.zoom(delta);
        }
    }

    /// `Block.rotate` for a block id.
    pub(super) fn block_rotates(&self, block: BlockId) -> bool {
        self.with_host(|host| host.block_rotates(block))
            .unwrap_or(false)
    }

    /// The building under the cursor is interactable and
    /// `block.rotate && block.quickRotate` (`DesktopInput.java:924-926`).
    fn cursor_build_quick_rotates(&self) -> bool {
        let Some((x, y)) = self.cursor_tile() else {
            return false;
        };
        self.with_host(|host| {
            let team = self.player_team();
            let interactive = host
                .building_team(x, y)
                .is_some_and(|building_team| building_team == team || building_team == 0);
            if !interactive {
                return false;
            }
            let Some(content) = host.content_registry() else {
                return false;
            };
            let Some((tx, ty)) = tile_i16(x, y) else {
                return false;
            };
            let Some(block) = host.grid().block_at(TilePos::new(tx, ty)) else {
                return false;
            };
            content.block(block).is_some_and(|def| def.quick_rotate) && host.block_rotates(block)
        })
        .unwrap_or(false)
    }
}
