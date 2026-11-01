// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.layout.Collapser (plan 14 §3.3).

//! `MindCollapser` — animated min-size collapse.
//!
//! `set_collapsed(true)` shrink-tweens the vertical minimum size, `false`
//! restores the remembered expanded height (`Collapser.setCollapsed`).

use godot::classes::{Control, IControl};
use godot::prelude::*;

/// Default tween duration in seconds (`Collapser` default duration).
const DEFAULT_DURATION: f32 = 0.2;

/// `MindCollapser` — a `Control` that collapses its height on demand.
#[derive(GodotClass)]
#[class(rename = RustMindCollapser, base=Control)]
pub struct MindCollapser {
    base: Base<Control>,
    /// Tween duration in seconds (`Collapser.setDuration`).
    #[export]
    #[var(set = set_duration)]
    pub duration: f32,
    /// Whether the content is currently collapsed.
    #[var(set = set_collapsed)]
    pub collapsed: bool,
    /// The height restored when expanding; captured on `ready` and whenever the
    /// control grows while expanded.
    expanded_height: f32,
}

#[godot_api]
impl IControl for MindCollapser {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            duration: DEFAULT_DURATION,
            collapsed: false,
            expanded_height: 0.0,
        }
    }

    fn ready(&mut self) {
        if self.expanded_height <= 0.0 {
            self.expanded_height = self.base().get_custom_minimum_size().y;
        }
    }
}

#[godot_api]
impl MindCollapser {
    /// `Collapser.setCollapsed`: tweens `custom_minimum_size:y` to `0` when
    /// collapsing, back to the remembered height when expanding.
    #[func]
    pub fn set_collapsed(&mut self, value: bool) {
        if value == self.collapsed {
            return;
        }
        self.collapsed = value;
        if !value {
            self.expanded_height = self
                .expanded_height
                .max(self.base().get_custom_minimum_size().y);
        }
        let target = if value { 0.0 } else { self.expanded_height };
        let this: Gd<Control> = self.to_gd().upcast();
        let mut tween = self.base_mut().create_tween();
        tween.tween_property(
            &this,
            "custom_minimum_size:y",
            &target.to_variant(),
            self.duration as f64,
        );
    }

    /// `Collapser.setDuration`.
    #[func]
    pub fn set_duration(&mut self, value: f32) {
        self.duration = value;
    }
}
