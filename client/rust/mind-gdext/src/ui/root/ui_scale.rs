// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The persisted UI-scale application and the deferred mod-init prompt.
//! Upstream: core/src/mindustry/core/UI.java

use super::*;

impl MindUiRoot {
    // ---- UI scale ---------------------------------------------------------

    /// `Vars.java:517 Scl.setProduct`: the persisted `uiscale` percent scales the
    /// whole UI at boot (minimum 25%, matching upstream). The window content
    /// scale factor is Godot's equivalent of the Scl product.
    pub(super) fn apply_ui_scale(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("settings_rows_json") {
            return;
        }
        let Ok(json) = ui
            .call(
                "settings_rows_json",
                &[GString::from("graphics").to_variant()],
            )
            .try_to::<GString>()
        else {
            return;
        };
        let parsed = Json::parse_string(&json);
        let Ok(rows) = parsed.try_to::<VarArray>() else {
            return;
        };
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarDictionary>() else {
                continue;
            };
            let key = row
                .get("key")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            if key != "uiscale" {
                continue;
            }
            let value = row
                .get("value")
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or(100.0);
            let ui_scale = (value.max(25.0) / 100.0) as f32;
            if let Some(mut window) = self.base().get_window() {
                window.set_content_scale_factor(ui_scale);
            }
            return;
        }
    }

    /// `Control.java:640` boot confirmation for a changed UI scale: the prompt
    /// counts down, `OK` keeps the new scale and `@uiscale.cancel` reverts to
    /// 100% and exits.
    pub(super) fn check_uiscale_changed(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("uiscale_changed") {
            return;
        }
        if !ui
            .call("uiscale_changed", &[])
            .try_to::<bool>()
            .unwrap_or(false)
        {
            return;
        }
        self.uiscale_countdown = f64::from(UISCALE_REVERT_SECONDS);
        let seconds = UISCALE_REVERT_SECONDS.to_string();
        let text = kit::tm_format("@uiscale.reset", &[seconds.as_str()]);
        let prompt = self.build_prompt("", &text.to_string(), false, 400.0);
        let keep = self.base().callable("on_uiscale_keep");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(120.0, 54.0),
            &keep,
        );
        let revert = self.base().callable("on_uiscale_revert");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@uiscale.cancel",
            "",
            Vector2::new(120.0, 54.0),
            &revert,
        );
        self.uiscale_prompt = Some(prompt);
        self.base_mut().set_process(true);
    }

    /// One UI-scale countdown frame (`_process`'s tail).
    pub(super) fn tick_uiscale(&mut self, delta: f64) {
        let Some(prompt) = self.uiscale_prompt.clone() else {
            return;
        };
        self.uiscale_countdown -= delta;
        let seconds = (self.uiscale_countdown.ceil() as i32).max(0);
        if seconds != self.uiscale_shown_seconds {
            self.uiscale_shown_seconds = seconds;
            let seconds_text = seconds.to_string();
            let text = kit::tm_format("@uiscale.reset", &[seconds_text.as_str()]);
            let mut body = prompt.body.clone();
            body.set_text(&text);
        }
        if self.uiscale_countdown <= 0.0 {
            self.finish_uiscale(false);
        }
    }

    /// `_finish_uiscale`: drops the prompt, clears `uiscalechanged`, and exits
    /// after a revert (100%).
    pub(super) fn finish_uiscale(&mut self, keep: bool) {
        let Some(prompt) = self.uiscale_prompt.take() else {
            return;
        };
        let mut root = prompt.root;
        root.queue_free();
        // `_process` also drains world-processor messages (H06 T4), so it stays on.
        self.base_mut().set_process(true);
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let _ = ui.call("set_uiscale_changed", &[false.to_variant()]);
        if !keep {
            let _ = ui.call(
                "settings_set",
                &[GString::from("uiscale").to_variant(), 100i64.to_variant()],
            );
            self.base_mut().get_tree().quit();
        }
    }

    // ---- deferred mod-init prompt ----------------------------------------

    /// `Control.java:75-80`: after the client load, a boot that skipped mod
    /// loading shows `@mods.initfailed` (deferred four ticks upstream).
    pub(super) fn check_mod_init_failure(&mut self) {
        if self.mod_init_failure_shown {
            return;
        }
        let Some(mut mods) = kit::node("/root/MindMods") else {
            return;
        };
        if !mods
            .call("skipped_mod_loading", &[])
            .try_to::<bool>()
            .unwrap_or(false)
        {
            return;
        }
        let empty = mods
            .call("list", &[])
            .try_to::<VarArray>()
            .map(|list| list.is_empty())
            .unwrap_or(true);
        if empty {
            return;
        }
        self.mod_init_failure_shown = true;
        self.mod_init_defer_frames = 4;
    }

    /// Counts the four deferred frames down, then shows `@mods.initfailed`.
    pub(super) fn tick_mod_init_failure(&mut self) {
        if self.mod_init_defer_frames <= 0 {
            return;
        }
        self.mod_init_defer_frames -= 1;
        if self.mod_init_defer_frames > 0 {
            return;
        }
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call(
                "show_info",
                &[GString::from("@mods.initfailed").to_variant()],
            );
        }
    }
}
