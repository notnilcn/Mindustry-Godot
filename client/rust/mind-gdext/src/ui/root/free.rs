// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Free helpers for the UI root: signal connect, prompt layout and manifest JSON readers.
//! Upstream: core/src/mindustry/ui/dialogs/BaseDialog.java

use super::*;

/// Connects `callable` to `signal` when `node` declares it and it is not already
/// connected (`UiRoot._connect_prompts`/`_connect_net`).
pub(super) fn connect_if(node: &mut Gd<Node>, signal: &str, callable: &Callable) {
    if node.has_signal(signal) && !node.is_connected(signal, callable) {
        node.connect(signal, callable);
    }
}

/// Prompt title row: accent label + 3 px rule (`BaseDialog.java:24-25`, the same
/// shape as `mind_dialog.gd _ensure_title_underline`).
pub(super) fn add_prompt_title(layout: &mut Gd<VBoxContainer>, title_text: &str, plain: bool) {
    if title_text.is_empty() {
        return;
    }
    // code-instantiated: prompt titles are transient and parameterized.
    // `showTextInput` builds a plain `Dialog(titleText)` (`UI.java:324`): the
    // title sits at the left with no rule under it.
    let text = if plain {
        title_text.to_owned()
    } else {
        format!("[center]{title_text}[/center]")
    };
    let title = factory::label(&text);
    let mut title_rich: Gd<RichTextLabel> = title.upcast();
    title_rich.add_theme_color_override("default_color", styles::ACCENT);
    title_rich.set_h_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&title_rich);
    if plain {
        return;
    }
    let mut rule = ColorRect::new_alloc();
    rule.set_color(styles::ACCENT);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    layout.add_child(&rule);
}

/// `BaseDialog` cell padding for a titled prompt (`showConfirm`): the title
/// rule's `pad(4f)` (`BaseDialog.java:25`), `Dialog.defaults().pad(3)` around
/// `cont`/`buttons` and their `defaults().pad(3)` (`Dialog.java:208-214`), the
/// body's `pad(4f)` and the buttons' `pad(2f)` (`UI.java:584-585`).
pub(super) fn apply_base_dialog_spacing(prompt: &Prompt) {
    let mut layout = prompt.layout.clone();
    layout.add_theme_constant_override("separation", 0);
    if prompt.body.get_index() >= 2 {
        // title | pad 4 | rule | pad 4 + cont 3 + body 4 (+2 line box) | body
        insert_prompt_gap(&mut layout, 1, 4.0);
        insert_prompt_gap(&mut layout, prompt.body.get_index(), 4.0 + 3.0 + 4.0 + 2.0);
    }
    // body | body 4 + cont 3 + buttons 3 + button 2 (+1 line box) | buttons
    insert_prompt_gap(
        &mut layout,
        prompt.buttons.get_index(),
        4.0 + 3.0 + 3.0 + 2.0 + 1.0,
    );
}

/// code-instantiated: fixed spacer inside a transient prompt.
pub(super) fn insert_prompt_gap(layout: &mut Gd<VBoxContainer>, index: i32, height: f32) {
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(0.0, height));
    gap.set_mouse_filter(MouseFilter::IGNORE);
    layout.add_child(&gap);
    layout.move_child(&gap, index);
}

/// Body of the `savefail` prompt: `ui.showException` renders
/// `"[accent]" + Core.bundle.get("savefail")` plus
/// `"\n[lightgray](" + message + ")"` (`UI.java:518-527`).
pub(super) fn save_failure_markup(savefail: &str, error: &str) -> String {
    if error.is_empty() {
        return format!("[accent]{savefail}[]");
    }
    format!("[accent]{savefail}[]\n[lightgray]({error})")
}

/// A string field from a manifest entry (empty when absent).
pub(super) fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A boolean field from a manifest entry (`false` when absent).
pub(super) fn json_bool(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}
