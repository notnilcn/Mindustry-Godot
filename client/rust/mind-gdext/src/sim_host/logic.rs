// SPDX-License-Identifier: GPL-3.0-only

//! Logic player-view composition pushed into the sim each frame.
//! Upstream: core/src/mindustry/logic/LExecutor.java (player view)

use super::*;

impl MindSimHost {
    /// `InputHandler.logicCutscene` state for the camera rig (plain Rust
    /// forwarder; H06 T3).
    pub fn logic_cutscene_state(&self) -> mind_core::logic::world::LogicCutscene {
        self.sim.logic_cutscene()
    }

    /// Takes the pending `CutsceneI` shake request (camera rig poller).
    pub fn take_logic_shake(&mut self) -> Option<(f32, f32)> {
        self.sim.take_logic_shake()
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    // ---- Plan 13 M8 logic host surface (§3.12) ----
    //
    // The pure probes (`logic_run`, `logic_statement_names`, `logic_compile`)
    // work everywhere. The sim-integrated probes (`logic_place`,
    // `logic_set_code`, `logic_get_state`, `logic_display_commands`) return
    // their empty/false form until plan 05/07 expose live logic-building state
    // in `Sim` (the P0 `Sim` uses `BuildingComp`, not plan-07 behaviors).
    // MCP recipes (plan 13 §7c; editor run is orchestrator-owned via the
    // single-editor mutex):
    //
    //   godot_exec call: $"/root/Spine/MindLogic" statement_names()   # (after adding MindLogic to game.tscn)
    //   godot_exec call: $"/root/Spine/SimHost" logic_run("op add a a 1\njump 0 always", 10, 2, true)
    //   godot_exec call: $"/root/Spine/SimHost" logic_place("logic-processor", 40, 40)  # false until 05/07
    //   godot_exec call: $"/root/Spine/SimHost" logic_get_state(40, 40)

    /// Whether the sim-integrated logic probes can mutate a live processor.
    #[func]
    pub fn logic_available(&self) -> bool {
        true
    }

    /// Registered statement names (plan-14 editor handoff).
    #[func]
    pub fn logic_statement_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for meta in mind_core::logic::statement::all_statements() {
            out.push(&GString::from(meta.registered_name));
        }
        out
    }

    /// Runs `code` on a temporary executor for `ticks` at `ipt`; dumps vars.
    #[func]
    pub fn logic_run(
        &self,
        code: GString,
        ticks: i64,
        ipt: i64,
        privileged: bool,
    ) -> Dictionary<GString, Variant> {
        let ticks = ticks.clamp(0, 1_000_000) as u64;
        let ipt = ipt.clamp(1, 1000) as i32;
        let mut out = Dictionary::<GString, Variant>::new();
        let Some(exec) =
            mind_core::logic::script::run_standalone(&code.to_string(), privileged, ticks, ipt)
        else {
            out.set("ok", &false.to_variant());
            out.set("error", &GString::from("assemble failed").to_variant());
            return out;
        };
        let mut vars = Dictionary::<GString, Variant>::new();
        for cell in &exec.arena.cells {
            if cell.constant || cell.name.starts_with('@') {
                continue;
            }
            let value: Variant = if cell.is_obj {
                match &cell.obj {
                    None => Variant::nil(),
                    Some(value) => GString::from(value.display().as_str()).to_variant(),
                }
            } else {
                cell.num.to_variant()
            };
            vars.set(cell.name.as_str(), &value);
        }
        out.set("ok", &true.to_variant());
        out.set(
            "text",
            &GString::from(exec.text_buffer.as_str()).to_variant(),
        );
        out.set("vars", &vars.to_variant());
        out
    }

    /// Placeholder for placing a logic block (awaits plan 05/07 integration).
    #[func]
    pub fn logic_place(&self, kind: GString, _x: i64, _y: i64) -> bool {
        log::warn!("logic_place(`{kind}`): sim-side logic building state is a plan-05/07 hook");
        false
    }

    /// Normalized processor program text (`LogicDialog.show`:
    /// `LAssembler.read` → `LAssembler.write`; empty string when no processor).
    #[func]
    pub fn logic_get_code(&self, x: i64, y: i64) -> GString {
        let world = self.world();
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            return GString::from("");
        };
        let Some(state) = world.get::<mind_core::logic::blocks::LogicBlockState>(e) else {
            return GString::from("");
        };
        let privileged = mind_core::logic::blocks::privileged_of(world, e);
        let text = mind_core::logic::Assembler::read(&state.code, privileged)
            .map(|statements| mind_core::logic::Assembler::write(&statements))
            .unwrap_or_else(|_| state.code.clone());
        GString::from(text.as_str())
    }

    /// Debug seam: writes `code` into the processor at `(x, y)` directly,
    /// bypassing `LogicBuild.accessible()` like the editor (world processors
    /// are not accessible in campaign; every later in-engine check uses them).
    /// Returns false with `logic_last_error() == "toolong:<len>"` past
    /// `LogicBlock.maxCompressedLen` (`showEditDialog`'s size gate).
    #[func]
    pub fn logic_set_code(&mut self, x: i64, y: i64, code: GString) -> bool {
        self.logic_last_error = String::new();
        let code = code.to_string();
        let world = self.world_mut();
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            self.logic_last_error = "noprocessor".to_owned();
            return false;
        };
        let links = world
            .get::<mind_core::logic::blocks::LogicBlockState>(e)
            .map(|state| state.relative_connections(world, e));
        let Some(links) = links else {
            self.logic_last_error = "noprocessor".to_owned();
            return false;
        };
        let bytes = match mind_core::logic::blocks::logic_block::encode_for_config(&code, &links) {
            Ok(bytes) => bytes,
            Err(len) => {
                self.logic_last_error = format!("toolong:{len}");
                return false;
            }
        };
        let Some(mut state) = world
            .entity_mut(e)
            .take::<mind_core::logic::blocks::LogicBlockState>()
        else {
            return false;
        };
        let result = state.read_compressed(world, e, &bytes, true);
        world.entity_mut(e).insert(state);
        result.is_ok()
    }

    /// Last `logic_set_code` failure (`""` when none).
    #[func]
    pub fn logic_last_error(&self) -> GString {
        GString::from(self.logic_last_error.as_str())
    }

    /// Processor readback (`privileged`/`accessible`/`ipt`/`tag`/`code`/`links`/
    /// `vars`); empty `Dictionary` when no processor is at `(x, y)`.
    #[func]
    pub fn logic_get_state(&self, x: i64, y: i64) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        let world = self.world();
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            return out;
        };
        let Some(snapshot) = mind_core::logic::blocks::logic_block::state_snapshot(world, e) else {
            return out;
        };
        out.set("privileged", &snapshot.privileged.to_variant());
        out.set("accessible", &snapshot.accessible.to_variant());
        out.set("ipt", &(snapshot.ipt as i64).to_variant());
        out.set(
            "tag",
            &match &snapshot.tag {
                Some(tag) => GString::from(tag.as_str()).to_variant(),
                None => Variant::nil(),
            },
        );
        out.set("code", &GString::from(snapshot.code.as_str()).to_variant());
        let mut links = Array::<Variant>::new();
        for (name, lx, ly, valid) in &snapshot.links {
            let mut row = Dictionary::<GString, Variant>::new();
            row.set("name", &GString::from(name.as_str()).to_variant());
            row.set("x", &(*lx as i64).to_variant());
            row.set("y", &(*ly as i64).to_variant());
            row.set("valid", &valid.to_variant());
            links.push(&row.to_variant());
        }
        out.set("links", &links.to_variant());
        let mut vars = Array::<Variant>::new();
        for (name, value, type_) in &snapshot.vars {
            let mut row = Dictionary::<GString, Variant>::new();
            row.set("name", &GString::from(name.as_str()).to_variant());
            row.set("value", &GString::from(value.as_str()).to_variant());
            row.set("type", &GString::from(*type_).to_variant());
            vars.push(&row.to_variant());
        }
        out.set("vars", &vars.to_variant());
        out
    }

    /// Last 64 drained logic-event kind tags (`LogicWorldEvent::kind`) as a
    /// JSON array (H06 T1 debug probe).
    #[func]
    pub fn logic_event_log_json(&self) -> GString {
        let value: Vec<&str> = self
            .sim
            .logic_event_log
            .iter()
            .map(String::as_str)
            .collect();
        crate::gdutil::json::to_gstring(&value, "[]")
    }

    /// Logic cutscene/HUD probe: `{"active","x","y","speed","zoom","hideHud"}`
    /// (`x`/`y` in world pixels; H06 T3).
    #[func]
    pub fn logic_cutscene_json(&self) -> GString {
        let cutscene = self.sim.logic_cutscene();
        let value = serde_json::json!({
            "active": cutscene.active,
            "x": cutscene.pan.0,
            "y": cutscene.pan.1,
            "speed": cutscene.speed,
            "zoom": cutscene.zoom,
            "hideHud": cutscene.hide_hud,
        });
        crate::gdutil::json::value_gstring(&value)
    }

    /// Drains queued world-processor messages (`announce`/`notify`/`toast`) as
    /// an Array of `{type, text, duration}` dictionaries (H06 T4).
    #[func]
    pub fn drain_logic_messages(&mut self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for (type_, text, duration) in self.sim.take_logic_messages() {
            let mut row = Dictionary::<GString, Variant>::new();
            row.set("type", &GString::from(type_.name()).to_variant());
            row.set("text", &GString::from(text.as_str()).to_variant());
            row.set("duration", &f64::from(duration).to_variant());
            out.push(&row.to_variant());
        }
        out
    }

    /// The queued draw commands of the display at `(x, y)`'s ROOT (T13).
    ///
    /// Returns the packed `DisplayCmd` queue the view-side rasterizer drains
    /// (`LogicDisplay.build.commands`; a tileable member forwards to its root).
    #[func]
    pub fn logic_display_commands(&self, x: i64, y: i64) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        let Ok(tx) = i16::try_from(x) else {
            return out;
        };
        let Ok(ty) = i16::try_from(y) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let root = self
            .world()
            .get::<mind_core::logic::blocks::LogicDisplayState>(e)
            .and_then(|state| state.root_display)
            .unwrap_or(e);
        let Some(state) = self
            .world()
            .get::<mind_core::logic::blocks::LogicDisplayState>(root)
        else {
            return out;
        };
        for command in &state.commands {
            out.push(*command as i64);
        }
        out
    }

    /// Display geometry/queue readback (`logic_display_state`; T13).
    #[func]
    pub fn logic_display_state(&self, x: i64, y: i64) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let world = self.world();
        let Some(state) = world.get::<mind_core::logic::blocks::LogicDisplayState>(e) else {
            return out;
        };
        let root = state.root_display.unwrap_or(e);
        let root_state = world
            .get::<mind_core::logic::blocks::LogicDisplayState>(root)
            .unwrap_or(state);
        let (root_x, root_y) = world
            .get::<mind_core::entities::comp::Building>(root)
            .map(|building| (building.tile.x(), building.tile.y()))
            .unwrap_or((tx, ty));
        let tileable = mind_core::logic::blocks::block_kind_of(world, e)
            == Some(mind_core::content::BlockKind::TileableLogicDisplay);
        let frame = mind_core::logic::blocks::tileable_def_of(world, e)
            .map(|def| def.frame_size)
            .unwrap_or(6);
        let (width, height) = if tileable {
            (
                root_state.tiles_width * 32 - frame * 2,
                root_state.tiles_height * 32 - frame * 2,
            )
        } else {
            (root_state.display_size, root_state.display_size)
        };
        out.set("root_x", &i64::from(root_x).to_variant());
        out.set("root_y", &i64::from(root_y).to_variant());
        out.set("width", &i64::from(width).to_variant());
        out.set("height", &i64::from(height).to_variant());
        out.set("queued", &(root_state.commands.len() as i64).to_variant());
        out.set("operations", &(root_state.operations as i64).to_variant());
        out.set("rectangular", &root_state.rectangular.to_variant());
        out.set("tileable", &tileable.to_variant());
        out
    }

    /// RGBA of one raster pixel (`logic_display_pixel`; command space,
    /// bottom-left origin). `-1` when the display or pixel is absent.
    #[func]
    pub fn logic_display_pixel(&self, x: i64, y: i64, px: i64, py: i64) -> i64 {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return -1;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return -1;
        };
        let root = self
            .world()
            .get::<mind_core::logic::blocks::LogicDisplayState>(e)
            .and_then(|state| state.root_display)
            .unwrap_or(e);
        let rasters = self
            .world()
            .get_resource::<mind_core::logic::display_raster::LogicDisplayRasters>()
            .and_then(|rasters| rasters.rasters.get(&root));
        rasters
            .and_then(|raster| raster.pixel(px as i32, py as i32))
            .map(i64::from)
            .unwrap_or(-1)
    }

    /// Processes the visible display roots and returns one row per visible
    /// display for the render pass (T13).
    ///
    /// Rows: `{draw_buffer, x, y, w, h, root_x, root_y, frame, frame_x,
    /// frame_y, bits, sprite}`. Processing drains each visible root's queue
    /// into its CPU raster (`LogicDisplay.processCommands`).
    #[func]
    pub fn logic_display_sync(
        &mut self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        if self.display_icon_names.is_none() {
            self.display_icon_names = self
                .sim
                .block_runtime()
                .map(|runtime| crate::display_context::IconNames::build(runtime.content()));
        }
        let assets = self
            .base()
            .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets");
        let Some(names) = self.display_icon_names.as_ref() else {
            return out;
        };
        let ctx = crate::display_context::AtlasIcons::new(assets, names);
        let rows = mind_core::logic::display_raster::display_rows(
            // Field path: `ctx` borrows `self.display_icon_names`.
            &mut self.sim.ecs.0,
            &ctx,
            min_x,
            min_y,
            max_x,
            max_y,
        );
        for row in rows {
            let mut dict = Dictionary::<GString, Variant>::new();
            dict.set("root_x", &i64::from(row.root_x).to_variant());
            dict.set("root_y", &i64::from(row.root_y).to_variant());
            dict.set("draw_buffer", &row.draw_buffer.to_variant());
            dict.set("x", &row.x.to_variant());
            dict.set("y", &row.y.to_variant());
            dict.set("w", &row.w.to_variant());
            dict.set("h", &row.h.to_variant());
            dict.set("frame", &row.frame.to_variant());
            dict.set("frame_x", &row.frame_x.to_variant());
            dict.set("frame_y", &row.frame_y.to_variant());
            dict.set("bits", &i64::from(row.bits).to_variant());
            dict.set("sprite", &i64::from(row.sprite).to_variant());
            out.push(&dict.to_variant());
        }
        out
    }

    /// Canvas draw rows for the render pass (T14).
    ///
    /// Rows: `{x, y, cx, cy, size, blending, invalidated}`.
    #[func]
    pub fn logic_canvas_sync(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for row in
            mind_core::logic::blocks::canvas::canvas_rows(self.world(), min_x, min_y, max_x, max_y)
        {
            let mut dict = Dictionary::<GString, Variant>::new();
            dict.set("x", &i64::from(row.x).to_variant());
            dict.set("y", &i64::from(row.y).to_variant());
            dict.set("cx", &row.cx.to_variant());
            dict.set("cy", &row.cy.to_variant());
            dict.set("size", &row.size.to_variant());
            dict.set("blending", &i64::from(row.blending).to_variant());
            dict.set("pixels", &i64::from(row.pixels).to_variant());
            dict.set("bpp", &i64::from(row.bits_per_pixel).to_variant());
            let mut palette = PackedInt64Array::new();
            for color in &row.palette {
                palette.push(i64::from(*color));
            }
            dict.set("palette", &palette.to_variant());
            dict.set("invalidated", &row.invalidated.to_variant());
            out.push(&dict.to_variant());
        }
        out
    }

    /// RGBA8 canvas pixels (bottom-row-first) for the canvas at `(x, y)`.
    ///
    /// Clears the canvas's `invalidated` flag (the view uploaded it).
    #[func]
    pub fn logic_canvas_image(&mut self, x: i64, y: i64) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let palette = mind_core::logic::blocks::canvas_def_of(self.world(), e)
            .map(|def| def.palette)
            .unwrap_or_default();
        let Some(mut state) = self
            .world_mut()
            .get_mut::<mind_core::logic::blocks::CanvasBlockState>(e)
        else {
            return out;
        };
        let size = state.canvas_size.max(0);
        let pixels = size * size;
        let mut bytes: Vec<u8> = Vec::with_capacity((pixels * 4) as usize);
        for row in (0..size).rev() {
            for col in 0..size {
                let pixel = state.pixel_rgba(row * size + col, &palette);
                bytes.push(((pixel >> 24) & 0xff) as u8);
                bytes.push(((pixel >> 16) & 0xff) as u8);
                bytes.push(((pixel >> 8) & 0xff) as u8);
                bytes.push((pixel & 0xff) as u8);
            }
        }
        state.invalidated = false;
        out.extend(bytes.iter().copied());
        out
    }

    /// Palette indices of the canvas at `(x, y)`, one byte per pixel
    /// (linear `pos = y * size + x`, `CanvasBuild.getPixel`).
    #[func]
    pub fn logic_canvas_pixels(&self, x: i64, y: i64) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let Some(state) = self
            .world()
            .get::<mind_core::logic::blocks::CanvasBlockState>(e)
        else {
            return out;
        };
        let pixels = state.canvas_size.max(0) * state.canvas_size.max(0);
        for pos in 0..pixels {
            let index = state.get_byte(pos * i32::from(state.bits_per_pixel));
            out.push((index.clamp(0, 255)) as u8);
        }
        out
    }

    /// RGBA8 bytes (row 0 = top) of the root raster at `(root_x, root_y)`.
    #[func]
    pub fn logic_display_image(&self, root_x: i64, root_y: i64) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(root_x, root_y) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let Some(raster) = self
            .world()
            .get_resource::<mind_core::logic::display_raster::LogicDisplayRasters>()
            .and_then(|rasters| rasters.rasters.get(&e))
        else {
            return out;
        };
        // The raster is stored top-row-first, but `canvas::draw_region` flips
        // every region (negative height), so the upload is bottom-row-first:
        // display y = 0 (the Java buffer's origin) ends up at the bottom.
        let mut bytes: Vec<u8> = Vec::with_capacity(raster.pixels.len() * 4);
        let width = raster.width as usize;
        let height = raster.height as usize;
        for row in (0..height).rev() {
            for col in 0..width {
                let Some(pixel) = raster.pixels.get(row * width + col) else {
                    continue;
                };
                bytes.push(((pixel >> 24) & 0xff) as u8);
                bytes.push(((pixel >> 16) & 0xff) as u8);
                bytes.push(((pixel >> 8) & 0xff) as u8);
                bytes.push((pixel & 0xff) as u8);
            }
        }
        out.extend(bytes.iter().copied());
        out
    }

    /// `LogicBuild.showEditDialog` -> `configure(bytes)` (I03 T7).
    ///
    /// Compresses the editor text with `relativeConnections`, returns
    /// `"toolong:<len>"` past `maxCompressedLen`, and otherwise queues a
    /// `Bytes` configure as the player (the campaign configure gate applies,
    /// unlike the `logic_set_code` test seam).
    #[func]
    pub fn logic_configure_code(&mut self, x: i64, y: i64, code: GString) -> GString {
        self.logic_last_error = String::new();
        let code = code.to_string();
        let world = self.world();
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            self.logic_last_error = "noprocessor".to_owned();
            return GString::from("noprocessor");
        };
        let Some(links) = world
            .get::<mind_core::logic::blocks::LogicBlockState>(e)
            .map(|state| state.relative_connections(world, e))
        else {
            self.logic_last_error = "noprocessor".to_owned();
            return GString::from("noprocessor");
        };
        match mind_core::logic::blocks::logic_block::encode_for_config(&code, &links) {
            Err(len) => {
                self.logic_last_error = format!("toolong:{len}");
                GString::from(format!("toolong:{len}").as_str())
            }
            Ok(bytes) => {
                let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
                    return GString::from("noprocessor");
                };
                self.pending_commands.push_back(SimCommand::Configure {
                    x: tx,
                    y: ty,
                    value: mind_core::determinism::ConfigValue::Bytes(bytes),
                });
                GString::new()
            }
        }
    }

    /// Runs a registered logic bench case (plan 13 §7d; not yet registered).
    #[func]
    pub fn logic_bench(&self, name: GString) -> Dictionary<GString, Variant> {
        log::warn!("logic_bench(`{name}`): no bench registered yet");
        Dictionary::new()
    }

    /// Dev `setrule` probe (awaits the plan-12 `LogicWorldState` resource).
    #[func]
    pub fn logic_set_rule(&self, name: GString, _value: Variant) -> bool {
        log::warn!("logic_set_rule(`{name}`): timeline/sim rule state is a plan-12 hook");
        false
    }

    /// G06 T4 verification seam: live campaign rules relevant to world
    /// processors (`waveSending`/`waveTimer`/`mapArea`/flags/wave).
    #[func]
    pub fn logic_rules_probe(&self) -> GString {
        let Some(runtime) = self.campaign_runtime() else {
            return crate::gdutil::json::empty_object();
        };
        let rules = &runtime.session.rules;
        let logic_rules = mind_core::logic::blocks::rules_ref(self.world());
        let value = serde_json::json!({
            // I03 T9: the add dialog hides the unit category when the rule is off
            // (`LogicDialog.showAddDialog`).
            "logicUnitControl": logic_rules.logic_unit_control(),
            "logicUnitBuild": logic_rules.logic_unit_build(),
            "waveSending": rules.wave_sending,
            "waveTimer": rules.wave_timer,
            "waves": rules.waves,
            "wave": runtime.session.wave,
            "wavetime": runtime.session.wavetime,
            "limitMapArea": rules.limit_map_area,
            "limitX": rules.limit_x,
            "limitY": rules.limit_y,
            "limitWidth": rules.limit_width,
            "limitHeight": rules.limit_height,
            "unitCap": rules.unit_cap,
            "flags": rules.objective_flags.iter().cloned().collect::<Vec<_>>(),
        });
        crate::gdutil::json::value_gstring(&value)
    }

    /// G06 T4 verification-only seam: set/clear an objective flag (what the
    /// Ground Zero tutorial objectives raise).
    #[func]
    pub fn logic_set_flag(&mut self, name: GString, on: bool) -> bool {
        let Some(mut runtime) = self.campaign_runtime_mut() else {
            return false;
        };
        let name = name.to_string();
        if on {
            runtime.session.rules.objective_flags.insert(name);
        } else {
            runtime.session.rules.objective_flags.shift_remove(&name);
        }
        true
    }
}

// ---------------------------------------------------------------------------
// GAPS #26 host half — the per-frame `LAccess` player-view push.
// ---------------------------------------------------------------------------

/// Node path of the HUD ping marker (`ui/fragments/hud.rs`
/// `MindPingOverlay`): the port's `PlayerComp.pingX/pingY/pingTime/pingText`
/// state (`PlayerComp.drawPing`).
const PING_OVERLAY_PATH: &str = "/root/Spine/Ui/UiRoot/HudGroup/hud/PingOverlay";

/// Composes the [`LogicPlayerView`] the host pushes each frame.
///
/// Java reads the same state off the local `Player`:
/// `cameraX..cameraHeight` from `Core.camera.position/width/height`
/// (`UnitComp.java:287-290`; sent by `NetClient.java:766-780` as
/// `con.viewX/viewY/viewWidth/viewHeight`), `selectedBlock`/`selectedRotation`
/// from the input handler (`InputHandler.java:998-999`), and
/// `pingX/pingY/pingText` from the ping marker
/// (`InputHandler.java:286-302`). Kept Godot-free so the mapping is testable.
pub fn compose_logic_player_view(
    camera: (f32, f32, f32, f32),
    selected_block: BlockId,
    selected_rotation: u8,
    ping: Option<(f32, f32, Option<String>)>,
) -> mind_core::logic::globals::LogicPlayerView {
    // `player.selectedBlock` is nullable; the port uses `BlockId::AIR` for "no
    // block selected" (`input/mod.rs::sync_selected_block`).
    let selected = if selected_block == BlockId::AIR {
        None
    } else {
        Some(selected_block)
    };
    let (ping_pos, ping_text) = match ping {
        Some((x, y, text)) => (Some((x, y)), text.filter(|text| !text.trim().is_empty())),
        None => (None, None),
    };
    mind_core::logic::globals::LogicPlayerView {
        camera_x: camera.0,
        camera_y: camera.1,
        camera_w: camera.2,
        camera_h: camera.3,
        selected_block: selected,
        // `Sim::set_selected_rotation` clamps to 0..4; the view push does it
        // here because the whole resource is replaced.
        selected_rotation: selected_rotation.rem_euclid(4),
        ping: ping_pos,
        ping_text,
    }
}

impl MindSimHost {
    /// `Core.camera.position/width/height` in world pixels (the Java values
    /// `NetClient.clientSnapshot` uploads; `NetClient.java:768-769`).
    pub(super) fn camera_view_bounds(&self) -> (f32, f32, f32, f32) {
        self.camera
            .as_ref()
            .map(|camera| camera.bind().logic_view_bounds())
            .unwrap_or((0.0, 0.0, 0.0, 0.0))
    }

    /// Live ping marker state from the HUD overlay
    /// (`PlayerComp.pingX/pingY/pingText`, cleared when `pingTime` runs out).
    pub(super) fn ping_state(&self) -> Option<(f32, f32, Option<String>)> {
        let mut overlay = self
            .base()
            .try_get_node_as::<godot::classes::Node>(PING_OVERLAY_PATH)?;
        if !overlay.has_method("ping_state") {
            return None;
        }
        let state = overlay
            .call("ping_state", &[])
            .try_to::<VarDictionary>()
            .ok()?;
        let visible = state
            .get("visible")
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false);
        let time = state
            .get("time")
            .and_then(|value| value.try_to::<f32>().ok())
            .unwrap_or(0.0);
        if !visible || time <= 0.0 {
            // `Player.isPinging()` is false once `pingTime` reaches 0.
            return None;
        }
        let x = state
            .get("x")
            .and_then(|value| value.try_to::<f32>().ok())?;
        let y = state
            .get("y")
            .and_then(|value| value.try_to::<f32>().ok())?;
        let text = state
            .get("text")
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|text| text.to_string());
        Some((x, y, text))
    }

    /// Replacement `LAccess` player view built from the live camera, input
    /// selection and ping marker.
    pub(super) fn logic_player_view(&self) -> mind_core::logic::globals::LogicPlayerView {
        let rotation = self
            .input
            .as_ref()
            .map(|input| input.bind().placement_view().rotation)
            .unwrap_or(0);
        compose_logic_player_view(
            self.camera_view_bounds(),
            self.sim.selected_block(),
            rotation,
            self.ping_state(),
        )
    }

    /// GAPS #26 host half: pushes camera/selection/ping into the sim each frame
    /// so the `LAccess` senses match Java (`UnitComp.java:287-311`).
    ///
    /// The whole resource is replaced, mirroring
    /// `NetServer.clientSnapshot`'s `con.viewX = ...` writes; the camera and
    /// selection halves were previously never uploaded, so every sense read
    /// the core defaults (`0`/`NaN`/`null`).
    pub(super) fn push_logic_player_view(&mut self) {
        let view = self.logic_player_view();
        self.sim.set_logic_player_view(view);
    }
}

#[cfg(test)]
mod logic_view_tests {
    use super::compose_logic_player_view;
    use mind_core::content::BlockId;

    #[test]
    fn compose_maps_camera_selection_and_ping() {
        let view = compose_logic_player_view(
            (160.0, 96.0, 320.0, 180.0),
            BlockId::new(42),
            2,
            Some((80.0, 96.0, Some("here".to_owned()))),
        );
        assert_eq!(view.camera_x, 160.0);
        assert_eq!(view.camera_y, 96.0);
        assert_eq!(view.camera_w, 320.0);
        assert_eq!(view.camera_h, 180.0);
        assert_eq!(view.selected_block, Some(BlockId::new(42)));
        assert_eq!(view.selected_rotation, 2);
        assert_eq!(view.ping, Some((80.0, 96.0)));
        assert_eq!(view.ping_text.as_deref(), Some("here"));
        assert!(view.is_pinging());
    }

    #[test]
    fn compose_uses_air_for_no_selection_and_clears_empty_ping_text() {
        let view = compose_logic_player_view(
            (0.0, 0.0, 64.0, 32.0),
            BlockId::AIR,
            7,
            Some((8.0, 16.0, Some("   ".to_owned()))),
        );
        assert_eq!(view.selected_block, None, "AIR is Java's null selection");
        assert_eq!(view.selected_rotation, 3, "rotation wraps into 0..4");
        assert_eq!(view.ping, Some((8.0, 16.0)), "a plain ping keeps its point");
        assert_eq!(view.ping_text, None, "an empty ping label is null");
        assert!(view.is_pinging());
    }

    #[test]
    fn compose_without_ping_reads_not_pinging() {
        let view = compose_logic_player_view((1.0, 2.0, 3.0, 4.0), BlockId::new(5), 0, None);
        assert_eq!(view.ping, None);
        assert_eq!(view.ping_text, None);
        assert!(!view.is_pinging());
    }
}
