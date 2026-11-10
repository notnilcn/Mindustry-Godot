// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The prompt surface: `MindUi` prompt signals, prompt button handlers and the prompt builder.
//! Upstream: core/src/mindustry/ui/dialogs/BaseDialog.java

use super::free::{add_prompt_title, apply_base_dialog_spacing, save_failure_markup};
use super::*;

#[godot_api(secondary)]
impl MindUiRoot {
    // ---- MindUi prompt signals -------------------------------------------

    /// `UI.showInfoToast` (`UI.java:405-419`): top-centre, `Styles.black3`.
    #[func]
    pub(super) fn on_show_info(&mut self, text: GString) {
        self.toast_label(text, 2.0, GString::from("ok"), true);
    }

    /// `HudFragment.showToast`: `if(state.isMenu()) return;`, then the
    /// `scheduleToast` slot (`HudFragment.java:729-740`): past the 3.5 s slot
    /// the toast shows now, else it is queued for the remainder and the slot
    /// advances by another 3.5 s.
    #[func]
    pub(super) fn on_toast(&mut self, text: GString, icon: GString) {
        if self
            .menu_group
            .as_ref()
            .is_some_and(|group| group.is_visible())
        {
            return;
        }
        // `showToast(Icon.warning, ..)` vs `showToast(text)` = `Icon.ok`.
        let glyph = match icon.to_string().as_str() {
            "warning" | "error" | "warn" => "warn",
            _ => "ok",
        };
        let now = self.now_ms();
        let since = now - self.last_toast_ms;
        if since > TOAST_SLOT_MS {
            self.last_toast_ms = now;
            self.play_toast(text, GString::from(glyph));
            return;
        }
        self.last_toast_ms += TOAST_SLOT_MS;
        let delay = (TOAST_SLOT_MS - since) / 1000.0;
        let mut tree = self.base().get_tree();
        let mut timer = tree.create_timer(delay);
        let this = self.to_gd();
        let glyph = GString::from(glyph);
        let queued = Callable::from_fn("queued_toast", move |_args: &[&Variant]| {
            if this.is_instance_valid() {
                let mut root = this.clone();
                root.bind_mut().play_toast(text.clone(), glyph.clone());
            }
            Variant::nil()
        });
        timer.connect("timeout", &queued);
    }

    #[func]
    pub(super) fn on_announce(&mut self, text: GString, duration: f32) {
        self.announce_box(text, duration.max(1.0));
    }

    /// `UI.showText`: `Dialog(title)` + text (400 wrap) + `@ok` 110x50.
    #[func]
    pub(super) fn on_show_text(&mut self, title: GString, text: GString) {
        let mut prompt = self.build_prompt(&title.to_string(), &text.to_string(), false, 400.0);
        prompt.cancel = PromptCancel::Dismiss;
        let close = self.base().callable("on_prompt_dismiss");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(110.0, 50.0),
            &close,
        );
        self.prompt = Some(prompt);
    }

    /// Polls the sim host's IO responses and shows the `@savefail` prompt for a
    /// failed save (`ui.showException("[accent]" + Core.bundle.get("savefail"),
    /// e)`; `PausedDialog.java:211`, `SaveDialog.java:55`,
    /// `PlanetDialog.java:1391`). Load failures keep their dedicated dialogs.
    pub(super) fn drain_save_failures(&mut self) {
        // A prompt is already up: leave the response queued (taking it here
        // would drop it) until the prompt closes.
        if self.prompt.is_some() {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("take_io_responses_json") {
            return;
        }
        let text = host
            .call("take_io_responses_json", &[])
            .try_to::<GString>()
            .unwrap_or_default()
            .to_string();
        if text.is_empty() {
            return;
        }
        let parsed = Json::parse_string(&text);
        let Ok(rows) = parsed.try_to::<VarArray>() else {
            return;
        };
        for entry in rows.iter_shared() {
            let Ok(row) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let field = |key: &str| {
                row.get(key)
                    .and_then(|value| value.try_to::<GString>().ok())
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            };
            if field("kind") != "save" {
                continue;
            }
            let error = field("error");
            if error.is_empty() {
                continue;
            }
            let failure = save_failure_markup(&kit::t("@savefail").to_string(), &error);
            self.on_show_text(kit::t("@error.title"), GString::from(failure.as_str()));
            // One prompt carries one failure; extra responses would only
            // overwrite (and leak) the modal just opened.
            break;
        }
    }

    /// `UI.java:582-606 showConfirm(text, cb)`: `@confirm` title, 500 px wrapped
    /// body, `@cancel`/`@ok` 200x54 with icons, Enter = OK, Esc = cancel.
    #[func]
    pub(super) fn on_show_confirm(&mut self, text: GString) {
        let mut prompt = self.build_prompt(
            &kit::t("@confirm").to_string(),
            &text.to_string(),
            false,
            500.0,
        );
        apply_base_dialog_spacing(&prompt);
        prompt.cancel = PromptCancel::Confirm;
        let cancel = self.base().callable("on_confirm_cancel");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@cancel",
            "cancel",
            Vector2::new(200.0, 54.0),
            &cancel,
        );
        let confirm = self.base().callable("on_confirm_ok");
        let mut ok = self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "ok",
            Vector2::new(200.0, 54.0),
            &confirm,
        );
        ok.grab_focus();
        self.prompt = Some(prompt);
    }

    /// `UI.java:309-355 showTextInput` (desktop): accent rule, message, 400x50
    /// field, `@cancel`/`@ok` 120x54; Enter = OK, Esc = cancel.
    #[func]
    pub(super) fn on_text_input_request(
        &mut self,
        title: GString,
        message: GString,
        max_length: i64,
        default_text: GString,
        numeric: bool,
        allow_empty: bool,
    ) {
        let mut prompt = self.build_prompt(&title.to_string(), &message.to_string(), true, 400.0);
        let mut field = prompt.field.clone();
        field.set_text(&default_text);
        if max_length > 0 {
            field.set_max_length(max_length as i32);
        }
        prompt.cancel = PromptCancel::TextInput;
        prompt.allow_empty = allow_empty;
        let cancel = self.base().callable("on_text_input_cancel");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@cancel",
            "",
            Vector2::new(120.0, 54.0),
            &cancel,
        );
        let approve = self.base().callable("on_text_input_ok");
        let mut ok = self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(120.0, 54.0),
            &approve,
        );
        ok.set_disabled(!allow_empty && field.get_text().is_empty());
        prompt.ok_button = Some(ok);
        if numeric {
            let filtered = self.base().callable("on_text_input_numeric_changed");
            field.connect("text_changed", &filtered);
        }
        let changed = self.base().callable("on_text_input_changed");
        field.connect("text_changed", &changed);
        // `UI.java:342-348`: Enter in the field confirms (when non-empty unless
        // `allowEmpty`).
        let submitted = self.base().callable("on_text_input_submitted");
        field.connect("text_submitted", &submitted);
        field.grab_focus();
        self.prompt = Some(prompt);
    }

    /// `showConfirm`'s dialog hides with its answer (`dialog.hide()` in both
    /// buttons), whichever path delivered it: the buttons, Enter/Esc, or a
    /// direct `MindUi.resolve_confirm` (MCP / script callers).
    #[func]
    pub(super) fn on_confirm_result(&mut self, _confirmed: bool) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if prompt.cancel == PromptCancel::Confirm {
            self.dismiss_prompt(&prompt.root);
        }
    }

    // ---- prompt button handlers -------------------------------------------

    /// `showText`/`showMessageEdit` close path (`_dismiss_prompt(root)`).
    #[func]
    pub(super) fn on_prompt_dismiss(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        self.dismiss_prompt(&prompt.root);
    }

    #[func]
    pub(super) fn on_confirm_ok(&mut self) {
        let root = self.prompt.as_ref().map(|prompt| prompt.root.clone());
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_confirm", &[true.to_variant()]);
        }
        if let Some(root) = root {
            self.dismiss_prompt(&root);
        }
    }

    #[func]
    pub(super) fn on_confirm_cancel(&mut self) {
        let root = self.prompt.as_ref().map(|prompt| prompt.root.clone());
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_confirm", &[false.to_variant()]);
        }
        if let Some(root) = root {
            self.dismiss_prompt(&root);
        }
    }

    #[func]
    pub(super) fn on_text_input_ok(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let value = prompt.field.get_text();
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_text_input", &[value.to_variant()]);
        }
        self.dismiss_prompt(&prompt.root);
    }

    #[func]
    pub(super) fn on_text_input_cancel(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_text_input", &[GString::new().to_variant()]);
        }
        self.dismiss_prompt(&prompt.root);
    }

    /// `ok.disabled = not allow_empty and value.is_empty()`.
    #[func]
    pub(super) fn on_text_input_changed(&mut self, value: GString) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if let Some(mut ok) = prompt.ok_button.clone() {
            ok.set_disabled(!prompt.allow_empty && value.is_empty());
        }
    }

    /// Digit-only filter for `numeric = true` prompts (`UI.java:330-340`).
    #[func]
    pub(super) fn on_text_input_numeric_changed(&mut self, value: GString) {
        let source = value.to_string();
        let filtered: String = source.chars().filter(char::is_ascii_digit).collect();
        if filtered == source {
            return;
        }
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let mut field = prompt.field.clone();
        field.set_text(&filtered);
        field.set_caret_column(filtered.chars().count() as i32);
    }

    /// Enter in the field confirms (when non-empty unless `allowEmpty`).
    #[func]
    pub(super) fn on_text_input_submitted(&mut self, _value: GString) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if prompt.allow_empty || !prompt.field.get_text().is_empty() {
            self.on_text_input_ok();
        }
    }

    /// `MessageBlock`'s `TextArea` filter: refuse text past `maxTextLength` or a
    /// newline past `maxNewlines`, then refresh the `n / max` counter.
    #[func]
    pub(super) fn on_message_edit_changed(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let Some(mut message) = prompt.message.clone() else {
            return;
        };
        let mut area = message.area.clone();
        let value = area.get_text().to_string();
        if value.chars().count() as i64 > message.max_text
            || value.matches('\n').count() as i64 > message.max_newlines
        {
            let caret = area.get_caret_column();
            area.set_text(&message.last);
            let length = message.last.chars().count() as i32;
            area.set_caret_column(caret.min(length));
        }
        message.last = area.get_text().to_string();
        message.counter.clone().set_text(&format!(
            "{} / {}",
            message.last.chars().count(),
            message.max_text
        ));
        if let Some(current) = self.prompt.as_mut() {
            current.message = Some(message);
        }
    }

    #[func]
    pub(super) fn on_message_edit_ok(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let Some(message) = prompt.message.clone() else {
            return;
        };
        let value = message.area.get_text();
        self.dismiss_prompt(&prompt.root);
        if message.on_ok.is_valid() {
            let mut args = Array::<Variant>::new();
            args.push(&value.to_variant());
            let _ = message.on_ok.callv(&args);
        }
    }

    /// `OK` keeps the new scale (`Control.java:640`).
    #[func]
    pub(super) fn on_uiscale_keep(&mut self) {
        self.finish_uiscale(true);
    }

    /// `@uiscale.cancel` reverts to 100% and exits.
    #[func]
    pub(super) fn on_uiscale_revert(&mut self) {
        self.finish_uiscale(false);
    }
}
impl MindUiRoot {
    // ---- prompt builder ---------------------------------------------------

    /// Builds a transient prompt panel (`showTextInput`/`showConfirm`/`showText`).
    ///
    /// code-instantiated: prompts are one-off transient overlays; their structure
    /// is parameterized by the request (field/no-field, body width) and has no
    /// static scene.
    pub(super) fn build_prompt(
        &mut self,
        title_text: &str,
        message: &str,
        with_field: bool,
        body_width: f32,
    ) -> Prompt {
        let mut root = ColorRect::new_alloc();
        root.set_color(styles::BLACK_9);
        root.set_anchors_preset(LayoutPreset::FULL_RECT);
        root.set_mouse_filter(MouseFilter::STOP);
        // Prompts are topmost (`UI` has one shared layer upstream). The z_index
        // keeps them drawn above any later `OverlayLayer` sibling.
        root.set_z_index(1000);
        if let Some(mut layer) = self.overlay_layer.clone() {
            layer.add_child(&root);
        }
        let root_control: Gd<Control> = root.clone().upcast();
        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(LayoutPreset::FULL_RECT);
        center.set_mouse_filter(MouseFilter::IGNORE);
        root.add_child(&center);
        let mut panel = PanelContainer::new_alloc();
        panel.set_theme_type_variation("defaultDialog");
        center.add_child(&panel);
        let mut layout = VBoxContainer::new_alloc();
        layout.add_theme_constant_override("separation", 6);
        panel.add_child(&layout);
        add_prompt_title(&mut layout, title_text, with_field);
        if with_field {
            // `UI.java:325`: the 400x4 accent rule above the text-input message.
            let mut rule = ColorRect::new_alloc();
            rule.set_color(styles::ACCENT);
            rule.set_custom_minimum_size(Vector2::new(400.0, 4.0));
            layout.add_child(&rule);
        }
        let body = factory::label(&format!("[center]{message}[/center]"));
        let mut body_rich: Gd<RichTextLabel> = body.upcast();
        body_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        let mut minimum = body_rich.get_custom_minimum_size();
        minimum.x = body_width;
        body_rich.set_custom_minimum_size(minimum);
        body_rich.set_h_size_flags(SizeFlags::EXPAND_FILL);
        layout.add_child(&body_rich);
        let mut field = LineEdit::new_alloc();
        // `cont.field(def, ...)`: Arc's default underline `TextField` style.
        field.set_theme_type_variation("defaultField");
        field.set_custom_minimum_size(Vector2::new(400.0, 50.0));
        if with_field {
            layout.add_child(&field);
        }
        let mut buttons = HBoxContainer::new_alloc();
        buttons.set_alignment(AlignmentMode::CENTER);
        buttons.add_theme_constant_override("separation", 4);
        layout.add_child(&buttons);
        let prompt = Prompt {
            root: root_control,
            field,
            buttons,
            body: body_rich,
            layout,
            ok_button: None,
            cancel: PromptCancel::None,
            allow_empty: false,
            message: None,
        };
        self.prompt = Some(prompt.clone());
        prompt
    }

    /// Prompt action button (`buttons.defaults().size(...)` +
    /// `button("@key", Icon)`).
    pub(super) fn add_prompt_button(
        &mut self,
        mut container: Gd<HBoxContainer>,
        label_key: &str,
        glyph: &str,
        button_size: Vector2,
        callback: &Callable,
    ) -> Gd<Button> {
        // code-instantiated: prompt action buttons are parameterized per prompt.
        let mut button = factory::icon_button(
            glyph,
            &kit::t(label_key).to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        button.set_custom_minimum_size(button_size);
        if callback.is_valid() {
            button.connect("pressed", callback);
        }
        container.add_child(&button);
        button
    }

    /// Frees one prompt root, clearing the active-prompt bookkeeping only when
    /// it is still that prompt (a listener may have opened the next one).
    pub(super) fn dismiss_prompt(&mut self, root: &Gd<Control>) {
        if !root.is_instance_valid() {
            return;
        }
        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.root == *root)
        {
            self.clear_prompt();
        }
        if !root.is_queued_for_deletion() {
            let mut root = root.clone();
            root.queue_free();
        }
    }

    /// Clears the active-prompt bookkeeping (used by every prompt exit path).
    pub(super) fn clear_prompt(&mut self) {
        self.prompt = None;
    }

    /// Runs a prompt's Esc/back path after its bookkeeping was cleared.
    pub(super) fn run_prompt_cancel(&mut self, cancel: PromptCancel, root: &Gd<Control>) {
        match cancel {
            PromptCancel::None => {}
            PromptCancel::Dismiss => self.dismiss_prompt(root),
            PromptCancel::Confirm => {
                if let Some(mut ui) = kit::ui() {
                    let _ = ui.call("resolve_confirm", &[false.to_variant()]);
                }
                self.dismiss_prompt(root);
            }
            PromptCancel::TextInput => {
                if let Some(mut ui) = kit::ui() {
                    let _ = ui.call("resolve_text_input", &[GString::new().to_variant()]);
                }
                self.dismiss_prompt(root);
            }
        }
    }

    /// The active prompt while its root is still alive (`_active_prompt`).
    pub(super) fn active_prompt(&mut self) -> Option<Prompt> {
        match self.prompt.clone() {
            Some(prompt) if prompt.root.is_instance_valid() => Some(prompt),
            Some(_) => {
                self.prompt = None;
                None
            }
            None => None,
        }
    }
}
