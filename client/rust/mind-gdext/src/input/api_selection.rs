// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Selection and command endpoints, including the block-config panel.
//! Upstream: core/src/mindustry/input/PlacementFragment.java

use super::*;

#[godot_api(secondary)]
impl MindInput {
    /// Feeds the selectable-unit list (`[{id,type,x,y,team,commandable}]`).
    #[func]
    pub fn set_selectable_units_json(&mut self, json: GString) -> i64 {
        self.bridge.set_selectable_units_json(&json.to_string()) as i64
    }

    /// Current RTS selection as a JSON id array.
    #[func]
    pub fn selection_json(&self) -> GString {
        let ids: Vec<i32> = self
            .bridge
            .controller
            .state
            .selected_units
            .iter()
            .copied()
            .collect();
        GString::from(
            serde_json::Value::Array(ids.into_iter().map(serde_json::Value::from).collect())
                .to_string()
                .as_str(),
        )
    }

    /// Sets a unit stance on the current selection (`setUnitStance`).
    #[func]
    pub fn set_unit_stance(&mut self, stance: i64, enabled: bool) {
        self.bridge.set_stance(stance.max(0) as u16, enabled);
    }

    /// `Call.setUnitCommand(player, selected ids, command)` by `UnitCommand`
    /// name ("move", "repair", ...). False when nothing is selected or the name
    /// is unknown (`PlacementFragment.java:579-581`). H08 reuses this seam.
    #[func]
    pub fn set_unit_command(&mut self, command: GString) -> bool {
        let Some(id) = self.command_id_by_name(command.to_string().as_str()) else {
            return false;
        };
        if self.bridge.controller.state.selected_units.is_empty() {
            return false;
        }
        self.bridge.command_units(id);
        true
    }

    /// `Call.setUnitStance(player, selected ids, stance, enabled)` by
    /// `UnitStance` name ("stop", "holdfire", ...). False when nothing is
    /// selected or the name is unknown (`PlacementFragment.java:602-603`).
    #[func]
    pub fn set_unit_stance_named(&mut self, stance: GString, enabled: bool) -> bool {
        let Some(id) = self.stance_id_by_name(stance.to_string().as_str()) else {
            return false;
        };
        if self.bridge.controller.state.selected_units.is_empty() {
            return false;
        }
        self.bridge.set_stance(id, enabled);
        true
    }

    /// Left click on a command-panel type icon: keep only units of `type_id`
    /// (`selectedUnits.removeAll(unit -> unit.type != type)`,
    /// `PlacementFragment.java:539-541`).
    #[func]
    pub fn select_only_type(&mut self, type_id: i64) -> bool {
        let wanted = type_id as i32;
        let types: std::collections::HashMap<i32, i32> = self
            .bridge
            .selectable
            .iter()
            .map(|unit| (unit.id, unit.type_id))
            .collect();
        self.bridge
            .retain_selection_types(&|id| types.get(&id).copied(), Some(wanted), None)
    }

    /// Right click on a command-panel type icon: drop units of `type_id`
    /// (`selectedUnits.removeAll(unit -> unit.type == type)`,
    /// `PlacementFragment.java:543-545`).
    #[func]
    pub fn deselect_type(&mut self, type_id: i64) -> bool {
        let wanted = type_id as i32;
        let types: std::collections::HashMap<i32, i32> = self
            .bridge
            .selectable
            .iter()
            .map(|unit| (unit.id, unit.type_id))
            .collect();
        self.bridge
            .retain_selection_types(&|id| types.get(&id).copied(), None, Some(wanted))
    }

    /// `DesktopBridge::config_spec` as JSON (probe/test seam): the panel the
    /// tap handler would open for the building at `(x, y)`.
    #[func]
    pub fn config_spec_json(&self, x: i32, y: i32) -> GString {
        self.bridge
            .config_spec(x, y)
            .map(|spec| GString::from(spec.as_str()))
            .unwrap_or_default()
    }

    /// Clears the config-panel selection (`config.hideConfig()`): the close
    /// button calls this so the tap dispatch stops treating the panel as open
    /// (H02 T6, PAR-0220).
    #[func]
    pub fn close_block_config(&mut self) {
        self.config_tile = None;
        self.bridge.config_selected = None;
    }

    /// `Block.selectScroll` writeback (`ItemSelection.java:103-108`): the
    /// `MindItemSelection` pane reports its scroll offset so the next open of a
    /// panel for that block restores it.
    #[func]
    pub fn set_config_scroll(&mut self, block: GString, scroll: f32) {
        let block = block.to_string();
        self.bridge.set_config_scroll(&block, scroll);
    }

    /// [`Self::config_selected`] as `"[x,y]"` or `"null"`.
    #[func]
    pub fn config_selected_json(&self) -> GString {
        match self.config_selected() {
            Some((x, y)) => GString::from(format!("[{x},{y}]").as_str()),
            None => GString::from("null"),
        }
    }
}

impl MindInput {
    /// The building whose config panel is open (`input.config.getSelected()`),
    /// as an (x, y) tile; None when the panel is hidden or was never opened.
    /// Rust-only accessor used by H03 T5's overlay and H06 T12; also exposed
    /// for probes through [`Self::config_selected_json`].
    pub fn config_selected(&self) -> Option<(i32, i32)> {
        let (x, y) = self.config_tile?;
        let ui_root = self.ui_root.clone()?;
        let node = ui_root.bind().hud_fragment("block_config")?;
        let visible = node.get("visible").try_to::<bool>().unwrap_or(false);
        visible.then_some((x, y))
    }

    /// Resolves a `UnitCommand` name through the host's content snapshot.
    fn command_id_by_name(&self, name: &str) -> Option<u16> {
        let host = self.bridge.host.clone()?;
        host.bind()
            .content_registry()
            .and_then(|content| content.unit_command_by_name(name))
            .map(|def| def.id.raw())
    }

    /// Resolves a `UnitStance` name through the host's content snapshot.
    fn stance_id_by_name(&self, name: &str) -> Option<u16> {
        let host = self.bridge.host.clone()?;
        host.bind()
            .content_registry()
            .and_then(|content| content.unit_stance_by_name(name))
            .map(|def| def.id.raw())
    }
}
