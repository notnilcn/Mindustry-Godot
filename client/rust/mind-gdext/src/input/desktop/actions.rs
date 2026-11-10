// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `dispatch_action`: the binding-name -> action match.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    /// Runs one named binding (`Binding.name`).
    pub(super) fn dispatch_action(&mut self, bindings: &BindingState, name: &'static str) {
        match name {
            "respawn" => {
                // `DesktopInput.java:423-431`: V clears the possession and
                // returns to the core unit (`Call.unitClear`); the camera
                // recentering follows the new player unit.
                self.dropping_item = false;
                self.emit_action(RemoteAction::UnitClear);
                self.fire_hotkey(name);
            }
            "rotateplaced" => {
                // T9: `rotateplaced` is a wheel modifier now (`cursor.build !=
                // null && ... && keyDown(rotatePlaced) && axisTap(rotate) != 0`,
                // DesktopInput.java:924-926); the key edge alone does nothing.
            }
            "rotate" => {
                // Axis edges (`rotate` defaults to the wheel).
                let step = if self.axis_negative(bindings, ids::ROTATE) {
                    -1
                } else {
                    1
                };
                self.rotate_placement_step(bindings, step);
            }
            "zoom" => {
                let amount = if self.axis_negative(bindings, ids::ZOOM) {
                    -1.0
                } else {
                    1.0
                };
                self.zoom(amount);
                self.fire_hotkey("zoom");
            }
            "clear_building" => {
                // `Binding.clearBuilding` (`DesktopInput.java:612`): the player's
                // queue is cleared too (`player.unit().clearBuilding()`), and
                // `pollInputPlayer` L632 also drops the copied plans.
                self.controller.state.select_block(None);
                self.controller.state.override_line_rotation = false;
                self.controller.state.select_plans.clear();
                self.refresh_select_preview();
                if let Some(mut host) = self.host.clone() {
                    host.bind_mut().clear_player_build_plans();
                }
                self.fire_hotkey(name);
            }
            "schematic_select" | "rebuild_select" => {
                // `DesktopInput.java:616-618`: F/B latch the selection start
                // (raw cursor); a break drag owns the tiles instead.
                if !self
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.kind == DragKind::Break)
                {
                    self.schem_start = self.raw_cursor_tile();
                }
                self.fire_hotkey(name);
            }
            "schematic_flip_x" => {
                self.flip_select_plans(true);
                self.fire_hotkey(name);
            }
            "schematic_flip_y" => {
                self.flip_select_plans(false);
                self.fire_hotkey(name);
            }
            "pause_building" => {
                // `DesktopInput.java:671-678`: toggles `isBuilding`, clears the
                // auto-pause latch and stops shooting when resuming; applied to
                // the unit in `InputHandler.update` (`updateBuilding`).
                self.controller.state.is_building = !self.controller.state.is_building;
                self.build_was_auto_paused = false;
                if self.controller.state.is_building {
                    self.shooting = false;
                }
                if let Some(mut host) = self.host.clone() {
                    host.bind_mut()
                        .set_player_building_paused(!self.controller.state.is_building);
                }
                self.fire_hotkey(name);
            }
            "block_select_01" | "block_select_02" | "block_select_03" | "block_select_04"
            | "block_select_05" | "block_select_06" | "block_select_07" | "block_select_08"
            | "block_select_09" | "block_select_10" => {
                let index = name
                    .rsplit('_')
                    .next()
                    .and_then(|suffix| suffix.parse::<usize>().ok())
                    .map(|n| n.saturating_sub(1))
                    .unwrap_or(0);
                if self.controller.state.command_mode {
                    self.control_group(bindings, index);
                } else {
                    // `PlacementFragment.gridUpdate` owns the digit combos: the
                    // fragment holds the live, unlock-filtered catalogue, so the
                    // binding edge only reaches it (Java `:745` gate).
                    self.fire_hotkey(name);
                }
            }
            "block_select_left" | "block_select_right" | "block_select_up"
            | "block_select_down" => {
                // `gridUpdate` arrows (`PlacementFragment.java:189-207`); the
                // fragment resolves them against the live catalogue.
                if !self.controller.state.command_mode {
                    self.fire_hotkey(name);
                }
            }
            "command_mode" => {
                // `input.keyTap` is the press edge (`justPressed`); hold mode is
                // applied per frame by `update_command_mode`.
                if !self.command_mode_hold && self.command_mode_allowed(bindings) {
                    self.toggle_command_mode();
                }
            }
            "select_all_units" => self.select_all_units(bindings, SelectAllKind::Units),
            "select_all_unit_transport" => {
                self.select_all_units(bindings, SelectAllKind::Transport)
            }
            "select_all_unit_factories" => {
                self.select_all_units(bindings, SelectAllKind::Factories)
            }
            "cancel_orders" => {
                // PlacementFragment command table: `cancelOrders` orders stance
                // `stop` (it does NOT deselect).
                if self.controller.state.command_mode {
                    self.set_stance_named("stop", true);
                }
            }
            "deselect" => {
                self.controller.state.selected_units.clear();
                self.controller.state.command_buildings.clear();
                self.emit_selection();
            }
            "pickupCargo" => {
                // `DesktopInput.java:1012-1040`: a tap sets the hold timer.
                self.payload_tap_ms = Time::singleton().get_ticks_msec();
                self.payload(PayloadAction::PickupUnit);
            }
            "dropCargo" => {
                self.payload_tap_ms = Time::singleton().get_ticks_msec();
                self.payload(PayloadAction::Drop);
            }
            "boost" => {
                // `HintsFragment.boost` = `keyDown(boost)`; the edge and the
                // per-frame refresh both push the trigger.
                self.push_hint("boost_down");
            }
            "unit_stance_hold_fire"
            | "unit_stance_pursue_target"
            | "unit_stance_patrol"
            | "unit_stance_ram"
            | "unit_stance_boost"
            | "unit_stance_hold_position" => {
                // PlacementFragment command table: names resolved through the
                // stance registry; command mode only.
                if self.controller.state.command_mode {
                    let stance = match name {
                        "unit_stance_hold_fire" => "holdfire",
                        "unit_stance_pursue_target" => "pursuetarget",
                        "unit_stance_patrol" => "patrol",
                        "unit_stance_ram" => "ram",
                        "unit_stance_boost" => "boost",
                        _ => "holdposition",
                    };
                    self.set_stance_named(stance, true);
                }
            }
            "unit_command_move"
            | "unit_command_repair"
            | "unit_command_rebuild"
            | "unit_command_assist"
            | "unit_command_mine"
            | "unit_command_enter_payload"
            | "unit_command_load_units"
            | "unit_command_load_blocks"
            | "unit_command_unload_payload"
            | "unit_command_loop_payload" => {
                if self.controller.state.command_mode {
                    let command = match name {
                        "unit_command_move" => "move",
                        "unit_command_repair" => "repair",
                        "unit_command_rebuild" => "rebuild",
                        "unit_command_assist" => "assist",
                        "unit_command_mine" => "mine",
                        "unit_command_enter_payload" => "enterPayload",
                        "unit_command_load_units" => "loadUnits",
                        "unit_command_load_blocks" => "loadBlocks",
                        "unit_command_unload_payload" => "unloadPayload",
                        _ => "loopPayload",
                    };
                    self.command_units_named(command);
                }
            }
            "minimap" => {
                self.effects
                    .push(Effect::ToggleFragment(String::from("minimap")));
                self.fire_hotkey(name);
            }
            "ping" => {
                // `DesktopInput.java:491-497`: `input.keyTap(Binding.ping)` at
                // `input.mouseWorldX/Y`; Ctrl asks for the label first
                // (`ui.showTextInput("", "@ping.text", maxPingTextLength, "",
                // result -> Call.pingLocation(..., UI.formatIcons(result)))`),
                // which `MindInput` drives through `Effect::PingPrompt`.
                if let Some((wx, wy)) = self.aim_world() {
                    if self.key_down(bindings, ids::CONTROL) {
                        self.effects.push(Effect::PingPrompt { x: wx, y: wy });
                    } else {
                        self.ping_at(wx, wy, None);
                    }
                }
                self.fire_hotkey(name);
            }
            "console" => {
                // `ConsoleFragment.java:43`: the console key only acts when the
                // `console` client setting is on (default false,
                // SettingsMenuDialog.java:566).
                if console_setting_enabled() {
                    self.effects
                        .push(Effect::ToggleFragment(String::from("console")));
                }
                self.fire_hotkey(name);
            }
            "chat" => {
                // `ChatFragment.java:65`: `net.active() && keyTap(chat) &&
                // !hasOtherFocus && !lastFrameHadFocus && !ui.consolefrag.shown()`
                // -> `toggle()`. `handle` already drops text-focus/dialog edges
                // (`hasOtherFocus`/`hasField`), and the chat fragment owns the
                // net-active and console gates, so the bridge only routes it.
                self.effects
                    .push(Effect::ToggleFragment(String::from("chat")));
                self.fire_hotkey(name);
            }
            "player_list" => {
                // `DesktopInput.java:233-235`: `net.active() &&
                // keyTap(Binding.playerList)` -> `ui.listfrag.toggle()`
                // (`Binding.playerList` defaults to Tab). `handle` already
                // drops text-focus/dialog edges (`scene.getKeyboardFocus() ==
                // null`), and the fragment owns the relay/in-game gate
                // (`relay_in_game`, from `net.active() && state.isGame()`), so
                // the bridge only routes the toggle.
                self.effects
                    .push(Effect::ToggleFragment(String::from("player_list")));
                self.fire_hotkey(name);
            }
            "research" => {
                // `DesktopInput.java:446-461`: J/N toggle campaign screens only
                // while a campaign sector is live.
                if self.campaign_active {
                    self.open_dialog("research", "");
                }
                self.fire_hotkey(name);
            }
            "planet_map" => {
                if self.campaign_active {
                    self.open_dialog("planet", "");
                }
                self.fire_hotkey(name);
            }
            "schematic_menu" => {
                self.open_dialog("schematics", "");
                self.fire_hotkey(name);
            }
            "block_info" => self.block_info_hotkey(),
            "pick" => self.pick_block(),
            "toggle_block_status" => {
                // `DesktopInput.java:455-457`: F6 flips `settings.blockstatus`;
                // the HUD fragment owns the setting write and the marker pass
                // reads it (`Renderer.drawStatus`).
                self.fire_hotkey(name);
            }
            "toggle_power_lines" => {
                // `DesktopInput.java:459-466`: F5 swaps `lasersopacity` with
                // `preferredlaseropacity` (the node owns the setting write).
                self.effects
                    .push(Effect::UiCall("toggle_power_lines".to_owned()));
                self.fire_hotkey(name);
            }
            "fullscreen" => {
                // `Control.java:690-694`: F11 (and the menu) flips the
                // persisted `fullscreen` setting and the window mode.
                self.effects
                    .push(Effect::UiCall("toggle_fullscreen".to_owned()));
            }
            "toggle_menus" => {
                // `HudFragment.java:455-461`: the HUD fragment flips its tables.
                self.fire_hotkey(name);
            }
            "category_prev" | "category_next" => {
                // `gridUpdate` category cycles (`PlacementFragment.java:240-258`);
                // `:745` gates them off in command mode.
                if !self.controller.state.command_mode {
                    self.fire_hotkey(name);
                }
            }
            "pause" => {
                // `Control.java:730-734`: the pause key is ignored while
                // `renderer.isCutscene()` (landing/launch).
                let cutscene = self
                    .camera
                    .clone()
                    .map(|camera| camera.bind().is_cutscene())
                    .unwrap_or(false);
                // `state.is(State.paused) || state.is(State.playing)`
                // (`Control.java:735`): the menu phase (after
                // `MindCampaign.reset_to_menu`, L02 T2) ignores the key.
                if !cutscene && let Some(mut host) = self.host.clone() {
                    let state = host.bind().get_state().to_string();
                    if state == "paused" || state == "playing" {
                        let paused = host.bind().is_paused();
                        host.bind_mut().set_paused(!paused);
                    }
                }
                self.fire_hotkey(name);
            }
            "screenshot" => self.effects.push(Effect::Screenshot),
            "detach_camera" => {
                if let Some(mut camera) = self.camera.clone() {
                    let following = camera.bind().is_following();
                    camera.bind_mut().set_follow(!following);
                }
                self.fire_hotkey(name);
            }
            "skip_wave" => {
                // `HudFragment.java:461-467`: `keyTap(skipWave) &&
                // canSkipWave() && !scene.hasDialog() && !scene.hasField()` ->
                // `logic.skipWave()`. The key-edge path in `handle` already
                // drops text-focus edges and dialog edges (the Java
                // `hasField`/`hasDialog` guards), and the gate is the same one
                // the skip button uses (`_refresh_skip_button`). Offline the
                // Java `net.client() && player.admin` branch is unreachable,
                // so this is `logic.skipWave()`.
                if self.wave_gate.can_skip_wave() {
                    self.effects.push(Effect::SkipWave);
                }
                self.fire_hotkey(name);
            }
            _ => {}
        }
    }
}
