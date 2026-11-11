// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Data patches, content/assets management and map/image export.
//! Upstream: core/src/mindustry/editor/MapAssetsDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    /// `MapAssetsDialog.assets()`: the loaded data-asset records.
    ///
    /// Rows come from the `MindMods` owner (`state.data`, PAR-0394): `kind`,
    /// `name`, `path`, `data`, image region/probe fields, audio id/streaming.
    #[func]
    pub fn assets(&self) -> Array<VarDictionary> {
        match crate::content_boot::mods_node() {
            Some(mods) => mods.bind().data_assets(),
            None => Array::new(),
        }
    }

    /// Content parse/load errors from the last data reload.
    #[func]
    pub fn data_errors(&self) -> PackedStringArray {
        match crate::content_boot::mods_node() {
            Some(mods) => mods.bind().data_errors(),
            None => PackedStringArray::new(),
        }
    }

    /// Number of `dp-`/`net-` atlas regions the data decorator registered.
    #[func]
    pub fn data_region_count(&self) -> i64 {
        match crate::content_boot::mods_node() {
            Some(mods) => mods.bind().data_region_count(),
            None => 0,
        }
    }

    /// Runs one data-owner action against the editor registry and publishes
    /// the decorator result (`pub: bool` success flag).
    pub(super) fn with_data<R>(
        &mut self,
        f: impl FnOnce(&mut crate::mods::MindMods, &mut ContentRegistry) -> R,
    ) -> Option<R> {
        let mut mods = crate::content_boot::mods_node()?;
        let content = self.content.as_mut()?;
        let mut guard = mods.bind_mut();
        let result = f(&mut guard, content);
        guard.publish_data_assets();
        Some(result)
    }

    /// Wraps a data-owner result (`Result<(), String>`) as `{ok, error}`.
    pub(super) fn data_result(result: Result<(), String>) -> VarDictionary {
        match result {
            Ok(()) => dict(vec![
                ("ok", true.to_variant()),
                ("error", GString::new().to_variant()),
            ]),
            Err(error) => dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from(error.as_str()).to_variant()),
            ]),
        }
    }

    /// `MapPatchesView.addPatch`: validates JSON, appends a record and re-applies.
    #[func]
    pub fn data_patch_add(&mut self, json: GString) -> VarDictionary {
        let text = json.to_string();
        if serde_json::from_str::<serde_json::Value>(&text).is_err() {
            return Self::data_result(Err(String::from("invalid JSON")));
        }
        let result = self.with_data(|mods, content| {
            let mut records: Vec<PatchAsset> = mods.data().patch_records();
            records.push(PatchAsset {
                name: format!("patches/patch-{}.json", records.len()),
                json: text,
            });
            mods.data_mut()
                .replace_patch_records(&records, content)
                .map_err(|error| error.to_string())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapPatchesView.addPatch(str, index)`: replaces one patch's JSON.
    #[func]
    pub fn data_patch_replace(&mut self, index: i64, json: GString) -> VarDictionary {
        let text = json.to_string();
        if serde_json::from_str::<serde_json::Value>(&text).is_err() {
            return Self::data_result(Err(String::from("invalid JSON")));
        }
        let result = self.with_data(|mods, content| {
            mods.data_mut()
                .set_patch_record(index.max(0) as usize, Some(text), content)
                .map_err(|error| error.to_string())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapPatchesView` delete: removes one patch and re-applies the rest.
    #[func]
    pub fn data_patch_remove(&mut self, index: i64) -> VarDictionary {
        let result = self.with_data(|mods, content| {
            mods.data_mut()
                .set_patch_record(index.max(0) as usize, None, content)
                .map_err(|error| error.to_string())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapContentView.addContent`: registers one content asset and reloads.
    #[func]
    pub fn data_content_add(
        &mut self,
        name: GString,
        kind: GString,
        json: GString,
    ) -> VarDictionary {
        let name = name.to_string();
        if name.trim().is_empty()
            || name.contains([
                '[', ']', '{', '}', '`', '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', ';',
                ':', ',',
            ])
            || name.contains(char::is_whitespace)
        {
            return Self::data_result(Err(String::from("badname")));
        }
        let Some(type_) = loadable_content_type(&kind.to_string()) else {
            return Self::data_result(Err(String::from("badtype")));
        };
        let json = json.to_string();
        if serde_json::from_str::<serde_json::Value>(&json).is_err() {
            return Self::data_result(Err(String::from("invalid JSON")));
        }
        let result = self.with_data(|mods, content| {
            let path = format!("content/{}/{name}.json", type_.folder());
            if mods
                .data()
                .has_asset_name_or_path(DataAssetType::Content, &name, &path)
            {
                return Err(String::from("exists"));
            }
            mods.data_mut()
                .push(mind_core::mods::assets::DataAsset::content(
                    path,
                    type_,
                    json.clone(),
                ));
            mods.data_mut()
                .refresh_all(content)
                .map_err(|error| error.to_string())?;
            mods.data_mut().regenerate_content_sprites(false);
            Ok(())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapContentView`/`MapImagesView` delete: removes a record and reloads.
    #[func]
    pub fn data_asset_delete(&mut self, kind: GString, name: GString) -> VarDictionary {
        let Some(type_) = DataAssetType::from_kind(&kind.to_string()) else {
            return Self::data_result(Err(String::from("badtype")));
        };
        let name = name.to_string();
        let result = self.with_data(|mods, content| {
            if mods.data_mut().remove_asset(type_, &name).is_none() {
                return Err(String::from("missing"));
            }
            mods.data_mut()
                .refresh_all(content)
                .map_err(|error| error.to_string())?;
            mods.data_mut().regenerate_content_sprites(false);
            Ok(())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapContentView` refresh: reloads content + regenerates sprites
    /// (`state.data.reloadContent(false); regenerateContentSprites(force)`).
    #[func]
    pub fn data_reload_content(&mut self, force_pack: bool) -> VarDictionary {
        let result = self.with_data(|mods, content| {
            mods.data_mut()
                .refresh_all(content)
                .map_err(|error| error.to_string())?;
            mods.data_mut().regenerate_content_sprites(force_pack);
            Ok(())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapBundlesView` edit: replaces/creates one bundle record from text.
    #[func]
    pub fn data_bundle_set(&mut self, path: GString, text: GString) -> VarDictionary {
        let path = path.to_string();
        if path.trim().is_empty() {
            return Self::data_result(Err(String::from("badpath")));
        }
        let result = self.with_data(|mods, _content| {
            mods.data_mut().remove_asset(DataAssetType::Bundle, &path);
            mods.data_mut()
                .push(mind_core::mods::assets::DataAsset::bundle(
                    path.clone(),
                    &text.to_string(),
                ));
            mods.data_mut().reload_bundles();
            Ok(())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapBundlesView`/`MapAudioView`/`MapImagesView` add: registers one file
    /// by native path (`FileChooser.submit`).
    #[func]
    pub fn data_file_add(&mut self, kind: GString, path: GString) -> VarDictionary {
        let Some(type_) = DataAssetType::from_kind(&kind.to_string()) else {
            return Self::data_result(Err(String::from("badtype")));
        };
        let file = Self::native_path(&path.to_string());
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => return Self::data_result(Err(error.to_string())),
        };
        let name = file
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = self.with_data(|mods, content| {
            let asset = match type_ {
                DataAssetType::Bundle => {
                    let Ok(text) = String::from_utf8(bytes.clone()) else {
                        return Err(String::from("bundle is not UTF-8"));
                    };
                    mind_core::mods::assets::DataAsset::bundle(name.clone(), &text)
                }
                _ => mind_core::mods::assets::DataAsset::blob(
                    format!("{}/{}", type_.folder(), name),
                    type_,
                    bytes.clone(),
                    false,
                ),
            };
            if mods
                .data()
                .has_asset_name_or_path(type_, &asset.name, &asset.path)
            {
                return Err(String::from("exists"));
            }
            mods.data_mut().push(asset);
            mods.data_mut()
                .refresh_all(content)
                .map_err(|error| error.to_string())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapAssetsDialog.showEditMenu` -> `@asset.importzip`: merges one zip's
    /// assets (existing paths are skipped, Java logs them as errors).
    #[func]
    pub fn data_import_zip(&mut self, path: GString) -> VarDictionary {
        let file = Self::native_path(&path.to_string());
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => return Self::data_result(Err(error.to_string())),
        };
        let result = self.with_data(|mods, content| {
            mods.data_mut().clear_generated_images();
            let imported =
                mind_core::editor::assets::import_zip(&bytes).map_err(|error| error.to_string())?;
            for asset in imported {
                if mods
                    .data()
                    .has_asset_name_or_path(asset.type_, &asset.name, &asset.path)
                {
                    continue;
                }
                mods.data_mut().push(asset);
            }
            mods.data_mut()
                .refresh_all(content)
                .map_err(|error| error.to_string())?;
            if !mods.data().assets_of(DataAssetType::Content).is_empty() {
                mods.data_mut().regenerate_content_sprites(false);
            }
            Ok(())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `@asset.exportzip`: writes every record's payload to one zip.
    #[func]
    pub fn data_export_zip(&self, path: GString) -> bool {
        let Some(mods) = crate::content_boot::mods_node() else {
            return false;
        };
        let assets = mods.bind().data().all().to_vec();
        let Ok(bytes) = mind_core::editor::assets::export_zip(&assets) else {
            return false;
        };
        std::fs::write(Self::native_path(&path.to_string()), bytes).is_ok()
    }

    /// `addCloseButton` equivalent of `asset.clearall`: unloads the record set.
    #[func]
    pub fn data_clear(&mut self) -> VarDictionary {
        let result = self.with_data(|mods, content| {
            mods.data_mut().unload_assets(content);
            Ok(())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapImagesView` pencil: renames one image record (path + `dp-` region).
    #[func]
    pub fn data_image_rename(&mut self, old_path: GString, new_path: GString) -> VarDictionary {
        let old_path = old_path.to_string();
        let mut new_path = new_path.to_string();
        if !new_path.ends_with(".png") {
            new_path.push_str(".png");
        }
        let result = self.with_data(|mods, content| {
            if !mods
                .data_mut()
                .rename_asset(DataAssetType::Image, &old_path, &new_path)
            {
                return Err(String::from("exists"));
            }
            mods.data_mut()
                .refresh_all(content)
                .map_err(|error| error.to_string())
        });
        Self::data_result(result.unwrap_or_else(|| Err(String::from("no data owner"))))
    }

    /// `MapImagesView`/`MapBundlesView`/`MapAudioView` export: writes one
    /// asset's payload (blob bytes or embedded text) to a native path.
    #[func]
    pub fn data_asset_export(&self, kind: GString, name: GString, path: GString) -> bool {
        let Some(type_) = DataAssetType::from_kind(&kind.to_string()) else {
            return false;
        };
        let Some(mods) = crate::content_boot::mods_node() else {
            return false;
        };
        let guard = mods.bind();
        let name = name.to_string();
        let bytes = guard.data().all().iter().find_map(|asset| {
            if asset.type_ != type_ || (asset.name != name && asset.path != name) {
                return None;
            }
            match &asset.data {
                DataAssetData::Blob(bytes) => Some(bytes.clone()),
                DataAssetData::Patch(text) => Some(text.clone().into_bytes()),
                DataAssetData::Content(record) => Some(record.json.clone().into_bytes()),
                DataAssetData::Bundle(map) => {
                    Some(mind_core::mods::assets::render_properties(map).into_bytes())
                }
            }
        });
        let target = Self::native_path(&path.to_string());
        match bytes {
            Some(bytes) => std::fs::write(target, bytes).is_ok(),
            None => false,
        }
    }

    /// `MapEditorDialog.export`: write the map with embedded data assets.
    #[func]
    pub fn export_map(&mut self, path: GString) -> bool {
        let Some(content) = self.content() else {
            return false;
        };
        let name = self.editor.tags.get("name").cloned().unwrap_or_default();
        let file = Self::native_path(&path.to_string());
        let base = editor_base_tags(
            self.grid.tiles.width as u16,
            self.grid.tiles.height as u16,
            &name,
        );
        let map_tags = self.editor.tags.clone();
        let result = data_patches(|patches| {
            save_editor_map_with_ecs_and_patches(
                &NativeFs,
                &file,
                &self.grid,
                content,
                &self.ecs.0,
                base,
                map_tags,
                true,
                patches,
            )
        });
        match result {
            Ok(()) => {
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_map_error(&error);
                false
            }
        }
    }

    /// `MapEditorDialog.exportImage`: write the color-mapped PNG.
    #[func]
    pub fn export_image(&mut self, path: GString) -> bool {
        let Some(content) = self.content() else {
            return false;
        };
        let file = Self::native_path(&path.to_string());
        let source = mind_core::editor::maps_glue::EditorMapSource::new(&self.grid, content);
        let image = self.editor.export_image(content, &source);
        match mind_core::io::map::encode_png(&image) {
            Ok(bytes) => match NativeFs.write(&file, &bytes) {
                Ok(()) => {
                    self.clear_error();
                    true
                }
                Err(error) => {
                    self.set_load_error(error);
                    false
                }
            },
            Err(error) => {
                self.set_load_error(error);
                false
            }
        }
    }

    /// `Maps.saveMap` shell: resolves the target file through the map registry
    /// (`Maps.java:206-221`: a same-name map's file is reused, otherwise
    /// `findFile` sanitizes and probes `_N`) and writes it there.
    ///
    /// M3 keeps the tile+tag save path; the M7 rules/objectives reset appends in
    /// its milestone.
    #[func]
    pub fn save(&mut self) -> VarDictionary {
        let Some(content) = self.content() else {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("no content").to_variant()),
            ]);
        };
        let name = self.editor.tags.get("name").cloned().unwrap_or_default();
        if name.trim().is_empty() {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("noname").to_variant()),
            ]);
        }
        let root = crate::assets::loader::user_data_root();
        let dir = mind_core::io::fs::Paths::new(&root).maps();
        // `MapEditorDialog.save` -> `handleSaveBuiltin`: a built-in map is
        // never overwritten (`ui.showErrorMessage("@editor.save.overwrite")`,
        // `MapEditorDialog.java:442-445`).
        if let Some(current) = self.file.as_ref()
            && !current.starts_with(&dir)
        {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("overwrite").to_variant()),
            ]);
        }
        let mut maps = crate::assets::loader::load_map_registry(&root);
        if maps.by_name(&name).is_some_and(|map| !map.custom) {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("overwrite").to_variant()),
            ]);
        }
        if let Err(error) = std::fs::create_dir_all(&dir) {
            return dict(vec![
                ("ok", false.to_variant()),
                (
                    "error",
                    GString::from(error.to_string().as_str()).to_variant(),
                ),
            ]);
        }
        let file = maps.save_target_file(&NativeFs, &dir, &name);
        let base = editor_base_tags(
            self.grid.tiles.width as u16,
            self.grid.tiles.height as u16,
            &name,
        );
        let map_tags = self.editor.tags.clone();
        let result = data_patches(|patches| {
            save_editor_map_with_ecs_and_patches(
                &NativeFs,
                &file,
                &self.grid,
                content,
                &self.ecs.0,
                base,
                map_tags,
                false,
                patches,
            )
        });
        match result {
            Ok(()) => {
                self.file = Some(file.clone());
                self.editor.saved = true;
                self.clear_error();
                // `Maps.saveMap` ends with `maps.add(map); maps.sort()`: register
                // the fresh entry (the replaced one is already gone). The live
                // map list (`MindPreview.maps_list`) reloads from disk when the
                // maps dialog is shown (`Maps.load`), so one backing file means
                // exactly one entry.
                if let Ok(header) = MapIo::create_map(&NativeFs, &file, true) {
                    maps.add(Map::from_header(&header, true));
                }
                dict(vec![
                    ("ok", true.to_variant()),
                    (
                        "path",
                        GString::from(file.display().to_string().as_str()).to_variant(),
                    ),
                    ("custom", true.to_variant()),
                    (
                        "name",
                        GString::from(maps.by_name(&name).map_or(name.as_str(), Map::name))
                            .to_variant(),
                    ),
                ])
            }
            Err(error) => {
                self.set_map_error(&error);
                dict(vec![
                    ("ok", false.to_variant()),
                    (
                        "error",
                        GString::from(error.to_string().as_str()).to_variant(),
                    ),
                ])
            }
        }
    }
}
