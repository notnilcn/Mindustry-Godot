// SPDX-License-Identifier: GPL-3.0-only

//! Save/load request plumbing over the Godot filesystem.
//! Upstream: core/src/mindustry/io/IO.java (save/load requests)

use super::*;

impl MindSimHost {
    /// Runs queued save/load requests at their `IoSet` boundaries without
    /// advancing the sim (the exit-save flush while the pump is paused).
    pub fn flush_io(&mut self) {
        self.sim.flush_io();
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    // ---- IoSet seam (plan 05 M9 / plan 04 §3.10; SimIoHandler wired in ready) ----
    //
    // `ready()` registers `mind_core::io::SimIoHandler`, so a queued save/load is
    // fulfilled at the tick boundary against the live grid (Godot `user://` /
    // `res://` paths are globalized to native paths for plan-04 `NativeFs`). The
    // plan-04 `MindIo` gdext autoload is a thin client-side wrapper; the in-engine
    // MCP round-trip itself remains orchestrator-owned (single-editor mutex).
    // Copy-pasteable MCP evals (do NOT launch the editor from this lane):
    //
    //   godot_exec eval: "var h=Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost'); print('MCP_TICK=',h.get_tick(),' STATE=',h.get_state(),' UP=',h.get_update_id())"
    //   godot_exec eval: "print('MCP_GROUPS=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').get_group_counts())"
    //   godot_exec eval: "Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').request_save('user://mcp.msav', false); print('MCP_PENDING=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').io_pending())"

    /// Queues a save at the next `IoSet::Capture` boundary.
    #[func]
    pub fn request_save(&mut self, path: GString, as_map: bool) {
        self.sim.request_save(globalize(&path), as_map);
    }

    /// `Saves.SaveSlot.savePreview`: writes `<user>/previews/save_slot_<name>.png`
    /// from the live grid (`Saves.java:343-375`). Returns whether the PNG landed.
    #[func]
    pub fn write_save_preview(&mut self, name: GString) -> bool {
        use mind_core::io::fs::{FileSystem, NativeFs, Paths};
        use mind_core::io::map::{BlockPalette, encode_png, generate_preview_from_tiles};
        use mind_core::world::map_source::EcsMapSource;

        let paths = Paths::new(crate::assets::loader::user_data_root());
        let Some(registry) = self.content_snapshot.as_ref() else {
            log::warn!("write_save_preview: no content registry");
            return false;
        };
        // `Saves.savePreview` writes `renderer.minimap.getPixmap()`: the real
        // per-block colours (`ContentLoader.loadColors`).
        let mut palette = BlockPalette::of(registry);
        palette.apply_block_colors(&crate::render::minimap::load_block_colors());
        let source = EcsMapSource::new(&self.sim.grid, registry, &self.sim.ecs.0);
        let image = generate_preview_from_tiles(&palette, &source);
        let Ok(bytes) = encode_png(&image) else {
            log::warn!("write_save_preview: PNG encode failed");
            return false;
        };
        match NativeFs.write(&paths.preview_for_save(name.to_string().as_str()), &bytes) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("write_save_preview: {error}");
                false
            }
        }
    }

    /// Queues a load at the next `IoSet::Apply` boundary.
    #[func]
    pub fn request_load(&mut self, path: GString) {
        self.sim.request_load(globalize(&path));
    }

    /// Number of queued IO requests.
    #[func]
    pub fn io_pending(&self) -> i64 {
        self.sim.io_pending() as i64
    }

    /// Whether a *save* is queued (`control.saves.isSaving()`; G03 T9 shows
    /// the small `@saving` label instead of the full loading overlay).
    #[func]
    pub fn io_saving(&self) -> bool {
        self.sim.io_saving()
    }

    /// Stores host playtime (`Saves.getTotalPlaytime`) for the next save's meta
    /// region (`SaveVersion.writeMeta`).
    #[func]
    pub fn set_save_playtime(&mut self, millis: i64) {
        self.sim.set_save_playtime(millis);
    }

    /// Drains queued IO requests as JSON (`[{kind,path,as_map?}]`) for `MindIo`.
    #[func]
    pub fn take_io_requests_json(&mut self) -> GString {
        let value: Vec<serde_json::Value> = self
            .sim
            .take_io_requests()
            .iter()
            .map(|request| match request {
                mind_core::sim::IoRequest::Save { path, as_map } => serde_json::json!({
                    "kind": "save",
                    "path": path.display().to_string(),
                    "as_map": as_map,
                }),
                mind_core::sim::IoRequest::Load { path } => serde_json::json!({
                    "kind": "load",
                    "path": path.display().to_string(),
                }),
            })
            .collect();
        GString::from(
            serde_json::to_string(&value)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// Drains queued IO responses as JSON (`[{kind,path,as_map?,error?}]`), so
    /// save/load failures are host-visible (`take_io_requests_json` response
    /// half; the loading overlay only polls `io_pending`).
    #[func]
    pub fn take_io_responses_json(&mut self) -> GString {
        let value: Vec<serde_json::Value> = self
            .sim
            .take_io_responses()
            .iter()
            .map(|response| {
                let mut row = match &response.request {
                    mind_core::sim::IoRequest::Save { path, as_map } => serde_json::json!({
                        "kind": "save",
                        "path": path.display().to_string(),
                        "as_map": as_map,
                    }),
                    mind_core::sim::IoRequest::Load { path } => serde_json::json!({
                        "kind": "load",
                        "path": path.display().to_string(),
                    }),
                };
                if let mind_core::sim::IoStatus::Failed(error) = &response.status {
                    row["error"] = serde_json::Value::String(error.clone());
                }
                row
            })
            .collect();
        GString::from(
            serde_json::to_string(&value)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// Delivers a `MindIo` result as JSON (`{kind,path,as_map?,error?}`).
    #[func]
    pub fn deliver_io_response_json(&mut self, json: GString) -> bool {
        let text = json.to_string();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            log::warn!("deliver_io_response_json: invalid JSON");
            return false;
        };
        let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            log::warn!("deliver_io_response_json: missing path");
            return false;
        }
        let request = match kind {
            "save" => mind_core::sim::IoRequest::Save {
                path: path.into(),
                as_map: value
                    .get("as_map")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            },
            "load" => mind_core::sim::IoRequest::Load { path: path.into() },
            other => {
                log::warn!("deliver_io_response_json: unknown kind `{other}`");
                return false;
            }
        };
        let status = match value.get("error").and_then(|v| v.as_str()) {
            Some(error) => mind_core::sim::IoStatus::Failed(error.to_owned()),
            None => mind_core::sim::IoStatus::Ok,
        };
        self.sim
            .deliver_io_response(mind_core::sim::IoResponse { request, status });
        true
    }
}
