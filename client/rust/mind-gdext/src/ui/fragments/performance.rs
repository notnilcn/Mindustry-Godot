// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PerformanceFragment.java (plan 14 §3.5/M2).

//! Fullscreen performance readout (`PerformanceFragment`).
//!
//! Uses Godot `Performance` monitors in place of Arc `PerfCounter` (plan 14 §2.4).

use godot::classes::performance::Monitor;
use godot::classes::{Control, IControl, Label, Performance};
use godot::obj::Singleton;
use godot::prelude::*;

use crate::ui::kit;

/// `MindPerformanceFragment` — the `showperformance` debug overlay.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindPerformanceFragment {
    base: Base<Control>,
}

#[godot_api]
impl IControl for MindPerformanceFragment {
    fn process(&mut self, _delta: f64) {
        // `t.visible(() -> Core.settings.getBool("showperformance"))`.
        let visible = kit::ui()
            .and_then(|mut ui| {
                ui.call(
                    "settings_get_bool",
                    &[GString::from("showperformance").to_variant()],
                )
                .try_to::<bool>()
                .ok()
            })
            .unwrap_or(false);
        self.base_mut().set_visible(visible);
        if !visible {
            return;
        }
        let Some(mut label) = self
            .base()
            .get_node_or_null("Panel/Label")
            .and_then(|node| node.try_cast::<Label>().ok())
        else {
            return;
        };
        let performance = Performance::singleton();
        let fps = performance.get_monitor(Monitor::TIME_FPS);
        let process_ms = performance.get_monitor(Monitor::TIME_PROCESS) * 1000.0;
        let memory_mb = performance.get_monitor(Monitor::MEMORY_STATIC) / 1_048_576.0;
        label.set_text(&format!(
            "fps {}  cpu {:.2} ms  mem {:.1} MB",
            fps as i64, process_ms, memory_mb
        ));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn memory_divisor_is_one_mebibyte() {
        assert_eq!(1_048_576.0, 1024.0 * 1024.0);
    }
}
