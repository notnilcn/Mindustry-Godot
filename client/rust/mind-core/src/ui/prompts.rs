// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/core/UI.java (prompt helpers:
//         `showInfo*`/`showText*`/`showConfirm`/`showCustomConfirm`/
//         `showOkText`/`showTextInput`/`announce`/`showLabel`).

//! Godot-free prompt-helper model (plan 14 §2.1 item 4 / §3.4).
//!
//! `UI.java`'s one-off prompt helpers are behaviourally simple but easy to get
//! subtly wrong (text filtering, `allowEmpty` gating, popup id replacement,
//! announcement tracking). This module captures that behaviour as pure data so
//! `cargo test -p mind-core` and `mind-headless ui prompts` can assert it
//! without Godot. Rendering stays in GDScript (`MindUi` + `ui_root.gd`).

use indexmap::IndexMap;

use crate::ui::builder::ui_relay::TextInput;

/// Every prompt helper this plan implements, keyed exactly as
/// `client/ui/dialogs_manifest.json` `prompts[]` (plan §6.4).
pub const PROMPT_HELPERS: &[&str] = &[
    "show_info",
    "show_info_fade",
    "show_info_toast",
    "show_info_popup",
    "show_info_on_hidden",
    "show_startup_info",
    "show_error",
    "show_exception",
    "show_text",
    "show_info_text",
    "show_small",
    "show_confirm",
    "show_custom_confirm",
    "show_ok_text",
    "show_text_input",
    "show_label",
    "announce",
    "toast",
    "unlock_toast",
];

/// A prompt helper kind (1:1 with the `UI.java` methods).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// `showInfo` — modal OK box, centered wrapped text.
    Info,
    /// `showInfoFade` — non-blocking fading label at the top.
    InfoFade,
    /// `showInfoToast` — non-blocking toast with a `black3` background.
    InfoToast,
    /// `showInfoPopup` — positioned popup, id-replaceable/removable.
    InfoPopup,
    /// `showInfoOnHidden` — `showInfo` plus a hidden callback.
    InfoOnHidden,
    /// `showStartupInfo` — `showInfo` left-aligned.
    StartupInfo,
    /// `showErrorMessage` — modal full-parent error box.
    Error,
    /// `showException` — error box + collapsible details.
    Exception,
    /// `showText` — modal title + centered text.
    Text,
    /// `showInfoText` — modal title + left-aligned text.
    InfoText,
    /// `showSmall` — compact modal with an accent line under the title.
    Small,
    /// `showConfirm` — two-button confirm (`@cancel`/`@ok`).
    Confirm,
    /// `showCustomConfirm` — two-button user-labelled choice.
    CustomConfirm,
    /// `showOkText` — single OK button + text (a "select one" acknowledgement).
    OkText,
    /// `showTextInput` — desktop dialog / mobile native keyboard.
    TextInput,
    /// `showLabel` — world label, id-replaceable/removable.
    Label,
    /// `announce` — centered fading overlay.
    Announce,
    /// `toast` — `showToast`/`showUnlock` icon toast.
    Toast,
    /// `unlock_toast` — content-unlock toast (icon derives from the content).
    UnlockToast,
}

impl PromptKind {
    /// All kinds in `PROMPT_HELPERS` order.
    pub const ALL: [PromptKind; 19] = [
        PromptKind::Info,
        PromptKind::InfoFade,
        PromptKind::InfoToast,
        PromptKind::InfoPopup,
        PromptKind::InfoOnHidden,
        PromptKind::StartupInfo,
        PromptKind::Error,
        PromptKind::Exception,
        PromptKind::Text,
        PromptKind::InfoText,
        PromptKind::Small,
        PromptKind::Confirm,
        PromptKind::CustomConfirm,
        PromptKind::OkText,
        PromptKind::TextInput,
        PromptKind::Label,
        PromptKind::Announce,
        PromptKind::Toast,
        PromptKind::UnlockToast,
    ];

    /// Manifest key for this kind.
    pub fn name(self) -> &'static str {
        PROMPT_HELPERS[self as usize]
    }

    /// Reverse of [`Self::name`].
    pub fn from_name(name: &str) -> Option<Self> {
        PROMPT_HELPERS
            .iter()
            .position(|candidate| *candidate == name)
            .and_then(|index| Self::ALL.get(index).copied())
    }

    /// Whether the helper creates a blocking modal dialog.
    ///
    /// `showInfoFade`/`showInfoToast`/`showInfoPopup`/`showLabel`/`announce`/
    /// `toast`/`unlock_toast` are overlays that never block input.
    pub fn is_modal(self) -> bool {
        !matches!(
            self,
            PromptKind::InfoFade
                | PromptKind::InfoToast
                | PromptKind::InfoPopup
                | PromptKind::Label
                | PromptKind::Announce
                | PromptKind::Toast
                | PromptKind::UnlockToast
        )
    }

    /// Whether showing this helper pauses the sim.
    ///
    /// Prompt helpers use `Dialog`/`BaseDialog` without setting `shouldPause`,
    /// so `BaseDialog.shouldPause` stays `false` (upstream default) for every
    /// one of them (plan §3.4).
    pub fn should_pause(self) -> bool {
        false
    }

    /// Whether this helper owns an id-addressable registry entry (popup/label).
    pub fn is_id_addressable(self) -> bool {
        matches!(self, PromptKind::InfoPopup | PromptKind::Label)
    }
}

/// Outcome of a text-input prompt (plan §3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputOutcome {
    /// Back/close: the relay result carries `text: None`.
    Cancelled,
    /// A valid submission: the relay result carries `text: Some(filtered)`.
    Submitted(String),
    /// The OK/Enter path was attempted but `allow_empty` rejected it.
    Rejected,
}

/// `showTextInput` semantics (`UI.showTextInput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputSpec {
    /// Dialog title (bundle key or literal).
    pub title: String,
    /// Prompt body.
    pub message: String,
    /// `field.setMaxLength(textLength)`.
    pub max_length: i32,
    /// Initial field text.
    pub default_text: String,
    /// Digits-only filter (`TextFieldFilter.digitsOnly`).
    pub numeric: bool,
    /// Whether an empty submission is accepted (`allowEmpty`).
    pub allow_empty: bool,
}

impl TextInputSpec {
    /// Builds a spec (`showTextInput(title, text, len, def, numeric, allowEmpty)`).
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        max_length: i32,
        default_text: impl Into<String>,
        numeric: bool,
        allow_empty: bool,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            max_length: max_length.max(0),
            default_text: default_text.into(),
            numeric,
            allow_empty,
        }
    }

    /// Builds a spec from a server `text_input` relay payload (§6.3).
    pub fn from_relay(input: &TextInput) -> Self {
        Self {
            title: input.title.clone(),
            message: input.message.clone(),
            max_length: input.len.max(0),
            default_text: input.def.clone(),
            numeric: input.numeric,
            allow_empty: input.allow_empty,
        }
    }

    /// `digitsOnly` filter predicate.
    pub fn permits(&self, c: char) -> bool {
        !self.numeric || c.is_ascii_digit()
    }

    /// Applies the filter then `setMaxLength` (both are per-character in Arc,
    /// so filtering first matches the field exactly).
    pub fn filter(&self, text: &str) -> String {
        let filtered: String = text.chars().filter(|c| self.permits(*c)).collect();
        if self.max_length >= 0 {
            filtered
                .chars()
                .take(self.max_length as usize)
                .collect::<String>()
        } else {
            filtered
        }
    }

    /// Whether the OK button is enabled / Enter submits (`!allowEmpty && empty`
    /// disables it).
    pub fn can_submit(&self, text: &str) -> bool {
        self.allow_empty || !text.is_empty()
    }

    /// Resolves a text-input interaction.
    ///
    /// `submitted` is `None` for cancel/back. `Some(text)` runs the filter and
    /// length clamp; an empty result is only accepted when `allow_empty`.
    pub fn outcome(&self, submitted: Option<&str>) -> TextInputOutcome {
        match submitted {
            None => TextInputOutcome::Cancelled,
            Some(text) => {
                let filtered = self.filter(text);
                if self.can_submit(&filtered) {
                    TextInputOutcome::Submitted(filtered)
                } else {
                    TextInputOutcome::Rejected
                }
            }
        }
    }

    /// The relay `text_input_result` value (`None` = cancelled).
    pub fn relay_result(&self, submitted: Option<&str>) -> Option<String> {
        match self.outcome(submitted) {
            TextInputOutcome::Submitted(text) => Some(text),
            TextInputOutcome::Cancelled | TextInputOutcome::Rejected => None,
        }
    }
}

/// A two-option confirm (`showConfirm`/`showCustomConfirm`/`showOkText`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmSpec {
    /// Dialog title.
    pub title: String,
    /// Body text.
    pub text: String,
    /// Confirm button label (`@ok` unless custom).
    pub yes: String,
    /// Cancel button label (`@cancel`; `None` for `showOkText`).
    pub no: Option<String>,
}

/// Result of a confirm prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Confirm/Enter.
    Confirmed,
    /// Cancel/Escape/Back.
    Cancelled,
}

impl ConfirmSpec {
    /// `showConfirm(title, text)`.
    pub fn confirm(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
            yes: String::from("@ok"),
            no: Some(String::from("@cancel")),
        }
    }

    /// `showCustomConfirm(title, text, yes, no)`.
    pub fn custom(
        title: impl Into<String>,
        text: impl Into<String>,
        yes: impl Into<String>,
        no: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
            yes: yes.into(),
            no: Some(no.into()),
        }
    }

    /// `showOkText(title, text)`.
    pub fn ok(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
            yes: String::from("@ok"),
            no: None,
        }
    }

    /// Maps a boolean confirm answer (`true` = confirm).
    pub fn outcome(confirmed: bool) -> ConfirmOutcome {
        if confirmed {
            ConfirmOutcome::Confirmed
        } else {
            ConfirmOutcome::Cancelled
        }
    }
}

/// A `showInfoPopup` entry (plan §3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct PopupEntry {
    /// Message body.
    pub message: String,
    /// Duration in seconds.
    pub duration: f32,
    /// Alignment bits (`Align`).
    pub align: i32,
    /// Top inset.
    pub top: i32,
    /// Left inset.
    pub left: i32,
    /// Bottom inset.
    pub bottom: i32,
    /// Right inset.
    pub right: i32,
}

/// Id-keyed popup registry (`UI.popups`): `None` id is transient and never
/// replaces anything; a `Some` id replaces the previous entry.
#[derive(Debug, Clone, Default)]
pub struct PopupRegistry {
    entries: IndexMap<String, PopupEntry>,
}

impl PopupRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// `showInfoPopup(message, id, …)`. Returns the replaced entry, if any.
    pub fn show(
        &mut self,
        message: impl Into<String>,
        id: Option<impl Into<String>>,
        entry: PopupEntry,
    ) -> Option<PopupEntry> {
        let message = message.into();
        let mut entry = entry;
        entry.message = message;
        match id {
            Some(id) => self.entries.insert(id.into(), entry),
            None => None,
        }
    }

    /// `showInfoPopup(null, id, …)`: removes the entry with `id`.
    pub fn remove(&mut self, id: &str) -> Option<PopupEntry> {
        self.entries.shift_remove(id)
    }

    /// Number of live id-keyed popups.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no popups are live.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reads an entry.
    pub fn get(&self, id: &str) -> Option<&PopupEntry> {
        self.entries.get(id)
    }
}

/// Tracks `UI.lastAnnouncement` for `hasAnnouncement()`.
#[derive(Debug, Clone, Default)]
pub struct AnnouncementTracker {
    active: bool,
}

impl AnnouncementTracker {
    /// Empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a new announcement (`showInfoToast`/`announce` set
    /// `lastAnnouncement`).
    pub fn announce(&mut self) {
        self.active = true;
    }

    /// Clears the announcement when it is removed from the scene.
    pub fn clear(&mut self) {
        self.active = false;
    }

    /// `UI.hasAnnouncement()`.
    pub fn has_announcement(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::builder::ui_relay::TextInput;

    #[test]
    fn prompt_catalogue_is_complete_and_unique() {
        assert_eq!(PROMPT_HELPERS.len(), 19);
        let mut seen = std::collections::HashSet::new();
        for name in PROMPT_HELPERS {
            assert!(seen.insert(*name), "duplicate prompt helper '{name}'");
        }
        for (index, kind) in PromptKind::ALL.iter().enumerate() {
            assert_eq!(kind.name(), PROMPT_HELPERS[index]);
            assert_eq!(PromptKind::from_name(kind.name()), Some(*kind));
        }
        assert_eq!(PromptKind::from_name("bogus"), None);
        assert!(!PromptKind::Info.should_pause());
        assert!(PromptKind::Confirm.is_modal());
        assert!(!PromptKind::Announce.is_modal());
        assert!(PromptKind::InfoPopup.is_id_addressable());
        assert!(!PromptKind::Info.is_id_addressable());
    }

    #[test]
    fn text_input_numeric_filter_and_length() {
        let spec = TextInputSpec::new("t", "m", 5, "ab", true, false);
        assert!(spec.permits('7'));
        assert!(!spec.permits('x'));
        // Non-digits are stripped, then the length is clamped.
        assert_eq!(spec.filter("a1b2c3d4e5f6"), "12345");
        assert_eq!(spec.filter("123456"), "12345");

        let plain = TextInputSpec::new("t", "m", 3, "", false, true);
        assert_eq!(plain.filter("abcdef"), "abc");
        assert_eq!(plain.filter("a b"), "a b");
    }

    #[test]
    fn text_input_allow_empty_gating() {
        let strict = TextInputSpec::new("t", "m", 8, "", false, false);
        assert!(!strict.can_submit(""));
        assert!(strict.can_submit("x"));
        assert_eq!(strict.outcome(None), TextInputOutcome::Cancelled);
        assert_eq!(strict.outcome(Some("")), TextInputOutcome::Rejected);
        assert_eq!(
            strict.outcome(Some("hi")),
            TextInputOutcome::Submitted(String::from("hi"))
        );
        assert_eq!(strict.relay_result(Some("")), None);
        assert_eq!(strict.relay_result(Some("hi")), Some(String::from("hi")));

        let lenient = TextInputSpec::new("t", "m", 8, "", false, true);
        assert!(lenient.can_submit(""));
        assert_eq!(
            lenient.outcome(Some("")),
            TextInputOutcome::Submitted(String::new())
        );
    }

    #[test]
    fn text_input_from_relay_round_trips_semantics() {
        let relay = TextInput {
            id: 3,
            title: "Name".to_owned(),
            message: "Enter".to_owned(),
            len: 4,
            def: "abcd".to_owned(),
            numeric: true,
            allow_empty: false,
        };
        let spec = TextInputSpec::from_relay(&relay);
        assert_eq!(spec.max_length, 4);
        assert!(spec.numeric);
        assert!(!spec.allow_empty);
        assert_eq!(spec.filter("1a2b3c"), "123");
    }

    #[test]
    fn confirm_specs_and_outcomes() {
        let confirm = ConfirmSpec::confirm("@confirm", "sure?");
        assert_eq!(confirm.yes, "@ok");
        assert_eq!(confirm.no.as_deref(), Some("@cancel"));
        assert_eq!(ConfirmSpec::outcome(true), ConfirmOutcome::Confirmed);
        assert_eq!(ConfirmSpec::outcome(false), ConfirmOutcome::Cancelled);

        let custom = ConfirmSpec::custom("@t", "body", "Yes!", "Nope");
        assert_eq!(custom.yes, "Yes!");
        assert_eq!(custom.no.as_deref(), Some("Nope"));
        assert!(ConfirmSpec::ok("@t", "body").no.is_none());
    }

    #[test]
    fn popup_registry_replace_and_remove() {
        let mut registry = PopupRegistry::new();
        let entry = PopupEntry {
            message: String::new(),
            duration: 2.0,
            align: 1,
            top: 0,
            left: 0,
            bottom: 0,
            right: 0,
        };
        assert!(registry.is_empty());
        assert!(registry.show("first", Some("p"), entry.clone()).is_none());
        assert_eq!(registry.len(), 1);
        let replaced = registry.show("second", Some("p"), entry.clone());
        assert_eq!(
            replaced.map(|value| value.message),
            Some(String::from("first"))
        );
        assert_eq!(
            registry.get("p").map(|value| value.message.as_str()),
            Some("second")
        );
        assert!(
            registry
                .show("transient", Option::<String>::None, entry)
                .is_none()
        );
        assert_eq!(registry.len(), 1);
        assert!(registry.remove("p").is_some());
        assert!(registry.remove("p").is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn announcement_tracker_tracks_last() {
        let mut tracker = AnnouncementTracker::new();
        assert!(!tracker.has_announcement());
        tracker.announce();
        assert!(tracker.has_announcement());
        tracker.announce();
        assert!(tracker.has_announcement());
        tracker.clear();
        assert!(!tracker.has_announcement());
    }
}
