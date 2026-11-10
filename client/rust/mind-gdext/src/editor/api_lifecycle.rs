// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map lifecycle: new/resize/load/save requests and the palette refresh.
//! Upstream: core/src/mindustry/editor/MapEditorDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- lifecycle (M1 model; M3 facade) ---

    /// `begin_new` / `beginEditSize`: a fresh stone-floored `width x height` map.
    #[func]
    pub fn begin_new(&mut self, width: i32, height: i32) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let width = width.clamp(1, 16_383);
        let height = height.clamp(1, 16_383);
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        self.editor
            .begin_edit_size(&mut world, content, width, height);
        self.editor.adopt_world(&mut world);
        self.file = None;
        self.clear_error();
        true
    }

    /// `begin_edit_map`: loads a `.msav` into the editor.
    #[func]
    pub fn begin_edit_map(&mut self, file: GString) -> bool {
        self.mark_tiles_changed();
        let path = Self::native_path(&file.to_string());
        let fs = NativeFs;
        let header = match MapIo::create_map(&fs, &path, true) {
            Ok(header) => header,
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        let map = Map::from_header(&header, true);
        let Some(content) = self.content.as_mut() else {
            return false;
        };
        match self
            .editor
            .begin_edit_map(&mut self.grid, content, &fs, &map)
        {
            Ok(()) => {
                // `SaveVersion.readDataPatches` -> `state.data.load(assets)`.
                // A map without a `patches` region still replaces (unloads) the
                // previous set, exactly like upstream.
                let assets = self.editor.take_patches();
                if let Some(content) = self.content.as_mut()
                    && let Some(mut mods) = crate::content_boot::mods_node()
                    && let Err(error) = mods.bind_mut().load_map_data(assets, content)
                {
                    log::warn!("[editor] map data assets failed: {error}");
                }
                self.file = Some(path);
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_map_error(&error);
                false
            }
        }
    }

    /// `begin_edit_image`: imports a color-mapped PNG map (≤800 per axis).
    #[func]
    pub fn begin_edit_image(&mut self, file: GString) -> bool {
        self.mark_tiles_changed();
        let path = Self::native_path(&file.to_string());
        let fs = NativeFs;
        let bytes = match fs.read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        let image = match mind_core::io::map::decode_png(&bytes) {
            Ok(image) => image,
            Err(error) => {
                self.last_error_key = Some("@editor.errorimage".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        if image.width > 800 || image.height > 800 {
            self.last_error_key = Some("@editor.errorimage".to_owned());
            self.last_error_message = Some("image map exceeds 800x800".to_owned());
            return false;
        }
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        match self
            .editor
            .begin_edit_image(&mut self.grid, content, &image)
        {
            Ok(()) => {
                self.file = Some(path);
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_map_error(&error);
                false
            }
        }
    }

    /// `MapResizeDialog` OK: resize + shift (clears the op stack).
    #[func]
    pub fn resize_map(&mut self, width: i32, height: i32, shift_x: i32, shift_y: i32) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let width = width.clamp(1, 16_383);
        let height = height.clamp(1, 16_383);
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        self.editor
            .resize(&mut world, content, width, height, shift_x, shift_y);
        true
    }
}
