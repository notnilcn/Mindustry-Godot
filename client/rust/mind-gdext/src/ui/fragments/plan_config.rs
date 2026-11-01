// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PlanConfigFragment.java
//         (plan 14 §3.5, M4).

//! Build-plan configuration popup (`BuildPlan.point_config`).
//!
//! The config dict is `{title, options: [{name, label}], value}`; choosing an
//! option emits `plan_config_applied` with the selection.

use godot::classes::{Button, Control, IControl, Label, VBoxContainer};
use godot::obj::NewAlloc;
use godot::prelude::*;

/// `MindPlanConfigFragment` — the plan-point config popup.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindPlanConfigFragment {
    base: Base<Control>,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    config: VarDictionary,
}

#[godot_api]
impl IControl for MindPlanConfigFragment {
    fn ready(&mut self) {
        self.base_mut().set_visible(false);
    }
}

#[godot_api]
impl MindPlanConfigFragment {
    /// Emitted with the plan-point config after a choice.
    #[signal]
    fn plan_config_applied(config: VarDictionary);

    /// Opens the popup for a plan point config.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn configure(&mut self, config: VarDictionary) {
        self.config = config;
        self.rebuild();
        self.base_mut().set_visible(true);
    }

    /// The current config dictionary.
    #[allow(dead_code)] // ported surface; no native caller yet.
    pub fn config(&self) -> VarDictionary {
        self.config.clone()
    }

    fn body(&self) -> Option<Gd<VBoxContainer>> {
        self.base().get_node_or_null("Panel/Body")?.try_cast().ok()
    }

    fn rebuild(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        for child in body.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        // code-instantiated: plan-config controls come from the point config
        // options (data-driven; count is runtime content).
        let mut title = Label::new_alloc();
        title.set_text(
            &self
                .config
                .get("title")
                .map(|value| value.to::<GString>())
                .unwrap_or_default(),
        );
        body.add_child(&title);
        let options = self
            .config
            .get("options")
            .map(|value| value.to::<VarArray>())
            .unwrap_or_default();
        if options.is_empty() {
            let mut apply = Button::new_alloc();
            apply.set_text("OK");
            let value = self
                .config
                .get("value")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let target = self.to_gd();
            let callable = Callable::from_fn("plan_ok", move |_args: &[&Variant]| {
                let mut target = target.clone();
                let _ = target.call("choose", &[value.to_variant()]);
            });
            apply.connect("pressed", &callable);
            body.add_child(&apply);
            return;
        }
        for option in options.iter_shared() {
            let Ok(option) = option.try_to::<VarDictionary>() else {
                continue;
            };
            let option_name = option
                .get("name")
                .map(|value| value.to::<GString>())
                .unwrap_or_default();
            let label = option
                .get("label")
                .map(|value| value.to::<GString>())
                .unwrap_or_else(|| option_name.clone());
            let mut button = Button::new_alloc();
            button.set_text(&label);
            button.set_tooltip_text(&label);
            let target = self.to_gd();
            let chosen = option_name.clone();
            let callable = Callable::from_fn("plan_choose", move |_args: &[&Variant]| {
                let mut target = target.clone();
                let _ = target.call("choose", &[chosen.to_variant()]);
            });
            button.connect("pressed", &callable);
            body.add_child(&button);
        }
    }

    #[func]
    fn choose(&mut self, value: GString) {
        self.config.set("value", &value.to_variant());
        let mut copy = self.config.clone();
        self.base_mut()
            .emit_signal("plan_config_applied", &[copy.to_variant()]);
        let _ = &mut copy;
        self.base_mut().set_visible(false);
    }
}
