// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World-processor discovery, editing and logic integration.
//! Upstream: core/src/mindustry/editor/MapProcessorsDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    /// `MapProcessorsDialog`: `world-processor` centers with tag/icon.
    #[func]
    pub fn processors(&self) -> Array<VarDictionary> {
        let mut out = Array::new();
        let Some(content) = self.content() else {
            return out;
        };
        let Some(processor) = content.block_id("world-processor") else {
            return out;
        };
        for index in 0..self.grid.tiles.len() {
            let tile = self.grid.tiles.geti(index);
            if tile.block != processor {
                continue;
            }
            let Some(entity) = tile.build else {
                continue;
            };
            let (tag, icon_tag) = self
                .ecs
                .0
                .get::<mind_core::logic::blocks::logic_block::LogicBlockState>(entity)
                .map(|state| (state.tag.clone(), state.icon_tag))
                .unwrap_or((None, '\0'));
            out.push(&dict(vec![
                ("x", (tile.x as i32).to_variant()),
                ("y", (tile.y as i32).to_variant()),
                (
                    "tag",
                    GString::from(tag.unwrap_or_default().as_str()).to_variant(),
                ),
                ("icon_tag", (icon_tag as i64).to_variant()),
            ]));
        }
        out
    }

    /// Editor building entity at `(x, y)` carrying `LogicBlockState`.
    pub(super) fn logic_entity_at(&self, x: i64, y: i64) -> Option<bevy_ecs::entity::Entity> {
        let (Ok(xi), Ok(yi)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let entity = self
            .grid
            .entity_at(mind_core::world::TilePos::new(xi, yi))?;
        self.ecs
            .0
            .get::<LogicBlockState>(entity)
            .is_some()
            .then_some(entity)
    }

    /// `MapProcessorsDialog` `@add`: the first non-synthetic tile becomes a
    /// `world-processor` (`MapProcessorsDialog.java:33-56`); false when the
    /// whole map is synthetic (`@editor.worldprocessors.nospace`).
    #[func]
    pub fn add_processor(&mut self) -> bool {
        let Some(content) = self.content() else {
            return false;
        };
        let Some(processor) = content.block_id("world-processor") else {
            return false;
        };
        let Some((x, y)) = mind_core::editor::processors::free_processor_tile(&self.grid, content)
        else {
            return false;
        };
        let team = Team::get(1).id;
        let _ = self.with_world(|_, world, _| world.set_block(x, y, processor, team, 0));
        self.mark_tiles_changed();
        true
    }

    /// `MapProcessorsDialog` rename: `log.tag = tag` directly (the Java
    /// comment bypasses privileged configure checks, `:100-105`).
    #[func]
    pub fn rename_processor(&mut self, x: i64, y: i64, tag: GString) -> bool {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return false;
        };
        let clamped = mind_core::editor::processors::clamp_tag(&tag.to_string());
        let Some(mut state) = self.ecs.0.get_mut::<LogicBlockState>(entity) else {
            return false;
        };
        state.tag = (!clamped.is_empty()).then_some(clamped);
        true
    }

    /// `MapProcessorsDialog` icon select: `log.iconTag = (char)ic` (`:86-90`);
    /// `0` clears the icon (`Icon.none`).
    #[func]
    pub fn set_processor_icon(&mut self, x: i64, y: i64, code: i64) -> bool {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return false;
        };
        let Some(mut state) = self.ecs.0.get_mut::<LogicBlockState>(entity) else {
            return false;
        };
        state.icon_tag = u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .unwrap_or('\0');
        true
    }

    /// `MapProcessorsDialog` delete (`:116-134`): a processor surrounded by
    /// walls/static blocks becomes `stone-wall` (the port does not track
    /// `floor().wall`, `editor/processors.rs`), else air. Confirmation is the
    /// caller's.
    #[func]
    pub fn delete_processor(&mut self, x: i64, y: i64) -> bool {
        let (Ok(xi), Ok(yi)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let (xi, yi) = (xi as i32, yi as i32);
        let Some(content) = self.content() else {
            return false;
        };
        if self.grid.tiles.getn(xi, yi).is_none() {
            return false;
        }
        let mut surrounded = true;
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let Some(neighbor) = self.grid.tiles.getn(xi + dx, yi + dy) else {
                continue; // map edge: `Tile.nearby` null counts as surrounded
            };
            let def = content.block(neighbor.block);
            let wall = def.is_some_and(|def| def.privileged || block_info::is_static(def));
            if !wall {
                surrounded = false;
                break;
            }
        }
        let replacement = if surrounded {
            content.block_id("stone-wall").unwrap_or(BlockId::AIR)
        } else {
            BlockId::AIR
        };
        let _ = self.with_world(|_, world, _| world.set_block(xi, yi, replacement, 0, 0));
        self.mark_tiles_changed();
        true
    }

    /// `LogicDialog.show` for an editor processor
    /// (`MapProcessorsDialog`'s name button -> `showEditDialog(true)`): the
    /// normalized program (`LAssembler.read` -> `write`) and privilege.
    #[func]
    pub fn logic_get_state(&self, x: i64, y: i64) -> VarDictionary {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return VarDictionary::new();
        };
        let Some(state) = self.ecs.0.get::<LogicBlockState>(entity) else {
            return VarDictionary::new();
        };
        let privileged = state.executor.privileged;
        let code = mind_core::logic::Assembler::read(&state.code, privileged)
            .map(|statements| mind_core::logic::Assembler::write(&statements))
            .unwrap_or_else(|_| state.code.clone());
        dict(vec![
            ("code", GString::from(code.as_str()).to_variant()),
            ("privileged", privileged.to_variant()),
            ("ipt", (state.ipt as i64).to_variant()),
            (
                "tag",
                match &state.tag {
                    Some(tag) => GString::from(tag.as_str()).to_variant(),
                    None => Variant::nil(),
                },
            ),
        ])
    }

    /// `LogicBuild.showEditDialog(true)`'s configure half for an editor
    /// processor: stores the compressed program + links in the editor's
    /// `LogicBlockState` (the editor does not run processors). Returns the
    /// `SimHost.logic_configure_code` error convention (`""` ok, `toolong:<n>`).
    #[func]
    pub fn logic_configure_code(&mut self, x: i64, y: i64, code: GString) -> GString {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return GString::from("noprocessor");
        };
        let Some(state) = self.ecs.0.get::<LogicBlockState>(entity) else {
            return GString::from("noprocessor");
        };
        let links = state.relative_connections(&self.ecs.0, entity);
        let code = code.to_string();
        let bytes = match mind_core::logic::blocks::logic_block::encode_for_config(&code, &links) {
            Ok(bytes) => bytes,
            Err(len) => return GString::from(format!("toolong:{len}").as_str()),
        };
        let Some(mut state) = self.ecs.0.entity_mut(entity).take::<LogicBlockState>() else {
            return GString::from("noprocessor");
        };
        let result = state.read_compressed(&mut self.ecs.0, entity, &bytes, true);
        self.ecs.0.entity_mut(entity).insert(state);
        if result.is_ok() {
            GString::new()
        } else {
            GString::from("invalid")
        }
    }
}
