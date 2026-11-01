// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/WarningBar.java (plan 14 §3.3).

//! `MindWarningBar` — the skewed stripe bar with top/bottom rules
//! (`WarningBar`, debug/overflow usage).

use godot::classes::{Control, IControl};
use godot::prelude::*;

/// Spacing/bar-width rule from `WarningBar`: `spacing = barWidth * 2`.
const SPACING_FACTOR: f32 = 2.0;
/// The top/bottom rule stroke width.
const RULE_WIDTH: f32 = 3.0;

/// `MindWarningBar` — diagonal stripes with 3 px rules along both edges.
#[derive(GodotClass)]
#[class(rename = RustMindWarningBar, base=Control)]
pub struct MindWarningBar {
    base: Base<Control>,
    /// Stripe width in px (`WarningBar.barWidth`).
    #[export]
    pub bar_width: f32,
    /// Stripe pitch in px; `ready` resets it to `bar_width * 2`.
    #[export]
    pub spacing: f32,
    /// Horizontal skew in px; `ready` resets it to `bar_width`.
    #[export]
    pub skew: f32,
}

/// `int(size.x / max(1, spacing)) + 2` stripes: the leading/trailing partial
/// stripes stay covered.
fn stripe_count(width: f32, spacing: f32) -> i32 {
    (width / spacing.max(1.0)) as i32 + 2
}

#[godot_api]
impl IControl for MindWarningBar {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            bar_width: 40.0,
            spacing: 80.0,
            skew: 40.0,
        }
    }

    fn ready(&mut self) {
        self.spacing = self.bar_width * SPACING_FACTOR;
        self.skew = self.bar_width;
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let color = self.base().get_modulate();
        let size = self.base().get_size();
        let amount = stripe_count(size.x, self.spacing);
        for index in 0..amount {
            let rx = (index - 1) as f32 * self.spacing;
            let mut points = PackedVector2Array::new();
            points.push(Vector2::new(rx, 0.0));
            points.push(Vector2::new(rx + self.skew, size.y));
            points.push(Vector2::new(rx + self.skew + self.bar_width, size.y));
            points.push(Vector2::new(rx + self.bar_width, 0.0));
            self.base_mut().draw_colored_polygon(&points, color);
        }
        self.base_mut()
            .draw_line_ex(Vector2::ZERO, Vector2::new(size.x, 0.0), color)
            .width(RULE_WIDTH)
            .done();
        self.base_mut()
            .draw_line_ex(
                Vector2::new(0.0, size.y),
                Vector2::new(size.x, size.y),
                color,
            )
            .width(RULE_WIDTH)
            .done();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripe_count_covers_both_edges() {
        // 400 px at the default 80 px pitch: five full stripes plus the two
        // off-screen partials.
        assert_eq!(stripe_count(400.0, 80.0), 7);
        assert_eq!(stripe_count(0.0, 80.0), 2);
    }

    #[test]
    fn stripe_count_guards_a_zero_pitch() {
        // `max(1, spacing)` keeps a zero/huge pitch from dividing by zero.
        assert_eq!(stripe_count(10.0, 0.0), 12);
        assert_eq!(stripe_count(10.0, 0.5), 12);
    }

    #[test]
    fn stripe_count_truncates_like_int() {
        assert_eq!(stripe_count(100.0, 80.5), 3);
        assert_eq!(stripe_count(159.0, 80.0), 3);
        assert_eq!(stripe_count(160.0, 80.0), 4);
    }
}
