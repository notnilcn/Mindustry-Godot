// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MinimapRenderer` provider (`graphics/MinimapRenderer.java`, plan 16 §3.7 /
//! M6). The interactive widget/fullscreen fragment is plan 14; this node owns
//! the world `Image`/`ImageTexture`, the per-tile `colorFor` pixels, the
//! 2-frame update batching and the camera-region crop. Deliberately exposes the
//! exact plan-14 boundary names (`get_texture`, `region`, `set_zoom`, …).
//!
//! Deviation S16-3: the upstream `Pixmap`+`glTexSubImage2D` per-pixel path
//! becomes a CPU `Image` + full `ImageTexture.update` on dirty frames; the dirty
//! set still batches to `updateInterval` frames, matching `MinimapRenderer.update`.

use std::collections::HashSet;

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Image, ImageTexture, Node2D, Texture2D};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::io::map::preview::{BlockPalette, color_for};
use mind_core::render::minimap::{
    ABOVE_SOLID_MULTIPLIER, LIQUID_EDGE_MULTIPLIER, UPDATE_INTERVAL, region_for_camera, shade,
    shade_rgb, zoom_clamp,
};
use mind_core::world::darkness::get_static_darkness;

use crate::sim_host::MindSimHost;

use super::atlas_bind::color_from;

/// Reads row 0 of `sprites/block_colors.png` as packed `rgba8888` per block id
/// (empty when the pack output is absent).
fn load_block_colors() -> Vec<u32> {
    let path = format!(
        "{}/sprites/block_colors.png",
        crate::assets::loader::resolve_assets_dir()
    );
    if !godot::classes::FileAccess::file_exists(&path) {
        log::warn!("MindMinimap: missing {path}; minimap uses registry colours");
        return Vec::new();
    }
    let bytes = godot::classes::FileAccess::get_file_as_bytes(&path);
    let mut image = Image::new_gd();
    if image.load_png_from_buffer(&bytes) != godot::global::Error::OK {
        return Vec::new();
    }
    (0..image.get_width())
        .map(|x| {
            image
                .get_pixel(x, 0)
                .to_u32(godot::builtin::ColorChannelOrder::RGBA)
        })
        .collect()
}

/// `MindMinimap` — the plan-14 minimap provider (plan 16 §3.7).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindMinimap {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    palette: Option<BlockPalette>,
    image: Option<Gd<Image>>,
    texture: Option<Gd<ImageTexture>>,
    width: i32,
    height: i32,
    zoom: f32,
    counter: f32,
    dirty: HashSet<i32>,
    last_revision: i64,
    /// `sprites/block_colors.png` row 0 (`ContentLoader.loadColors`), loaded once.
    block_colors: Option<Vec<u32>>,
}

#[godot_api]
impl INode2D for MindMinimap {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            palette: None,
            image: None,
            texture: None,
            width: 0,
            height: 0,
            zoom: 4.0,
            counter: 0.0,
            dirty: HashSet::new(),
            last_revision: i64::MIN,
            block_colors: None,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        // `ready()` is one-shot; rebuild (idempotently) after a hot reload.
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, delta: f64) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let host = host.bind();
        let revision = host.world_revision();
        drop(host);
        if revision != self.last_revision {
            self.last_revision = revision;
            self.rebuild_all();
            return;
        }
        self.counter += delta as f32;
        if self.counter >= UPDATE_INTERVAL {
            self.counter %= UPDATE_INTERVAL;
            self.flush_dirty();
        }
    }
}

impl MindMinimap {
    /// Re-resolves the sim host and rebuilds the texture (idempotent).
    fn bootstrap(&mut self) {
        self.host = self.base().try_get_node_as::<MindSimHost>("../../SimHost");
        if self.host.is_none() {
            log::warn!("MindMinimap: no MindSimHost at ../../SimHost");
        }
        self.rebuild_all();
    }

    /// Whether the provider has a texture (plan 14 gate).
    pub fn has_texture(&self) -> bool {
        self.texture.is_some()
    }

    fn rebuild_all(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let (width, height, rgba) = {
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return;
            };
            let width = world.width();
            let height = world.height();
            if width <= 0 || height <= 0 {
                return;
            }
            let mut palette = BlockPalette::of(content);
            palette.apply_block_colors(self.block_colors.get_or_insert_with(load_block_colors));
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for y in 0..height {
                // Top row first: `pixmap.set(x, height - 1 - y)`.
                for x in 0..width {
                    let team = host.building_team(x, y).unwrap_or(0);
                    let color = self.tile_color(&palette, world, content, x, y, team);
                    rgba.extend_from_slice(&color.to_be_bytes());
                }
            }
            self.palette = Some(palette);
            (width, height, rgba)
        };

        let data = PackedByteArray::from(rgba);
        let image = Image::create_from_data(
            width,
            height,
            false,
            godot::classes::image::Format::RGBA8,
            &data,
        );
        let Some(image) = image else {
            return;
        };
        let texture = ImageTexture::create_from_image(&image);
        self.image = Some(image);
        self.texture = texture;
        self.width = width;
        self.height = height;
        self.dirty.clear();
    }

    fn flush_dirty(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let Some(host) = self.host.clone() else {
            self.dirty.clear();
            return;
        };
        let (mut image, mut texture) = match (self.image.clone(), self.texture.clone()) {
            (Some(image), Some(texture)) => (image, texture),
            _ => {
                self.dirty.clear();
                return;
            }
        };
        let width = self.width;
        let height = self.height;
        let host = host.bind();
        let world = host.grid();
        let Some(content) = host.content_registry() else {
            self.dirty.clear();
            return;
        };
        let palette = self.palette.clone();
        let Some(palette) = palette else {
            self.dirty.clear();
            return;
        };
        let indices: Vec<i32> = self.dirty.drain().collect();
        for index in indices {
            let x = index % width;
            let y = index / width;
            let team = host.building_team(x, y).unwrap_or(0);
            let color = self.tile_color(&palette, world, content, x, y, team);
            // Texture is top-row-first.
            image.set_pixel(x, height - 1 - y, color_from(color));
        }
        texture.update(&image);
    }

    /// `MinimapRenderer.colorFor` over plan-02 metadata + plan-04 `color_for`.
    fn tile_color(
        &self,
        palette: &BlockPalette,
        world: &mind_core::world::WorldGrid,
        content: &mind_core::content::ContentRegistry,
        x: i32,
        y: i32,
        team: u8,
    ) -> u32 {
        let tile = world.tile(x, y);
        let wall = tile.block.raw();
        let floor = tile.floor.raw();
        let overlay = tile.overlay.raw();
        let real = wall;

        // `real.minimapColor(tile)` falls through to `MapIO.colorFor`.
        // `real.synthetic() -> tile.getTeam().color`.
        let mut color = color_for(palette, real, floor, overlay, team);
        color = shade(
            color,
            mind_core::render::minimap::darkness_multiplier(get_static_darkness(
                world, content, x, y,
            )),
        );

        let real_air = real == 0;
        let next_solid = y + 1 < world.height() && {
            let below = world.tile(x, y + 1);
            content
                .block(below.block)
                .map(|def| def.solid)
                .unwrap_or(false)
        };
        if real_air && y < world.height() - 1 && next_solid {
            color = shade(color, ABOVE_SOLID_MULTIPLIER);
        } else if is_liquid(content, floor)
            && (y >= world.height() - 1 || !is_liquid(content, world.tile(x, y + 1).floor.raw()))
        {
            color = shade_rgb(
                color,
                LIQUID_EDGE_MULTIPLIER[0],
                LIQUID_EDGE_MULTIPLIER[1],
                LIQUID_EDGE_MULTIPLIER[2],
            );
        }
        color
    }

    /// The camera crop in tiles (`[x, y, w, h]`).
    pub fn region_tiles(&self) -> [f32; 4] {
        let Some(host) = self.host.clone() else {
            return [0.0; 4];
        };
        let host = host.bind();
        let (width, height) = host.world_size();
        // `Core.camera.position` (world px, y-up): the sibling `World/Camera2D`.
        let camera = self
            .base()
            .try_get_node_as::<godot::classes::Camera2D>("../Camera2D")
            .map(|camera| camera.get_position())
            .unwrap_or_default();
        region_for_camera(
            camera.x,
            camera.y,
            TILESIZE as f32,
            width,
            height,
            self.zoom,
        )
    }
}

/// Whether a floor block is a liquid (`Floor.isLiquid` approximation: kind).
fn is_liquid(content: &mind_core::content::ContentRegistry, floor: u16) -> bool {
    content
        .block(mind_core::content::BlockId::new(floor))
        .map(|def| matches!(def.kind, mind_core::content::BlockKind::ShallowLiquid))
        .unwrap_or(false)
}

#[godot_api]
impl MindMinimap {
    /// The world minimap texture (`None` before the first rebuild).
    #[func]
    pub fn get_texture(&self) -> Option<Gd<Texture2D>> {
        self.texture
            .clone()
            .map(|texture| texture.upcast::<Texture2D>())
    }

    /// The camera crop as a `Rect2` in texture pixels (top-left origin).
    #[func]
    pub fn region(&self) -> Rect2 {
        let [x, y, w, h] = self.region_tiles();
        Rect2::new(Vector2::new(x, y), Vector2::new(w, h))
    }

    /// World pixels → minimap tooltip/screen coordinates for the widget.
    #[func]
    pub fn minimap_to_screen(&self, x: f64, y: f64) -> Vector2 {
        let [rx, ry, rw, rh] = self.region_tiles();
        let _ = (rw, rh);
        Vector2::new(
            x as f32 / TILESIZE as f32 - rx,
            y as f32 / TILESIZE as f32 - ry,
        )
    }

    /// Minimap screen coordinates → world pixels.
    #[func]
    pub fn screen_to_world(&self, x: f64, y: f64) -> Vector2 {
        let [rx, ry, _, _] = self.region_tiles();
        Vector2::new(
            (x as f32 + rx) * TILESIZE as f32,
            (y as f32 + ry) * TILESIZE as f32,
        )
    }

    /// `MinimapRenderer.zoomBy`.
    #[func]
    pub fn zoom_by(&mut self, amount: f64) {
        self.zoom += amount as f32;
        self.set_zoom(self.zoom as f64);
    }

    /// `MinimapRenderer.setZoom` (clamped).
    #[func]
    pub fn set_zoom(&mut self, amount: f64) {
        let (width, height) = self
            .host
            .clone()
            .map(|host| host.bind().world_size())
            .unwrap_or((0, 0));
        self.zoom = zoom_clamp(amount as f32, width, height);
    }

    /// Current zoom.
    #[func]
    pub fn get_zoom(&self) -> f64 {
        self.zoom as f64
    }

    /// Queues one tile for the next batched update.
    #[func]
    pub fn update_at(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        self.dirty.insert(y * self.width + x);
    }

    /// `MinimapRenderer.updateAll`.
    #[func]
    pub fn update_all(&mut self) {
        self.rebuild_all();
    }

    /// Test hook: the RGBA8888 pixel for a tile (plan 16 §7c-3 oracle).
    #[func]
    pub fn minimap_color_at(&self, x: i32, y: i32) -> i64 {
        let Some(host) = self.host.clone() else {
            return 0;
        };
        let host = host.bind();
        let (world, content) = (host.grid(), host.content_registry());
        let Some(content) = content else {
            return 0;
        };
        let Some(palette) = self.palette.clone() else {
            return 0;
        };
        if x < 0 || y < 0 || x >= world.width() || y >= world.height() {
            return 0;
        }
        let team = host.building_team(x, y).unwrap_or(0);
        self.tile_color(&palette, world, content, x, y, team) as i64
    }
}
