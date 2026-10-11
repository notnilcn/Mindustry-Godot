// SPDX-License-Identifier: GPL-3.0-only

//! `MapObjectives` world labels (view-only).
//!
//! Ported from `core/src/mindustry/game/MapObjectives.java` marker drawing:
//! `ShapeTextMarker` renders a regular-polygon outline plus a `WorldLabel`
//! (background + outline) at the marker's world position. World coordinates
//! are projected through the camera's canvas transform, so the pass draws in
//! screen space inside a `CanvasLayer` above the world bands and below the
//! HUD (`Ui`).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, Control, INode2D, Node2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::assets::icons::Iconc;
use mind_core::ui::text::markup_runs;

use crate::assets::bundle::load_iconc;
use crate::assets::loader::resolve_assets_dir;
use crate::campaign::MindCampaign;

/// One drawable marker row (world px + WorldLabel style).
#[derive(Debug, Clone)]
struct MarkerRow {
    pos: Vector2,
    text: String,
    radius: f32,
    rotation: f32,
    sides: i32,
    color: Color,
    text_height: f32,
    font_size: f32,
}

/// Screen-space `MapObjectives` world-label pass.
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
        self.accum += delta;
        if self.accum < 0.2 {
            return;
        }
        self.accum = 0.0;
        let Some(source) = self.source.clone() else {
            return;
        };
        let json = source.bind().objective_markers_json().to_string();
        if json != self.last_json {
            self.last_json = json;
            self.markers = parse_rows(&self.last_json);
        }
        if !self.markers.is_empty() {
            self.base_mut().queue_redraw();
        }
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
        let rows = self.markers.clone();
        for marker in &rows {
            let screen = transform * marker.pos;
            // `Lines.poly(pos, sides, (radius + 1) * scaleFactor)`.
            let sides = marker.sides.clamp(3, 300);
            let mut points = PackedVector2Array::new();
            for i in 0..=sides {
                let angle = marker.rotation.to_radians()
                    + (i % sides) as f32 * std::f32::consts::TAU / sides as f32;
                points.push(
                    screen + Vector2::new(angle.cos(), -angle.sin()) * (marker.radius + 1.0) * zoom,
                );
            }
            self.base_mut()
                .draw_polyline_ex(&points, Color::from_rgba(0.53, 0.53, 0.53, 1.0))
                .width(3.0 * zoom * 0.25)
                .done();
            // `Lines.stroke(scaleFactor, color)` over the gray outline.
            self.base_mut()
                .draw_polyline_ex(&points, marker.color)
                .width(1.0 * zoom * 0.25)
                .done();

            let Some(font) = font.clone() else {
                continue;
            };
            // WorldLabel: font scale `0.25 * fontSize` on the 16 px outline
            // font, scaled by the camera zoom; the label centre sits
            // `(radius + textHeight)` world units above the marker.
            let font_size =
                (16.0 * 0.25 * marker.font_size.max(0.1) * zoom).clamp(8.0, 64.0) as i32;
            let label_lift = (marker.radius + marker.text_height) * zoom;
            let line_height = font_size as f32 * 1.2;
            let mut line_index = 0.0;
            for line in marker.text.split('\n') {
                if line.is_empty() {
                    line_index += 1.0;
                    continue;
                }
                // `WorldLabel` draws with `markupEnabled`: coloured runs, icons
                // as content-icon glyphs (font fallback), base colour white.
                let runs = markup_runs(line, &self.iconc);
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
                let height = font.get_height_ex().font_size(font_size).done();
                let ascent = font.get_ascent_ex().font_size(font_size).done();
                let top = Vector2::new(
                    screen.x - width * 0.5,
                    screen.y - height * 0.5 - label_lift + line_index * line_height,
                );
                let background = Rect2::new(
                    top - Vector2::new(6.0, 2.0),
                    Vector2::new(width, height) + Vector2::new(12.0, 4.0),
                );
                self.base_mut()
                    .draw_rect_ex(background, Color::from_rgba(0.0, 0.0, 0.0, 0.3))
                    .done();
                let mut x = top.x;
                for (run, run_width) in runs.iter().zip(&widths) {
                    let origin = Vector2::new(x, top.y + ascent);
                    let text = GString::from(run.text.as_str());
                    let color = run.color.map_or(Color::WHITE, |c| {
                        Color::from_rgba8(
                            (c >> 24) as u8,
                            (c >> 16) as u8,
                            (c >> 8) as u8,
                            c as u8,
                        )
                    });
                    self.base_mut()
                        .draw_string_outline_ex(&font, origin, &text)
                        .font_size(font_size)
                        .size(4)
                        .modulate(Color::BLACK)
                        .done();
                    self.base_mut()
                        .draw_string_ex(&font, origin, &text)
                        .font_size(font_size)
                        .modulate(color)
                        .done();
                    x += run_width;
                }
                line_index += 1.0;
            }
        }
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

fn parse_rows(json: &str) -> Vec<MarkerRow> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let color = row.get("color")?.as_array()?;
            Some(MarkerRow {
                pos: Vector2::new(
                    row.get("x")?.as_f64()? as f32,
                    row.get("y")?.as_f64()? as f32,
                ),
                text: row.get("text")?.as_str()?.to_owned(),
                radius: row.get("radius").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32,
                rotation: row.get("rotation").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                sides: row.get("sides").and_then(|v| v.as_i64()).unwrap_or(4) as i32,
                color: Color::from_rgba(
                    color.first()?.as_f64()? as f32,
                    color.get(1)?.as_f64()? as f32,
                    color.get(2)?.as_f64()? as f32,
                    color.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                ),
                text_height: row
                    .get("textHeight")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(7.0) as f32,
                font_size: row.get("fontSize").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            })
        })
        .collect()
}
