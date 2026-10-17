// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Content boot with mods (I07 T4): the one registry construction path every
//! host uses.
//!
//! Upstream order (`ClientLauncher.java:166-189`): `content.createBaseContent()`
//! / `content.loadColors()` -> `mods.loadScripts()` -> `content.createModContent()`
//! -> `content.init()` / `content.load()` -> `mods.loadModPatches()`. This module
//! owns the pure part: base content, the enabled mods' JSON content, the
//! lifecycle, then each mod's data patches (`Mods.loadModPatches`,
//! `Mods.java:922-935`).

use crate::content::bundle::BundleView;
use crate::content::settings_store::UnlockStore;
use crate::content::{ContentRegistry, create_base_content};
use crate::io::FileSystem;
use crate::mods::Mods;
use crate::mods::patch::{DataPatcher, collect_mod_patches};
use crate::mods::provider::{ModBootContent, ModContentFailure, ModsContentProvider};

/// Boot result: the registry plus the mod failures/warnings the dialog surface
/// needs (`Mods.handleContentError`, `Mods.java:958-960`).
#[derive(Debug, Default, Clone)]
pub struct ModBootReport {
    /// Failed content files per mod (`LoadedMod.erroredContent` input).
    pub failures: Vec<ModContentFailure>,
    /// `Mods.loadModPatches` warnings (one per mod whose patches failed).
    pub patch_warnings: Vec<String>,
}

/// Collects the enabled mods' boot data: JSON content files then patch sets
/// (`Mods.buildFiles`/`loadModPatches` order).
pub fn collect_boot_content(mods: &mut Mods, fs: &dyn FileSystem) -> ModBootContent {
    ModBootContent {
        files: mods.collect_content_files(fs),
        patches: collect_mod_patches(mods, fs),
    }
}

/// `createBaseContent` -> mod content -> `init()`/`postInit()` -> patches.
///
/// `headless` is the existing `ContentRegistry::new` flag the host passed to
/// `create_base_content` (the client boots with `true`, which skips
/// `loadIcon()`/`load()` only).
pub fn build_registry_with_mods(
    bundle: &dyn BundleView,
    unlocks: &dyn UnlockStore,
    headless: bool,
    boot: &ModBootContent,
) -> Result<(ContentRegistry, ModBootReport), String> {
    let mut report = ModBootReport::default();
    let mut registry = create_base_content(bundle, unlocks, headless)
        .map_err(|error| format!("base content: {error}"))?;

    if !boot.files.is_empty() {
        let mut provider = ModsContentProvider::new(boot.files.clone(), unlocks);
        if let Err(errors) = registry.create_mod_content(&mut provider) {
            // Per-file failures are isolated (Java marks each and continues);
            // the provider keeps the mod attribution for the dialog.
            log::warn!("[content] {} mod content file(s) failed", errors.len());
        }
        report.failures = provider.mod_failures().to_vec();
    }

    registry
        .init()
        .map_err(|error| format!("content init: {error}"))?;
    registry
        .post_init()
        .map_err(|error| format!("content post-init: {error}"))?;

    // `Blocks.java` turret `ammo(...)` tables: the anonymous ammo bullets join
    // the registry so placed turrets resolve a live `TurretBehavior`
    // (PAR-0086, `sim::runtime::load_vanilla_content` does the same for the
    // harness). Without this the live client registers no turret behaviors and
    // placed turrets lose their `TurretState` (I01 T16 finding).
    crate::world::blocks::defense::turrets::install_turret_ammo(&mut registry);

    for set in &boot.patches {
        if set.assets.is_empty() {
            continue;
        }
        let mut patcher = DataPatcher::new();
        if let Err(error) = patcher.apply(&mut registry, &set.assets) {
            report.patch_warnings.push(format!(
                "Failed to apply patches from mod {}: {error}",
                set.mod_name
            ));
        }
        report.patch_warnings.extend(set.warnings.iter().cloned());
    }

    Ok((registry, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::io::SettingsStore;
    use crate::io::fs::NativeFs;
    use std::path::Path;

    /// I01 T16 finding: the live boot path must install the turret ammo
    /// bullets — without them `defense::register` skips every turret and a
    /// placed turret has no `TurretState` (live-client regression).
    #[test]
    fn live_boot_installs_turret_behaviors() {
        let (registry, _) = build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &ModBootContent::default(),
        )
        .expect("boot");
        let behaviors = crate::world::blocks::default_registry(&registry);
        // The defense family registers turrets BY NAME
        // (`BehaviorRegistry::register_named`); `BlockTable::build_default`
        // resolves the name against content.
        for name in ["duo", "scatter", "foreshadow"] {
            assert!(
                behaviors.get_named(name).is_some(),
                "`{name}` has its turret behavior after the live boot"
            );
        }
        // The ammo bullets themselves joined the registry (the gate
        // `ammo_in_content` checks).
        assert!(registry.bullet_by_name("duo_copper").is_some());
    }

    /// J09 T1: the `consumers` fixture's `consumes` array parses the mapped
    /// `Consume*` classes (with Java's defaults); the unsupported ones warn.
    #[test]
    fn consumers_fixture_parses_consume_classes() {
        use crate::content::registries::blocks::Consume;

        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../parity/mod_fixtures/consumers");
        let fs = NativeFs;
        let mut mods = Mods::new(true, &dir);
        mods.load_single(&fs, &dir, &SettingsStore::new())
            .expect("load consumers");
        let boot = collect_boot_content(&mut mods, &fs);
        let (registry, report) =
            build_registry_with_mods(&MemoryBundle::new(), &MemoryUnlockStore::new(), true, &boot)
                .expect("boot with mods");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let block = registry
            .block_by_name("consumers-consumer-kit")
            .expect("modded block");
        assert!(block.minfo.is_modded());
        assert_eq!(block.consumes.len(), 5, "mapped consumers");
        let Consume::Items(items) = &block.consumes[0].consume else {
            panic!("ConsumeItems");
        };
        assert_eq!(items[0].amount, 10);
        let Consume::Power { usage, buffered } = &block.consumes[3].consume else {
            panic!("ConsumePower");
        };
        assert_eq!((*usage, *buffered), (1.5, 1000.0));
        let Consume::Coolant {
            amount,
            allow_liquid,
            allow_gas,
        } = &block.consumes[4].consume
        else {
            panic!("ConsumeCoolant");
        };
        assert_eq!((*amount, *allow_liquid, *allow_gas), (1.0, true, false));
    }

    /// I07 T4 (PAR-0115 part 1): `parity/mod_fixtures/allkinds` through the one
    /// boot path — the modded block/item/unit register with `is_modded()` and
    /// the base content ids are unchanged.
    #[test]
    fn allkinds_boot_registry_contains_mod_content() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../parity/mod_fixtures/allkinds");
        let fs = NativeFs;
        let mut mods = Mods::new(true, &dir);
        mods.load_single(&fs, &dir, &SettingsStore::new())
            .expect("load allkinds");
        let boot = collect_boot_content(&mut mods, &fs);
        assert!(!boot.files.is_empty(), "content files collected");

        // Base-only comparison registry (`createBaseContent` ids are frozen).
        let base = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("base content");

        let (registry, report) =
            build_registry_with_mods(&MemoryBundle::new(), &MemoryUnlockStore::new(), true, &boot)
                .expect("boot with mods");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let item = registry
            .item_by_name("all-kinds-all-ingot")
            .expect("modded item");
        assert!(item.minfo.is_modded());
        let block = registry
            .block_by_name("all-kinds-all-wall")
            .expect("modded block");
        assert!(block.minfo.is_modded());
        let unit = registry
            .unit_by_name("all-kinds-all-mech")
            .expect("modded unit");
        assert!(unit.minfo.is_modded());

        // Base content ids are unchanged (first 300 blocks, dense order).
        let base_blocks: Vec<u16> = base.blocks().iter().map(|b| b.id.raw()).take(300).collect();
        let boot_blocks: Vec<u16> = registry
            .blocks()
            .iter()
            .map(|b| b.id.raw())
            .take(300)
            .collect();
        assert_eq!(base_blocks.len(), 300);
        assert_eq!(boot_blocks, base_blocks);
    }
}
