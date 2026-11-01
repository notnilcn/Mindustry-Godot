// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/HudFragment.java (`makeStatusTable`:
//         the hexagon backdrop, the two `SideBar` health chevrons and the
//         `player.icon()` image, stacked in a 120x80 cell).
//
// Read-only HUD widget: polls `MindSimHost.player_unit_status()` and draws the
// player unit portrait inside the health hexagon.

//! `MindPlayerStatus` — the HUD player portrait with the two `SideBar`
//! health chevrons (`HudFragment.makeStatusTable`).
//!
//! The widget only reads `MindSimHost.player_unit_status()`; `_draw` mirrors
//! the Java `Fill.poly` hexagon, the `Scaling.bounded` portrait and Arc's
//! `SideBar.drawInner` quads.

use godot::builtin::{PackedVector2Array, VarDictionary};
use godot::classes::control::MouseFilter;
use godot::classes::{Control, IControl, Texture2D};
use godot::prelude::*;

use crate::ui::kit;

/// `Pal.darkerGray` hexagon fill.
const HEX_COLOR: Color = Color::from_rgba(0.2, 0.2, 0.2, 1.0);
/// `Pal.darkishGray` empty-bar backdrop.
const BACK_COLOR: Color = Color::from_rgba(0.3, 0.3, 0.3, 1.0);
/// `Pal.health` (`Color("ff341c")`).
const HEALTH_COLOR: Color = Color::from_rgba8(0xff, 0x34, 0x1c, 0xff);
/// Table layout: `bw = 40`, `pad = -20`, image `maxWidth(54)`, centred in 120.
const BAR_WIDTH: f32 = 40.0;
const BAR_PAD: f32 = -20.0;
const IMAGE_MAX_WIDTH: f32 = 54.0;
/// `MindSimHost` (`scenes/game.tscn`).
const HOST_PATH: &str = "/root/Spine/SimHost";

/// `MindPlayerStatus` — 120x80 read-only player status cell.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindPlayerStatus {
    base: Base<Control>,
    /// The eased health fraction the bars chase.
    #[init(val = 1.0)]
    healthf: f32,
    #[init(val = true)]
    dead: bool,
    /// The player unit's content name (empty while dead).
    type_name: String,
    /// `player.icon()` region (`unit-<name>-ui`).
    icon: Option<Gd<Texture2D>>,
    /// The name `icon` was resolved for (dead players show `alpha`).
    icon_type: String,
    /// The `circle-shadow` region under the portrait.
    shadow: Option<Gd<Texture2D>>,
    /// Per-bar `SideBar` animation state: `[last, blink, value]`.
    #[init(val = [[1.0, 0.0, 1.0], [1.0, 0.0, 1.0]])]
    bars: [[f32; 3]; 2],
}

#[godot_api]
impl IControl for MindPlayerStatus {
    fn ready(&mut self) {
        self.base_mut()
            .set_custom_minimum_size(Vector2::new(120.0, 80.0));
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
    }

    fn process(&mut self, delta: f64) {
        if let Some(mut host) = kit::node(HOST_PATH)
            && host.has_method("player_unit_status")
            && let Ok(status) = host
                .call("player_unit_status", &[])
                .try_to::<VarDictionary>()
        {
            self.dead = status_value(&status, "dead")
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(true);
            self.healthf = status_value(&status, "healthf")
                .and_then(|value| value.try_to::<f32>().ok())
                .unwrap_or(0.0);
            self.type_name = status_value(&status, "type")
                .and_then(|value| value.try_to::<GString>().ok())
                .map(|value| value.to_string())
                .unwrap_or_default();
        }
        // `player.icon()`: dead players show the default (`alpha`) icon.
        let shown = if self.dead || self.type_name.is_empty() {
            "alpha".to_string()
        } else {
            self.type_name.clone()
        };
        if shown != self.icon_type {
            self.icon_type = shown;
            self.icon = region(&format!("unit-{}-ui", self.icon_type));
        }
        if self.shadow.is_none() {
            self.shadow = region("circle-shadow");
        }
        let amount = if self.dead { 0.0 } else { self.healthf };
        let mut bars = self.bars;
        for bar in &mut bars {
            step_bar(bar, amount, delta as f32);
        }
        self.bars = bars;
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let mut canvas: Gd<Control> = self.to_gd().upcast();
        let size = canvas.get_size();
        let center = size * 0.5;
        // `Fill.poly(cx, cy, 6, height / sqrt3)` (pointy left/right) + the
        // shadow.
        let radius = size.y / 3.0f32.sqrt();
        let mut hex = PackedVector2Array::new();
        for index in 0..6 {
            let angle = std::f32::consts::TAU * index as f32 / 6.0;
            hex.push(center + Vector2::new(angle.cos(), angle.sin()) * radius);
        }
        canvas.draw_colored_polygon(&hex, HEX_COLOR);
        if let Some(shadow) = self.shadow.clone() {
            let side = size.y * 1.13;
            canvas
                .draw_texture_rect_ex(
                    &shadow,
                    Rect2::new(
                        center - Vector2::new(side, side) * 0.5,
                        Vector2::new(side, side),
                    ),
                    false,
                )
                .modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.4))
                .done();
        }
        let total = BAR_WIDTH + BAR_PAD + IMAGE_MAX_WIDTH + BAR_PAD + BAR_WIDTH;
        let left = (size.x - total) * 0.5;
        let image_x = left + BAR_WIDTH + BAR_PAD;
        let right_x = image_x + IMAGE_MAX_WIDTH + BAR_PAD;
        // Left bar is `flip = true`; the image is brought to the front last.
        draw_bar(
            &mut canvas,
            Rect2::new(Vector2::new(left, 0.0), Vector2::new(BAR_WIDTH, size.y)),
            self.bars[0],
            true,
        );
        draw_bar(
            &mut canvas,
            Rect2::new(Vector2::new(right_x, 0.0), Vector2::new(BAR_WIDTH, size.y)),
            self.bars[1],
            false,
        );
        if let Some(icon) = self.icon.clone() {
            // `Scaling.bounded`: shrink to fit, never upscale.
            let tex_size = icon.get_size();
            let bound = Vector2::new(IMAGE_MAX_WIDTH, size.y);
            let zoom = 1.0f32.min((bound.x / tex_size.x).min(bound.y / tex_size.y));
            let drawn = tex_size * zoom;
            canvas.draw_texture_rect(
                &icon,
                Rect2::new(Vector2::new(image_x, 0.0) + (bound - drawn) * 0.5, drawn),
                false,
            );
        }
    }
}

/// Reads one `player_unit_status` field (GDScript `status.get(key)`).
fn status_value(status: &VarDictionary, key: &str) -> Option<Variant> {
    let key = key.to_variant();
    status.get(&key)
}

/// `player.icon()` / `MindWidgets.icon_texture`: the atlas region, `None` when
/// absent.
fn region(name: &str) -> Option<Gd<Texture2D>> {
    kit::icon_texture(name)
}

/// `SideBar.draw` easing: blink on a drop, `lerpDelta` towards the new value.
fn step_bar(bar: &mut [f32; 3], next: f32, delta: f32) {
    let mut next = next;
    if next.is_nan() || next.is_infinite() {
        next = 1.0;
    }
    if next < bar[0] {
        bar[1] = 1.0;
    }
    let frames = delta * 60.0;
    bar[1] = lerp_f32(bar[1], 0.0, 1.0 - (1.0 - 0.2f32).powf(frames));
    bar[2] = lerp_f32(bar[2], next, 1.0 - (1.0 - 0.15f32).powf(frames));
    bar[0] = next;
}

/// GDScript `lerpf` (`from + (to - from) * weight`, never clamped).
fn lerp_f32(from: f32, to: f32, weight: f32) -> f32 {
    from + (to - from) * weight
}

/// Filled backdrop plus the eased health chevron.
fn draw_bar(canvas: &mut Gd<Control>, rect: Rect2, bar: [f32; 3], flip: bool) {
    draw_inner(canvas, rect, BACK_COLOR, 1.0, flip);
    draw_inner(
        canvas,
        rect,
        HEALTH_COLOR.lerp(Color::WHITE, f64::from(bar[1])),
        bar[2],
        flip,
    );
}

/// `SideBar.drawInner` in Arc's y-up space, mapped into the rect (y-down).
fn draw_inner(canvas: &mut Gd<Control>, rect: Rect2, color: Color, fract: f32, flip: bool) {
    if fract < 0.0 {
        return;
    }
    let fract = fract.clamp(0.0, 1.0);
    let w = rect.size.x;
    let h = rect.size.y;
    let stroke = w * 0.35;
    let bh = h / 2.0;
    let f1 = (fract * 2.0).min(1.0);
    let f2 = (fract - 0.5) * 2.0;
    let bo = -(1.0 - f1) * (w - stroke);
    draw_quad(
        canvas,
        rect,
        flip,
        color,
        [
            Vector2::new(0.0, 0.0),
            Vector2::new(stroke, 0.0),
            Vector2::new(w + bo, bh * f1),
            Vector2::new(w - stroke + bo, bh * f1),
        ],
    );
    if f2 > 0.0 {
        let bx = (w - stroke) * (1.0 - f2);
        draw_quad(
            canvas,
            rect,
            flip,
            color,
            [
                Vector2::new(w, bh),
                Vector2::new(w - stroke, bh),
                Vector2::new(bx, h * fract),
                Vector2::new(bx + stroke, h * fract),
            ],
        );
    }
}

/// `Fill.quad` in Arc's y-up space, mapped into the y-down rect.
fn draw_quad(
    canvas: &mut Gd<Control>,
    rect: Rect2,
    flip: bool,
    color: Color,
    points: [Vector2; 4],
) {
    let mut out = [Vector2::ZERO; 4];
    for (index, point) in points.iter().enumerate() {
        let x = if flip { rect.size.x - point.x } else { point.x };
        out[index] = rect.position + Vector2::new(x, rect.size.y - point.y);
    }
    // `Fill.quad` winds either way; Godot needs a non-degenerate polygon.
    let edge_a = out[1] - out[0];
    let edge_b = out[2] - out[0];
    let edge_c = out[3] - out[0];
    if edge_a.cross(edge_b).abs() < 0.001 && edge_b.cross(edge_c).abs() < 0.001 {
        return;
    }
    let mut polygon = PackedVector2Array::new();
    for point in out {
        polygon.push(point);
    }
    canvas.draw_colored_polygon(&polygon, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_bar_blinks_on_a_drop_and_eases_down() {
        // A drop flips the blink flag back to 1 (`SideBar.draw`), then both the
        // blink and the value ease towards their targets.
        let mut bar = [1.0f32, 0.0, 1.0];
        step_bar(&mut bar, 0.5, 1.0 / 60.0);
        // The drop resets the blink to 1, which the same call eases to 0.8.
        assert_eq!(bar[1], 0.8);
        assert!(bar[2] < 1.0 && bar[2] > 0.5);
        assert_eq!(bar[0], 0.5);
        // No drop: the blink keeps fading from where it was.
        let mut healed = [0.5f32, 0.0, 0.5];
        step_bar(&mut healed, 0.75, 1.0 / 60.0);
        assert_eq!(healed[1], 0.0);
    }

    #[test]
    fn step_bar_treats_nan_and_infinity_as_full_health() {
        let mut bar = [0.5f32, 0.0, 0.5];
        step_bar(&mut bar, f32::NAN, 1.0 / 60.0);
        assert_eq!(bar[0], 1.0);
        let mut bar = [0.5f32, 0.0, 0.5];
        step_bar(&mut bar, f32::INFINITY, 1.0 / 60.0);
        assert_eq!(bar[0], 1.0);
    }

    #[test]
    fn lerp_matches_gdscript_lerpf() {
        assert_eq!(lerp_f32(1.0, 0.0, 0.25), 0.75);
        // No clamping (GDScript `lerpf` extrapolates).
        assert_eq!(lerp_f32(0.0, 1.0, 2.0), 2.0);
    }

    #[test]
    fn bar_layout_is_centred_in_the_cell() {
        // total = 40 - 20 + 54 - 20 + 40 = 94; a 120 px cell leaves 13 px.
        let total = BAR_WIDTH + BAR_PAD + IMAGE_MAX_WIDTH + BAR_PAD + BAR_WIDTH;
        assert_eq!(total, 94.0);
        assert_eq!((120.0 - total) * 0.5, 13.0);
    }
}
