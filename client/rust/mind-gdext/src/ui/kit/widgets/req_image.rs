// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/ReqImage.java (plan 14 §3.3).
//
// Stacks a requirement image with a red X overlay shown while `valid` is false.

//! `MindReqImage` — a requirement image with its validity cross.
//!
//! The GDScript class extended `MindStack`; gdext 0.5 cannot derive a native
//! class from another user class (`#[class(base=...)]` only accepts engine
//! classes), so the port derives from the same engine base (`Container`) and
//! reproduces `MindStack`'s stack sort and minimum-size rules through
//! [`stack_minimum_size`] / [`sort_stack`]. The observable layout is identical.

use godot::classes::notify::ContainerNotification;
use godot::classes::{Container, Control, IContainer, IControl};
use godot::prelude::*;

use super::super::styles;

/// `Pal.removeBack` (`a73e3e`): the shadow stroke drawn under `Pal.remove`.
const REMOVE_BACK: Color = Color::from_rgba(0.655, 0.243, 0.243, 1.0);

/// `MindStack.get_minimum_size`: the largest child minimum size.
pub(super) fn stack_minimum_size(container: &Gd<Container>) -> Vector2 {
    let mut result = Vector2::ZERO;
    for child in container.get_children().iter_shared() {
        if let Ok(control) = child.try_cast::<Control>() {
            let min_size = control.get_combined_minimum_size();
            result = Vector2::new(result.x.max(min_size.x), result.y.max(min_size.y));
        }
    }
    result
}

/// `MindStack`'s sort pass: every `Control` child shares the full rectangle.
pub(super) fn sort_stack(container: &mut Gd<Container>) {
    let rect = Rect2::new(Vector2::ZERO, container.get_size());
    for child in container.get_children().iter_shared() {
        if let Ok(mut control) = child.try_cast::<Control>() {
            control.set_position(rect.position);
            control.set_size(rect.size);
        }
    }
}

/// `MindReqImage` — stacks the requirement image under its cross overlay.
#[derive(GodotClass)]
#[class(rename = RustMindReqImage, base=Container, init)]
pub struct MindReqImage {
    base: Base<Container>,
    /// `ReqImage.valid` predicate (`Callable()` in the GDScript until setup).
    #[init(val = Callable::invalid())]
    valid_fn: Callable,
    /// `ReqImage.overlay`: the private cross shown while the predicate is false.
    overlay: Option<Gd<Control>>,
}

#[godot_api]
impl IContainer for MindReqImage {
    fn on_notification(&mut self, what: ContainerNotification) {
        if what == ContainerNotification::SORT_CHILDREN {
            sort_stack(&mut self.base_mut());
        }
    }

    fn get_minimum_size(&self) -> Vector2 {
        stack_minimum_size(&self.base())
    }

    fn process(&mut self, _delta: f64) {
        let Some(mut overlay) = self.overlay.clone() else {
            return;
        };
        if !overlay.is_instance_valid() {
            return;
        }
        let valid = self.valid();
        overlay.set_visible(!valid);
    }
}

#[godot_api]
impl MindReqImage {
    /// Builds the requirement image with its validity predicate
    /// (`ReqImage(Image, Boolp)`).
    #[func]
    pub fn setup(&mut self, image: Gd<Control>, is_valid: Callable) {
        self.valid_fn = is_valid;
        self.base_mut().add_child(&image);
        // code-instantiated: the cross overlay is a private draw-only element
        // with no static scene (plan 14 §3.3 ReqImage).
        let overlay = MindRequirementX::new_alloc();
        let control: Gd<Control> = overlay.upcast();
        self.base_mut().add_child(&control);
        self.overlay = Some(control);
    }

    /// `ReqImage.valid()`: the predicate result, false while unbound.
    #[func]
    pub fn valid(&self) -> bool {
        if !self.valid_fn.is_valid() {
            return false;
        }
        let args = Array::<Variant>::new();
        self.valid_fn
            .callv(&args)
            .try_to_relaxed::<bool>()
            .unwrap_or(false)
    }
}

/// `_RequirementX` — the private draw-only cross overlay.
///
/// `ReqImage.java:22-29`: two parallel top-left -> bottom-right strokes,
/// `Pal.removeBack` (`a73e3e`) shifted 2 px down under `Pal.remove`
/// (`e55454`). Arc's y-up `(x, y - 2 + height) -> (x + width, y - 2)` is
/// `(0, 2) -> (w, h + 2)` in Godot's y-down space. `Lines.line` caps by
/// default: each end extends `stroke / 2` along the line
/// (`Lines.java:58-60,94-112`).
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindRequirementX {
    base: Base<Control>,
}

#[godot_api]
impl IControl for MindRequirementX {
    fn draw(&mut self) {
        let size = self.base().get_size();
        self.capped(
            Vector2::new(0.0, 2.0),
            Vector2::new(size.x, size.y + 2.0),
            REMOVE_BACK,
        );
        self.capped(Vector2::ZERO, size, styles::REMOVE);
    }
}

#[godot_api]
impl MindRequirementX {
    /// One capped stroke: extend both ends by half the width before drawing
    /// (`Lines.line`'s default cap).
    fn capped(&mut self, from: Vector2, to: Vector2, color: Color) {
        const STROKE: f32 = 2.0;
        let dir = (to - from).normalized() * (STROKE / 2.0);
        self.base_mut()
            .draw_line_ex(from - dir, to + dir, color)
            .width(STROKE)
            .done();
    }
}
