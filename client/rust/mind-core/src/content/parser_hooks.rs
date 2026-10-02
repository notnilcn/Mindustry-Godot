// SPDX-License-Identifier: GPL-3.0-only

//! Registry-side mod/patch interfaces consumed by plan 20.
//!
//! Ported from the interface shapes of `mod/ContentParser.java`,
//! `mod/DataPatcher.java`, `mod/DataManager.java` and
//! `Mods.handleContentError` (`ContentLoader.initialize` error routing). Plan 20
//! implements these traits; plan 02 ships the trait definitions and registry APIs
//! they call. The `ModSet` parameter from plan 02 §3.6 is deferred to plan 20
//! (reconcile at M6), so the hooks only see the registry.

use super::load::ContentRegistry;
use super::{ContentError, ContentRef, ContentType};

/// One content asset parsed from a data mod/save (`ContentParser` input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentAsset {
    /// Asset name (unprefixed mod name; `transform_name` applies the `dp`/mod prefix).
    pub name: String,
    /// Declared content type.
    pub type_: ContentType,
    /// Source file/line text for error messages.
    pub source_file: Option<String>,
}

/// One data-patch asset (`DataPatcher` input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchAsset {
    /// Patch name.
    pub name: String,
    /// Raw JSON payload (plan 20 owns the schema).
    pub json: String,
}

/// A single field-restore closure produced by a patch (`ResetAction`).
///
/// Index membership is restored separately by
/// [`ContentRegistry::restore_index`](super::load::ContentRegistry::restore_index).
pub type ResetAction = Box<dyn FnOnce(&mut ContentRegistry) + 'static>;

/// Collections of registry errors returned by bulk mod operations.
pub type ContentErrors = Vec<ContentError>;

/// Parse failure surface (plan 20 formats the message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentParseError {
    /// Human-readable failure.
    pub message: String,
}

impl ContentParseError {
    /// Builds an error from any displayable message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ContentParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ContentParseError {}

/// `mods.loadContent()` equivalent: plan 20 adds mod content into the registry.
pub trait ModContentProvider {
    /// Loads all mod content; per-content failures are routed by the caller.
    fn load_content(&mut self, registry: &mut ContentRegistry) -> Result<(), ContentErrors>;
}

/// `ContentParser.parse` equivalent.
pub trait ContentParserHook {
    /// Parses one content asset and registers it; returns the new reference.
    fn parse(
        &mut self,
        registry: &mut ContentRegistry,
        asset: &ContentAsset,
    ) -> Result<ContentRef, ContentParseError>;
}

/// `DataPatcher.apply` equivalent.
pub trait PatchHook {
    /// Applies patches and returns field-restore actions for `logic.reset()`.
    fn apply(
        &mut self,
        registry: &mut ContentRegistry,
        patches: &[PatchAsset],
        content: &[ContentAsset],
    ) -> Result<Vec<ResetAction>, ContentError>;
}

/// `Mods.handleContentError` equivalent: per-content mod failures are reported
/// here instead of aborting the sweep.
pub trait ModErrorSink {
    /// Reports a lifecycle failure for a mod content record.
    fn handle_content_error(&mut self, content: ContentRef, error: &ContentError);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::bundle::MemoryBundle;
    use crate::content::color::Rgba;
    use crate::content::load::ContentRegistry;
    use crate::content::registries::items::Item;
    use crate::content::settings_store::MemoryUnlockStore;
    use crate::content::stacks::ItemStack;
    use crate::content::test_support::test_registry;
    use crate::content::{ContentType, ModId};

    /// One fake data asset for [`FakeProvider`] (`DataAssetTests.loadContent`).
    struct FakeAsset {
        name: &'static str,
        hardness: i32,
        localized: Option<&'static str>,
        fail: bool,
    }

    /// Fake data-patch provider replicating the `dp` pseudo-mod path of
    /// `DataAssetTests` (plan 20 replaces this with the JSON `ContentParser`).
    struct FakeProvider {
        assets: Vec<FakeAsset>,
    }

    impl FakeProvider {
        fn new(assets: Vec<FakeAsset>) -> Self {
            Self { assets }
        }

        fn parse_all(&mut self, registry: &mut ContentRegistry) -> Result<(), ContentErrors> {
            let mut bundle = MemoryBundle::new();
            for asset in &self.assets {
                if let Some(localized) = asset.localized {
                    bundle.insert(format!("item.dp-{}.name", asset.name), localized);
                }
            }
            let store = MemoryUnlockStore::new();
            let mut errors = Vec::new();
            for asset in &self.assets {
                let name = registry.transform_name(asset.name);
                if asset.fail {
                    // DataAssetTests.noContentAddedWithError: nothing is pushed
                    // and no name is registered for a failed asset.
                    errors.push(ContentError::Parse(format!("bad asset `{name}`")));
                    continue;
                }
                let mut item = Item::new(&name, Rgba::new(1.0, 1.0, 1.0, 1.0), &bundle, &store);
                item.hardness = asset.hardness;
                item.minfo.source_file = Some(format!("{}.json", asset.name));
                if let Err(error) = registry.add_item(item) {
                    errors.push(error);
                }
            }
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors)
            }
        }
    }

    impl ModContentProvider for FakeProvider {
        fn load_content(&mut self, registry: &mut ContentRegistry) -> Result<(), ContentErrors> {
            let previous = registry.current_mod().cloned();
            registry.set_current_mod(Some(ModId(String::from("dp"))));
            let result = self.parse_all(registry);
            registry.set_current_mod(previous);
            result
        }
    }

    /// Plan 02 §7a: `parser_hooks::tests::provider_contract` —
    /// `DataAssetTests.basicItem`/`basicUnit`/`noContentAddedWithError`/
    /// `noNullFieldsAllowed` with the fake provider. Add → resolve by
    /// `dp-<name>`; error case leaves the registry count unchanged and no
    /// dangling name.
    #[test]
    fn provider_contract() {
        let mut registry = test_registry();
        let items_before = registry.items().len();
        let stances_before = registry.unit_stances().len();
        let epoch_before = registry.arr_epoch();

        let mut provider = FakeProvider::new(vec![
            FakeAsset {
                name: "testitem",
                hardness: 10,
                localized: Some("Test Item"),
                fail: false,
            },
            FakeAsset {
                name: "baditem",
                hardness: 0,
                localized: None,
                fail: true,
            },
        ]);
        let errors = registry
            .create_mod_content(&mut provider)
            .expect_err("the bad asset reports an error");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("bad asset"), "{}", errors[0]);

        // The good asset was still registered (`Mods.loadContent` continues).
        assert_eq!(registry.items().len(), items_before + 1);
        assert!(registry.arr_epoch() > epoch_before);

        let reference = registry
            .get_by_name(ContentType::Item, "dp-testitem")
            .expect("prefixed name resolves");
        assert_eq!(reference.type_, ContentType::Item);
        assert_eq!(registry.by_name("dp-testitem"), Some(reference));
        let item = registry.item_by_name("dp-testitem").expect("item record");
        assert_eq!(item.hardness, 10);
        assert_eq!(item.unlock.localized_name, "Test Item");
        assert!(item.minfo.is_patch_content());
        assert_eq!(item.minfo.source_file.as_deref(), Some("testitem.json"));
        assert_eq!(item.minfo.mod_id.as_ref().map(ModId::name), Some("dp"));

        // `UnitStances.loadAfterMods` materialized a stance for the mod item.
        assert_eq!(registry.unit_stances().len(), stances_before + 1);
        assert!(
            registry
                .unit_stances()
                .iter()
                .any(|stance| stance.item == Some(item.id)),
            "mod item stance exists"
        );

        // Failed asset: no content, no dangling name (`noContentAddedWithError`).
        assert_eq!(registry.items().len(), items_before + 1);
        assert!(
            registry
                .get_by_name(ContentType::Item, "dp-baditem")
                .is_none(),
            "failed asset must not leave a name"
        );

        // `ContentLoader.remove` drops the record and its name.
        registry.remove(reference);
        assert_eq!(registry.items().len(), items_before);
        assert!(registry.by_name("dp-testitem").is_none());
        assert!(
            registry
                .get_by_name(ContentType::Item, "dp-testitem")
                .is_none()
        );

        // `remove_last` only drops the actual last record of its type.
        let store = MemoryUnlockStore::new();
        let bundle = MemoryBundle::new();
        let extra = Item::new("last-item", Rgba::new(1.0, 1.0, 1.0, 1.0), &bundle, &store);
        registry.add_item(extra).expect("last item registers");
        assert_eq!(registry.items().len(), items_before + 1);
        registry.remove_last();
        assert_eq!(registry.items().len(), items_before);
        assert!(registry.item_by_name("last-item").is_none());
    }

    /// Plan 02 §7a: `parser_hooks::tests::index_snapshot_restore` —
    /// `PatcherTests.unitWeapons` + `specificArrayRequirements` reset semantics:
    /// snapshot → mutate/append → `restore_index` + `ResetAction`s reproduce the
    /// prior state with an identical ID/name set.
    #[test]
    fn index_snapshot_restore() {
        let mut registry = test_registry();
        let snapshot = registry.snapshot_index();
        let epoch_before = registry.arr_epoch();

        let dagger = registry.unit_id("dagger").expect("dagger");
        let scatter = registry.block_id("scatter").expect("scatter");
        let original_weapons = registry.unit(dagger).expect("unit").weapons.clone();
        let original_first = registry.block(scatter).expect("block").requirements[0];
        let items_before = registry.items().len();
        let stances_before = registry.unit_stances().len();
        let unit_ids: Vec<u16> = registry
            .entries(ContentType::Unit)
            .iter()
            .map(|entry| entry.id)
            .collect();

        // PatcherTests.unitWeapons: append a weapon to dagger.
        let patched_weapon = {
            let mut weapon = original_weapons[0].clone();
            weapon.name = String::from("patched-weapon");
            weapon
        };
        registry
            .unit_mut(dagger)
            .expect("unit mutable")
            .weapons
            .push(patched_weapon);
        assert_eq!(
            registry.unit(dagger).expect("unit").weapons.len(),
            original_weapons.len() + 1
        );

        // PatcherTests.specificArrayRequirements: replace one array entry.
        let surge = registry.item_id("surge-alloy").expect("surge-alloy");
        registry
            .block_mut(scatter)
            .expect("block mutable")
            .requirements[0] = ItemStack::new(surge, 10);
        assert_eq!(
            registry.block(scatter).expect("block").requirements[0],
            ItemStack::new(surge, 10)
        );

        // Mod content added during the "patch" window is truncated on restore.
        let mut provider = FakeProvider::new(vec![FakeAsset {
            name: "snapshotitem",
            hardness: 1,
            localized: None,
            fail: false,
        }]);
        registry
            .create_mod_content(&mut provider)
            .expect("provider loads");
        assert_eq!(registry.items().len(), items_before + 1);

        // `logic.reset()` equivalent: index restore first, then field resets.
        let actions: Vec<ResetAction> = vec![
            {
                let original = original_weapons.clone();
                Box::new(move |registry: &mut ContentRegistry| {
                    registry.unit_mut(dagger).expect("unit").weapons = original.clone();
                })
            },
            Box::new(move |registry: &mut ContentRegistry| {
                registry.block_mut(scatter).expect("block").requirements[0] = original_first;
            }),
        ];
        registry.restore_index(snapshot);
        for action in actions {
            action(&mut registry);
        }

        assert_eq!(
            registry.unit(dagger).expect("unit").weapons,
            original_weapons
        );
        assert_eq!(
            registry.block(scatter).expect("block").requirements[0],
            original_first
        );
        assert_eq!(registry.items().len(), items_before);
        assert_eq!(registry.unit_stances().len(), stances_before);
        assert!(
            registry
                .get_by_name(ContentType::Item, "dp-snapshotitem")
                .is_none(),
            "restored index drops mod names"
        );
        assert!(registry.arr_epoch() > epoch_before);
        let restored_ids: Vec<u16> = registry
            .entries(ContentType::Unit)
            .iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(restored_ids, unit_ids, "ID set identical after restore");
    }
}
