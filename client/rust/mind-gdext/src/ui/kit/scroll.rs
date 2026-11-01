// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.ScrollPane (plan 14 §3.3).

//! `MindScroll` — a `ScrollContainer` with the scene2d scroll API and Arc's
//! smooth wheel (`ScrollPane.act`).
//!
//! Unlike the GDScript port there is no global hook: every scroll pane in the
//! project (scene and code-built) is a `MindScroll`, so the wheel easing lives
//! on the node itself.

use godot::classes::scroll_container::ScrollMode;
use godot::classes::{
    Control, IScrollContainer, InputEvent, InputEventMouseButton, ScrollContainer,
};
use godot::obj::NewAlloc;
use godot::prelude::*;

use super::table::MindTable;

/// Per-pane Arc scroll state: `amount` is the target and `visual` what is
/// shown, stepped once per frame while they differ (`ScrollPane.act`).
#[derive(GodotClass)]
#[class(rename = RustMindScroll, base=ScrollContainer, init)]
pub struct MindScroll {
    base: Base<ScrollContainer>,
    content: Option<Gd<MindTable>>,
    amount: Vector2,
    visual: Vector2,
    /// Bar values written by the last step; any other value means something
    /// else (a drag, `ensure_control_visible`, `request_scroll`) moved the pane.
    written: Vector2,
    animating: bool,
}

/// `act`: `Math.max(200 * delta, (amount - visual) * 7 * delta)`.
const MIN_SPEED: f32 = 200.0;
const EASE: f32 = 7.0;

#[godot_api]
impl IScrollContainer for MindScroll {
    fn ready(&mut self) {
        self.base_mut().set_process(false);
        if self.content.is_none()
            && self.base().get_child_count() == 1
            && let Some(first) = self.base().get_child(0)
            && let Ok(table) = first.try_cast::<MindTable>()
        {
            self.content = Some(table);
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        self.on_wheel(event);
    }

    fn process(&mut self, delta: f64) {
        self.step(delta as f32);
    }
}

#[godot_api]
impl MindScroll {
    /// The content table dialogs add cells to (created on demand).
    pub fn content(&mut self) -> Gd<MindTable> {
        if let Some(content) = &self.content {
            return content.clone();
        }
        // code-instantiated: the scroll content table is owned by the dialog
        // that creates this pane; there is no static scene for a generic body.
        let mut table = MindTable::new_alloc();
        let mut control: Gd<Control> = table.clone().upcast();
        control.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        control.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        control.set_name("Content");
        self.base_mut().add_child(&control);
        table = control.clone().try_cast::<MindTable>().unwrap_or(table);
        self.content = Some(table.clone());
        table
    }

    /// The content table when one already exists.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn content_if_any(&self) -> Option<Gd<MindTable>> {
        self.content.clone()
    }

    /// `ScrollPane.setScrollingDisabled`.
    pub fn set_scrolling_disabled(&mut self, x: bool, y: bool) {
        self.base_mut().set_horizontal_scroll_mode(if x {
            ScrollMode::DISABLED
        } else {
            ScrollMode::AUTO
        });
        self.base_mut().set_vertical_scroll_mode(if y {
            ScrollMode::DISABLED
        } else {
            ScrollMode::AUTO
        });
    }

    /// `ScrollPane.setFadeScrollBars` (Godot has no fading bars).
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn set_fade_scroll_bars(&mut self, _fade: bool) {}

    /// `ScrollPane.hasScroll`.
    pub fn has_scroll(&self) -> bool {
        match self.base().get_v_scroll_bar() {
            Some(bar) => bar.get_max() > bar.get_page(),
            None => false,
        }
    }

    /// `ScrollPane.setScrollY`.
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn request_scroll(&mut self, y: f32) {
        self.base_mut().set_v_scroll(y.round() as i32);
    }

    /// Focus bookkeeping hook (the input layer owns focus).
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn update_scroll_focus(&mut self) {}

    /// `ScrollPane.java:197-204` (`scrolled`) and the smooth `act` chase.
    fn on_wheel(&mut self, event: Gd<InputEvent>) {
        let Ok(button) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !button.is_pressed() {
            return;
        }
        let factor = button.get_factor();
        let (sx, sy) = match button.get_button_index() {
            godot::global::MouseButton::WHEEL_UP => (0.0, -factor),
            godot::global::MouseButton::WHEEL_DOWN => (0.0, factor),
            godot::global::MouseButton::WHEEL_LEFT => (factor, 0.0),
            godot::global::MouseButton::WHEEL_RIGHT => (-factor, 0.0),
            _ => return,
        };
        let Some(v_bar) = self.base().get_v_scroll_bar() else {
            return;
        };
        let Some(h_bar) = self.base().get_h_scroll_bar() else {
            return;
        };
        let scroll_y = v_bar.get_max() > v_bar.get_page()
            && self.base().get_vertical_scroll_mode() != ScrollMode::DISABLED;
        let scroll_x = h_bar.get_max() > h_bar.get_page()
            && self.base().get_horizontal_scroll_mode() != ScrollMode::DISABLED;
        if !scroll_x && !scroll_y {
            return;
        }
        if !self.animating || self.moved_elsewhere(&v_bar, &h_bar) {
            self.amount = Vector2::new(h_bar.get_value() as f32, v_bar.get_value() as f32);
            self.visual = self.amount;
        }
        // `getMouseWheelY()`: `Math.min(areaHeight, areaHeight * 0.9f / 4)`.
        if scroll_y {
            let step = (v_bar.get_page() * 0.9 / 4.0).min(v_bar.get_page());
            let max = (v_bar.get_max() - v_bar.get_page()).max(0.0);
            self.amount.y =
                ((self.amount.y as f64 + step * sy as f64) as f32).clamp(0.0, max as f32);
        }
        if scroll_x {
            let step = (h_bar.get_page() * 0.9 / 4.0).min(h_bar.get_page());
            let max = (h_bar.get_max() - h_bar.get_page()).max(0.0);
            self.amount.x =
                ((self.amount.x as f64 + step * sx as f64) as f32).clamp(0.0, max as f32);
        }
        self.base_mut().accept_event();
        if !self.animating {
            self.animating = true;
            self.written = Vector2::new(h_bar.get_value() as f32, v_bar.get_value() as f32);
            self.base_mut().set_process(true);
        }
    }

    fn moved_elsewhere(
        &self,
        v_bar: &Gd<godot::classes::VScrollBar>,
        h_bar: &Gd<godot::classes::HScrollBar>,
    ) -> bool {
        (h_bar.get_value() as f32) != self.written.x || (v_bar.get_value() as f32) != self.written.y
    }

    /// `ScrollPane.act` smooth scroll: the visual amount chases the target at
    /// `max(200 * delta, remaining * 7 * delta)` px, never overshooting.
    fn step(&mut self, delta: f32) {
        if !self.base().is_inside_tree() {
            self.stop();
            return;
        }
        let Some(mut v_bar) = self.base().get_v_scroll_bar() else {
            self.stop();
            return;
        };
        let Some(mut h_bar) = self.base().get_h_scroll_bar() else {
            self.stop();
            return;
        };
        if (h_bar.get_value() as f32) != self.written.x
            || (v_bar.get_value() as f32) != self.written.y
        {
            self.stop();
            return;
        }
        self.visual.x = chase(self.visual.x, self.amount.x, delta);
        self.visual.y = chase(self.visual.y, self.amount.y, delta);
        h_bar.set_value(self.visual.x as f64);
        v_bar.set_value(self.visual.y as f64);
        self.written = Vector2::new(h_bar.get_value() as f32, v_bar.get_value() as f32);
        if self.visual == self.amount {
            self.stop();
        }
    }

    fn stop(&mut self) {
        self.animating = false;
        self.base_mut().set_process(false);
    }
}

/// One `act` chase step.
fn chase(visual: f32, amount: f32, delta: f32) -> f32 {
    if visual < amount {
        return amount.min(visual + (MIN_SPEED * delta).max((amount - visual) * EASE * delta));
    }
    if visual > amount {
        return amount.max(visual - (MIN_SPEED * delta).max((visual - amount) * EASE * delta));
    }
    visual
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chase_never_overshoots() {
        assert_eq!(chase(0.0, 10.0, 1.0), 10.0);
        assert_eq!(chase(10.0, 0.0, 1.0), 0.0);
        assert_eq!(chase(5.0, 5.0, 1.0), 5.0);
    }

    #[test]
    fn chase_moves_toward_target() {
        let next = chase(0.0, 1000.0, 1.0 / 60.0);
        assert!(next > 0.0 && next < 1000.0);
    }
}
