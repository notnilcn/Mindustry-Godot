// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Transfer, rotate, zoom, pick and the remaining hotkeys.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl DesktopBridge {
    /// `tryDropItems`/`requestItem`: withdraw or deposit at a tile.
    pub fn transfer_item(&mut self, x: i32, y: i32, item: &str, amount: i32, deposit: bool) {
        let item = self.item_ids.get(item).copied();
        self.emit_action(RemoteAction::Inventory {
            kind: if deposit {
                InventoryKind::Deposit
            } else {
                InventoryKind::Withdraw
            },
            x: x as i16,
            y: y as i16,
            item,
            amount,
            angle: 0.0,
        });
    }

    /// `rotateBlock` on the building under the cursor; rotates the pending
    /// placement when a block is selected (`Binding.rotateplaced`).
    pub fn rotate_under_cursor(&mut self, bindings: &BindingState) {
        if self.controller.state.block.is_some() {
            self.rotate_placement(bindings);
            return;
        }
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        self.emit_action(RemoteAction::Rotate {
            x: x as i16,
            y: y as i16,
            direction: true,
        });
        self.fire_hotkey("rotateplaced");
    }

    /// Rotates the pending placement by +1 (`input.rotation`).
    pub fn rotate_placement(&mut self, bindings: &BindingState) {
        self.rotate_placement_step(bindings, 1);
    }

    /// Rotates the pending placement by `step` (`input.rotation`).
    pub(super) fn rotate_placement_step(&mut self, bindings: &BindingState, step: i32) {
        // Same snap as the wheel path (`pollInputPlayer` L762-768 runs for any
        // `axisTap(rotate)`, wheel or bound key, before L836-849 steps it).
        let diagonal_held = self.key_down(bindings, ids::DIAGONAL_PLACEMENT);
        if let Some(drag) = self.drag
            && drag.kind == DragKind::Place
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
        // `DesktopInput.java:836-849`: `splan.rotation` steps with the placement
        // rotation while a queued plan is grabbed.
        if let Some(drag) = self.splan.as_mut() {
            drag.plan.rotation = (drag.plan.rotation as i32 + step).rem_euclid(4) as u8;
            drag.moved = true;
        }
        if let Some(drag) = self.drag
            && drag.kind == DragKind::Place
        {
            self.update_place_line(bindings, drag.start, drag.last);
        } else if !self.controller.state.select_plans.is_empty() {
            // `pollInputPlayer` L843-858: the axis rotates copied plans when no
            // placement line owns the wheel.
            self.rotate_select_plans(step);
        }
        self.fire_hotkey("rotate");
    }

    /// `Renderer.scaleCamera(amount)` from the `zoom` binding.
    pub(super) fn zoom(&mut self, amount: f32) {
        if let Some(mut camera) = self.camera.clone() {
            camera.bind_mut().zoom_by(amount as f64);
        }
    }

    /// Opens the config UI or content info for the block under the cursor.
    /// F1 (`Binding.blockInfo`, `PlacementFragment.java:260-271`): the content
    /// dialog for the hovered unit, else the selected block, else the block
    /// under the cursor (constructs resolve to their `current`). It never opens
    /// the config panel (that is the configure tap, not `blockInfo`).
    pub(super) fn block_info_hotkey(&mut self) {
        // `hovered() instanceof Unit unit && unit.type.unlockedNow()`.
        if let Some(name) = self.hovered_unit_name() {
            self.open_dialog("content", &format!("{{\"content\":\"{name}\"}}"));
        } else {
            // `displayBlock = input.block ?? hovering` (`:263-266`).
            let hovering = self
                .cursor_tile()
                .and_then(|(x, y)| self.host.clone()?.bind().pick_target_at(x, y))
                .map(|(block, _)| block);
            let display = self
                .selected_block_name()
                .and_then(|name| self.block_id_by_name(&name))
                .or(hovering);
            if let Some(block) = display
                && let Some(name) = self.placeable_block_name(block)
            {
                self.open_dialog("content", &format!("{{\"content\":\"{name}\"}}"));
            }
        }
        self.fire_hotkey("block_info");
    }

    /// Content id for a block name (`content.block_id`).
    fn block_id_by_name(&self, name: &str) -> Option<mind_core::content::BlockId> {
        let host = self.host.clone()?;
        let guard = host.bind();
        guard.content_registry()?.block_id(name)
    }

    pub(super) fn open_dialog(&mut self, name: &str, ctx: &str) {
        self.effects.push(Effect::OpenDialog {
            name: name.to_owned(),
            ctx: ctx.to_owned(),
        });
    }

    /// Middle-click pick (`PlacementFragment.updatePick` `:132-172`).
    ///
    /// Runs only outside command mode (`:745`), with a live player unit and no
    /// dialog (`:134`): the hovered building (a `ConstructBuild` resolves to
    /// its `current`) or a queued non-breaking plan under the cursor is
    /// selected when it is player-placeable, and a copied config is remembered
    /// (`tryBlock.lastConfig = tryConfig`, `:170`).
    pub(super) fn pick_block(&mut self) {
        if self.controller.state.command_mode || self.ui_dialog {
            return;
        }
        let Some(mut host) = self.host.clone() else {
            return;
        };
        // `player.isBuilder()`: a live player unit (the builder check itself is
        // the campaign placement gate's).
        if !host.bind().has_player_unit() {
            return;
        }
        let Some((x, y)) = self.cursor_tile() else {
            return;
        };
        let Some((wx, wy)) = self.aim_world() else {
            return;
        };
        // `tryBlock`/`tryConfig` from the hovered building (`:141-143`).
        let mut trial = host.bind().pick_target_at(x, y);
        // Queued non-breaking player plans override the hovered building
        // (`:145-154`).
        if let Some((block, config)) = host.bind().player_plan_at_world(wx, wy) {
            trial = Some((block, Some(config)));
        }
        let Some((block, config)) = trial else {
            return;
        };
        // `unlocked(tryBlock)` (`:169`): the bridge registry has no live unlock
        // store, so the static player-placeable half is enforced here and the
        // placement gate owns the tech-unlock half.
        let Some(name) = self.placeable_block_name(block) else {
            return;
        };
        if !host.bind_mut().select_block(GString::from(&name)) {
            return;
        }
        let id = host
            .bind()
            .content_registry()
            .and_then(|content| content.block_id(&name));
        if let Some(id) = id {
            self.controller.state.select_block(Some(id));
        }
        if let Some(value) = config {
            // `tryBlock.lastConfig = tryConfig` (`Block.java:827-832`).
            self.last_config.insert(block.raw(), value);
        }
        self.clear_select_plans();
        // `HintsFragment` `schematicSelect` includes `keyTap(pick)`.
        self.push_hint("pick_tap");
        self.fire_hotkey("pick");
    }

    /// The block's content name when it may reach the placement palette
    /// (`PlacementFragment.unlocked`'s static half: visible, player-placeable).
    fn placeable_block_name(&self, block: BlockId) -> Option<String> {
        use mind_core::content::registries::blocks::BuildVisibility;
        let host = self.host.clone()?;
        let guard = host.bind();
        let content = guard.content_registry()?;
        let def = content.block(block)?;
        if def.removed
            || !def.placeable_player
            || matches!(
                def.build_visibility,
                BuildVisibility::Hidden | BuildVisibility::DebugOnly | BuildVisibility::EditorOnly
            )
        {
            return None;
        }
        Some(def.name.clone())
    }

    /// The block name under the cursor (`""` for air/unset).
    fn selected_block_name(&self) -> Option<String> {
        let host = self.host.clone()?;
        let name = host.bind().selected_block().to_string();
        (!name.is_empty() && name != "air").then_some(name)
    }

    /// Nearest player-team unit whose body overlaps the cursor
    /// (`PlacementFragment.hovered` `:798-801`,
    /// `Units.closestOverlap(team, x, y, 5f, ...)`; world units).
    fn hovered_unit_name(&self) -> Option<String> {
        let (wx, wy) = self.aim_world()?;
        let team = self.player_team();
        let host = self.host.clone()?;
        let json = host.bind().selectable_units_json();
        let rows = serde_json::from_str::<serde_json::Value>(json.to_string().as_str()).ok()?;
        let mut best: Option<(f32, i32)> = None;
        for row in rows.as_array()? {
            if row.get("team").and_then(serde_json::Value::as_u64) != Some(u64::from(team)) {
                continue;
            }
            let (Some(x), Some(y), Some(hit_size), Some(kind)) = (
                row.get("x").and_then(serde_json::Value::as_f64),
                row.get("y").and_then(serde_json::Value::as_f64),
                row.get("hit_size").and_then(serde_json::Value::as_f64),
                row.get("type").and_then(serde_json::Value::as_i64),
            ) else {
                continue;
            };
            let dx = x as f32 - wx;
            let dy = y as f32 - wy;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > hit_size as f32 / 2.0 + 5.0 {
                continue;
            }
            if best.is_none_or(|(current, _)| distance < current) {
                best = Some((distance, kind as i32));
            }
        }
        let (_, kind) = best?;
        let guard = host.bind();
        let def = guard
            .content_registry()?
            .unit(mind_core::content::UnitTypeId::new(kind as u16))?;
        Some(def.name.clone())
    }

    /// One `Effect::Hotkey` for the fired binding (probe/UI signal).
    pub(super) fn fire_hotkey(&mut self, name: &str) {
        self.last_action = name.to_owned();
        self.action_count += 1;
        self.effects.push(Effect::Hotkey(name.to_owned()));
    }

    /// Broadcasts the current selection (`SelectionChanged`).
    pub fn emit_selection(&mut self) {
        let ids: Vec<i32> = self
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        self.effects.push(Effect::SelectionChanged(
            serde_json::Value::Array(ids.into_iter().map(serde_json::Value::from).collect())
                .to_string(),
        ));
    }

    /// `command_tap` on the world (right click in command mode).
    pub(super) fn command_tap(&mut self, bindings: &BindingState) {
        let queue = self.key_down(bindings, ids::COMMAND_QUEUE);
        self.command_tap_with_queue(queue);
    }

    /// `DesktopInput.touchDown` (DI:924-934): the middle click
    /// (`Binding.commandQueue`) issues a queued order in command mode.
    pub(super) fn command_tap_queued(&mut self) {
        self.command_tap_with_queue(true);
    }

    fn command_tap_with_queue(&mut self, queue: bool) {
        let Some((x, y)) = self.cursor_world() else {
            return;
        };
        // `InputHandler.commandTap` IH:1166-1168: the building under the
        // cursor is the attack target when its team is not the player's
        // (derelict counts as an enemy target, as in Java).
        let player_team = self.player_team();
        let enemy_building = self.cursor_tile().and_then(|(tx, ty)| {
            let host = self.host.clone()?;
            let team = host.bind().building_team(tx, ty)?;
            (team != player_team).then_some(mind_core::world::TilePos::new(tx as i16, ty as i16))
        });
        let actions =
            self.controller
                .command_tap(&self.selectable, player_team, x, y, queue, enemy_building);
        for action in actions {
            self.emit_action(action);
        }
    }

    /// Emits one [`RemoteAction`] through the batcher into the sim command queue.
    pub fn emit_action(&mut self, action: RemoteAction) {
        let batches = match self.controller.batcher.emit(0, action) {
            Ok(batches) => batches,
            Err(error) => {
                log::debug!("input action rejected: {error}");
                return;
            }
        };
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let mut guard = host.bind_mut();
        for batch in batches {
            for command in batch.commands {
                guard.submit_player_command(command);
            }
        }
    }

    /// `InputHandler.pingLocation(player, x, y, text)`
    /// (`InputHandler.java:286-302`): both transport forms of the ping. The
    /// controller records the `RemoteAction::Ping` (no `SimCommand`; a live net
    /// client forwards it in `input/command_emit.rs`) and the HUD shows the
    /// marker (`PlayerComp.drawPing` / `MinimapRenderer`).
    pub fn ping_at(&mut self, x: f32, y: f32, text: Option<&str>) {
        // `InputHandler.java:298`: an empty label is `null`; a longer one is
        // truncated to `Vars.maxPingTextLength` plus an ellipsis.
        let label = text
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                if text.chars().count() > MAX_PING_TEXT {
                    let mut truncated: String = text.chars().take(MAX_PING_TEXT).collect();
                    truncated.push_str("...");
                    truncated
                } else {
                    text.to_owned()
                }
            })
            .unwrap_or_default();
        self.emit_action(RemoteAction::Ping { x, y });
        self.effects.push(Effect::Ping { x, y, text: label });
    }

    // ---- pointer path ----

    /// Cursor tile from the camera transform.
    pub fn cursor_tile(&self) -> Option<(i32, i32)> {
        let camera = self.camera.clone()?;
        let tile = camera
            .bind()
            .screen_to_tile(self.mouse.0 as f64, self.mouse.1 as f64);
        Some((tile.x, tile.y))
    }

    /// Exact cursor world position (`input.mouseWorldX/Y`), not tile-snapped.
    pub fn aim_world(&self) -> Option<(f32, f32)> {
        let camera = self.camera.clone()?;
        let point = camera
            .bind()
            .screen_to_world(self.mouse.0 as f64, self.mouse.1 as f64);
        Some((point.x, point.y))
    }

    /// `InputHandler.tileX/tileY` (`InputHandler.java:2105-2120`): while a block
    /// is selected (and not breaking), the cursor tile is shifted by the block's
    /// `offset`, centring even-size blocks under the cursor.
    pub(super) fn placement_cursor_tile(&self) -> Option<(i32, i32)> {
        let block = self.controller.state.block?;
        if self.controller.state.place_mode == PlaceMode::Breaking {
            return self.cursor_tile();
        }
        let offset = self
            .host
            .clone()
            .and_then(|host| {
                host.bind()
                    .content_registry()
                    .and_then(|content| content.block(block))
                    .map(|def| def.offset)
            })
            .unwrap_or(0.0);
        let (wx, wy) = self.aim_world()?;
        Some(mind_core::input::cursor::placement_tile(wx, wy, offset))
    }

    /// `player.shooting` after the input-side `canShoot` gates
    /// (`InputHandler.java:2269-2272`; the sim half re-checks building/mining).
    pub fn shooting_allowed(&self) -> bool {
        self.shooting
            && !self.dropping_item
            && self.controller.state.block.is_none()
            && !self.controller.state.command_mode
    }

    /// Raw cursor tile (`InputHandler.rawTileX/rawTileY`), no block offset.
    pub fn raw_cursor_tile(&self) -> Option<(i32, i32)> {
        self.cursor_tile()
    }
}
