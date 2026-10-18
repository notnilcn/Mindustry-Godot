// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logic display render pass (`LogicDisplayBuild.draw`,
//! `TileableLogicDisplayBuild.draw`; I03 T13).
//!
//! `process()` asks the sim host to process the visible displays
//! (`LogicDisplay.processCommands` drains each root's queue into its CPU
//! raster), uploads changed rasters into one `ImageTexture` per root
//! (`LogicDisplayBuild.buffer` equivalent) and caches the draw list; `draw()`
//! only issues the cached quads plus the tileable frame sprite
//! (`tile-logic-display-<TileBitmask.values[bits]>`).
//!
//! The host interactions sit in `process()` (never `draw()`): the sim host is
//! borrowed mutably there, and a draw-time `bind_mut` can overlap a shared
//! borrow held by whatever triggered the frame (e.g. `MindCampaign` calls).

use std::collections::HashMap;

use godot::builtin::PackedByteArray;
use godot::classes::image::Format;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Image, ImageTexture, Node2D, Texture2D};
use godot::prelude::*;

use crate::sim_host::MindSimHost;

use super::canvas::{self, RegionCache};

/// Row dictionary (`Dictionary<GString, Variant>`).
type Row = Dictionary<GString, Variant>;

/// Reads an integer dictionary field (`0` when absent).
fn dict_i64(dict: &Row, key: &str) -> i64 {
    dict.get(&GString::from(key))
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(0)
}

/// Reads a float dictionary field (`0.0` when absent).
fn dict_f32(dict: &Row, key: &str) -> f32 {
    dict.get(&GString::from(key))
        .and_then(|value| value.try_to::<f32>().ok())
        .unwrap_or(0.0)
}

/// Reads a boolean dictionary field (`false` when absent).
fn dict_bool(dict: &Row, key: &str) -> bool {
    dict.get(&GString::from(key))
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

/// Logic display pass (`Node2D` under the `Layer.block` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub(crate) struct LogicDisplayRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    /// Self as a `Node2D` (region-cache lookups).
    node: Option<Gd<Node2D>>,
    /// Root tile → (texture, width, height).
    textures: HashMap<(i32, i32), (Gd<ImageTexture>, i32, i32)>,
    /// Canvas tile → (texture, canvas size in pixels).
    canvas_textures: HashMap<(i32, i32), (Gd<ImageTexture>, i32)>,
    /// Cached draw list: `(texture, centre x, centre y, w, h, rotation)`.
    cmds: Vec<(Gd<Texture2D>, f32, f32, f32, f32, f32)>,
    regions: RegionCache,
}

#[godot_api]
impl INode2D for LogicDisplayRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            textures: HashMap::new(),
            canvas_textures: HashMap::new(),
            cmds: Vec::new(),
            regions: RegionCache::new(),
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            self.refresh();
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let cmds = std::mem::take(&mut self.cmds);
        let mut base = self.base_mut();
        for (texture, x, y, w, h, rot) in &cmds {
            // `Draw.blend(disabled)`; the camera flip lives in `draw_region`.
            canvas::draw_region(&mut base, texture, *x, *y, *w, *h, *rot, Color::WHITE);
        }
        drop(base);
        self.cmds = cmds;
    }
}

impl LogicDisplayRenderer {
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.textures.clear();
        self.canvas_textures.clear();
        self.cmds.clear();
        self.base_mut().queue_redraw();
    }

    /// Processes the visible displays and rebuilds the cached draw list.
    fn refresh(&mut self) {
        let Some(mut host) = self.host.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let Some((min_x, min_y, max_x, max_y)) = self.visible_rect() else {
            return;
        };
        let rows: Vec<Row> = {
            let array = host
                .bind_mut()
                .logic_display_sync(min_x, min_y, max_x, max_y);
            array
                .iter_shared()
                .filter_map(|value| value.try_to::<Row>().ok())
                .collect()
        };
        // `Renderer.drawDisplays = !settings.getBool("hidedisplays")`
        // (`Renderer.java:172`): with displays hidden, `LogicDisplayBuild.draw`
        // / `CanvasBuild.draw` draw only the block sprite and
        // `TileableLogicDisplayBuild.draw` only its back + frame regions
        // (`LogicDisplay.java:102`, `TileableLogicDisplay.java:172-176`,
        // `CanvasBlock.java:257-260`): no buffer and no canvas texture.
        let draw_displays = !crate::settings_hub::row_bool("hidedisplays");
        // Upload changed rasters first (one texture per root).
        for row in &rows {
            if !draw_displays || !dict_bool(row, "draw_buffer") {
                continue;
            }
            let key = (
                dict_i64(row, "root_x") as i32,
                dict_i64(row, "root_y") as i32,
            );
            self.update_texture(&host, key);
        }
        // Canvases (`CanvasBlock.draw`; T14): one texture per canvas tile.
        let mut cmds: Vec<(Gd<Texture2D>, f32, f32, f32, f32, f32)> = Vec::new();
        let canvases = if draw_displays {
            host.bind().logic_canvas_sync(min_x, min_y, max_x, max_y)
        } else {
            Array::<Variant>::new()
        };
        for value in canvases.iter_shared() {
            let Some(dict) = value.try_to::<Row>().ok() else {
                continue;
            };
            let key = (dict_i64(&dict, "x") as i32, dict_i64(&dict, "y") as i32);
            let pixels = dict_i64(&dict, "pixels") as i32;
            if pixels <= 0 {
                continue;
            }
            // `if(texture == null || invalidated) updateTexture()`
            // (`CanvasBlock.java:267-269`).
            let invalidated = dict_bool(&dict, "invalidated");
            let exists = self
                .canvas_textures
                .get(&key)
                .is_some_and(|(_, old)| *old == pixels);
            if invalidated || !exists {
                let bytes: PackedByteArray = host
                    .bind_mut()
                    .logic_canvas_image(i64::from(key.0), i64::from(key.1));
                if bytes.len() < (pixels as usize) * (pixels as usize) * 4 {
                    continue;
                }
                let Some(image) =
                    Image::create_from_data(pixels, pixels, false, Format::RGBA8, &bytes)
                else {
                    continue;
                };
                match self.canvas_textures.get(&key) {
                    Some((texture, old)) if *old == pixels => {
                        let mut texture = texture.clone();
                        texture.update(&image);
                    }
                    _ => {
                        let Some(texture) = ImageTexture::create_from_image(&image) else {
                            continue;
                        };
                        self.canvas_textures.insert(key, (texture, pixels));
                    }
                }
            }
            if let Some((texture, _)) = self.canvas_textures.get(&key) {
                let world_size = dict_f32(&dict, "size").max(1.0);
                let cx = dict_f32(&dict, "cx");
                let cy = dict_f32(&dict, "cy");
                cmds.push((
                    texture.clone().upcast::<Texture2D>(),
                    cx,
                    cy,
                    world_size,
                    world_size,
                    0.0,
                ));
                // `CanvasBlock.draw` (`CanvasBlock.java:274-288`): an edge
                // sprite for every side with no same-block neighbour, plus the
                // corner sprites where the neighbouring side joins (the
                // second corner is y-flipped).
                let blending = dict_i64(&dict, "blending") as u8;
                if blending != 0 {
                    for side in 0..4u8 {
                        if blending & (1 << side) != 0 {
                            continue;
                        }
                        let side_region = if side >= 2 { "side2" } else { "side1" };
                        if let Some(texture) = self.regions.get(&node, side_region) {
                            let (w, h) = RegionCache::natural_size(&texture);
                            cmds.push((texture, cx, cy, w, h, side as f32 * 90.0));
                        }
                        let corner_region = if side >= 2 { "corner2" } else { "corner1" };
                        if blending & (1 << ((side + 1) % 4)) != 0
                            && let Some(texture) = self.regions.get(&node, corner_region)
                        {
                            let (w, h) = RegionCache::natural_size(&texture);
                            cmds.push((texture, cx, cy, w, h, side as f32 * 90.0));
                        }
                        if blending & (1 << ((side + 3) % 4)) != 0
                            && let Some(texture) = self.regions.get(&node, corner_region)
                        {
                            let (w, h) = RegionCache::natural_size(&texture);
                            cmds.push((texture, cx, cy, w, -h, side as f32 * 90.0));
                        }
                    }
                }
            }
        }
        // Rebuild the display draw list (resolves frame regions through the cache).
        for row in &rows {
            if draw_displays && dict_bool(row, "draw_buffer") {
                let key = (
                    dict_i64(row, "root_x") as i32,
                    dict_i64(row, "root_y") as i32,
                );
                if let Some((texture, _, _)) = self.textures.get(&key) {
                    cmds.push((
                        texture.clone().upcast::<Texture2D>(),
                        dict_f32(row, "x"),
                        dict_f32(row, "y"),
                        dict_f32(row, "w"),
                        dict_f32(row, "h"),
                        0.0,
                    ));
                }
            }
            if dict_bool(row, "frame") {
                let sprite = dict_i64(row, "sprite");
                let name = format!("tile-logic-display-{sprite}");
                if let Some(texture) = self.regions.get(&node, &name) {
                    let (w, h) = RegionCache::natural_size(&texture);
                    cmds.push((
                        texture,
                        dict_f32(row, "frame_x"),
                        dict_f32(row, "frame_y"),
                        w,
                        h,
                        0.0,
                    ));
                }
            }
        }
        self.cmds = cmds;
    }

    /// World-space visible rect `(min x, min y, max x, max y)`.
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<godot::classes::Camera2D>("/root/Spine/World/Camera2D")?;
        let viewport = self.base().get_viewport()?;
        let rect = viewport.get_visible_rect();
        let inverse = camera.get_canvas_transform().affine_inverse();
        let corners = [
            inverse * rect.position,
            inverse * (rect.position + Vector2::new(rect.size.x, 0.0)),
            inverse * rect.position + rect.size,
            inverse * (rect.position + Vector2::new(0.0, rect.size.y)),
        ];
        let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Some((min_x, min_y, max_x, max_y))
    }

    /// Creates or updates the root's texture from the host's raster bytes.
    fn update_texture(&mut self, host: &Gd<MindSimHost>, key: (i32, i32)) {
        let state = host
            .bind()
            .logic_display_state(i64::from(key.0), i64::from(key.1));
        let width = dict_i64(&state, "width") as i32;
        let height = dict_i64(&state, "height") as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let bytes: PackedByteArray = host
            .bind()
            .logic_display_image(i64::from(key.0), i64::from(key.1));
        if bytes.len() < (width as usize) * (height as usize) * 4 {
            return;
        }
        let Some(image) = Image::create_from_data(width, height, false, Format::RGBA8, &bytes)
        else {
            return;
        };
        match self.textures.get(&key) {
            Some((texture, old_w, old_h)) if *old_w == width && *old_h == height => {
                let mut texture = texture.clone();
                texture.update(&image);
            }
            _ => {
                let Some(texture) = ImageTexture::create_from_image(&image) else {
                    return;
                };
                self.textures.insert(key, (texture, width, height));
            }
        }
    }
}
