// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Command mode, unit selection commands, stances and payloads.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    /// `commandMode` toggle (`DesktopInput.update`).
    pub fn toggle_command_mode(&mut self) {
        self.controller.state.command_mode = !self.controller.state.command_mode;
        if !self.controller.state.command_mode {
            self.controller.state.selected_units.clear();
            self.controller.state.command_buildings.clear();
            self.controller.state.command_rect = None;
            self.emit_selection();
        }
        self.fire_hotkey("command_mode");
    }

    /// `pollInputPlayer` L632: selecting a block (`isPlacing()`) clears the
    /// copied plans.
    pub fn clear_select_plans(&mut self) {
        if !self.controller.state.select_plans.is_empty() {
            self.controller.state.select_plans.clear();
            self.refresh_select_preview();
        }
    }

    /// `createControlGroup`/`recallControlGroup` for a digit binding.
    pub(super) fn control_group(&mut self, bindings: &BindingState, index: usize) {
        let creating = self.key_down(bindings, ids::CREATE_CONTROL_GROUP);
        let now_ms = Time::singleton().get_ticks_msec();
        if creating {
            self.controller.create_control_group(index, true);
        } else if self.controller.recall_control_group(index, now_ms) {
            self.center_on_selection();
        }
        self.emit_selection();
    }

    fn center_on_selection(&mut self) {
        if self.controller.state.selected_units.is_empty() {
            return;
        }
        let mut sum = (0.0f32, 0.0f32);
        let mut count = 0.0f32;
        for id in &self.controller.state.selected_units {
            if let Some(unit) = self.selectable.iter().find(|unit| unit.id == *id) {
                sum.0 += unit.x;
                sum.1 += unit.y;
                count += 1.0;
            }
        }
        if count <= 0.0 {
            return;
        }
        if let Some(mut camera) = self.camera.clone() {
            camera
                .bind_mut()
                .pan_to((sum.0 / count) as f64, (sum.1 / count) as f64);
        }
    }

    /// Live player team (`player.team().id`) for RTS selection filters.
    pub fn player_team(&self) -> u8 {
        self.host
            .as_ref()
            .map(|host| host.bind().default_team().clamp(0, 255) as u8)
            .unwrap_or(0)
    }

    pub(super) fn select_all_units(&mut self, bindings: &BindingState, kind: SelectAllKind) {
        // `DI:315-357`: select-all runs only while command mode is active.
        if !self.controller.state.command_mode {
            return;
        }
        let screens = self.key_down(bindings, ids::SELECT_ACROSS_SCREEN);
        let rect = screens.then(|| self.viewport_world_rect());
        self.controller.state.command_buildings.clear();
        match kind {
            SelectAllKind::Factories => {
                // `build.isCommandable()` buildings (`CoreBlock`, `UnitFactory`,
                // `Reconstructor`, `UnitAssembler`, `PayloadSource`).
                self.controller.state.selected_units.clear();
                let tile = mind_core::config::TILESIZE as f32;
                if let Some(host) = self.host.clone() {
                    self.controller.state.command_buildings = host
                        .bind()
                        .commandable_buildings()
                        .into_iter()
                        .filter(|(x, y)| {
                            rect.is_none_or(|rect| {
                                rect.contains(*x as f32 * tile, *y as f32 * tile)
                            })
                        })
                        .map(|(x, y)| TilePos::new(x, y))
                        .collect();
                }
            }
            SelectAllKind::Units | SelectAllKind::Transport => {
                // `type.controlSelectGlobal` (`mono` excluded) or
                // `unit instanceof Payloadc`; no infinite rect (NaN bounds).
                let transport = kind == SelectAllKind::Transport;
                let team = self.player_team();
                self.controller.state.selected_units = self
                    .selectable
                    .iter()
                    .filter(|unit| {
                        unit.team == team
                            && unit.commandable
                            && if transport {
                                unit.payload
                            } else {
                                unit.select_global
                            }
                            && rect.is_none_or(|rect| rect.contains(unit.x, unit.y))
                    })
                    .map(|unit| unit.id)
                    .collect();
            }
        }
        self.emit_selection();
    }

    /// Viewport rectangle in world pixels (selection across screen).
    fn viewport_world_rect(&self) -> SelectRect {
        let Some(camera) = self.camera.clone() else {
            return SelectRect::default();
        };
        let size = camera.bind().viewport_size();
        let (x0, y0) = camera_world_corner(&camera, 0.0, 0.0);
        let (x1, y1) = camera_world_corner(&camera, size.x, size.y);
        SelectRect {
            x: x0.min(x1),
            y: y0.min(y1),
            w: (x1 - x0).abs(),
            h: (y1 - y0).abs(),
        }
    }

    /// `commandUnits` with an explicit command id over the current selection.
    pub(crate) fn command_units(&mut self, command: u16) {
        if self.controller.state.selected_units.is_empty() {
            return;
        }
        let units: SmallVec<[i32; 32]> = self
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        self.emit_action(RemoteAction::SetUnitCommand { units, command });
    }

    /// `PlacementFragment` unit-type filter (`PlacementFragment.java:537-548`):
    /// left click keeps only units of `keep`, right click drops `drop`;
    /// `type_of` maps a selected id to its unit type id. Returns whether the
    /// selection changed (emits `selection_changed` like `select_rect`).
    pub(crate) fn retain_selection_types(
        &mut self,
        type_of: &dyn Fn(i32) -> Option<i32>,
        keep: Option<i32>,
        drop: Option<i32>,
    ) -> bool {
        let before = self.controller.state.selected_units.len();
        self.controller.state.selected_units.retain(|id| {
            let Some(type_id) = type_of(*id) else {
                return false;
            };
            keep.is_none_or(|keep| type_id == keep) && drop != Some(type_id)
        });
        let changed = self.controller.state.selected_units.len() != before;
        if changed {
            self.controller.state.command_buildings.clear();
            self.emit_selection();
        }
        changed
    }

    /// `setUnitStance` over the current selection.
    pub fn set_stance(&mut self, stance: u16, enabled: bool) {
        if self.controller.state.selected_units.is_empty() {
            return;
        }
        let units: SmallVec<[i32; 32]> = self
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        self.emit_action(RemoteAction::SetUnitStance {
            units,
            stance,
            enabled,
        });
    }

    /// `Call.setUnitStance(player, selected ids, stance, enabled)` by
    /// `UnitStance` name. Java toggles `!activeStances.get(id)`; without a
    /// per-unit active-stance feed the port passes `enabled` through (callers
    /// send `true`).
    pub fn set_stance_named(&mut self, name: &str, enabled: bool) -> bool {
        if self.controller.state.selected_units.is_empty() {
            return false;
        }
        let Some(stance) = self.host.as_ref().and_then(|host| {
            host.bind()
                .content_registry()
                .and_then(|content| content.unit_stance_by_name(name))
                .map(|def| def.id.get())
        }) else {
            return false;
        };
        self.set_stance(stance, enabled);
        true
    }

    /// `Call.setUnitCommand(player, selected ids, command)` by `UnitCommand`
    /// name (`false` when nothing is selected or the name is unknown).
    pub fn command_units_named(&mut self, name: &str) -> bool {
        if self.controller.state.selected_units.is_empty() {
            return false;
        }
        let Some(command) = self.host.as_ref().and_then(|host| {
            host.bind()
                .content_registry()
                .and_then(|content| content.unit_command_by_name(name))
                .map(|def| def.id.get())
        }) else {
            return false;
        };
        self.command_units(command);
        true
    }

    /// `requestUnitPayload`/`requestDropPayload` at the cursor.
    pub(super) fn payload(&mut self, kind: PayloadAction) {
        let Some((x, y)) = self.cursor_world() else {
            return;
        };
        self.emit_action(RemoteAction::Payload {
            kind,
            x,
            y,
            target: None,
        });
    }

    /// Payload pickup at the cursor (`pickupCargo`).
    pub fn pickup_payload(&mut self) {
        self.payload(PayloadAction::PickupUnit);
    }

    /// Payload drop at the cursor (`dropCargo`).
    pub fn drop_payload(&mut self) {
        self.payload(PayloadAction::Drop);
    }

    /// `DesktopInput.updateMovement` payload hold-repeat
    /// (`DesktopInput.java:1012-1040`): after a tap, holding the cargo key
    /// repeats the action every >20 ms once the tap is >200 ms old.
    pub fn update_payload_hold(&mut self, bindings: &BindingState) {
        let held = if self.key_down(bindings, ids::PICKUP_CARGO) {
            Some(PayloadAction::PickupUnit)
        } else if self.key_down(bindings, ids::DROP_CARGO) {
            Some(PayloadAction::Drop)
        } else {
            None
        };
        let Some(action) = held else {
            return;
        };
        let now = Time::singleton().get_ticks_msec();
        if now.saturating_sub(self.payload_tap_ms) > 200
            && now.saturating_sub(self.payload_hold_ms) > 20
        {
            self.payload(action);
            self.payload_hold_ms = now;
        }
    }

    /// Per-frame `pollInputPlayer` L762-768 mirror: outside an active placement
    /// line (`mode == placing && block != null`) the wheel snap latch clears.
    pub fn sync_line_override(&mut self) {
        if !(self.controller.state.is_placing() && self.controller.state.block.is_some()) {
            self.controller.state.override_line_rotation = false;
        }
    }

    /// `InputHandler.update` L955-957 + `drawUnitSelection` L1354-1368: while a
    /// command-mode select-rect drag is held, `command_rect` tracks Java's live
    /// rect — the press point stored by [`Self::left_press`] plus
    /// `(mouseWorldX - x, mouseWorldY - y)` recomputed from the current camera
    /// every frame (a pan/zoom while the button is held moves the second
    /// corner even without a new mouse event). Outside command mode Java drops
    /// the rect (`commandRect = false`); the release commits the tile rect in
    /// [`Self::commit_select`].
    pub fn sync_command_rect(&mut self) {
        if !self
            .drag
            .is_some_and(|drag| drag.kind == DragKind::SelectRect)
        {
            return;
        }
        if !self.controller.state.command_mode {
            self.controller.state.command_rect = None;
            return;
        }
        let Some((sx, sy, ..)) = self.controller.state.command_rect else {
            return;
        };
        if let Some((wx, wy)) = self.aim_world() {
            self.controller.state.command_rect = Some((sx, sy, wx - sx, wy - sy));
        }
    }

    /// `HintsFragment` triggers that fire while a key is held (G05 T4b):
    /// `boost` (`keyDown(boost)`) and `rebuildSelecting`
    /// (`input.isRebuildSelecting()` = `keyDown(rebuildSelect)`).
    pub fn refresh_hints(&mut self, bindings: &BindingState) {
        if self.key_down(bindings, ids::BOOST) {
            self.push_hint("boost_down");
        }
        if self.key_down(bindings, ids::REBUILD_SELECT) {
            self.push_hint("rebuild_selecting");
        }
    }

    /// Returns and clears the collected hint triggers (order of first
    /// occurrence).
    pub fn take_hint_keys(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.hint_keys)
    }

    /// Deduplicated hint trigger push.
    pub(super) fn push_hint(&mut self, key: &'static str) {
        if !self.hint_keys.contains(&key) {
            self.hint_keys.push(key);
        }
    }
}
