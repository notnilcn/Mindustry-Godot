// SPDX-License-Identifier: GPL-3.0-only

//! Building config and item JSON views used by the config UI.
//! Upstream: none (port-only; config-UI JSON seams)

use super::*;

/// JSON view of a building's `config()` for the config-UI seams.
pub(super) fn config_to_json(value: &mind_core::world::config::ConfigValue) -> serde_json::Value {
    use mind_core::world::config::ConfigValue as V;
    use serde_json::json;
    match value {
        V::None => serde_json::Value::Null,
        V::Item(id) => json!({"type": "item", "id": id.raw()}),
        V::Liquid(id) => json!({"type": "liquid", "id": id.raw()}),
        V::Block(id) => json!({"type": "block", "id": id.raw()}),
        V::Unit(id) => json!({"type": "unit", "id": id.raw()}),
        V::Content(type_, id) => json!({"type": "content", "content": type_.name(), "id": id}),
        V::Point2(x, y) => json!({"type": "point", "x": x, "y": y}),
        V::Point2Array(points) => json!({
            "type": "point_array",
            "points": points.iter().collect::<Vec<_>>()
        }),
        V::Number(number) => serde_json::Number::from_f64(*number)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        V::Bool(flag) => serde_json::Value::Bool(*flag),
        V::Building(_) => json!({"type": "building"}),
        V::Bytes(bytes) => json!({
            "type": "bytes",
            "len": bytes.len(),
            // The canvas editor reads the packed payload (`CanvasEditDialog`).
            "data": bytes.iter().copied().collect::<Vec<u8>>()
        }),
        V::String(text) => json!({"type": "string", "value": text}),
        V::IntArray(values) => json!({"type": "int_array", "values": values}),
    }
}

/// Item counts for the building at `(x, y)` as JSON; `None` when the tile has
/// no item-capable building or its items total zero. See
/// [`MindSimHost::block_items_json`].
pub(super) fn block_items_json(
    sim: &Sim,
    content: &ContentRegistry,
    x: i32,
    y: i32,
) -> Option<String> {
    let (tx, ty) = crate::gdutil::coords::tile_i16(x, y)?;
    let pos = mind_core::world::TilePos::new(tx, ty);
    let def = content.block(sim.grid.block_at(pos)?)?;
    if !def.has_items {
        return None;
    }
    let entity = sim.grid.entity_at(pos)?;
    let module = sim.ecs.0.get::<ItemModule>(entity)?;
    if module.total() <= 0 {
        return None;
    }
    let mut items = serde_json::Map::new();
    for (index, amount) in module.items.iter().enumerate() {
        if *amount <= 0 {
            continue;
        }
        let Some(item) = content.item(ItemId::new(index as u16)) else {
            continue;
        };
        items.insert(item.name.clone(), serde_json::Value::from(*amount));
    }
    serde_json::to_string(&serde_json::Value::Object(items)).ok()
}

impl MindSimHost {
    /// Config value kinds the block's behavior accepts (`Block.config(...)`,
    /// `Block.configurations` keys) from the live block runtime. Empty when the
    /// runtime is not installed or the behavior declares none.
    pub fn block_config_kinds(&self, block: BlockId) -> &'static [mind_core::world::ConfigKind] {
        self.world()
            .get_resource::<mind_core::world::block::BlockTable>()
            .and_then(|table| table.get(block).map(|inst| inst.config_kinds()))
            .unwrap_or(&[])
    }

    /// `MessageBlock.maxTextLength`/`maxNewlines` for the block's live
    /// instance (`MessageBlock.java:29-30`; the port's `MessageDef`); `None`
    /// for other kinds.
    pub fn message_limits(&self, block: BlockId) -> Option<(i32, i32)> {
        let table = self
            .world()
            .get_resource::<mind_core::world::block::BlockTable>()?;
        match &table.get(block)?.kind_data {
            mind_core::world::block_kind_data::BlockKindData::Message(message) => {
                Some((message.max_text, message.max_newlines))
            }
            _ => None,
        }
    }

    /// `UnitFactoryBuild` config state for the plan grid + command row (J05
    /// T2): the selected plan index (`-1` = none) and the queued command
    /// override (`UnitFactory.java:227`).
    pub fn unit_factory_config(&self, x: i32, y: i32) -> Option<(i32, Option<u16>)> {
        let (tx, ty) = crate::gdutil::coords::tile_i16(x, y)?;
        let entity = self.sim.grid.entity_at(TilePos::new(tx, ty))?;
        let state = self
            .world()
            .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)?;
        Some((
            state.current_plan,
            state.command.map(mind_core::content::UnitCommandId::raw),
        ))
    }

    /// Item counts of the building at `(x, y)` as JSON (`{item-name: amount}`).
    ///
    /// `None` when the tile holds no item-capable building or its item module
    /// is empty (`Block.hasItems && items.total() > 0`,
    /// `InputHandler.tileTapped`). Rust-only input-bridge seam; not MCP API.
    pub fn block_items_json(&self, x: i32, y: i32) -> Option<String> {
        let content = self.content_snapshot.as_ref()?;
        block_items_json(&self.sim, content, x, y)
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Building config readback (`Building.config()`) as JSON, for config-UI
    /// verification (`{"type":"string","value":...}`, `{"type":"item","id":..}`,
    /// `null` for no config).
    #[func]
    pub fn building_config_json(&self, x: i32, y: i32) -> GString {
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return GString::from("null");
        };
        let config = mind_core::world::config::read_config(self.world(), entity);
        let value = config_to_json(&config);
        crate::gdutil::json::value_gstring(&value)
    }

    /// `StatusEffect.permanent` for a `@status-<name>` content reference
    /// (`LStatements.java:1465-1474` hides `seconds` for permanent statuses).
    #[func]
    pub fn status_permanent(&self, effect: GString) -> bool {
        let name = effect.to_string();
        let name = name.trim_start_matches("@status-");
        self.content_snapshot
            .as_ref()
            .and_then(|content| content.status_by_name(name))
            .is_some_and(|status| status.permanent)
    }

    /// Logic-editor content picker rows (`LStatements.java` `showSelectTable`
    /// content grids): `unit`/`item`/`liquid`/`status`/`weather`/`block`.
    ///
    /// Rows are `{name, hidden, unlocked, always_unlocked, unlocked_now}`
    /// (`unlocked_now` is `UnlockableContent.unlockedNow`: unlocked outside a
    /// campaign), plus `logic_controllable` for units and `permanent`/`show`
    /// for statuses. Unknown kinds return `[]`.
    #[func]
    pub fn logic_picker_json(&self, kind: GString) -> GString {
        let Some(registry) = self.content_snapshot.as_ref() else {
            return crate::gdutil::json::empty_array();
        };
        let campaign = self.campaign_runtime().is_some();
        let unlocked_now = |unlock: &mind_core::content::UnlockFields| {
            unlock.unlocked || unlock.always_unlocked || !campaign
        };
        let base_row = |name: &str, unlock: &mind_core::content::UnlockFields, hidden: bool| {
            serde_json::json!({
                "name": name,
                "hidden": hidden,
                "unlocked": unlock.unlocked,
                "always_unlocked": unlock.always_unlocked,
                "unlocked_now": unlocked_now(unlock),
            })
        };
        let rows: Vec<serde_json::Value> = match kind.to_string().as_str() {
            "unit" => registry
                .units()
                .iter()
                .filter(|unit| !unit.removed)
                .map(|unit| {
                    let mut row = base_row(&unit.name, &unit.unlock, unit.hidden);
                    row["logic_controllable"] = unit.logic_controllable.into();
                    row
                })
                .collect(),
            "item" => registry
                .items()
                .iter()
                .filter(|item| !item.removed)
                .map(|item| base_row(&item.name, &item.unlock, item.hidden))
                .collect(),
            "liquid" => registry
                .liquids()
                .iter()
                .filter(|liquid| !liquid.removed)
                .map(|liquid| base_row(&liquid.name, &liquid.unlock, liquid.hidden))
                .collect(),
            "status" => registry
                .statuses()
                .iter()
                .filter(|status| !status.removed)
                .map(|status| {
                    let mut row = base_row(&status.name, &status.unlock, !status.show);
                    row["permanent"] = status.permanent.into();
                    row["show"] = status.show.into();
                    row
                })
                .collect(),
            "weather" => registry
                .weathers()
                .iter()
                .filter(|weather| !weather.removed)
                .map(|weather| base_row(&weather.name, &weather.unlock, weather.hidden))
                .collect(),
            "block" => registry
                .blocks()
                .iter()
                .filter(|block| !block.removed)
                .map(|block| base_row(&block.name, &block.unlock, block.is_hidden()))
                .collect(),
            _ => Vec::new(),
        };
        crate::gdutil::json::to_gstring(&rows, "[]")
    }

    /// Read-only `Building.onConfigureBuildTapped` preview for the tap handler
    /// (H02 T6, PAR-0220): `0` = Java `return false` (panel stays), `1` =
    /// `deselect(); return false` (panel closes), `2` = `return true` (switch
    /// the panel to the tapped building). `-1` when either tile has no build.
    #[func]
    pub fn preview_config_tap(&self, selected_x: i32, selected_y: i32, x: i32, y: i32) -> i32 {
        let (Ok(sx), Ok(sy), Ok(tx), Ok(ty)) = (
            i16::try_from(selected_x),
            i16::try_from(selected_y),
            i16::try_from(x),
            i16::try_from(y),
        ) else {
            return -1;
        };
        let Some(selected) = self.sim.grid.entity_at(TilePos::new(sx, sy)) else {
            return -1;
        };
        let Some(other) = self.sim.grid.entity_at(TilePos::new(tx, ty)) else {
            return -1;
        };
        match mind_core::world::config_tap::config_tap(
            self.world(),
            &self.sim.grid,
            selected,
            other,
        ) {
            mind_core::world::behavior::ConfigTapResult::Consume(_) => 0,
            mind_core::world::behavior::ConfigTapResult::ConsumeAndDeselect(_) => 1,
            mind_core::world::behavior::ConfigTapResult::SwitchAfter(_) => 2,
        }
    }

    /// Building config readback as a plain string (`nil` for non-string configs).
    #[func]
    pub fn building_config_string(&self, x: i32, y: i32) -> Variant {
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return Variant::nil();
        };
        match mind_core::world::config::read_config(self.world(), entity) {
            mind_core::world::config::ConfigValue::String(text) => Variant::from(text.as_str()),
            _ => Variant::nil(),
        }
    }

    /// Packed RGBA color of a `LightBlock` at `(x, y)` (`nil` when absent).
    #[func]
    pub fn light_color(&self, x: i32, y: i32) -> Variant {
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return Variant::nil();
        };
        match self
            .world()
            .get::<mind_core::world::behavior::light::LightState>(entity)
        {
            Some(state) => Variant::from(i64::from(state.color)),
            None => Variant::nil(),
        }
    }

    /// `Building.enabled` at `(x, y)` (`nil` when the tile has no building).
    #[func]
    pub fn building_enabled(&self, x: i32, y: i32) -> Variant {
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return Variant::nil();
        };
        match self
            .world()
            .get::<mind_core::entities::comp::Building>(entity)
        {
            Some(building) => Variant::from(building.enabled),
            None => Variant::nil(),
        }
    }

    /// [`Self::unit_factory_config`] as `"[plan, command]"` (`command` = `-1`
    /// unset); `"null"` without a factory build. Probe/UI seam.
    #[func]
    pub fn factory_command_json(&self, x: i32, y: i32) -> GString {
        match self.unit_factory_config(x, y) {
            Some((plan, command)) => {
                GString::from(format!("[{plan},{}]", command.map_or(-1, i64::from)).as_str())
            }
            None => GString::from("null"),
        }
    }

    /// Item counts of the building at `(x, y)` as JSON (`{item-name: amount}`),
    /// `{}` when the tile holds no item-capable building. MCP-facing wrapper
    /// over [`Self::block_items_json`] (T0 inventory playtest seam).
    #[func]
    pub fn building_items_json(&self, x: i32, y: i32) -> GString {
        GString::from(
            self.block_items_json(x, y)
                .unwrap_or_else(|| "{}".to_string())
                .as_str(),
        )
    }

    /// `Units.anyEntities(tile)` for the door tap gate (`Door.java:148-154`;
    /// T13): whether a ground unit overlaps the building at `(x, y)`.
    #[func]
    pub fn building_has_unit_overlap(&self, x: i32, y: i32) -> bool {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return false;
        }
        let pos = mind_core::world::TilePos::new(tx, ty);
        let Some(block) = self.sim.grid.block_at(pos) else {
            return false;
        };
        let Some(content) = self.content_registry() else {
            return false;
        };
        mind_core::sim::builder::unit_overlap_at(self.world(), content, pos, block)
    }

    /// `Door.DoorBuild.tapped`'s 60-tick timer (`Door.java:149`):
    /// `origin().timer(timerToggle, 60f)`. Returns whether the tap may toggle;
    /// consumes the origin door's slot when it fires. The caller checks
    /// `building_has_unit_overlap` first (Java short-circuit order).
    #[func]
    pub fn door_tap_allowed(&mut self, x: i32, y: i32) -> bool {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        let pos = mind_core::world::TilePos::new(tx, ty);
        let Some(entity) = self.sim.grid.entity_at(pos) else {
            return false;
        };
        mind_core::world::behavior::defense::tap_allowed(self.world_mut(), entity)
    }
}
