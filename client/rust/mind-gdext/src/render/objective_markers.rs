// SPDX-License-Identifier: GPL-3.0-only

//! `MapObjectives` world markers (view-only).
//!
//! Ported from `core/src/mindustry/game/MapObjectives.java` marker drawing and
//! `core/src/mindustry/core/Renderer.java:352-360`: every marker of every
//! running objective with `world != -1` is drawn in the normal per-frame pass
//! (`marker.draw(marker.autoscale ? 4f / getDisplayScale() : 1)`).
//!
//! The pass lives in a screen-space `CanvasLayer` above the world bands and
//! below the HUD (`Ui`): world coordinates are projected through the camera's
//! canvas transform on **every** frame, so a world-anchored label never lags
//! the terrain. Row data is re-read from `MindCampaign.objective_markers_json`
//! on the 0.2 s poll; the projection is not.
//!
//! Implemented classes: `ShapeText`, `Text`, `Shape`, `Point`, `Line`,
//! `Texture`, `Quad` (plus the WIP `Light` markers, which implement only
//! `drawLight` upstream and therefore contribute no world geometry).

use godot::builtin::{PackedColorArray, PackedVector2Array, Rect2};
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{AtlasTexture, Camera2D, Control, Font, INode2D, Node2D, Texture2D, Time};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::assets::icons::Iconc;
use mind_core::config::TILESIZE;
use mind_core::ui::text::markup_runs;

use crate::assets::bundle::load_iconc;
use crate::assets::loader::resolve_assets_dir;
use crate::campaign::MindCampaign;
use crate::render::canvas::RegionCache;

/// `WorldLabel.flagBackground`.
const FLAG_BACKGROUND: i32 = 1 << 0;
/// `WorldLabel.flagOutline`.
const FLAG_OUTLINE: i32 = 1 << 1;
/// `WorldLabel.flagAutoscale`.
const FLAG_AUTOSCALE: i32 = 1 << 4;
/// `Align.top`.
const ALIGN_TOP: i32 = 1 << 1;
/// `Align.bottom`.
const ALIGN_BOTTOM: i32 = 1 << 2;
/// `Align.left`.
const ALIGN_LEFT: i32 = 1 << 3;
/// `Align.right`.
const ALIGN_RIGHT: i32 = 1 << 4;

/// One drawable marker (`MapObjectives.ObjectiveMarker` subclasses), world
/// pixels and label style only.
#[derive(Debug, Clone)]
enum MarkerRow {
    Point {
        pos: Vector2,
        radius: f32,
        stroke: f32,
        color: Color,
        autoscale: bool,
    },
    ShapeText {
        pos: Vector2,
        text: String,
        font_size: f32,
        text_height: f32,
        radius: f32,
        rotation: f32,
        sides: i32,
        color: Color,
        flags: i32,
        text_align: i32,
        line_align: i32,
        autoscale: bool,
    },
    Shape {
        pos: Vector2,
        radius: f32,
        rotation: f32,
        stroke: f32,
        start_angle: f32,
        end_angle: f32,
        fill: bool,
        outline: bool,
        sides: i32,
        color: Color,
        autoscale: bool,
    },
    Text {
        pos: Vector2,
        text: String,
        font_size: f32,
        flags: i32,
        text_align: i32,
        line_align: i32,
        autoscale: bool,
    },
    Line {
        start: Vector2,
        end: Vector2,
        stroke: f32,
        outline: bool,
        color1: Color,
        color2: Color,
        autoscale: bool,
    },
    Texture {
        pos: Vector2,
        rotation: f32,
        width: f32,
        height: f32,
        texture: String,
        color: Color,
        autoscale: bool,
    },
    Quad {
        vertices: [f32; 24],
        texture: String,
        autoscale: bool,
    },
}

/// Screen-space `MapObjectives` world-marker pass.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindObjectiveMarkers {
    base: Base<Node2D>,
    markers: Vec<MarkerRow>,
    camera: Option<Gd<Camera2D>>,
    source: Option<Gd<MindCampaign>>,
    accum: f64,
    last_json: String,
    /// `:icon:` table for the label markup (`UI.formatIcons`).
    iconc: Iconc,
    /// `TextureHolder` name → atlas texture (misses cached too).
    regions: RegionCache,
    /// Frames this pass has drawn (PAR-0478 probe: the projection must run
    /// every frame, not on the 0.2 s JSON poll).
    draws: u64,
    /// Projected screen anchor of the first label drawn last frame.
    last_label: Option<Vector2>,
}

#[godot_api]
impl INode2D for MindObjectiveMarkers {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            markers: Vec::new(),
            camera: None,
            source: None,
            accum: 0.0,
            last_json: String::new(),
            iconc: Iconc::default(),
            regions: RegionCache::new(),
            draws: 0,
            last_label: None,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        // `ready()` is not re-run on hot reload; re-resolve the Godot refs.
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, delta: f64) {
        // The JSON poll only refreshes the marker list; the *projection* runs
        // every frame (`Renderer.java:352-360` draws markers in the normal
        // draw pass), so a redraw is requested unconditionally.
        self.accum += delta;
        if self.accum >= 0.2 {
            self.accum = 0.0;
            if let Some(source) = self.source.clone() {
                let json = source.bind().objective_markers_json().to_string();
                if json != self.last_json {
                    self.last_json = json;
                    self.markers = parse_rows(&self.last_json);
                }
            }
        }
        // Also covers the empty list: without the redraw the last painted
        // label would stay frozen on screen after its objective completes.
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        if self.markers.is_empty() {
            return;
        }
        let Some(camera) = self.camera.clone() else {
            return;
        };
        // World -> viewport transform (includes the y-flip zoom, PAR-0037).
        let transform = camera.get_canvas_transform();
        let zoom = camera.get_zoom().x.abs().max(0.01);
        let font = self
            .base()
            .try_get_node_as::<Control>("/root/Spine/Ui/UiRoot")
            .and_then(|ui| ui.get_theme_default_font());
        // Detached handle: the atlas resolver and every `draw_*` call both
        // mutably borrow canvas state, so neither may borrow `self`.
        let mut canvas = self.to_gd().upcast::<Node2D>();
        let rows = self.markers.clone();
        self.draws += 1;
        self.last_label = None;
        for marker in &rows {
            match marker {
                MarkerRow::Point {
                    pos,
                    radius,
                    stroke,
                    color,
                    autoscale,
                } => {
                    // `PointMarker.draw`: `rad = radius * tilesize`, a pulsing
                    // circle (`Interp.pow2Out` over `Time.globalTime / 100f`).
                    let scale = marker_scale(*autoscale, zoom);
                    let now = Time::singleton().get_ticks_msec() as f32;
                    let fin = pow2_out((now / 100.0) % 1.0);
                    let screen = transform * *pos;
                    let rad = *radius * TILESIZE as f32 * scale * fin * zoom;
                    let width = ((1.0 - fin) * *stroke + 0.1) * zoom * 0.25;
                    canvas
                        .draw_arc_ex(screen, rad, 0.0, std::f32::consts::TAU, 48, *color)
                        .width(width.max(0.1))
                        .done();
                }
                MarkerRow::ShapeText {
                    pos,
                    text,
                    font_size,
                    text_height,
                    radius,
                    rotation,
                    sides,
                    color,
                    flags,
                    text_align,
                    line_align,
                    autoscale,
                } => {
                    let scale = marker_scale(*autoscale, zoom);
                    let screen = transform * *pos;
                    draw_shape_ring(
                        &mut canvas,
                        screen,
                        *sides,
                        (*radius + 1.0) * scale * zoom,
                        *rotation,
                        *rotation,
                        *rotation + 360.0,
                        *color,
                        scale,
                        zoom,
                        true,
                        3.0,
                    );
                    if *font_size == 0.0 {
                        continue;
                    }
                    let Some(font) = font.clone() else {
                        continue;
                    };
                    // `WorldLabel.drawAt(fetchedText, pos.x, pos.y + radius *
                    // scaleFactor + textHeight * scaleFactor, …)`.
                    let anchor =
                        screen + Vector2::new(0.0, -(*radius + *text_height) * scale * zoom);
                    if self.last_label.is_none() {
                        self.last_label = Some(anchor);
                    }
                    draw_label(
                        &mut canvas,
                        &font,
                        &self.iconc,
                        anchor,
                        text,
                        label_px(*font_size, *flags, scale, zoom),
                        *flags,
                        *text_align,
                        *line_align,
                    );
                }
                MarkerRow::Shape {
                    pos,
                    radius,
                    rotation,
                    stroke,
                    start_angle,
                    end_angle,
                    fill,
                    outline,
                    sides,
                    color,
                    autoscale,
                } => {
                    let scale = marker_scale(*autoscale, zoom);
                    let screen = transform * *pos;
                    let from = *rotation + *start_angle;
                    let to = *rotation + *end_angle;
                    let sides = (*sides).min(200);
                    if !*fill {
                        if *outline {
                            draw_shape_ring(
                                &mut canvas,
                                screen,
                                sides,
                                (*radius + 1.0) * scale * zoom,
                                from,
                                from,
                                to,
                                Color::from_rgba(0.53, 0.53, 0.53, 1.0),
                                scale,
                                zoom,
                                true,
                                *stroke + 2.0,
                            );
                        }
                        draw_shape_ring(
                            &mut canvas,
                            screen,
                            sides,
                            (*radius + 1.0) * scale * zoom,
                            from,
                            from,
                            to,
                            *color,
                            scale,
                            zoom,
                            true,
                            *stroke,
                        );
                    } else {
                        // `Fill.arc(pos.x, pos.y, radius * scaleFactor, …)`.
                        let points = arc_points(
                            screen,
                            *radius * scale * zoom,
                            from.min(to),
                            from.max(to),
                            sides,
                        );
                        if !points.is_empty() {
                            canvas.draw_colored_polygon(&points, *color);
                        }
                    }
                }
                MarkerRow::Text {
                    pos,
                    text,
                    font_size,
                    flags,
                    text_align,
                    line_align,
                    autoscale,
                } => {
                    // `TextMarker.draw`: `Mathf.equal(fontSize, 0f)` returns.
                    if *font_size == 0.0 {
                        continue;
                    }
                    let Some(font) = font.clone() else {
                        continue;
                    };
                    let scale = marker_scale(*autoscale, zoom);
                    if self.last_label.is_none() {
                        self.last_label = Some(transform * *pos);
                    }
                    draw_label(
                        &mut canvas,
                        &font,
                        &self.iconc,
                        transform * *pos,
                        text,
                        label_px(*font_size, *flags, scale, zoom),
                        *flags,
                        *text_align,
                        *line_align,
                    );
                }
                MarkerRow::Line {
                    start,
                    end,
                    stroke,
                    outline,
                    color1,
                    color2,
                    autoscale,
                } => {
                    let scale = marker_scale(*autoscale, zoom);
                    // `Draw` works in world px; Java's stroke values are
                    // design px, so every width uses the `zoom * 0.25` factor.
                    let width = *stroke * scale * zoom * 0.25;
                    let a = transform * *start;
                    let b = transform * *end;
                    if *outline {
                        canvas
                            .draw_line_ex(a, b, Color::from_rgba(0.53, 0.53, 0.53, 1.0))
                            .width((width + 2.0).max(0.1))
                            .done();
                    }
                    let mut points = PackedVector2Array::new();
                    points.push(a);
                    points.push(b);
                    let mut colors = PackedColorArray::new();
                    colors.push(*color1);
                    colors.push(*color2);
                    canvas
                        .draw_polyline_colors_ex(&points, &colors)
                        .width(width.max(0.1))
                        .done();
                }
                MarkerRow::Texture {
                    pos,
                    rotation,
                    width,
                    height,
                    texture,
                    color,
                    autoscale,
                } => {
                    let scale = marker_scale(*autoscale, zoom);
                    let Some(texture) = self.regions.get(&canvas, texture) else {
                        continue;
                    };
                    // `Draw.rect(fetchedRegion, pos.x, pos.y, width *
                    // scaleFactor, height * scaleFactor, rotation)`: a zero
                    // width/height follows the region's natural size
                    // (`TextureRegion.scl()` — the atlas packs 4 px per world
                    // unit, `RegionCache::natural_size`).
                    let (natural_w, natural_h) = RegionCache::natural_size(&texture);
                    let mut w = if *width == 0.0 { natural_w } else { *width };
                    let mut h = if *height == 0.0 { natural_h } else { *height };
                    w *= scale * zoom;
                    h *= scale * zoom;
                    let screen = transform * *pos;
                    canvas
                        .draw_set_transform_ex(screen)
                        .rotation(rotation.to_radians())
                        .scale(Vector2::ONE)
                        .done();
                    canvas
                        .draw_texture_rect_ex(
                            &texture,
                            Rect2::new(Vector2::new(-w * 0.5, -h * 0.5), Vector2::new(w, h)),
                            false,
                        )
                        .modulate(*color)
                        .done();
                    canvas.draw_set_transform(Vector2::ZERO);
                }
                MarkerRow::Quad {
                    vertices,
                    texture,
                    autoscale,
                } => {
                    let Some(texture) = self.regions.get(&canvas, texture) else {
                        continue;
                    };
                    let scale = marker_scale(*autoscale, zoom);
                    // `Draw.vert(fetchedRegion.texture, vertices)`: the stored
                    // UVs are atlas-normalised (`QuadMarker.setUv` maps into
                    // `fetchedRegion.u..u2`), so sample the page texture.
                    let page = texture
                        .clone()
                        .try_cast::<AtlasTexture>()
                        .ok()
                        .and_then(|atlas| atlas.get_atlas())
                        .map(|atlas| atlas.upcast::<Texture2D>())
                        .unwrap_or_else(|| texture.clone());
                    let mut points = PackedVector2Array::new();
                    let mut colors = PackedColorArray::new();
                    let mut uvs = PackedVector2Array::new();
                    for i in 0..4 {
                        let x = vertices[i * 6];
                        let y = vertices[i * 6 + 1];
                        points.push(transform * (Vector2::new(x, y) * scale));
                        colors.push(float_bits_color(vertices[i * 6 + 2]));
                        uvs.push(Vector2::new(vertices[i * 6 + 3], vertices[i * 6 + 4]));
                    }
                    canvas
                        .draw_primitive_ex(&points, &colors, &uvs)
                        .texture(&page)
                        .done();
                }
            }
        }
    }
}

#[godot_api]
impl MindObjectiveMarkers {
    /// Debug seam (PAR-0478): `draws` counts the frames this pass actually
    /// painted and `label` is the projected screen anchor of its first label on
    /// the last painted frame — a probe compares `draws` against
    /// `Engine.get_process_frames()` and watches `label` follow the camera.
    #[func]
    pub fn marker_draw_debug(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set(&GString::from("draws"), &(self.draws as i64).to_variant());
        out.set(
            &GString::from("frame"),
            &(godot::classes::Engine::singleton().get_process_frames() as i64).to_variant(),
        );
        out.set(
            &GString::from("rows"),
            &(self.markers.len() as i64).to_variant(),
        );
        if let Some(label) = self.last_label {
            out.set(&GString::from("labelX"), &f64::from(label.x).to_variant());
            out.set(&GString::from("labelY"), &f64::from(label.y).to_variant());
        }
        out
    }
}

impl MindObjectiveMarkers {
    fn bootstrap(&mut self) {
        self.camera = self
            .base()
            .try_get_node_as::<Camera2D>("/root/Spine/World/Camera2D");
        self.source = self
            .base()
            .try_get_node_as::<MindCampaign>("/root/Spine/MindCampaign");
        self.iconc = load_iconc(&resolve_assets_dir());
    }
}

/// `marker.autoscale ? 4f / displayScale : 1f` (`Renderer.java:353-357`).
///
/// The Godot camera zoom is the display scale (both are 4 at the default
/// zoom), and the pass multiplies world geometry by `zoom`, so the autoscaled
/// factor `4 / zoom` cancels it exactly like upstream.
fn marker_scale(autoscale: bool, zoom: f32) -> f32 {
    if autoscale { 4.0 / zoom } else { 1.0 }
}

/// `WorldLabel` font px: `0.25 * fontSize` on the 16 px outline font, in
/// design px, scaled by the camera zoom — and divided by the label's own
/// `flagAutoscale` term (`WorldLabel.java:345`).
fn label_px(font_size: f32, flags: i32, scale: f32, zoom: f32) -> f32 {
    let divisor = if flags & FLAG_AUTOSCALE != 0 {
        0.2 * zoom + 0.05
    } else {
        1.0
    };
    (16.0 * 0.25 * font_size.max(0.01) * scale * zoom / divisor).clamp(1.0, 128.0)
}

/// `Interp.pow2Out.apply(a)`.
fn pow2_out(a: f32) -> f32 {
    1.0 - (1.0 - a) * (1.0 - a)
}

/// `Color.fromFloatBits` (`Color.toFloatBits` is packed RGBA8888).
fn float_bits_color(bits: f32) -> Color {
    let value = bits.to_bits();
    Color::from_rgba8(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

/// Points along `Lines.poly`/`Fill.arc`'s ring: `sides` segments from
/// `start_angle` to `end_angle` (degrees), y-flipped for screen space.
fn ring_points(
    center: Vector2,
    radius: f32,
    start_angle: f32,
    space: f32,
    sides: i32,
) -> PackedVector2Array {
    let mut points = PackedVector2Array::new();
    let sides = sides.clamp(3, 300);
    for i in 0..=sides {
        let angle = (start_angle + space * i as f32).to_radians();
        points.push(center + Vector2::new(angle.cos(), -angle.sin()) * radius);
    }
    points
}

/// `Fill.arc` fan over `[start, end]` degrees.
fn arc_points(
    center: Vector2,
    radius: f32,
    start: f32,
    end: f32,
    sides: i32,
) -> PackedVector2Array {
    let mut points = PackedVector2Array::new();
    let sides = sides.clamp(3, 200);
    let space = (end - start) / sides as f32;
    points.push(center);
    for i in 0..=sides {
        let angle = (start + space * i as f32).to_radians();
        points.push(center + Vector2::new(angle.cos(), -angle.sin()) * radius);
    }
    points
}

/// `Lines.stroke(width); Lines.poly(...)`: the outline ring of a polygonal
/// marker, drawn at `radius` with `width` design px.
#[allow(clippy::too_many_arguments)]
fn draw_shape_ring(
    canvas: &mut Gd<Node2D>,
    center: Vector2,
    sides: i32,
    radius: f32,
    rotation: f32,
    start_angle: f32,
    end_angle: f32,
    color: Color,
    scale: f32,
    zoom: f32,
    closed: bool,
    stroke: f32,
) {
    let _ = rotation;
    let _ = closed;
    let sides = sides.clamp(3, 300);
    let space = (end_angle - start_angle) / sides as f32;
    let points = ring_points(center, radius, start_angle, space, sides);
    canvas
        .draw_polyline_ex(&points, color)
        .width((stroke * scale * zoom * 0.25).max(0.1))
        .done();
}

/// `WorldLabel.drawAt` for one marker label: markup runs, optional background
/// box and outline, `textAlign`/`lineAlign` anchoring (`WorldLabel.java:
/// 336-364`).
#[allow(clippy::too_many_arguments)]
fn draw_label(
    canvas: &mut Gd<Node2D>,
    font: &Gd<Font>,
    iconc: &Iconc,
    anchor: Vector2,
    text: &str,
    px: f32,
    flags: i32,
    text_align: i32,
    line_align: i32,
) {
    if text.is_empty() {
        return;
    }
    let font_size = px.round().max(1.0) as i32;
    let line_height = font.get_height_ex().font_size(font_size).done();
    let ascent = font.get_ascent_ex().font_size(font_size).done();
    // One `GlyphLayout` per label: per-line markup runs, run widths and the
    // line width (the block width is the widest line).
    let lines: Vec<(Vec<mind_core::ui::text::MarkupRun>, Vec<f32>, f32)> = text
        .split('\n')
        .map(|line| {
            let runs = markup_runs(line, iconc);
            let widths: Vec<f32> = runs
                .iter()
                .map(|run| {
                    font.get_string_size_ex(run.text.as_str())
                        .font_size(font_size)
                        .done()
                        .x
                })
                .collect();
            let width: f32 = widths.iter().sum();
            (runs, widths, width)
        })
        .collect();
    let width = lines.iter().map(|line| line.2).fold(0.0, f32::max);
    let height = line_height * lines.len() as f32;
    let border = if flags & FLAG_BACKGROUND != 0 {
        1.0
    } else {
        0.0
    };
    // Java anchors in world (y-up) coordinates: `y += layout.height / 2` for
    // the centre case, `y += layout.height + border * 1.5f` for bottom and
    // `y -= border * 1.5f` for top; screen y is flipped.
    let top = if text_align & ALIGN_BOTTOM != 0 {
        anchor.y - height - border * 1.5
    } else if text_align & ALIGN_TOP != 0 {
        anchor.y + border * 1.5
    } else {
        anchor.y - height * 0.5
    };
    let left = if text_align & ALIGN_LEFT != 0 {
        anchor.x + border
    } else if text_align & ALIGN_RIGHT != 0 {
        anchor.x - width - border
    } else {
        anchor.x - width * 0.5
    };
    if flags & FLAG_BACKGROUND != 0 {
        canvas
            .draw_rect_ex(
                Rect2::new(
                    Vector2::new(left - 1.0, top - 1.5),
                    Vector2::new(width + 2.0, height + 3.0),
                ),
                Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            )
            .done();
    }
    let mut line_top = top;
    for (runs, widths, line_width) in &lines {
        let mut x = if line_align & ALIGN_LEFT != 0 {
            left
        } else if line_align & ALIGN_RIGHT != 0 {
            left + width - line_width
        } else {
            left + (width - line_width) * 0.5
        };
        let origin_y = line_top + ascent;
        for (run, run_width) in runs.iter().zip(widths) {
            if run.text.is_empty() {
                x += run_width;
                continue;
            }
            let origin = Vector2::new(x, origin_y);
            let text = GString::from(run.text.as_str());
            if flags & FLAG_OUTLINE != 0 {
                canvas
                    .draw_string_outline_ex(font, origin, &text)
                    .font_size(font_size)
                    .size(4)
                    .modulate(Color::BLACK)
                    .done();
            }
            let color = run.color.map_or(Color::WHITE, |c| {
                Color::from_rgba8((c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8)
            });
            canvas
                .draw_string_ex(font, origin, &text)
                .font_size(font_size)
                .modulate(color)
                .done();
            x += run_width;
        }
        line_top += line_height;
    }
}

fn f32_field(row: &serde_json::Value, key: &str, default: f32) -> f32 {
    row.get(key)
        .and_then(|v| v.as_f64())
        .unwrap_or(default as f64) as f32
}

fn i32_field(row: &serde_json::Value, key: &str, default: i32) -> i32 {
    row.get(key)
        .and_then(|v| v.as_i64())
        .unwrap_or(default as i64) as i32
}

fn bool_field(row: &serde_json::Value, key: &str, default: bool) -> bool {
    row.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn pos_field(row: &serde_json::Value) -> Vector2 {
    Vector2::new(f32_field(row, "x", 0.0), f32_field(row, "y", 0.0))
}

fn color_field(row: &serde_json::Value, key: &str) -> Color {
    let Some(values) = row.get(key).and_then(|v| v.as_array()) else {
        return Color::WHITE;
    };
    let channel = |index: usize, default: f32| {
        values
            .get(index)
            .and_then(|v| v.as_f64())
            .unwrap_or(default as f64) as f32
    };
    Color::from_rgba(
        channel(0, 1.0),
        channel(1, 1.0),
        channel(2, 1.0),
        channel(3, 1.0),
    )
}

fn text_field(row: &serde_json::Value, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned()
}

/// `TextureHolder` → atlas region name: the explicit string wins (upstream
/// `atlas.find(name)`), then the content's `-full` region
/// (`UnlockableContent.fullIcon`); building-position holders are skipped.
fn texture_field(row: &serde_json::Value) -> String {
    let Some(holder) = row.get("texture") else {
        return "white".to_owned();
    };
    if let Some(name) = holder.get("string").and_then(|v| v.as_str())
        && !name.is_empty()
    {
        return name.to_owned();
    }
    if let Some(content) = holder.get("content").and_then(|v| v.as_str()) {
        return format!("{content}-full");
    }
    "white".to_owned()
}

/// Parses the campaign endpoint's marker rows, ordered by `drawLayer`
/// (`Renderer.java`: markers draw at their own `drawLayer`).
fn parse_rows(json: &str) -> Vec<MarkerRow> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    let mut ordered: Vec<(f32, MarkerRow)> = Vec::new();
    for (order, row) in rows.iter().enumerate() {
        let draw_layer = f32_field(row, "drawLayer", 120.0);
        let autoscale = bool_field(row, "autoscale", false);
        let marker = match row.get("class").and_then(|v| v.as_str()).unwrap_or("") {
            "Point" => MarkerRow::Point {
                pos: pos_field(row),
                radius: f32_field(row, "radius", 5.0),
                stroke: f32_field(row, "stroke", 11.0),
                color: color_field(row, "color"),
                autoscale,
            },
            "ShapeText" => MarkerRow::ShapeText {
                pos: pos_field(row),
                text: text_field(row, "text"),
                font_size: f32_field(row, "fontSize", 1.0),
                text_height: f32_field(row, "textHeight", 7.0),
                radius: f32_field(row, "radius", 6.0),
                rotation: f32_field(row, "rotation", 0.0),
                sides: i32_field(row, "sides", 4),
                color: color_field(row, "color"),
                flags: i32_field(row, "flags", FLAG_BACKGROUND | FLAG_OUTLINE),
                text_align: i32_field(row, "textAlign", 1),
                line_align: i32_field(row, "lineAlign", 1),
                autoscale,
            },
            "Shape" => MarkerRow::Shape {
                pos: pos_field(row),
                radius: f32_field(row, "radius", 8.0),
                rotation: f32_field(row, "rotation", 0.0),
                stroke: f32_field(row, "stroke", 1.0),
                start_angle: f32_field(row, "startAngle", 0.0),
                end_angle: f32_field(row, "endAngle", 360.0),
                fill: bool_field(row, "fill", false),
                outline: bool_field(row, "outline", true),
                sides: i32_field(row, "sides", 4),
                color: color_field(row, "color"),
                autoscale,
            },
            "Text" => MarkerRow::Text {
                pos: pos_field(row),
                text: text_field(row, "text"),
                font_size: f32_field(row, "fontSize", 1.0),
                flags: i32_field(row, "flags", FLAG_BACKGROUND | FLAG_OUTLINE),
                text_align: i32_field(row, "textAlign", 1),
                line_align: i32_field(row, "lineAlign", 1),
                autoscale,
            },
            "Line" => {
                let start = pos_field(row);
                MarkerRow::Line {
                    start,
                    end: Vector2::new(
                        f32_field(row, "endX", start.x),
                        f32_field(row, "endY", start.y),
                    ),
                    stroke: f32_field(row, "stroke", 1.0),
                    outline: bool_field(row, "outline", true),
                    color1: color_field(row, "color1"),
                    color2: color_field(row, "color2"),
                    autoscale,
                }
            }
            "Texture" => MarkerRow::Texture {
                pos: pos_field(row),
                rotation: f32_field(row, "rotation", 0.0),
                width: f32_field(row, "width", 0.0),
                height: f32_field(row, "height", 0.0),
                texture: texture_field(row),
                color: color_field(row, "color"),
                autoscale,
            },
            "Quad" => {
                let mut vertices = [0.0f32; 24];
                if let Some(values) = row.get("vertices").and_then(|v| v.as_array()) {
                    for (slot, value) in vertices.iter_mut().zip(values) {
                        *slot = value.as_f64().unwrap_or(0.0) as f32;
                    }
                }
                MarkerRow::Quad {
                    vertices,
                    texture: texture_field(row),
                    autoscale,
                }
            }
            // Unknown class (newer save): skip rather than draw garbage.
            _ => continue,
        };
        ordered.push((draw_layer, marker));
        let _ = order;
    }
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    ordered.into_iter().map(|(_, marker)| marker).collect()
}
