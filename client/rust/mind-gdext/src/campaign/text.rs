// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! User-facing text: error prompts, unlock toasts and the localized content/sector names.
//! Upstream: ctype/UnlockableContent.java (localizedName); core/src/mindustry/core/UI.java (showErrorMessage); ui/fragments/HudFragment.java (showUnlock)

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Localized preset name for a planet-local sector index (H03 T17: Java's
    /// sector-lost toast passes `state.getSector().name()`, `Control.java`).
    /// Falls back to the internal preset name / an empty string.
    #[func]
    pub fn sector_name_localized(&self, planet: GString, sector: i64) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::new();
        };
        let Some(campaign) = self.campaign.as_ref() else {
            return GString::new();
        };
        let Ok(sector) = u16::try_from(sector) else {
            return GString::new();
        };
        let Some(planet_id) = registry.planet_id(&planet.to_string()) else {
            return GString::new();
        };
        let Some(cell) = campaign.sector(planet_id, sector) else {
            return GString::new();
        };
        let name = match cell.preset {
            Some(preset) => self.sector_localized_name(registry, preset),
            None => cell.preset_name.clone().unwrap_or_default(),
        };
        GString::from(name.as_str())
    }
}

impl MindCampaign {
    /// `UI.showErrorMessage(text)` (`UI.java:495-508`): a modal error prompt.
    /// The port has no dedicated error dialog; `show_text` with the
    /// `@error.title` header is the same shape. The argument is a bundle key
    /// (`@`-prefixed like the Java call sites).
    pub(super) fn show_error(&self, message: &str) {
        let text = self.bundle.get(message.trim_start_matches('@')).to_owned();
        self.show_error_text(&text);
    }

    /// [`Self::show_error`] with a bundle key formatted against `args`
    /// (`Core.bundle.format`).
    pub(super) fn show_error_key(&self, key: &str, args: &[&str]) {
        let text = self.bundle.format(key.trim_start_matches('@'), args);
        self.show_error_text(&text);
    }

    fn show_error_text(&self, text: &str) {
        let Some(mut ui) = self.autoload("MindUi") else {
            log::warn!("error shown with no UI available: {text}");
            return;
        };
        let title = self.bundle.get("error.title").to_owned();
        // `Dialog.add(text)` parses markup upstream; the prompt labels take
        // BBCode, so render both through `UI.formatIcons` first.
        let render = |raw: &str| -> String {
            self.assets_string("markup_text", &[GString::from(raw).to_variant()])
                .unwrap_or_else(|| raw.to_owned())
        };
        let title = render(&title);
        let text = render(text);
        ui.call(
            "show_text",
            &[
                GString::from(title.as_str()).to_variant(),
                GString::from(text.as_str()).to_variant(),
            ],
        );
    }

    /// `Core.scene` menu restore for an invalid map (`World.loadMap` posts
    /// `state.set(State.menu)`, `World.java:391`).
    pub(super) fn set_menu_visible(&self, visible: bool) {
        let Some(mut ui_root) = wiring::optional(&self.ui_root) else {
            return;
        };
        ui_root.bind_mut().set_menu_visible(visible);
    }

    /// Emits an `unlock` for the toast when the content `showUnlock()`s.
    ///
    /// `Control.java:160-165` (`UnlockEvent` -> `hudfrag.showUnlock`); the
    /// default is true (`UnlockableContent.java:242`) and `StatusEffect` is the
    /// only `false` override (`StatusEffect.java:140`), so status unlocks are
    /// skipped. No HUD present (tests) is a no-op.
    pub(super) fn push_unlock_toast(&self, registry: &ContentRegistry, content: ContentRef) {
        if content.type_ == ContentType::Status {
            return;
        }
        let Some(name) = tech_tree::content_name(registry, content) else {
            return;
        };
        let Some(mut hud) = self.autoload("MindHud") else {
            return;
        };
        if hud.has_method("push_unlock") {
            hud.call("push_unlock", &[GString::from(name.as_str()).to_variant()]);
        }
    }

    /// `<type>.<name>.name` through the live bundle, falling back to `name`.
    pub(super) fn localized_content_name(&self, type_name: &str, name: &str) -> String {
        let key = format!("{type_name}.{name}.name");
        match self.bundle_lookup(&key) {
            Some(value) => value.to_owned(),
            None => name.to_owned(),
        }
    }

    /// `<type>.<name>.description` (empty when the bundle has no entry).
    pub(super) fn localized_content_description(&self, type_name: &str, name: &str) -> String {
        self.localized_content_field(type_name, name, "description")
    }

    /// `<type>.<name>.<field>` through the live bundle (empty when missing).
    pub(super) fn localized_content_field(
        &self,
        type_name: &str,
        name: &str,
        field: &str,
    ) -> String {
        let key = format!("{type_name}.{name}.{field}");
        self.bundle_lookup(&key)
            .map(str::to_owned)
            .unwrap_or_default()
    }

    /// `SectorPreset.localizedName`.
    pub(super) fn sector_localized_name(&self, registry: &ContentRegistry, id: SectorId) -> String {
        let name = registry
            .sector(id)
            .map(|record| record.name.clone())
            .unwrap_or_default();
        self.localized_content_name("sector", &name)
    }

    /// A `/root` autoload node by name (`Core.scene`-style singleton lookup;
    /// these are autoloads, not scene-spine deps).
    pub(super) fn autoload(&self, name: &str) -> Option<Gd<Node>> {
        let path = format!("/root/{name}");
        self.base().get_node_or_null(path.as_str())
    }

    /// Calls `method` on the `MindAssets` autoload and returns its `GString`
    /// result as a `String` (`UI.formatIcons` / `Core.bundle` proxies).
    pub(super) fn assets_string(&self, method: &str, args: &[Variant]) -> Option<String> {
        let mut assets = self.autoload("MindAssets")?;
        assets
            .call(method, args)
            .try_to::<GString>()
            .ok()
            .map(|value| value.to_string())
    }

    /// `Core.bundle` lookup with the "value == key means missing" rule
    /// (`Bundle.get` returns the key when it has no entry).
    pub(super) fn bundle_lookup<'a>(&'a self, key: &'a str) -> Option<&'a str> {
        let value = self.bundle.get(key);
        (value != key).then_some(value)
    }

    /// Drops `MindUi`'s cached placement catalog after an unlock change; its
    /// campaign-less fallback reads the persisted settings file directly.
    pub(super) fn invalidate_ui_block_catalog(&self) {
        let Some(mut ui) = self.autoload("MindUi") else {
            return;
        };
        if ui.has_method("invalidate_block_catalog") {
            let _ = ui.call("invalidate_block_catalog", &[]);
        }
    }
}

/// `state.rules.sector.preset.description` for the paused dialog's
/// `@objective` entry (`PausedDialog.java:76-80`).
///
/// The campaign registry boots with an empty `MemoryBundle`, so
/// `UnlockableContent.description` is usually unset; fall back to the live
/// bundle's `sector.<preset>.description` key (`UnlockableContent.java:91`
/// loads it through `Core.bundle`).
pub(super) fn resolve_preset_description(
    registry: Option<&ContentRegistry>,
    bundle: &Bundle,
    preset: Option<SectorId>,
) -> String {
    let Some(def) = preset.and_then(|preset| registry.and_then(|registry| registry.sector(preset)))
    else {
        return String::new();
    };
    def.unlock
        .description
        .clone()
        .filter(|description| !description.is_empty())
        .or_else(|| {
            bundle
                .get_or_null(&format!("sector.{}.description", def.name))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

/// `UnlockableContent.load` (`UnlockableContent.java:117`) and the
/// `SectorPreset` override (`SectorPreset.java:134`): sector presets resolve
/// the `sector-<name>` atlas region (falling back to `Icon.terrain` -- the
/// GDScript `MindWidgets.content_icon` supplies that glyph when the region is
/// missing); every other content resolves `<type>-<name>-ui`. The port's
/// content-type name for `SectorPreset` is `sector` (`ContentType` parity).
pub(super) fn content_icon_name(type_name: &str, name: &str) -> String {
    // T01: one implementation for the core research builder and the facade
    // (`mind_core::ui::campaign::content_icon_name`).
    mind_core::ui::campaign::content_icon_name(type_name, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::assets::bundle::parse_properties;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;

    /// EV-0063: the registry boots with an empty `MemoryBundle`, so the paused
    /// dialog's `@objective` gate must resolve the description from the live
    /// `Core.bundle` key (`sector.<preset>.description`).
    #[test]
    fn preset_description_falls_back_to_the_live_bundle() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let Some(preset) = registry.sector_by_name("groundZero") else {
            panic!("groundZero preset missing");
        };
        assert!(
            preset.unlock.description.is_none(),
            "empty MemoryBundle must leave the registry description unset"
        );
        let properties = parse_properties("sector.groundZero.description = Defend the sector.");
        let bundle = Bundle::from_layers(vec![properties]);
        assert_eq!(
            resolve_preset_description(Some(&registry), &bundle, Some(preset.id)),
            "Defend the sector."
        );
    }

    /// A registry built with the description keeps precedence over the bundle.
    #[test]
    fn preset_description_prefers_the_registry_value() {
        let content =
            MemoryBundle::with_pairs([("sector.groundZero.description", "Registry text")]);
        let registry = match create_base_content(&content, &MemoryUnlockStore::new(), true) {
            Ok(registry) => registry,
            Err(error) => panic!("base content: {error}"),
        };
        let Some(preset) = registry.sector_by_name("groundZero") else {
            panic!("groundZero preset missing");
        };
        let bundle = Bundle::from_layers(vec![parse_properties(
            "sector.groundZero.description = Bundle text",
        )]);
        assert_eq!(
            resolve_preset_description(Some(&registry), &bundle, Some(preset.id)),
            "Registry text"
        );
    }
}
