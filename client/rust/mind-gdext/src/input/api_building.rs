// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-callable building endpoints: select, rotate, configure and the payload calls.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

#[godot_api(secondary)]
impl MindInput {
    /// Selects a buildable block by name and syncs the placement state.
    #[func]
    pub fn select_block_by_name(&mut self, name: GString) -> bool {
        let Some(mut host) = self.bridge.host.clone() else {
            return false;
        };
        let name_text = name.to_string();
        if !host.bind_mut().select_block(name) {
            return false;
        }
        let guard = host.bind();
        if let Some(content) = guard.content_registry()
            && let Some(id) = content.block_id(&name_text)
        {
            // `BlockId::AIR` means "no block selected" (`player.block == null`);
            // the command-mode gate needs it (PAR-0140).
            let selected = if id == mind_core::content::BlockId::AIR {
                None
            } else {
                Some(id)
            };
            self.bridge.controller.state.select_block(selected);
            // `isPlacing()` clears copied plans (`DesktopInput.java:632`).
            self.bridge.clear_select_plans();
        }
        true
    }

    /// Uses a saved schematic in game (`SchematicsDialog` click ->
    /// `control.input.useSchematic`; G08 T6b change 13 / G05 T10b): copies its
    /// plans to the raw cursor tile, clears the block selection and returns
    /// true when plans were adopted. False outside a game or for a bad index.
    #[func]
    pub fn use_schematic(&mut self, index: i32) -> bool {
        if !self.bridge.in_game {
            return false;
        }
        let Some(campaign) = self.campaign.clone() else {
            return false;
        };
        let Some(schem) = campaign.bind().schematic_at(index) else {
            return false;
        };
        let Some((cx, cy)) = self.bridge.raw_cursor_tile() else {
            return false;
        };
        let Some(mut host) = self.bridge.host.clone() else {
            return false;
        };
        let plans = {
            let guard = host.bind();
            let Some(content) = guard.content_registry() else {
                return false;
            };
            mind_core::game::schematics::plans_for(&schem, cx, cy, true, content)
                .iter()
                .map(mind_core::input::ClientPlan::from_build_plan)
                .collect::<Vec<_>>()
        };
        {
            let mut guard = host.bind_mut();
            guard.select_block(GString::from("air"));
        }
        self.bridge.adopt_select_plans(plans, (cx, cy));
        !self.bridge.controller.state.select_plans.is_empty()
    }

    /// Rotates the pending placement 90° (`Binding.rotate`).
    #[func]
    pub fn rotate_placement(&mut self) {
        self.bridge.rotate_placement(&self.bindings);
    }

    /// Rotates the placed building under the cursor or the pending placement.
    /// Test seam: the `rotateplaced` binding is wheel-only as of T9
    /// (`DesktopInput.java:924-926`).
    #[func]
    pub fn rotate_placed(&mut self) {
        self.bridge.rotate_under_cursor(&self.bindings);
    }

    /// Clears the selected block (`Binding.clear_building`).
    #[func]
    pub fn clear_building(&mut self) {
        self.bridge.controller.state.select_block(None);
    }

    /// Toggles placement building pause (`Binding.pause_building`).
    #[func]
    pub fn set_building_paused(&mut self, paused: bool) {
        self.bridge.controller.state.is_building = !paused;
    }

    /// Sets the active catalogue category for the `1`..`0` binds.
    ///
    /// Only the category changes: upstream shows the category's blocks without
    /// selecting one, and a boot auto-select kept `player.block` non-null (the
    /// command-mode gate never opened; PAR-0140).
    #[func]
    pub fn set_catalog_category(&mut self, index: i64) {
        self.bridge.catalog_category = index.max(0) as usize;
    }

    /// Submits a building config (`tileConfig`) through the sim command queue.
    ///
    /// `kind` is `none`/`string`/`int`/`content`/`item`/`liquid`/`block`/`unit`/
    /// `number`/`bool`/`point`; content-ish kinds carry the content name in
    /// `value`; `point` carries a relative `"dx,dy"` offset (`ItemBridge.java:66-69`).
    /// `string` and `int` bypass `RemoteAction` (its `to_sim_config` collapses
    /// `String` to the content wire) and enqueue the wire `Text`/`Int` value
    /// directly.
    #[func]
    pub fn configure_building(&mut self, x: i32, y: i32, kind: GString, value: GString) -> bool {
        let kind = kind.to_string();
        let value = value.to_string();
        let Some((x, y)) = tile_i16(x, y) else {
            return false;
        };
        let direct = match kind.as_str() {
            "string" => Some(mind_core::determinism::ConfigValue::Text(value.clone())),
            "int" => match value.parse::<i64>() {
                // Java carries raw 32-bit ints (colors are negative when the
                // high bit is set); the picking UI submits the unsigned
                // decimal, so wrap into the wire `Int(i32)`.
                Ok(number) => Some(mind_core::determinism::ConfigValue::Int(number as i32)),
                Err(_) => return false,
            },
            // `int[]` config (`TargetDummy.config(int[])`): comma-separated
            // values submitted by the 5-int panel.
            "ints" => {
                let mut values = Vec::new();
                for part in value.split(',') {
                    let Ok(number) = part.trim().parse::<i64>() else {
                        return false;
                    };
                    values.push(number as i32);
                }
                Some(mind_core::determinism::ConfigValue::IntArray(values))
            }
            _ => None,
        };
        if let Some(config) = direct {
            let submitted = self.bridge.with_host_mut(|host| {
                host.submit_player_command(SimCommand::Configure {
                    x,
                    y,
                    value: config,
                })
            });
            return submitted.is_some();
        }
        let config = match kind.as_str() {
            "none" => ConfigValue::None,
            "number" => match value.parse::<f64>() {
                Ok(number) => ConfigValue::Number(number),
                Err(_) => return false,
            },
            "bool" => ConfigValue::Bool(value == "true" || value == "1"),
            "point" => {
                // Relative packed offset (`ItemBridge.config(Point2.class, ..)`).
                let Some((dx, dy)) = value.split_once(',') else {
                    return false;
                };
                match (dx.trim().parse::<i32>(), dy.trim().parse::<i32>()) {
                    (Ok(dx), Ok(dy)) => ConfigValue::Point2(dx, dy),
                    _ => return false,
                }
            }
            // `content`/`item`/`liquid`/`block`/`unit`: by-name lookup on the
            // sim side (`resolve_config`).
            _ => ConfigValue::String(value),
        };
        // `BuildingComp.configure` -> `Block.lastConfig`: the next plan of this
        // block carries the value (`nextConfig`, T5).
        self.bridge
            .record_last_config(i32::from(x), i32::from(y), config.clone());
        self.bridge
            .emit_action(mind_core::input::RemoteAction::Configure {
                x,
                y,
                value: config,
            });
        true
    }

    /// Submits a raw byte config (`CanvasBlock.config(byte[])`; T14).
    ///
    /// The canvas editor packs the pixmap and sends the payload here; the
    /// length check lives in the block's `configured` (`CanvasBlock.java:48-53`).
    #[func]
    pub fn configure_building_bytes(&mut self, x: i32, y: i32, data: PackedByteArray) -> bool {
        let Some((x, y)) = tile_i16(x, y) else {
            return false;
        };
        let bytes: Vec<u8> = data.to_vec();
        let config = ConfigValue::Bytes(bytes.into());
        self.bridge
            .record_last_config(i32::from(x), i32::from(y), config.clone());
        self.bridge
            .emit_action(mind_core::input::RemoteAction::Configure {
                x,
                y,
                value: config,
            });
        true
    }

    /// Withdraws/deposits an item at a block (`tryDropItems`/`requestItem`).
    #[func]
    pub fn transfer_item(&mut self, x: i32, y: i32, item: GString, amount: i64, deposit: bool) {
        self.bridge
            .transfer_item(x, y, &item.to_string(), amount as i32, deposit);
    }

    /// Payload pickup (`[`): `requestUnitPayload` at the cursor.
    #[func]
    pub fn pickup_payload(&mut self) {
        self.bridge.pickup_payload();
    }

    /// Payload drop (`]`): `requestDropPayload` at the cursor.
    #[func]
    pub fn drop_payload(&mut self) {
        self.bridge.drop_payload();
    }

    /// Latches `keybinds.json` into `BindingState` (rebind dialogs call this).
    #[func]
    pub fn reload_bindings(&mut self) {
        self.bindings = bindings::load();
    }
}
