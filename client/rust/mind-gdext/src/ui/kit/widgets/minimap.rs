// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Minimap.java (plan 14 §3.3/§3.5, M4).
//
// 140 px minimap widget: right-drag pans the camera, the wheel zooms, a left tap
// toggles fullscreen. Terrain pixels come from the native `MindMinimap`
// provider (`World/MinimapProvider`); the camera-region border is drawn from its
// `region()`. Pan/zoom/tap are emitted as intents so the renderer hook and
// plan-15 camera stay in their lanes.
//
// Named `MindMinimapWidget` (not `MindMinimap`) because plan 16 already
// registers the native `MindMinimap` provider node (render/minimap.rs); this is
// the plan-14 interactive control that consumes it.

//! `MindMinimapWidget` — the interactive minimap control.
//!
//! The HUD copy (`crop_to_region`) draws only the camera-centred crop inside a
//! `Tex.pane` frame and routes the wheel to `MinimapRenderer.zoomBy`; the
//! fullscreen fragment leaves the flag off and draws the whole map with the
//! camera-region outline, the pulsing attack indicators and the team-tinted
//! unit silhouettes.
//!
//! All camera movement is emitted as [`MindMinimapWidget::pan_requested`],
//! [`MindMinimapWidget::zoom_changed`] and [`MindMinimapWidget::tapped`]
//! intents; the widget never touches the sim or the camera itself.

use std::collections::HashMap;

use godot::builtin::Side;
use godot::classes::control::MouseFilter;
use godot::classes::image::Format;
use godot::classes::{
    Control, IControl, ImageTexture, InputEvent, InputEventMouseButton, InputEventMouseMotion,
    Node, StyleBox, StyleBoxTexture, Texture2D, Time,
};
use godot::global::MouseButton;
use godot::obj::{NewGd, Singleton};
use godot::prelude::*;

use crate::ui::kit;

/// Upstream 140 px size.
const BASE_SIZE: f32 = 140.0;
/// `Minimap` table margin around the map element (`margin(5f)`).
const PANE_MARGIN: f32 = 5.0;
/// `Minimap` zoom clamp (plan §7c step 10).
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 10.0;
/// `mind_core::config::TILESIZE` (world pixels per minimap texture pixel).
const TILE_SIZE: f32 = 8.0;
/// Provider node path (`scenes/game.tscn`).
const PROVIDER_PATH: &str = "/root/Spine/World/MinimapProvider";
/// `MinimapRenderer.drawEntities` unit icon size (`icon_size = 8`).
const UNIT_ICON_SIZE: f32 = 8.0;
/// Godot's `CMP_EPSILON` (`is_equal_approx` tolerance floor).
const CMP_EPSILON: f32 = 0.00001;

/// `MindMinimapWidget` — 140 px map crop with pan/zoom/tap intents.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindMinimapWidget {
    base: Base<Control>,
    /// HUD minimap (`ui/Minimap.java`): draw only `renderer.minimap.getRegion()`
    /// (the camera-centred crop) inside a `Tex.pane` frame, and route the wheel
    /// to `MinimapRenderer.zoomBy`. The fullscreen fragment leaves this off and
    /// draws the whole map with the camera-region outline.
    #[export]
    pub crop_to_region: bool,
    /// The absolute widget zoom (`set_zoom` clamps it to
    /// [`MIN_ZOOM`]..[`MAX_ZOOM`] and emits `zoom_changed`). Not a `#[var]`:
    /// the generated property setter would collide with the `#[func]
    /// set_zoom` the fragments call.
    #[init(val = 1.0)]
    pub zoom: f32,
    /// Full map extent in world pixels (updated from the provider texture size).
    #[var]
    #[init(val = Vector2::new(200.0, 200.0))]
    pub map_extent: Vector2,
    dragging: bool,
    drag_last: Vector2,
    /// Cached `PROVIDER_PATH` node (re-resolved when it is freed).
    provider: Option<Gd<Node>>,
    /// Cached `Tex.pane` ninepatch (built once).
    pane: Option<Gd<StyleBox>>,
    /// `unit-<name>-ui` white masks for the `mixcol` unit draw
    /// (`unit_silhouette`).
    silhouettes: HashMap<String, Option<Gd<Texture2D>>>,
}

#[godot_api]
impl IControl for MindMinimapWidget {
    fn ready(&mut self) {
        let side = BASE_SIZE
            + if self.crop_to_region {
                PANE_MARGIN * 2.0
            } else {
                0.0
            };
        self.base_mut()
            .set_custom_minimum_size(Vector2::new(side, side));
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        // The provider rebuilds its texture/world revision and the camera region
        // moves every frame; redraw so both stay live.
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let Some(texture) = self.native_texture() else {
            self.draw_placeholder();
            return;
        };
        // World pixels covered by the full minimap texture.
        self.map_extent = texture.get_size() * TILE_SIZE;
        if self.crop_to_region {
            self.draw_cropped(&texture);
            return;
        }
        let size = self.base().get_size();
        let rect = Rect2::new(Vector2::ZERO, size);
        self.base_mut()
            .draw_rect(rect, Color::from_rgba(0.0, 0.0, 0.0, 0.6));
        self.base_mut().draw_texture_rect(&texture, rect, false);
        self.draw_region(&texture);
        self.base_mut()
            .draw_rect_ex(rect, Color::from_rgba(0.8, 0.8, 0.8, 0.5))
            .filled(false)
            .width(1.0)
            .done();
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
            let pressed = button.is_pressed();
            let index = button.get_button_index();
            let wheel = if index == MouseButton::WHEEL_UP && pressed {
                -1.0f32
            } else if index == MouseButton::WHEEL_DOWN && pressed {
                1.0f32
            } else {
                0.0f32
            };
            if wheel != 0.0 {
                if self.crop_to_region {
                    // `renderer.minimap.zoomBy(amounty)`.
                    if let Some(mut node) = self.provider_node()
                        && node.has_method("zoom_by")
                    {
                        node.call("zoom_by", &[f64::from(wheel).to_variant()]);
                    }
                } else if wheel < 0.0 {
                    self.set_zoom(self.zoom * 1.1);
                } else {
                    self.set_zoom(self.zoom / 1.1);
                }
                self.base_mut().accept_event();
                return;
            }
            if index == MouseButton::RIGHT {
                self.dragging = pressed;
                self.drag_last = button.get_position();
                self.base_mut().accept_event();
            } else if index == MouseButton::LEFT && pressed {
                self.base_mut().emit_signal("tapped", &[]);
                self.base_mut().accept_event();
            }
            return;
        }
        if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
            && self.dragging
        {
            let delta = motion.get_position() - self.drag_last;
            self.drag_last = motion.get_position();
            let pan_scale = self.map_extent / self.base().get_size();
            let pan = -delta * pan_scale;
            self.base_mut()
                .emit_signal("pan_requested", &[pan.to_variant()]);
            self.base_mut().accept_event();
        }
    }
}

#[godot_api]
impl MindMinimapWidget {
    /// Right-drag world delta (world pixels), applied by the camera host.
    #[signal]
    fn pan_requested(world_delta: Vector2);

    /// Absolute zoom factor after [`Self::set_zoom`] clamped it.
    #[signal]
    fn zoom_changed(factor: f32);

    /// A left tap on the map (`MinimapFragment.toggle`).
    #[signal]
    fn tapped();

    /// Sets the zoom (clamped, emits `zoom_changed`).
    #[func]
    pub fn set_zoom(&mut self, value: f32) {
        let clamped = value.clamp(MIN_ZOOM, MAX_ZOOM);
        if is_equal_approx(clamped, self.zoom) {
            return;
        }
        self.zoom = clamped;
        let factor = self.zoom;
        self.base_mut()
            .emit_signal("zoom_changed", &[factor.to_variant()]);
    }

    /// `Minimap.draw`: `Draw.rect(renderer.minimap.getRegion(), ...)` over the
    /// `Tex.pane` background (5 px margin).
    fn draw_cropped(&mut self, texture: &Gd<Texture2D>) {
        let size = self.base().get_size();
        let full = Rect2::new(Vector2::ZERO, size);
        match self.pane_box() {
            Some(pane) => self.base_mut().draw_style_box(&pane, full),
            None => {
                self.base_mut()
                    .draw_rect(full, Color::from_rgba(0.0, 0.0, 0.0, 1.0));
                self.base_mut()
                    .draw_rect_ex(full, Color::from_rgba(0.27, 0.27, 0.27, 1.0))
                    .filled(false)
                    .width(3.0)
                    .done();
            }
        }
        let margin = Vector2::new(PANE_MARGIN, PANE_MARGIN);
        let inner = Rect2::new(margin, size - margin * 2.0);
        self.base_mut().draw_rect(inner, Color::BLACK);
        let texture_size = texture.get_size();
        let region = self
            .provider_region()
            .unwrap_or(Rect2::new(Vector2::ZERO, texture_size));
        // The crop may extend past a map smaller than the view; clip to the
        // texture and keep the scale so the map stays square-pixelled.
        let clipped = region
            .intersect(Rect2::new(Vector2::ZERO, texture_size))
            .unwrap_or(Rect2::new(Vector2::ZERO, Vector2::ZERO));
        if clipped.size.x <= 0.0 || clipped.size.y <= 0.0 || region.size.x <= 0.0 {
            return;
        }
        let render_scale = inner.size / region.size;
        let dest = Rect2::new(
            inner.position + (clipped.position - region.position) * render_scale,
            clipped.size * render_scale,
        );
        self.base_mut()
            .draw_texture_rect_region(texture, dest, clipped);
        self.draw_markers(inner, region, render_scale);
    }

    /// `MinimapRenderer.drawEntities` + `AttackIndicators.draw`: team-tinted
    /// unit icons and the pulsing damage squares, mapped into the crop.
    fn draw_markers(&mut self, inner: Rect2, region: Rect2, render_scale: Vector2) {
        let Some(mut node) = self.provider_node() else {
            return;
        };
        let Some(texture) = self.native_texture() else {
            return;
        };
        let tex_size = texture.get_size();
        self.draw_indicators(&mut node, inner, region, tex_size, render_scale);
        self.draw_units(&mut node, inner, region, tex_size);
    }

    /// Attack indicators: `fin = (Time.globalTime / 30f) % 1f`,
    /// `rad = fin * 5 + tilesize - 2`, stroke `(1 - fin) * 4 + 0.5`, colour
    /// `orange.lerp(scarlet, clamp(time / 70f))` scaled into widget px.
    fn draw_indicators(
        &mut self,
        node: &mut Gd<Node>,
        inner: Rect2,
        region: Rect2,
        tex_size: Vector2,
        render_scale: Vector2,
    ) {
        if !node.has_method("indicators_json") {
            return;
        }
        let Ok(raw) = node.call("indicators_json", &[]).try_to::<GString>() else {
            return;
        };
        let text = raw.to_string();
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let Some(rows) = parsed.as_array() else {
            return;
        };
        let now_s = Time::singleton().get_ticks_msec() as f64 / 1000.0;
        let fin = ((now_s * 2.0) % 1.0) as f32;
        let world_to_widget = render_scale.x / TILE_SIZE;
        for row in rows {
            let Some(row) = row.as_array() else {
                continue;
            };
            if row.len() < 3 {
                continue;
            }
            // `offset = build.block.offset / tilesize` centres multiblock
            // squares (`MinimapRenderer.java:266-271`; 0 when no building).
            let offset = row
                .get(3)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            let tx = row
                .first()
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32
                + offset;
            let ty = row
                .get(1)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32
                + offset;
            let marker_pos = map_point(inner, region, tex_size, tx, ty);
            let rad = (fin * 5.0 + TILE_SIZE - 2.0) * world_to_widget;
            let stroke = ((1.0 - fin) * 4.0 + 0.5) * world_to_widget;
            let age = row
                .get(2)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            let red = (age / 70.0).clamp(0.0, 1.0);
            let color = Color::from_rgba(1.0, 0.647, 0.0, 1.0)
                .lerp(Color::from_rgba(1.0, 0.0, 0.196, 1.0), f64::from(red));
            self.base_mut()
                .draw_rect_ex(
                    Rect2::new(
                        marker_pos - Vector2::new(rad, rad),
                        Vector2::new(rad, rad) * 2.0,
                    ),
                    color,
                )
                .filled(false)
                .width(stroke)
                .done();
        }
    }

    /// Units (`drawEntities`, `MinimapRenderer.java:157-166`): `unit.icon()`
    /// drawn under `Draw.mixcol(unit.team.color, 1f)` — a solid team-colour
    /// silhouette (a white alpha mask modulated by the team colour).
    fn draw_units(&mut self, node: &mut Gd<Node>, inner: Rect2, region: Rect2, tex_size: Vector2) {
        if !node.has_method("minimap_units_json") {
            return;
        }
        let Ok(raw) = node.call("minimap_units_json", &[]).try_to::<GString>() else {
            return;
        };
        let text = raw.to_string();
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let Some(rows) = parsed.as_array() else {
            return;
        };
        for row in rows {
            let Some(row) = row.as_array() else {
                continue;
            };
            if row.len() < 5 {
                continue;
            }
            let unit_name = row[3].as_str().unwrap_or_default().to_string();
            let Some(texture_icon) = self.unit_silhouette(&unit_name) else {
                continue;
            };
            let x = row[0].as_f64().unwrap_or(0.0) as f32 / TILE_SIZE;
            let y = row[1].as_f64().unwrap_or(0.0) as f32 / TILE_SIZE;
            let marker_pos = map_point(inner, region, tex_size, x, y);
            let tint = row[4]
                .as_str()
                .and_then(color_from_hex)
                .unwrap_or(Color::WHITE);
            let rotation = row[2].as_f64().unwrap_or(0.0) as f32;
            self.base_mut()
                .draw_set_transform_ex(marker_pos)
                .rotation((rotation - 90.0).to_radians())
                .scale(Vector2::ONE)
                .done();
            self.base_mut()
                .draw_texture_rect_ex(
                    &texture_icon,
                    Rect2::new(
                        -Vector2::new(UNIT_ICON_SIZE, UNIT_ICON_SIZE) / 2.0,
                        Vector2::new(UNIT_ICON_SIZE, UNIT_ICON_SIZE),
                    ),
                    false,
                )
                .modulate(tint)
                .done();
            self.base_mut().draw_set_transform(Vector2::ZERO);
        }
    }

    /// `Draw.mixcol(color, 1f)` source: the `unit-<name>-ui` icon as a white
    /// mask (rgb = 1, alpha kept), so the draw modulate yields the team colour
    /// exactly. Cached per unit type.
    fn unit_silhouette(&mut self, unit_name: &str) -> Option<Gd<Texture2D>> {
        if let Some(cached) = self.silhouettes.get(unit_name) {
            return cached.clone();
        }
        let mut result: Option<Gd<Texture2D>> = None;
        if let Some(icon) = kit::icon_texture(&format!("unit-{unit_name}-ui"))
            && let Some(mut image) = icon.get_image()
            && !image.is_empty()
        {
            if image.is_compressed() {
                image.decompress();
            }
            image.convert(Format::RGBA8);
            for y in 0..image.get_height() {
                for x in 0..image.get_width() {
                    let alpha = image.get_pixel(x, y).a;
                    image.set_pixel(x, y, Color::from_rgba(1.0, 1.0, 1.0, alpha));
                }
            }
            if let Some(texture) = ImageTexture::create_from_image(&image) {
                result = Some(texture.upcast());
            }
        }
        self.silhouettes
            .insert(unit_name.to_string(), result.clone());
        result
    }

    /// `Tex.pane` ninepatch (36x27, splits 12) from the atlas, built once.
    fn pane_box(&mut self) -> Option<Gd<StyleBox>> {
        if let Some(pane) = &self.pane {
            return Some(pane.clone());
        }
        let texture = kit::icon_texture("pane")?;
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, 12.0);
        box_.set_texture_margin(Side::TOP, 12.0);
        box_.set_texture_margin(Side::RIGHT, 12.0);
        box_.set_texture_margin(Side::BOTTOM, 12.0);
        let pane: Gd<StyleBox> = box_.upcast();
        self.pane = Some(pane.clone());
        Some(pane)
    }

    /// Camera crop indicator (`MinimapRenderer.getRegion` scaled to the widget).
    fn draw_region(&mut self, texture: &Gd<Texture2D>) {
        let Some(region) = self.provider_region() else {
            return;
        };
        let texture_size = texture.get_size();
        if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
            return;
        }
        let size = self.base().get_size();
        let scaled = Rect2::new(
            region.position / texture_size * size,
            region.size / texture_size * size,
        );
        self.base_mut()
            .draw_rect_ex(scaled, Color::from_rgba(1.0, 1.0, 1.0, 0.8))
            .filled(false)
            .width(1.0)
            .done();
    }

    fn draw_placeholder(&mut self) {
        let size = self.base().get_size();
        let rect = Rect2::new(Vector2::ZERO, size);
        self.base_mut()
            .draw_rect(rect, Color::from_rgba(0.0, 0.0, 0.0, 0.6));
        let step = 20.0;
        let mut x = step;
        while x < size.x {
            self.base_mut().draw_line(
                Vector2::new(x, 0.0),
                Vector2::new(x, size.y),
                Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            );
            x += step;
        }
        let mut y = step;
        while y < size.y {
            self.base_mut().draw_line(
                Vector2::new(0.0, y),
                Vector2::new(size.x, y),
                Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            );
            y += step;
        }
        self.base_mut()
            .draw_rect_ex(rect, Color::from_rgba(0.8, 0.8, 0.8, 0.5))
            .filled(false)
            .width(1.0)
            .done();
    }

    fn provider_node(&mut self) -> Option<Gd<Node>> {
        if self
            .provider
            .as_ref()
            .is_none_or(|node| !node.is_instance_valid())
        {
            self.provider = kit::node(PROVIDER_PATH);
        }
        self.provider.clone()
    }

    fn native_texture(&mut self) -> Option<Gd<Texture2D>> {
        let mut node = self.provider_node()?;
        if !node.has_method("get_texture") {
            return None;
        }
        node.call("get_texture", &[]).try_to::<Gd<Texture2D>>().ok()
    }

    /// The provider's camera crop (`MinimapRenderer.getRegion`), when the
    /// provider exposes one.
    fn provider_region(&mut self) -> Option<Rect2> {
        let mut node = self.provider_node()?;
        if !node.has_method("region") {
            return None;
        }
        node.call("region", &[]).try_to::<Rect2>().ok()
    }
}

/// Tile `(tx, ty)` (world-pixel centre 0.5 offsets) -> widget px inside the crop
/// (`MinimapRenderer` draws texture row 0 at the map top).
fn map_point(inner: Rect2, region: Rect2, tex_size: Vector2, tx: f32, ty: f32) -> Vector2 {
    let texture_point = Vector2::new(tx + 0.5, (tex_size.y - 1.0 - ty) + 0.5);
    inner.position + (texture_point - region.position) * (inner.size / region.size)
}

/// `Color(str)` for the provider's `RRGGBBAA` team colour (`Color.from_string`
/// first, like the Godot constructor).
fn color_from_hex(text: &str) -> Option<Color> {
    Color::from_string(text).or_else(|| Color::from_html(text))
}

/// Godot's `is_equal_approx`: exact equality first (so infinities compare
/// equal), then a relative `CMP_EPSILON` tolerance.
fn is_equal_approx(first: f32, second: f32) -> bool {
    if first == second {
        return true;
    }
    let mut tolerance = CMP_EPSILON * first.abs();
    if tolerance < CMP_EPSILON {
        tolerance = CMP_EPSILON;
    }
    (first - second).abs() < tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_clamps_to_the_minimap_range() {
        assert_eq!(0.1f32.clamp(MIN_ZOOM, MAX_ZOOM), 0.25);
        assert_eq!(42.0f32.clamp(MIN_ZOOM, MAX_ZOOM), 10.0);
        assert_eq!(1.0f32.clamp(MIN_ZOOM, MAX_ZOOM), 1.0);
    }

    #[test]
    fn equality_matches_godot_is_equal_approx() {
        assert!(is_equal_approx(1.0, 1.0));
        assert!(is_equal_approx(f32::INFINITY, f32::INFINITY));
        assert!(!is_equal_approx(1.0, 1.1));
        // A relative tolerance: 5e-5 off at 10.0 is inside 1e-5 * 10.
        assert!(is_equal_approx(10.0, 10.000_05));
        // ... while 1.1e-4 off is outside it.
        assert!(!is_equal_approx(10.0, 10.000_11));
    }

    #[test]
    fn map_point_flips_the_texture_row() {
        // Full texture: tile (0, 0) is the bottom-left of the map, so it lands
        // at the widget bottom after the row flip.
        let inner = Rect2::new(Vector2::ZERO, Vector2::new(100.0, 100.0));
        let region = Rect2::new(Vector2::ZERO, Vector2::new(10.0, 10.0));
        let tex_size = Vector2::new(10.0, 10.0);
        let top_left = map_point(inner, region, tex_size, 0.0, 9.0);
        let bottom_left = map_point(inner, region, tex_size, 0.0, 0.0);
        assert!(top_left.y < bottom_left.y);
        assert_eq!(top_left.x, bottom_left.x);
    }
}
