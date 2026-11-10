// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mouse press/release, plan grabbing and the tile-tap decision tree.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    pub(super) fn mouse_button(&mut self, bindings: &BindingState, button: &str, down: bool) {
        let Some(key) = mouse_key_name(button) else {
            return;
        };
        if down {
            self.pressed.insert(key.to_owned());
        } else {
            self.pressed.remove(key);
        }
        // `Binding.select`/`deselect`/`break_block`/`pick`/`command_queue` own
        // the pointer gestures (T9); the raw button is only their default key
        // (`DesktopInput` reads the bindings, not the button).
        let uses = |id: u16| matches!(bindings.value(id), BindingValue::Key(name) if name == key);
        let select = uses(ids::SELECT);
        let right = uses(ids::BREAK_BLOCK) || uses(ids::DESELECT);
        let middle = uses(ids::PICK) || uses(ids::COMMAND_QUEUE);
        if down {
            if select {
                self.left_press(bindings);
            }
            if right {
                self.right_press(bindings);
            }
            if middle {
                if self.controller.state.command_mode {
                    // DI:924-934: command mode queues the order on the middle
                    // click (`Binding.commandQueue`); outside it, pick (T5).
                    self.command_tap_queued();
                } else {
                    self.pick_block();
                }
            }
        } else {
            if select {
                self.left_release(bindings);
            }
            if right {
                self.right_release(bindings);
            }
        }
        // Every other binding sharing the button still gets its key-down edge
        // (`pan` = mouseForward, `mouseMove` = mouseBack, user rebinds).
        let mut fired: Vec<&'static str> = Vec::new();
        for (index, bind) in KeyBindTable::all().iter().enumerate() {
            if POINTER_BINDS.contains(&bind.name) {
                continue;
            }
            if uses(index as u16) {
                fired.push(bind.name);
            }
        }
        for name in fired {
            if down {
                self.dispatch_action(bindings, name);
            }
        }
    }

    pub(super) fn left_press(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        // `DesktopInput.update` L408-421: Ctrl+select possesses an own AI unit
        // (`Call.unitControl`) or a controllable building
        // (`Call.buildingControlSelect`). Java does not return here, so the
        // press still runs its normal select/place handling.
        if self.key_down(bindings, ids::CONTROL) {
            if let Some(id) = self.selected_unit_for_control() {
                self.emit_action(RemoteAction::UnitControl { unit: Some(id) });
                self.shooting = false;
            } else if let Some((bx, by)) = self.selected_control_build() {
                self.emit_action(RemoteAction::BuildingControlSelect {
                    x: bx as i16,
                    y: by as i16,
                });
            }
        }
        if self.controller.state.command_mode {
            // `DesktopInput.java:567-573` / `:722-726`: a command-mode press
            // starts the rect at the raw cursor point (`commandRectX/Y`); the
            // live mouse is the second corner (`drawUnitSelection`), so the
            // size is zero until the pointer moves.
            let (wx, wy) = self.aim_world().unwrap_or((
                x as f32 * mind_core::config::TILESIZE as f32,
                y as f32 * mind_core::config::TILESIZE as f32,
            ));
            self.drag = Some(Drag {
                kind: DragKind::SelectRect,
                start: (x, y),
                last: (x, y),
                moved: false,
            });
            self.controller.state.command_rect = Some((wx, wy, 0.0, 0.0));
            return;
        }
        // `pollInputPlayer` select tap (`DesktopInput.java:704-705`): a click
        // with copied plans flushes them to the player queue and keeps them
        // selected for a repeat paste (`validPlace` filters inside).
        if !self.controller.state.select_plans.is_empty() {
            let plans = self.controller.state.select_plans.clone();
            for plan in plans {
                let _ = self.with_host_mut(|host| {
                    host.queue_player_place_config(
                        plan.x as i16,
                        plan.y as i16,
                        plan.block,
                        plan.rotation,
                        plan.config,
                    )
                });
            }
            return;
        }
        if self.controller.state.block.is_none() {
            // `InputHandler.java:726` (`!checkConfigTap() && selected != null &&
            // !tryRepairDerelict(selected)`; H08 T11): a derelict building
            // under the cursor queues its repair before the
            // tap-player/tile-tap chain.
            if let Some(true) = self.with_host_mut(|host| host.try_repair_derelict(x, y)) {
                return;
            }
            // `DesktopInput.java:701-721` order: `tryTapPlayer`, `tileTapped`,
            // the active build plan, then `tryStopMine`/`tryBeginMine`;
            // anything consumed keeps `player.shooting` false.
            if let Some((wx, wy)) = self.aim_world()
                && self.can_tap_player(wx, wy)
            {
                self.dropping_item = true;
                return;
            }
            if self.tile_tapped(x, y) {
                return;
            }
            if self
                .host
                .as_ref()
                .is_some_and(|host| host.bind().player_actively_building())
            {
                return;
            }
            if self.try_mine_at(x, y) {
                return;
            }
            // `DesktopInput.java:710-717`: tapping one of the player's own
            // unstarted queued plans grabs it (`splan = plan`) instead of
            // shooting; the release drops it at the dragged tile and Ctrl opens
            // its point config (`planConfig.showConfig`).
            if self.grab_plan() {
                return;
            }
            // `shouldShoot = !scene.hasMouse() && !locked && !state.isEditor()`;
            // the UI gates ran in `MindInput::input`.
            self.shooting = !self.locked;
            if self.shooting {
                // `HintsFragment.desktopShoot` (`player.shooting`).
                self.push_hint("shoot");
            }
            return;
        }
        if !self.controller.state.is_building {
            return;
        }
        // `tileX/tileY`: even-size blocks are centred on the cursor.
        let Some((x, y)) = self.placement_cursor_tile() else {
            return;
        };
        self.controller.state.begin_place();
        // A fresh drag never starts with a latched wheel override
        // (`pollInputPlayer` L762-768 clears it on every frame between drags).
        self.controller.state.override_line_rotation = false;
        self.drag = Some(Drag {
            kind: DragKind::Place,
            start: (x, y),
            last: (x, y),
            moved: false,
        });
        self.update_place_line(bindings, (x, y), (x, y));
    }

    pub(super) fn left_release(&mut self, bindings: &BindingState) {
        // `DesktopInput.java:507-509`: releasing select stops shooting; the
        // release also resolves a pending tap-unit gesture (`tryDropItems`).
        self.shooting = false;
        self.try_drop_items();
        // `DesktopInput.java:800-813`: a grabbed `splan` is committed before
        // the normal drag-release branches.
        if self.release_splan(bindings) {
            return;
        }
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag.kind {
            DragKind::Place => self.commit_place(drag),
            DragKind::SelectRect => self.commit_select(drag),
            DragKind::Break => {}
        }
    }

    // ---- queued plan drag + point config (GAPS #46, PAR-0555) -------------

    /// `getPlan(cursorX, cursorY)` over the player's own queue: a non-breaking,
    /// not-yet-started plan whose `Block.bounds` contain the world point
    /// (`DesktopInput.java:656/710`). The config is carried from
    /// [`MindSimHost::player_plan_at_world`].
    fn queued_plan_at(&self, wx: f32, wy: f32) -> Option<ClientPlan> {
        self.with_host(|guard| {
            let content = guard.content_registry()?;
            let json = guard.player_build_plans_json();
            let plans: Vec<serde_json::Value> = serde_json::from_str(&json.to_string()).ok()?;
            for entry in &plans {
                // `!plan.breaking && !plan.initialized && plan.progress <= 0f`.
                if entry.get("breaking").and_then(serde_json::Value::as_bool) != Some(false) {
                    continue;
                }
                if entry
                    .get("progress")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0)
                    > 0.0
                {
                    continue;
                }
                let Some(block_name) = entry.get("block").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(block) = content.block_id(block_name) else {
                    continue;
                };
                let Some(def) = content.block(block) else {
                    continue;
                };
                let (Some(x), Some(y), Some(rotation)) = (
                    entry.get("x").and_then(serde_json::Value::as_i64),
                    entry.get("y").and_then(serde_json::Value::as_i64),
                    entry.get("rotation").and_then(serde_json::Value::as_i64),
                ) else {
                    continue;
                };
                // `Block.bounds` (`Block.java:1591-1594`; same derivation as
                // `player_plan_at_world`): even-size blocks centre on the cursor.
                let offset = ((def.size + 1) % 2) as f32 * 4.0;
                let half = def.size as f32 * 4.0;
                let cx = x as f32 * 8.0 + offset;
                let cy = y as f32 * 8.0 + offset;
                if wx < cx - half || wx >= cx + half || wy < cy - half || wy >= cy + half {
                    continue;
                }
                let (_, config) = guard.player_plan_at_world(wx, wy)?;
                return Some(ClientPlan {
                    x: x as i32,
                    y: y as i32,
                    rotation: rotation as u8,
                    block,
                    config,
                    breaking: false,
                    anim_scale: 1.0,
                });
            }
            None
        })
        .flatten()
    }

    /// `DesktopInput.java:710-717`: latch a queued plan under the cursor
    /// (`splan = plan; movedPlan = false; buildPlanMouseOffset...`).
    fn grab_plan(&mut self) -> bool {
        let Some((wx, wy)) = self.aim_world() else {
            return false;
        };
        let Some(plan) = self.queued_plan_at(wx, wy) else {
            return false;
        };
        self.splan = Some(PlanDrag {
            origin: (plan.x, plan.y),
            grab_world: (wx, wy),
            tile: (plan.x, plan.y),
            plan,
            moved: false,
        });
        true
    }

    /// `pollInputPlayer` L657-665: `splan.x/y = round((mouseWorld + offset) /
    /// tilesize)`; `movedPlan` latches when the tile changes.
    pub(super) fn update_splan(&mut self) {
        let Some((wx, wy)) = self.aim_world() else {
            return;
        };
        let Some(drag) = self.splan.as_mut() else {
            return;
        };
        let tile = plan_drag_tile(drag.origin, drag.grab_world, (wx, wy));
        if tile != drag.tile {
            drag.tile = tile;
            drag.plan.x = tile.0;
            drag.plan.y = tile.1;
            drag.moved = true;
        }
    }

    /// `DesktopInput.java:800-813`: commit the dragged plan (if moved), then
    /// Ctrl -> `planConfig.showConfig(splan)`, else `planConfig.hide()`.
    pub(super) fn release_splan(&mut self, bindings: &BindingState) -> bool {
        let Some(drag) = self.splan.take() else {
            return false;
        };
        let mut plan = drag.plan.clone();
        plan.x = drag.tile.0;
        plan.y = drag.tile.1;
        if drag.moved {
            self.move_queued_plan(&plan, drag.origin);
        }
        if self.key_down(bindings, ids::CONTROL) {
            // `inv.hide(); config.hideConfig(); planConfig.showConfig(splan);`
            self.effects.push(Effect::HidePanels);
            if let Some(spec) = self.plan_config_spec(&plan) {
                self.plan_config_plan = Some(plan);
                self.effects.push(Effect::OpenPlanConfig { spec });
            }
        } else {
            // `planConfig.hide()`.
            self.effects.push(Effect::HidePlanConfig);
            self.plan_config_plan = None;
        }
        true
    }

    /// Moves a queued plan to the released tile. The port's queue has no
    /// in-place move, so the origin plan is dropped from its rect and re-queued
    /// at the new tile (queue position moves to the tail; Java keeps it).
    fn move_queued_plan(&mut self, plan: &ClientPlan, from: (i32, i32)) {
        let Some((x, y)) = tile_i16(plan.x, plan.y) else {
            return;
        };
        let config = plan.config.clone();
        let _ = self.with_host_mut(|host| host.remove_plans_in(from.0, from.1, from.0, from.1, 1));
        let _ = self.with_host_mut(|host| {
            host.queue_player_place_config(x, y, plan.block, plan.rotation, config)
        });
    }

    /// `PlanConfigFragment.showConfig`'s option source:
    /// `Block.getPlanConfigs(options)` — the base declared item/liquid classes
    /// (`Block.java:1664-1673`) and the `UnitFactory`/`Constructor`/
    /// `PayloadSource`/`PayloadRouter` overrides, resolved on the runtime
    /// [`mind_core::world::block::BlockInstance`] with the live rules filters.
    fn plan_config_spec(&self, plan: &ClientPlan) -> Option<String> {
        self.with_host(|guard| {
            let content = guard.content_registry()?;
            let def = content.block(plan.block)?;
            // `PlanConfigFragment.showConfig`: `if(!block.configurable) return;`.
            if !def.configurable {
                return None;
            }
            let rules = guard.live_rules();
            // The overrides read the runtime instance (the `Constructor` size range
            // lives on the behavior, `UnitFactory.plans` on the block); the table is
            // only built when the popup actually opens (Ctrl-release of a plan).
            let table = mind_core::world::block::BlockTable::build_default(content).ok()?;
            let instance = table.instance(plan.block)?;
            let options = instance.plan_configs(content, rules.as_ref());
            // `ItemSelection` checked entry (`selected.config`).
            let current = match &plan.config {
                mind_core::world::config::ConfigValue::Item(id) => {
                    content.item(*id).map(|item| item.name.clone())
                }
                mind_core::world::config::ConfigValue::Liquid(id) => {
                    content.liquid(*id).map(|liquid| liquid.name.clone())
                }
                mind_core::world::config::ConfigValue::Unit(id) => {
                    content.unit(*id).map(|unit| unit.name.clone())
                }
                mind_core::world::config::ConfigValue::Block(id) => {
                    content.block(*id).map(|block| block.name.clone())
                }
                _ => None,
            };
            plan_config_spec_json(
                &format!("block.{}.name", def.name),
                &options,
                current.as_deref(),
            )
        })
        .flatten()
    }

    /// `PlanConfigFragment`'s `selected.config = content` then
    /// `planConfig.hide()`: resolves the chosen content name and re-queues the
    /// plan with it (the queue has no in-place config setter), recording
    /// `Block.lastConfig` like the placed-config path.
    pub fn apply_plan_config(&mut self, value: &str) -> bool {
        let Some(plan) = self.plan_config_plan.take() else {
            return false;
        };
        let Some(mut host) = self.host.clone() else {
            return false;
        };
        let config = {
            let guard = host.bind();
            let Some(content) = guard.content_registry() else {
                return false;
            };
            if value.is_empty() {
                mind_core::world::config::ConfigValue::None
            } else if let Some(id) = content.item_id(value) {
                mind_core::world::config::ConfigValue::Item(id)
            } else if let Some(id) = content.liquid_id(value) {
                mind_core::world::config::ConfigValue::Liquid(id)
            } else if let Some(id) = content.unit_id(value) {
                mind_core::world::config::ConfigValue::Unit(id)
            } else if let Some(id) = content.block_id(value) {
                mind_core::world::config::ConfigValue::Block(id)
            } else {
                return false;
            }
        };
        self.last_config.insert(plan.block.raw(), config.clone());
        let Some((x, y)) = tile_i16(plan.x, plan.y) else {
            return false;
        };
        host.bind_mut()
            .remove_plans_in(plan.x, plan.y, plan.x, plan.y, 1);
        host.bind_mut()
            .queue_player_place_config(x, y, plan.block, plan.rotation, config)
    }

    /// `InputHandler.canTapPlayer` (`InputHandler.java:2032-2043`): no block
    /// selected, the cursor is within `playerSelectRange` (11) of the player
    /// unit and the unit carries items.
    fn can_tap_player(&self, wx: f32, wy: f32) -> bool {
        if self.controller.state.block.is_some() {
            return false;
        }
        self.with_host(|host| {
            let Some((px, py)) = host.player_unit_position() else {
                return false;
            };
            let dx = px - wx;
            let dy = py - wy;
            if dx * dx + dy * dy > PLAYER_SELECT_RANGE * PLAYER_SELECT_RANGE {
                return false;
            }
            host.player_unit_stack()
                .is_some_and(|(_, amount)| amount > 0)
        })
        .unwrap_or(false)
    }

    /// `InputHandler.tryDropItems` (`InputHandler.java:2286-2308`): a pending
    /// tap-unit gesture on release deposits into an accepting building or drops
    /// the stack (`Call.dropItem`).
    fn try_drop_items(&mut self) {
        if !self.dropping_item {
            return;
        }
        self.dropping_item = false;
        let Some(paused) = self.with_host(|host| host.is_paused()) else {
            return;
        };
        if paused {
            return;
        }
        let Some((item_name, amount)) = self.with_host(|host| host.player_unit_stack()).flatten()
        else {
            return;
        };
        if amount <= 0 {
            return;
        }
        let Some((wx, wy)) = self.aim_world() else {
            return;
        };
        // Released back on the unit: `canTapPlayer` -> no action.
        if self.can_tap_player(wx, wy) {
            return;
        }
        let (tx, ty) = (WorldGrid::to_tile(wx), WorldGrid::to_tile(wy));
        let now_ms = Time::singleton().get_ticks_msec();
        // Probe in its own bind window: `transfer_item` needs `bind_mut`, so the
        // read guard must drop before the arms run.
        let accepts = self
            .with_host(|host| host.player_can_deposit_at(tx, ty))
            .flatten();
        match accepts {
            Some(true) => {
                if now_ms.saturating_sub(self.last_deposit_ms) >= ITEM_DEPOSIT_COOLDOWN_MS {
                    self.transfer_item(tx, ty, &item_name, amount, true);
                    self.last_deposit_ms = now_ms;
                }
            }
            // Accepting building on cooldown: Java neither deposits nor drops.
            Some(_) => {}
            None => {
                let Some((x, y)) = tile_i16(tx, ty) else {
                    return;
                };
                self.emit_action(RemoteAction::Inventory {
                    kind: InventoryKind::DropItem,
                    x,
                    y,
                    item: None,
                    amount: 0,
                    angle: 0.0,
                });
            }
        }
    }

    /// `InputHandler.tryStopMine`/`tryBeginMine`: toggles the player unit's
    /// mine order for a no-selection left press. Returns `true` when consumed.
    fn try_mine_at(&self, x: i32, y: i32) -> bool {
        let Some((tx, ty)) = tile_i16(x, y) else {
            return false;
        };
        self.with_host_mut(|host| {
            if !host.player_can_mine(tx, ty) {
                return false;
            }
            if host.player_mine_target() == Some((tx, ty)) {
                host.set_player_mine(None);
            } else {
                host.set_player_mine(Some((tx, ty)));
            }
            true
        })
        .unwrap_or(false)
    }

    /// `build.interactable(player.team())` (`Tile.interactable`,
    /// `Tile.java:576-578`): own team or derelict.
    fn tile_interactable(&self, x: i32, y: i32) -> bool {
        let team = self.player_team();
        self.with_host(|host| {
            host.building_team(x, y)
                .is_some_and(|tile_team| tile_team == team || tile_team == 0)
        })
        .unwrap_or(false)
    }

    /// `build.block.configurable` for the tap dispatch (H02 T6): whether the
    /// panel may switch to the tapped building.
    fn block_configurable_at(&self, x: i32, y: i32) -> bool {
        self.with_host(|host| {
            let Some(content) = host.content_registry() else {
                return false;
            };
            let Some((tx, ty)) = tile_i16(x, y) else {
                return false;
            };
            host.grid()
                .block_at(TilePos::new(tx, ty))
                .and_then(|block| content.block(block))
                .is_some_and(|def| def.configurable)
        })
        .unwrap_or(false)
    }

    /// `build.block.synthetic()` (`Block.synthetic()` = `update || destructible`).
    fn block_synthetic_at(&self, x: i32, y: i32) -> bool {
        self.with_host(|host| {
            let Some(content) = host.content_registry() else {
                return false;
            };
            let Some((tx, ty)) = tile_i16(x, y) else {
                return false;
            };
            host.grid()
                .block_at(TilePos::new(tx, ty))
                .and_then(|block| content.block(block))
                .is_some_and(mind_core::maps::filters::block_info::synthetic)
        })
        .unwrap_or(false)
    }

    /// `tileTapped` (`InputHandler.java:1973-2030`): a left tap on a placed
    /// building opens its config UI, or its inventory when it stores items.
    /// Returns `true` when the tap was consumed (`InputHandler.tileTapped`
    /// returns `consumed`; `DesktopInput.java:701` gates shooting on it).
    fn tile_tapped(&mut self, x: i32, y: i32) -> bool {
        // `if(build == null){ inv.hide(); config.hideConfig(); return false; }`
        if !self
            .host
            .as_ref()
            .is_some_and(|host| host.bind().building_team(x, y).is_some())
        {
            self.effects.push(Effect::HidePanels);
            self.config_selected = None;
            return false;
        }
        // `config.isShown() && config.getSelected().onConfigureBuildTapped(build)`
        // (`InputHandler.java:1993-2005`; H02 T6, PAR-0220): the building whose
        // panel is open reacts to the second tap (bridge/node/driver link). Java
        // runs this for configurable and non-configurable buildings alike, before
        // any `tapped()` handling.
        if let Some((sx, sy)) = self.config_selected {
            let (Some((sx16, sy16)), Some((other_x, other_y))) = (tile_i16(sx, sy), tile_i16(x, y))
            else {
                self.config_selected = None;
                return false;
            };
            // Read-only preview (the guard drops at the end of the call).
            let Some(preview) = self
                .with_host(|host| host.preview_config_tap(i32::from(sx16), i32::from(sy16), x, y))
            else {
                return false;
            };
            if preview < 0 {
                // The selected building is gone: forget the stale selection.
                self.config_selected = None;
            } else {
                let _ = self.with_host_mut(|host| {
                    host.submit_player_command(SimCommand::ConfigTap {
                        x: sx16,
                        y: sy16,
                        other_x,
                        other_y,
                    })
                });
                match preview {
                    0 => {
                        // Java `return false`: consumed, the panel stays on the
                        // selected building (`inv.hide()` at the end).
                        self.effects.push(Effect::HideInventory);
                        return true;
                    }
                    1 => {
                        // Java `deselect(); return false`.
                        self.config_selected = None;
                        self.effects.push(Effect::HidePanels);
                        return true;
                    }
                    _ => {
                        // Java `return true`: the panel agrees to switch. A
                        // configurable, interactable target opens its own panel
                        // below; every other building consumes the tap and
                        // closes the panel (`config.hideConfig()`).
                        self.config_selected = None;
                        if !(self.block_configurable_at(x, y) && self.tile_interactable(x, y)) {
                            self.effects.push(Effect::HidePanels);
                            return true;
                        }
                    }
                }
            }
        }
        // `build.interactable(player.team())`: enemy buildings never open a
        // panel or inventory (IH:1980-1990).
        if !self.tile_interactable(x, y) {
            self.effects.push(Effect::HideInventory);
            return false;
        }
        // `Accelerator.configTapped` (`Accelerator.java:258-261`):
        // `configure()`/`deselect()` opens the planet launch page, never the
        // generic item panel (PAR-0296).
        if self.block_kind_at(x, y) == Some(BlockKind::Accelerator) {
            let _ = self.with_host_mut(|host| host.accelerator_tapped(x, y));
            self.effects.push(Effect::HideInventory);
            return true;
        }
        // `build.block.configurable && build.interactable(team)`.
        // `SwitchBlock.SwitchBuild.configTapped`: a tap toggles `enabled` and
        // returns false, so no panel opens (PAR-0108).
        if self.block_kind_at(x, y) == Some(BlockKind::SwitchBlock) {
            let Some((tx, ty)) = tile_i16(x, y) else {
                return false;
            };
            let Some(host) = self.host.clone() else {
                return false;
            };
            let enabled = match host.bind().building_enabled(x, y).try_to::<bool>() {
                Ok(enabled) => enabled,
                Err(_) => return false,
            };
            // `SwitchBlock.SwitchBuild.clickSound.at(this)`
            // (`SwitchBlock.java:58-62`; T9): the UI click goes through the
            // `MindUi` signal like every other UI sound (plan 18).
            if let Some(mut ui) = host.try_get_node_as::<godot::classes::Node>("/root/MindUi") {
                let name = GString::from("click");
                ui.call("play_ui_sound", &[name.to_variant()]);
            }
            self.emit_action(RemoteAction::Configure {
                x: tx,
                y: ty,
                value: mind_core::world::config::ConfigValue::Bool(!enabled),
            });
            // `if(!showedInventory) inv.hide();`
            self.effects.push(Effect::HideInventory);
            return true;
        }
        // `Door.tapped` (`Door.java:148-154`, T13): a tap toggles unless a unit
        // stands in an open door; `Door.configurable` is false upstream, so no
        // panel opens. The `origin().timer(timerToggle, 60f)` gate is consumed
        // by `door_tap_allowed` after the short-circuit unit check.
        if matches!(
            self.block_kind_at(x, y),
            Some(BlockKind::Door | BlockKind::AutoDoor)
        ) {
            let Some((tx, ty)) = tile_i16(x, y) else {
                return false;
            };
            let Some(open) = self.with_host(|host| host.building_config_json(x, y) == "true")
            else {
                return false;
            };
            if open
                && self
                    .with_host(|host| host.building_has_unit_overlap(x, y))
                    .unwrap_or(false)
            {
                // `Door.tapped` returns without toggling: the tap is not consumed.
                self.effects.push(Effect::HideInventory);
                return false;
            }
            if !self
                .with_host_mut(|host| host.door_tap_allowed(x, y))
                .unwrap_or(false)
            {
                // Throttled tap: no toggle, but `consumesTap` still consumes it
                // (`InputHandler.java:2015-2017`).
                self.effects.push(Effect::HideInventory);
                return true;
            }
            self.emit_action(RemoteAction::Configure {
                x: tx,
                y: ty,
                value: mind_core::world::config::ConfigValue::Bool(!open),
            });
            self.effects.push(Effect::HideInventory);
            return true;
        }
        if let Some(spec) = self.config_spec(x, y) {
            self.config_selected = Some((x, y));
            self.effects.push(Effect::OpenBlockConfig {
                x,
                y,
                screen: self.mouse,
                spec,
            });
            self.effects.push(Effect::HideInventory);
            return true;
        }
        // `build.interactable(team) && build.block.synthetic() &&
        // build.block.hasItems && build.items.total() > 0` (IH:1998-2004).
        if self.block_synthetic_at(x, y) {
            let Some(items) = self.with_host(|host| host.block_items_json(x, y)) else {
                return false;
            };
            if let Some(items) = items {
                self.effects.push(Effect::OpenBlockInventory {
                    x,
                    y,
                    screen: self.mouse,
                    items,
                });
                return true;
            }
        }
        // `if(!showedInventory) inv.hide();`
        self.effects.push(Effect::HideInventory);
        false
    }

    pub(super) fn right_press(&mut self, bindings: &BindingState) {
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        // `DesktopInput.java:608-610` runs before the select/deselect taps: the
        // right tap clears `mineTile` when no placement mode, unit plans or
        // command mode owns the tap. Do not return: the break drag still
        // starts (`DI:740-748`).
        let plans = self.with_host(|host| host.player_plan_count()).unwrap_or(0);
        if mind_core::input::desktop::deselect_stops_mining(
            self.controller.state.block.is_some(),
            self.controller.state.command_mode,
            plans,
        ) {
            let _ = self.with_host_mut(|host| host.set_player_mine(None));
        }
        if self.controller.state.command_mode {
            self.command_tap(bindings);
            return;
        }
        // `DesktopInput.java:737` (`Binding.deselect` shares the right button
        // with breaking): while placing, the press clears the block selection
        // instead of entering breaking mode - otherwise every left press stays
        // trapped in the placement branch and ore clicks can never mine
        // (PAR-0045).
        if self.controller.state.block.is_some() {
            self.controller.state.select_block(None);
            self.controller.state.override_line_rotation = false;
            let _ = self.with_host_mut(|host| {
                host.select_block(GString::from("air"));
            });
            return;
        }
        // `DesktopInput.java:741-744`: the deselect tap clears copied plans
        // (before entering breaking mode).
        if !self.controller.state.select_plans.is_empty() {
            self.controller.state.select_plans.clear();
            self.refresh_select_preview();
            return;
        }
        // `Binding.breakBlock` (`DesktopInput.pollInputPlayer`): the right
        // button always enters breaking mode; config/inventory open on the
        // left tap.
        self.controller.state.begin_break();
        // `DesktopInput.java:746-748`: the break drag latches the raw cursor for
        // a potential F-cut.
        self.schem_start = self.raw_cursor_tile();
        self.drag = Some(Drag {
            kind: DragKind::Break,
            start: (x, y),
            last: (x, y),
            moved: false,
        });
    }

    pub(super) fn right_release(&mut self, bindings: &BindingState) {
        // `DesktopInput.java:799`: the break release also resolves a pending
        // tap-unit gesture (`tryDropItems`).
        self.try_drop_items();
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        // Cut: `keyRelease(breakBlock) && keyDown(schematicSelect)` creates the
        // schematic BEFORE `removeSelection` and pastes it after
        // (`DesktopInput.java:765-790`).
        let cut = if self.key_down(bindings, ids::SCHEMATIC_SELECT) {
            match (self.schem_start, self.raw_cursor_tile()) {
                (Some(start), Some(end)) => {
                    Some((end, host.bind().copy_schematic_plans(start, end, end)))
                }
                _ => None,
            }
        } else {
            None
        };
        let mut commands: Vec<SimCommand> = Vec::new();
        // `InputHandler.removeSelection` clamps the rect to `maxLength` (100;
        // 64 with F held for the cut, `DesktopInput.java:789`).
        let max_length = if self.key_down(bindings, ids::SCHEMATIC_SELECT) {
            64
        } else {
            100
        };
        let r = mind_core::input::placement::normalize_area(
            drag.start.0,
            drag.start.1,
            drag.last.0,
            drag.last.1,
            0,
            false,
            max_length,
        );
        {
            let guard = host.bind();
            let world = HostWorld {
                grid: guard.grid(),
                content: guard.content_registry(),
                host: &guard,
            };
            'outer: for y in r.y..=r.y2 {
                for x in r.x..=r.x2 {
                    if commands.len() >= DRAG_TILE_CAP {
                        break 'outer;
                    }
                    if world.block_at(x, y) == BlockId::AIR {
                        continue;
                    }
                    let Some((x, y)) = tile_i16(x, y) else {
                        continue;
                    };
                    // `player: Some` routes to the player unit's build queue
                    // (`InputHandler.removeSelection`; PAR-0098).
                    commands.push(SimCommand::Break {
                        x,
                        y,
                        player: Some(0),
                    });
                }
            }
        }
        self.enqueue_commands(&commands);
        self.controller.state.place_mode = PlaceMode::None;
        // `removeSelection` plan half (`InputHandler.java:1905-1935`): drop the
        // player's queued plans and the team ghosts inside the rect.
        let _ = self.with_host_mut(|host| {
            host.remove_plans_in(
                drag.start.0,
                drag.start.1,
                drag.last.0,
                drag.last.1,
                max_length,
            )
        });
        if let Some((end, plans)) = cut {
            self.adopt_select_plans(plans, end);
        }
    }
}
