// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Boot presentation: theme build, manifest binding and the signal wiring of the static children.
//! Upstream: core/src/mindustry/core/UI.java

use super::free::{connect_if, json_bool, json_string};
use super::*;

#[godot_api(secondary)]
impl MindUiRoot {
    // ---- boot / theme -----------------------------------------------------

    /// Builds and applies the single runtime theme (`_apply_theme`).
    pub(super) fn apply_theme(&mut self) {
        let started = Time::singleton().get_ticks_msec();
        let theme = kit::theme::build();
        self.theme_build_ms = Time::singleton().get_ticks_msec().saturating_sub(started) as i64;
        let manifest = kit::theme::load_manifest();
        if !kit::theme::verify(&theme, &manifest) {
            log::warn!("[ui] theme verification failed (styles_manifest)");
        }
        self.base_mut().set_theme(&theme);
    }

    /// Boot presentation (`MenuFragment`/`LoadingFragment`/`FadeInFragment`
    /// order): the standalone menu is shown, the in-game HUD group is hidden,
    /// the loading overlay is cleared and the black boot cover fades out.
    pub(super) fn boot(&mut self) {
        self.set_menu_visible(true);
        if let Some(mut fade) = self.fade_in_node()
            && fade.has_method("start")
        {
            // `FadeInFragment.start(duration)`; the former GDScript default was
            // 0.5 s, which the native fragment requires explicitly.
            let _ = fade.call("start", &[BOOT_FADE_SECONDS.to_variant()]);
        }
    }

    // ---- manifest binding -------------------------------------------------

    /// Binds every manifest dialog/fragment to the statically instanced node of
    /// the same name under its layer group and registers the dialogs.
    pub(super) fn bind_manifest(&mut self) {
        if !FileAccess::file_exists(DIALOGS_MANIFEST) {
            log::warn!("[ui] missing {DIALOGS_MANIFEST}");
            return;
        }
        let text = FileAccess::get_file_as_string(DIALOGS_MANIFEST).to_string();
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            log::warn!("[ui] dialogs_manifest is not an object");
            return;
        };
        if !parsed.is_object() {
            log::warn!("[ui] dialogs_manifest is not an object");
            return;
        }
        let mut expected: Vec<String> = Vec::new();
        if let Some(dialogs) = parsed.get("dialogs").and_then(serde_json::Value::as_array)
            && let Some(layer) = self.dialog_layer.clone()
        {
            for entry in dialogs {
                if self.bind_entry(entry, &layer, false) {
                    expected.push(json_string(entry, "name"));
                }
            }
        }
        if let Some(fragments) = parsed
            .get("fragments")
            .and_then(serde_json::Value::as_array)
        {
            for entry in fragments {
                let group = json_string(entry, "group");
                let Some(parent) = self.group_for(&group) else {
                    continue;
                };
                if self.bind_entry(entry, &parent, true) {
                    expected.push(json_string(entry, "name"));
                }
            }
        }
        self.verify_manifest(&expected);
    }

    /// `_bind`: looks the manifest name up under `parent`, hides fragments per
    /// `hidden`, and registers dialogs with `MindUi`.
    pub(super) fn bind_entry(
        &mut self,
        entry: &serde_json::Value,
        parent: &Gd<Control>,
        is_fragment: bool,
    ) -> bool {
        let name = json_string(entry, "name");
        if name.is_empty() {
            return false;
        }
        let Some(instance) = parent.get_node_or_null(name.as_str()) else {
            log::error!(
                "[ui] manifest entry `{name}` has no scene node under {}",
                parent.get_name()
            );
            return false;
        };
        if is_fragment {
            if let Ok(mut control) = instance.clone().try_cast::<Control>() {
                control.set_visible(!json_bool(entry, "hidden"));
            }
            return true;
        }
        self.dialogs.insert(name.clone(), instance.clone());
        if let Some(mut ui) = kit::ui() {
            let should_pause = json_bool(entry, "pause");
            let _ = ui.call(
                "register_dialog",
                &[
                    GString::from(name.as_str()).to_variant(),
                    instance.to_variant(),
                    should_pause.to_variant(),
                ],
            );
        }
        true
    }

    /// Cross-check the manifest against the statically-instanced scene children
    /// (mirrors `MindThemeBuilder.verify`): every manifest name resolves, and
    /// every static child under a layer group is a manifest entry.
    pub(super) fn verify_manifest(&mut self, expected: &[String]) {
        let layers = [
            self.dialog_layer.clone(),
            self.menu_group.clone(),
            self.hud_group.clone(),
            self.overlay_layer.clone(),
            self.loading_layer.clone(),
        ];
        for layer in layers.into_iter().flatten() {
            let parent_name = layer.get_name().to_string();
            for child in layer.get_children().iter_shared() {
                let child_name = child.get_name().to_string();
                if !expected.contains(&child_name) {
                    log::error!(
                        "[ui] scene node `{child_name}` under {parent_name} is missing from dialogs_manifest"
                    );
                }
            }
        }
    }

    /// The layer group for a fragment manifest `group` name.
    pub(super) fn group_for(&self, group_name: &str) -> Option<Gd<Control>> {
        match group_name {
            "menu" => self.menu_group.clone(),
            "hud" => self.hud_group.clone(),
            "loading" => self.loading_layer.clone(),
            _ => self.overlay_layer.clone(),
        }
    }

    // ---- signal wiring ----------------------------------------------------

    /// Connects the `MindUi` prompt/toast signals to this root's renderers.
    pub(super) fn connect_prompts(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        for (signal, method) in [
            ("show_info", "on_show_info"),
            ("toast", "on_toast"),
            ("announce", "on_announce"),
            ("show_text", "on_show_text"),
            ("show_confirm", "on_show_confirm"),
            ("text_input_request", "on_text_input_request"),
            // `_on_confirm_resolved`: the prompt closes once the answer has
            // been delivered, whichever path resolved it.
            ("confirm_result", "on_confirm_result"),
        ] {
            let callable = self.base().callable(method);
            connect_if(&mut ui, signal, &callable);
        }
    }

    /// Wires manifest dialog intents that cross into the Rust facades.
    pub(super) fn connect_net(&mut self) {
        if let Some(mut host_dialog) = self.dialogs.get("host").cloned() {
            let callable = self.base().callable("on_host_requested");
            connect_if(&mut host_dialog, "host_requested", &callable);
        }
        let Some(mut join_dialog) = self.dialogs.get("join").cloned() else {
            return;
        };
        let connect = self.base().callable("on_connect_requested");
        connect_if(&mut join_dialog, "connect_requested", &connect);
        let join = self.base().callable("on_join_requested");
        connect_if(&mut join_dialog, "join_requested", &join);
    }

    /// Wires the campaign victory signal (`Control.java:175-181`): a first
    /// final-sector capture opens the campaign-complete overlay two seconds
    /// later, whether or not the planet page is open (PAR-0491).
    pub(super) fn connect_campaign(&mut self) {
        let Some(campaign) = self.campaign.clone() else {
            return;
        };
        let mut campaign: Gd<Node> = campaign.upcast();
        let callable = self.base().callable("on_campaign_complete");
        connect_if(&mut campaign, "campaign_complete", &callable);
    }

    /// `SectorCaptureEvent` -> `Time.run(60 * 2, ...)`: one-shot timer, never a
    /// per-frame poll. `MindCampaign` emits `campaign_complete` from inside its
    /// own `process` pump, so the dialog itself must open outside that borrow.
    #[func]
    pub(super) fn on_campaign_complete(&mut self) {
        let mut tree = self.base().get_tree();
        let callable = self.base().callable("open_campaign_complete_now");
        let mut timer = tree.create_timer(CAMPAIGN_COMPLETE_DELAY_SECONDS);
        let _ = timer.connect("timeout", &callable);
    }

    /// `ui.campaignComplete.show(e.sector.planet)`: close the planet page when
    /// it is open, then open the modal. `CampaignCompleteDialog` resolves the
    /// planet/playtime view from the live campaign for an empty context.
    #[func]
    pub(super) fn open_campaign_complete_now(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let _ = ui.call("close_dialog", &[GString::from("planet").to_variant()]);
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("campaign_complete").to_variant(),
                GString::new().to_variant(),
            ],
        );
    }

    /// Installs the global click-sound hook (`UI.java:146`) on every
    /// `BaseButton` in the tree and on every later addition.
    pub(super) fn install_global_hooks(&mut self) {
        let mut tree = self.base().get_tree();
        if self.node_added_hook.is_none() {
            // A custom callable, *not* a `#[func]` receiver: `node_added` is
            // emitted synchronously from `add_child`, which the UI calls from
            // bound `MindUiRoot` methods (`announce_box`/`toast_label`/prompt
            // builders). A `#[func]` receiver would `bind_mut()` the root a
            // second time and panic (PAR-0019): this one only records the
            // instance id and `process` hooks it outside the borrow.
            let hook = Callable::from_fn("mind_ui_node_added", |args: &[&Variant]| {
                if let Some(node) = args
                    .first()
                    .and_then(|value| value.try_to::<Gd<Node>>().ok())
                {
                    queue_node_hook(&node);
                }
                Variant::nil()
            });
            let _ = tree.connect("node_added", &hook);
            self.node_added_hook = Some(hook);
        }
        // `owned = false`: the default `owned = true` finds nothing under the
        // ownerless root Window, so code-instantiated buttons would be missed.
        let root = tree.get_root();
        let buttons = root
            .find_children_ex("*")
            .type_("BaseButton")
            .owned(false)
            .done();
        for node in buttons.iter_shared() {
            self.hook_button(&node);
        }
    }

    /// Hooks every node queued by the `node_added` listener since the last
    /// drain — outside the borrow, so an add from a bound method cannot panic.
    pub(super) fn drain_node_hooks(&mut self) {
        for id in take_node_hooks() {
            // A node queued and freed before the drain (a toast/en-route box)
            // is skipped instead of holding a dangling pointer.
            let Ok(node) = Gd::<Node>::try_from_instance_id(id) else {
                continue;
            };
            self.hook_button(&node);
        }
    }

    /// Hooks one button's `pressed` to the shared click sound.
    pub(super) fn hook_button(&mut self, node: &Gd<Node>) {
        let Ok(mut button) = node.clone().try_cast::<BaseButton>() else {
            return;
        };
        let callable = self.base().callable("on_any_button_pressed");
        if !button.is_connected("pressed", &callable) {
            button.connect("pressed", &callable);
        }
    }
}
