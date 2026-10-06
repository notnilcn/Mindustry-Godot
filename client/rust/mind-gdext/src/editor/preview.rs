// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindPreview` — plan-19 map-preview provider (gdext facade half).
//!
//! The Godot-free half is `mind_core::editor::preview::PreviewPipeline`
//! (plan 19 §3.8): it owns the queue, the generated [`PreviewImage`]s and the
//! decoded cache. This node is the `Texture2D` binding seam named by plan 19
//! §3.1/§3.8 and the Rust-side data provider for `EditorMapsDialog`
//! (`maps_list`).
//!
//! The in-engine MCP §7c run is deferred to the single-editor mutex (like the
//! rest of the editor shell); this class compiles and is exercisable from
//! GDScript without any live-world dependency.

use std::path::PathBuf;

use godot::builtin::{Array, GString, PackedByteArray, VarDictionary};
use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Image, ImageTexture, Node, ProjectSettings, Texture2D};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::editor::preview::PreviewPipeline;
use mind_core::io::fs::{FileSystem, NativeFs, Paths};
use mind_core::io::map::{MapIo, PreviewImage, decode_png};
use mind_core::maps::{Map, Maps, preview_file};

/// `MindPreview` — map preview textures + `EditorMapsDialog` provider rows.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindPreview {
    base: Base<Node>,
    pipeline: PreviewPipeline,
    maps: Maps,
    /// The data root that holds `maps/` and `previews/`.
    root: PathBuf,
}

#[godot_api]
impl INode for MindPreview {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            pipeline: PreviewPipeline::new(),
            maps: Maps::new(),
            root: default_root(),
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

impl MindPreview {
    /// Rebuilds the node's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        log::info!("MindPreview ready ({} maps)", self.maps.len());
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.root)
    }

    fn map_for(&self, file: &str) -> Option<Map> {
        self.maps
            .all()
            .iter()
            .find(|map| map.file.display().to_string() == file)
            .cloned()
    }
}

#[godot_api]
impl MindPreview {
    /// Data root used for `maps/` + `previews/` (defaults to `user://`).
    #[func]
    pub fn data_root(&self) -> GString {
        GString::from(self.root.display().to_string().as_str())
    }

    /// Overrides the data root (`res://`/`user://` resolved via Godot).
    #[func]
    pub fn set_data_root(&mut self, root: GString) {
        let raw = root.to_string();
        self.root = if raw.starts_with("res://") || raw.starts_with("user://") {
            PathBuf::from(
                ProjectSettings::singleton()
                    .globalize_path(&raw)
                    .to_string(),
            )
        } else {
            PathBuf::from(raw)
        };
    }

    /// Scans `<root>/maps` for `.msav` files, registering them by meta only.
    ///
    /// Mirrors the registry subset `EditorMapsDialog` needs; the actual
    /// `Maps.load` orchestration stays plan 06's.
    #[func]
    pub fn refresh(&mut self) -> i64 {
        self.maps = Maps::new();
        let dir = self.root.join("maps");
        let mut count = 0i64;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("msav"))
            .collect();
        paths.sort();
        for path in paths {
            if let Ok(header) = MapIo::create_map(&NativeFs, &path, true)
                && !header.name.trim().is_empty()
            {
                self.maps.add(Map::from_header(&header, true));
                count += 1;
            }
        }
        count
    }

    /// `CustomGameDialog`/`EditorMapsDialog` provider rows:
    /// `{name, path, custom, author, description, width, height, preview}`.
    #[func]
    pub fn maps_list(&self) -> Array<VarDictionary> {
        let paths = self.paths();
        let mut out = Array::<VarDictionary>::new();
        for map in self.maps.all() {
            let (name, path, custom, author, description, width, height) = map_row_fields(map);
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &GString::from(name.as_str()).to_variant(),
            );
            row.set(
                &GString::from("path"),
                &GString::from(path.as_str()).to_variant(),
            );
            row.set(&GString::from("custom"), &custom.to_variant());
            row.set(
                &GString::from("author"),
                &GString::from(author.as_str()).to_variant(),
            );
            row.set(
                &GString::from("description"),
                &GString::from(description.as_str()).to_variant(),
            );
            row.set(&GString::from("width"), &width.to_variant());
            row.set(&GString::from("height"), &height.to_variant());
            row.set(
                &GString::from("preview"),
                &NativeFs.exists(&preview_file(&paths, map)).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Number of registered maps.
    #[func]
    pub fn map_count(&self) -> i64 {
        self.maps.len() as i64
    }

    /// The on-disk preview PNG path for a map file ('' when unknown).
    #[func]
    pub fn preview_path(&self, map_file: GString) -> GString {
        match self.map_for(&map_file.to_string()) {
            Some(map) => GString::from(
                preview_file(&self.paths(), &map)
                    .display()
                    .to_string()
                    .as_str(),
            ),
            None => GString::new(),
        }
    }

    /// `Map.safeTexture()`: binds a map's preview pixels to a `Texture2D`.
    ///
    /// The core pipeline's generated image is preferred; otherwise the cached
    /// preview PNG is decoded (plan 19 §3.8 `error.png` fallback is the caller's
    /// concern until the plan-16 frame hook lands).
    #[func]
    pub fn texture_for(&mut self, map_file: GString) -> Option<Gd<Texture2D>> {
        let file = map_file.to_string();
        let map = self.map_for(&file)?;
        if let Some(image) = self.pipeline.texture_for(&map) {
            return bind_texture(image);
        }
        let preview = preview_file(&self.paths(), &map);
        let bytes = NativeFs.read(&preview).ok()?;
        let image = decode_png(&bytes).ok()?;
        bind_texture(&image)
    }
}

/// Plain metadata of one registered map (`maps_list` row fields).
fn map_row_fields(map: &Map) -> (String, String, bool, String, String, i32, i32) {
    (
        map.name().to_owned(),
        map.file.display().to_string(),
        map.custom,
        map.author().to_owned(),
        map.description().to_owned(),
        map.width,
        map.height,
    )
}

/// Binds a [`PreviewImage`] to a Godot `Texture2D` (RGBA8).
fn bind_texture(image: &PreviewImage) -> Option<Gd<Texture2D>> {
    let data = PackedByteArray::from(image.rgba.as_slice());
    let godot_image = Image::create_from_data(
        image.width as i32,
        image.height as i32,
        false,
        godot::classes::image::Format::RGBA8,
        &data,
    )?;
    ImageTexture::create_from_image(&godot_image).map(|texture| texture.upcast::<Texture2D>())
}

/// Default data root (`user://`).
fn default_root() -> PathBuf {
    PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::io::StringMap;

    fn fixture_map() -> Map {
        let mut tags = StringMap::new();
        tags.insert("name".to_owned(), "alpha".to_owned());
        tags.insert("author".to_owned(), "tester".to_owned());
        tags.insert("description".to_owned(), "a test map".to_owned());
        Map::new(
            PathBuf::from("/tmp/maps/alpha.msav"),
            64,
            32,
            tags,
            true,
            1,
            -1,
        )
    }

    #[test]
    fn map_row_fields_carry_registry_metadata() {
        let (name, path, custom, author, description, width, height) =
            map_row_fields(&fixture_map());
        assert_eq!(name, "alpha");
        assert_eq!(path, "/tmp/maps/alpha.msav");
        assert!(custom);
        assert_eq!(author, "tester");
        assert_eq!(description, "a test map");
        assert_eq!((width, height), (64, 32));
    }
}
