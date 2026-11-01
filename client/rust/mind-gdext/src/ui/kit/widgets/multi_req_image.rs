// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/MultiReqImage.java (plan 14 §3.3).
//
// Cycles the visible requirement every second when none is valid
// (`time += Time.delta/60` in the source, i.e. one second per full cycle).

//! `MindMultiReqImage` — a stack of requirement images that shows the first
//! valid one, or cycles through them once per second while none is valid.
//!
//! The GDScript base was `MindStack`; gdext 0.5 cannot derive a native class
//! from another user class (`#[class(base=...)]` only accepts engine classes),
//! so this class derives from the same engine base (`Container`) and reproduces
//! `MindStack`'s stack sort and minimum-size rules through the helpers shared
//! with [`super::req_image::MindReqImage`]. The observable layout is identical.

use godot::classes::notify::ContainerNotification;
use godot::classes::{Container, Control, IContainer};
use godot::prelude::*;

use super::req_image::{MindReqImage, sort_stack, stack_minimum_size};

/// `MindMultiReqImage` — `MultiReqImage`'s cycling requirement stack.
#[derive(GodotClass)]
#[class(base=Container, init)]
pub struct MindMultiReqImage {
    base: Base<Container>,
    /// `MultiReqImage.displays`, in add order.
    displays: Vec<Gd<MindReqImage>>,
    /// `MultiReqImage.time`, advanced by the frame delta.
    time: f64,
}

#[godot_api]
impl IContainer for MindMultiReqImage {
    fn on_notification(&mut self, what: ContainerNotification) {
        if what == ContainerNotification::SORT_CHILDREN {
            sort_stack(&mut self.base_mut());
        }
    }

    fn get_minimum_size(&self) -> Vector2 {
        stack_minimum_size(&self.base())
    }

    fn process(&mut self, delta: f64) {
        self.time += delta;
        for display in &self.displays {
            set_display_visible(display, false);
        }
        // The first valid requirement wins; the rest stay hidden.
        for display in &self.displays {
            if !display.is_instance_valid() {
                continue;
            }
            if display.bind().valid() {
                set_display_visible(display, true);
                return;
            }
        }
        // Nothing valid: show one requirement per second, in order.
        if self.displays.is_empty() {
            return;
        }
        let index = (self.time as i64 % self.displays.len() as i64) as usize;
        if let Some(display) = self.displays.get(index) {
            set_display_visible(display, true);
        }
    }
}

#[godot_api]
impl MindMultiReqImage {
    /// `MultiReqImage.add(ReqImage)`: appends one requirement display.
    #[func]
    pub fn add_req(&mut self, display: Gd<MindReqImage>) {
        self.displays.push(display.clone());
        let control: Gd<Control> = display.upcast();
        self.base_mut().add_child(&control);
    }
}

/// `display.visible = value` through the `Control` base (`Gd<UserClass>` has no
/// `Deref`).
fn set_display_visible(display: &Gd<MindReqImage>, visible: bool) {
    if !display.is_instance_valid() {
        return;
    }
    let mut control: Gd<Control> = display.clone().upcast();
    control.set_visible(visible);
}
