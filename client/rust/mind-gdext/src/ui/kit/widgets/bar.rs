// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Bar.java (plan 14 §3.3).

//! `MindBar` — the HUD/stat bar (`Bar`).
//!
//! The `Tex.bar` 9-patch backing, the `Tex.barTop` 9-patch fill tinted by the
//! value colour (lerped, with a blink flash when the value drops), the optional
//! outline, and a centred `Fonts.outline` label. The owner assigns `fraction`
//! every tick; `process` eases the drawn value with `Mathf.lerpDelta` in 60 Hz
//! ticks, exactly as the GDScript port did.

use godot::classes::{Control, IControl, Texture2D};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use super::super::{assets, icon_texture};

/// `Fonts.outline` border (`ui/Fonts.java:247-249, 264-271`): 2 px of
/// `Color.darkGray`. Godot's glyph stroke radius is `outline_size / 4` px (see
/// `MindThemeBuilder` `outlineLabel`), so 8 draws Java's 2 px border.
pub const LABEL_OUTLINE_SIZE: i32 = 8;
/// `Color.darkGray` (0x3f3f3fff).
pub const LABEL_OUTLINE_COLOR: Color =
    Color::from_rgba(63.0 / 255.0, 63.0 / 255.0, 63.0 / 255.0, 1.0);
/// Pack splits `[left, right, top, bottom]` of `bar.9.png`, used when the atlas
/// carries none.
pub const BAR_SPLITS: [i32; 4] = [9, 9, 9, 9];
/// Pack splits `[left, right, top, bottom]` of `bar-top.9.png`.
pub const BAR_TOP_SPLITS: [i32; 4] = [9, 10, 9, 10];

/// `Draw.colorl(0.1f)`: the backing is the `bar` patch at 10% grey.
const BACKING_COLOR: Color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
/// `Bar.draw()`'s blink ease rate (`Mathf.lerpDelta(blink, 0, 0.2f)`).
const BLINK_RATE: f64 = 0.2;
/// `Bar.draw()`'s value ease rate (`Mathf.lerpDelta(value, computed, 0.15f)`).
const VALUE_RATE: f64 = 0.15;

/// `MindBar` — a themed 9-patch value bar with a centred name label.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindBar {
    base: Base<Control>,
    /// The centered `Fonts.outline` label; empty draws no text.
    #[var]
    pub bar_name: GString,
    /// The target fraction (`Bar`'s value source, `0..=1`).
    #[var]
    pub fraction: f64,
    /// The value colour (`Bar.color`).
    #[var]
    pub bar_color: Color,
    /// The flash colour (`Bar.blinkColor`; starts as the bar colour).
    #[var]
    pub blink_color: Color,
    /// Optional outline colour (`Bar.outline`).
    #[var]
    pub outline_color: Color,
    /// Outline stroke width in px; `0` draws no outline.
    #[var]
    pub outline_radius: f32,
    /// The eased fill fraction actually drawn (`Bar.value`).
    value: f64,
    /// The fraction the last drop was detected from (`Bar.lastValue`).
    last_value: f64,
    /// The flash weight (`Bar.blink`), eased to 0 every frame.
    blink: f64,
}

/// One `Mathf.lerpDelta` step: `lerp(from, to, clamp(rate * delta * 60))`.
///
/// The GDScript port approximates `Time.delta` with `delta * 60.0` ticks; the
/// clamp is what makes a single long frame snap instead of overshooting.
fn lerp_delta(from: f64, to: f64, rate: f64, delta: f64) -> f64 {
    let ticks = delta * 60.0;
    let weight = (rate * ticks).clamp(0.0, 1.0);
    from + (to - from) * weight
}

#[godot_api]
impl IControl for MindBar {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            bar_name: GString::new(),
            fraction: 0.0,
            bar_color: Color::WHITE,
            blink_color: Color::WHITE,
            outline_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            outline_radius: 0.0,
            value: 0.0,
            last_value: 0.0,
            blink: 0.0,
        }
    }

    /// The animation half of `Bar.draw()` (`ui/Bar.java:77-98`): a drop below
    /// the last value starts a blink; blink and value ease with
    /// `Mathf.lerpDelta`.
    fn process(&mut self, delta: f64) {
        let mut computed = self.fraction.clamp(0.0, 1.0);
        if computed.is_nan() {
            computed = 0.0;
        }
        if self.last_value > computed {
            self.blink = 1.0;
            self.last_value = computed;
        }
        if self.last_value.is_nan() {
            self.last_value = 0.0;
        }
        if self.last_value.is_infinite() {
            self.last_value = 1.0;
        }
        if self.value.is_nan() {
            self.value = 0.0;
        }
        if self.value.is_infinite() {
            self.value = 1.0;
        }
        self.blink = lerp_delta(self.blink, 0.0, BLINK_RATE, delta);
        self.value = lerp_delta(self.value, computed, VALUE_RATE, delta);
        self.base_mut().queue_redraw();
    }

    /// `Bar.draw()` (`ui/Bar.java:100-136`).
    fn draw(&mut self) {
        let size = self.base().get_size();
        let radius = self.outline_radius;
        let outline_color = self.outline_color;
        let bar = icon_texture("bar");
        let bar_splits = splits("bar", BAR_SPLITS);
        if radius > 0.0 {
            let outer = Rect2::new(
                Vector2::new(-radius, -radius),
                size + Vector2::new(radius, radius) * 2.0,
            );
            let clip_x = outer.size.x;
            draw_nine_patch(
                &mut self.base_mut(),
                bar.as_ref(),
                bar_splits,
                outer,
                outline_color,
                clip_x,
            );
        }
        // `Draw.colorl(0.1f)`: the backing is the `bar` patch at 10% grey.
        // Parent alpha is applied by Godot's modulate chain
        // (`Draw.alpha(parentAlpha)`).
        let full = Rect2::new(Vector2::ZERO, size);
        draw_nine_patch(
            &mut self.base_mut(),
            bar.as_ref(),
            bar_splits,
            full,
            BACKING_COLOR,
            size.x,
        );
        let mut top_color = self.bar_color.lerp(self.blink_color, self.blink);
        top_color.a = 1.0;
        let top = icon_texture("bar-top");
        let top_splits = splits("bar-top", BAR_TOP_SPLITS);
        let top_width = size.x as f64 * self.value;
        let region_width = top
            .as_ref()
            .map_or(0.0, |texture| texture.get_width() as f32);
        if top_width > region_width as f64 {
            let width = top_width as f32;
            draw_nine_patch(
                &mut self.base_mut(),
                top.as_ref(),
                top_splits,
                Rect2::new(Vector2::ZERO, Vector2::new(width, size.y)),
                top_color,
                width,
            );
        } else {
            // Below the region width the patch keeps its packed width and is
            // scissored to `topWidth` (`ScissorStack.push`, which rounds the
            // rect and skips a clip narrower than 1 px).
            let clip = top_width.round() as f32;
            if clip >= 1.0 {
                draw_nine_patch(
                    &mut self.base_mut(),
                    top.as_ref(),
                    top_splits,
                    Rect2::new(Vector2::ZERO, Vector2::new(region_width, size.y)),
                    top_color,
                    clip,
                );
            }
        }
        if !self.bar_name.is_empty() {
            self.draw_name();
        }
    }
}

#[godot_api]
impl MindBar {
    /// Initializes the bar (`Bar(name, color, fraction)` / `Bar.set`): the
    /// blink colour starts as the bar colour (`blinkColor.set(color)`), so only
    /// bars that call [`MindBar::blink`] flash a different colour.
    #[func]
    pub fn setup(&mut self, new_name: GString, color: Color, frac: f64) {
        self.bar_name = new_name;
        self.bar_color = color;
        self.blink_color = color;
        self.fraction = frac.clamp(0.0, 1.0);
        self.value = self.fraction;
        self.last_value = self.fraction;
        self.base_mut().queue_redraw();
    }

    /// Resets the animated value (`Bar.reset`: `value = lastValue = blink = value`).
    #[func]
    pub fn reset(&mut self, frac: f64) {
        self.value = frac;
        self.last_value = frac;
        self.blink = frac;
        self.base_mut().queue_redraw();
    }

    /// Snaps to the current fraction (`Bar.snap`).
    #[func]
    pub fn snap(&mut self) {
        self.value = self.fraction;
        self.last_value = self.fraction;
        self.base_mut().queue_redraw();
    }

    /// Triggers a blink flash (`Bar.flash`).
    #[func]
    pub fn flash(&mut self) {
        self.blink = 1.0;
        self.base_mut().queue_redraw();
    }

    /// Sets the flash colour (`Bar.blink`); returns `self` for chaining.
    #[func]
    pub fn blink(&mut self, color: Color) -> Gd<MindBar> {
        self.blink_color = color;
        self.to_gd()
    }

    /// Sets the outline (`Bar.outline`).
    #[func]
    pub fn outline(&mut self, color: Color, stroke: f32) {
        self.outline_color = color;
        self.outline_radius = stroke;
        self.base_mut().queue_redraw();
    }
}

impl MindBar {
    /// The centered `Fonts.outline` label: white with the 2 px dark-grey
    /// border, at `x + width/2 - lay.width/2`, `y + height/2 + lay.height/2 + 1`
    /// (`ui/Bar.java:126-133`, y-up; `lay.height` is the cap height).
    fn draw_name(&mut self) {
        let Some(font) = self.base().get_theme_default_font() else {
            return;
        };
        let font_size = self.base().get_theme_default_font_size();
        let text = self.bar_name.clone();
        let text_size = font
            .get_string_size_ex(&text)
            .alignment(HorizontalAlignment::LEFT)
            .width(-1.0)
            .font_size(font_size)
            .done();
        let size = self.base().get_size();
        // Java puts the cap top `lay.height/2 + 1` above the centre (y-up);
        // with this font's line height that lands the baseline 5 px above
        // `(height + lineHeight) / 2`. `lay.width` of `Fonts.outline` includes
        // the last glyph's 2 px border, so the run sits 1 px left of the plain
        // centre. Arc draws the cached glyphs on whole pixels, so the origin is
        // rounded (twin-measured against the 18 px HUD bars).
        let pos = Vector2::new(
            ((size.x - text_size.x) * 0.5 - 1.0).round(),
            ((size.y + text_size.y) * 0.5 - 5.0).round(),
        );
        self.base_mut()
            .draw_string_outline_ex(&font, pos, &text)
            .alignment(HorizontalAlignment::LEFT)
            .width(-1.0)
            .font_size(font_size)
            .size(LABEL_OUTLINE_SIZE)
            .modulate(LABEL_OUTLINE_COLOR)
            .done();
        self.base_mut()
            .draw_string_ex(&font, pos, &text)
            .alignment(HorizontalAlignment::LEFT)
            .width(-1.0)
            .font_size(font_size)
            .modulate(Color::WHITE)
            .done();
    }
}

/// Atlas splits `[left, right, top, bottom]` for a 9-patch region.
fn splits(region: &str, fallback: [i32; 4]) -> [f32; 4] {
    if let Some(assets) = assets() {
        let values = assets.bind().tex_splits(GString::from(region));
        if let [left, right, top, bottom] = values.as_slice() {
            return [*left as f32, *right as f32, *top as f32, *bottom as f32];
        }
    }
    [
        fallback[0] as f32,
        fallback[1] as f32,
        fallback[2] as f32,
        fallback[3] as f32,
    ]
}

/// Arc `NinePatch.draw` (`NinePatch.java:353-374`): the corners keep their
/// packed size, the edges/centre stretch, and a patch whose rect is too small
/// for its splits collapses its middle row/column. Pieces are clipped on the
/// right at `clip_x` (local x) for the scissored `barTop` path. Without the
/// atlas region the rect is drawn flat.
fn draw_nine_patch(
    canvas: &mut Control,
    texture: Option<&Gd<Texture2D>>,
    splits: [f32; 4],
    rect: Rect2,
    color: Color,
    clip_x: f32,
) {
    let Some(texture) = texture else {
        let mut flat = rect;
        flat.size.x = rect.size.x.min(clip_x - rect.position.x);
        if flat.size.x > 0.0 {
            canvas.draw_rect(flat, color);
        }
        return;
    };
    let tw = texture.get_width() as f32;
    let th = texture.get_height() as f32;
    let [left, right, top, bottom] = splits;
    let end = rect.end();
    // Arc lays the patch out y-up; in y-down terms the top row starts at the
    // rect top, the bottom row ends at the rect bottom and the middle row spans
    // what is left. Arc draws the bottom row first, so when the rect is shorter
    // than `top + bottom` the top row paints over the overlap.
    let xs = [
        [rect.position.x, left, 0.0, left],
        [
            rect.position.x + left,
            rect.size.x - left - right,
            left,
            tw - left - right,
        ],
        [end.x - right, right, tw - right, right],
    ];
    let ys = [
        [end.y - bottom, bottom, th - bottom, bottom],
        [
            rect.position.y + top,
            rect.size.y - top - bottom,
            top,
            th - top - bottom,
        ],
        [rect.position.y, top, 0.0, top],
    ];
    for row in ys {
        if row[1] <= 0.0 || row[3] <= 0.0 {
            continue;
        }
        for col in xs {
            if col[1] <= 0.0 || col[3] <= 0.0 {
                continue;
            }
            let dest_x = col[0];
            let dest_w = col[1];
            let src_x = col[2];
            let src_w = col[3];
            let visible_w = dest_w.min(clip_x - dest_x);
            if visible_w <= 0.0 {
                continue;
            }
            let src_visible = src_w * visible_w / dest_w;
            canvas
                .draw_texture_rect_region_ex(
                    texture,
                    Rect2::new(
                        Vector2::new(dest_x, row[0]),
                        Vector2::new(visible_w, row[1]),
                    ),
                    Rect2::new(
                        Vector2::new(src_x, row[2]),
                        Vector2::new(src_visible, row[3]),
                    ),
                )
                .modulate(color)
                .done();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_delta_matches_the_lerpf_form() {
        // One 60 Hz frame: `ticks = 1`, so the weight is the raw rate.
        assert_eq!(lerp_delta(0.0, 1.0, 0.15, 1.0 / 60.0), 0.15);
        assert_eq!(lerp_delta(1.0, 0.0, 0.2, 1.0 / 60.0), 0.8);
    }

    #[test]
    fn lerp_delta_clamps_the_weight() {
        // A long frame snaps instead of overshooting.
        assert_eq!(lerp_delta(1.0, 0.0, 0.2, 0.5), 0.0);
        assert_eq!(lerp_delta(0.0, 1.0, 0.15, 10.0), 1.0);
        // Paused / non-positive delta leaves the value untouched.
        assert_eq!(lerp_delta(0.5, 0.0, 0.2, 0.0), 0.5);
        assert_eq!(lerp_delta(0.5, 0.0, 0.2, -1.0), 0.5);
    }
}
