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

/// `createBaseContent` -> `loadColors` -> mod content -> `init()`/`postInit()`
/// -> patches.
///
/// `headless` is the existing `ContentRegistry::new` flag the host passed to
/// `create_base_content` (the client boots with `true`, which skips
/// `loadIcon()`/`load()` only). `block_colors` is row 0 of
/// `sprites/block_colors.png` (`ContentLoader.loadColors`,
/// `ClientLauncher.java:166-168`); empty skips it (the headless server never
/// loads colours).
pub fn build_registry_with_mods(
    bundle: &dyn BundleView,
    unlocks: &dyn UnlockStore,
    headless: bool,
    boot: &ModBootContent,
    block_colors: &[u32],
) -> Result<(ContentRegistry, ModBootReport), String> {
    let mut report = ModBootReport::default();
    let mut registry = create_base_content(bundle, unlocks, headless)
        .map_err(|error| format!("base content: {error}"))?;
    registry.load_colors(block_colors);

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

    /// Boots one `parity/mod_fixtures/<fixture>` mod through the same path the
    /// live client/host uses (load -> collect -> base + mod content + patches).
    fn boot_fixture(fixture: &str) -> (ContentRegistry, ModBootReport) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../parity/mod_fixtures")
            .join(fixture);
        let fs = NativeFs;
        let mut mods = Mods::new(true, &dir);
        mods.load_single(&fs, &dir, &SettingsStore::new())
            .unwrap_or_else(|error| panic!("load {fixture}: {error}"));
        let boot = collect_boot_content(&mut mods, &fs);
        build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &boot,
            &[],
        )
        .unwrap_or_else(|error| panic!("boot {fixture}: {error}"))
    }

    /// L08 T3: every `UnlockableContent` field reads from mod JSON into
    /// `UnlockFields` (`UnlockableContent.java:34-79`), including the
    /// bundle-backed `details`/`credit` strings and the cross-kind
    /// `shownPlanets`/`databaseTabs` references.
    #[test]
    fn unlock_fixture_fills_unlock_fields() {
        let (registry, report) = boot_fixture("unlock");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let item = registry
            .item_by_name("unlock-unlock-ingot")
            .expect("modded item");
        let unlock = &item.unlock;
        assert_eq!(unlock.localized_name, "Unlock Ingot");
        assert_eq!(unlock.description.as_deref(), Some("A fixture ingot."));
        assert_eq!(unlock.details.as_deref(), Some("Details line."));
        assert_eq!(unlock.credit.as_deref(), Some("Fixture Author"));
        assert!(unlock.always_unlocked);
        assert!(unlock.unlocked(), "alwaysUnlocked bypasses the store flag");
        assert!(!unlock.inline_description);
        assert!(!unlock.hide_details);
        assert!(unlock.hide_database);
        assert!(!unlock.generate_icons);
        assert_eq!(unlock.selection_size, 32.0);
        assert_eq!(unlock.full_override, "item-copper");
        assert!(unlock.all_database_tabs);
        assert_eq!(unlock.database_category.as_deref(), Some("material"));
        assert_eq!(unlock.database_tag.as_deref(), Some("custom"));

        let serpulo = registry.planet_id("serpulo").expect("serpulo");
        let copper = registry.item_id("copper").expect("copper");
        assert_eq!(unlock.shown_planets, vec![serpulo]);
        assert!(
            unlock
                .database_tabs
                .contains(&crate::content::ContentRef::item(copper))
        );
        assert!(
            unlock
                .database_tabs
                .contains(&crate::content::ContentRef::of(
                    crate::content::ContentType::Planet,
                    serpulo
                ))
        );

        let block = registry
            .block_by_name("unlock-unlock-wall")
            .expect("modded block");
        assert_eq!(block.unlock.selection_size, 16.0);
        assert!(block.unlock.always_unlocked);
        assert!(block.unlock.hide_database);
        assert_eq!(block.unlock.database_category.as_deref(), Some("defense"));
        assert_eq!(block.unlock.database_tag.as_deref(), Some("wall"));
    }

    /// L08 T3: `Drill.drillMultipliers` (`ObjectFloatMap<Item>`) reads from mod
    /// JSON and resolves each item name to its registry id in `BlockDef`
    /// (`Drill.java:59`, `BlockSpec.drill_multipliers`).
    #[test]
    fn drill_fixture_resolves_drill_multipliers() {
        let (registry, report) = boot_fixture("drill");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let block = registry
            .block_by_name("drill-drill-rig")
            .expect("modded drill");
        let copper = registry.item_id("copper").expect("copper");
        let lead = registry.item_id("lead").expect("lead");
        assert_eq!(
            block.drill_multipliers,
            vec![(copper, 2.5), (lead, 1.25)],
            "insertion order follows the JSON object (serde preserve_order)"
        );
    }

    /// L08 T3: block-level `plans` reads into `BlockSpec.unit_plans` for
    /// `UnitFactory` (`UnitFactory.java:211-223`) and
    /// `BlockSpec.assembler_plans` for `UnitAssembler`
    /// (`UnitAssembler.java:280-294`), resolving units, item stacks and
    /// unit/block payloads.
    #[test]
    fn plans_fixture_resolves_factory_and_assembler_plans() {
        let (registry, report) = boot_fixture("plans");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let factory = registry
            .block_by_name("plans-plan-factory")
            .expect("modded factory");
        assert_eq!(factory.unit_plans.len(), 2);
        let dagger = registry.unit_id("dagger").expect("dagger");
        let flare = registry.unit_id("flare").expect("flare");
        let copper = registry.item_id("copper").expect("copper");
        let lead = registry.item_id("lead").expect("lead");
        let silicon = registry.item_id("silicon").expect("silicon");
        assert_eq!(factory.unit_plans[0].unit, dagger);
        assert_eq!(factory.unit_plans[0].time, 120.0);
        assert_eq!(
            factory.unit_plans[0]
                .requirements
                .iter()
                .map(|stack| (stack.item, stack.amount))
                .collect::<Vec<_>>(),
            vec![(copper, 10), (lead, 5)]
        );
        assert_eq!(factory.unit_plans[1].unit, flare);
        assert_eq!(factory.unit_plans[1].time, 240.0);
        assert_eq!(factory.unit_plans[1].requirements[0].item, silicon);
        assert_eq!(factory.unit_plans[1].requirements[0].amount, 15);

        let assembler = registry
            .block_by_name("plans-plan-assembler")
            .expect("modded assembler");
        assert_eq!(assembler.assembler_plans.len(), 1);
        let plan = &assembler.assembler_plans[0];
        assert_eq!(plan.unit, dagger);
        assert_eq!(plan.time, 600.0);
        assert_eq!(plan.payloads.len(), 2);
        assert_eq!(plan.payloads[0].amount, 4);
        assert_eq!(
            plan.payloads[0].item,
            crate::content::ContentRef::of(crate::content::ContentType::Unit, flare)
        );
        assert_eq!(plan.payloads[1].amount, 2);
        assert_eq!(
            plan.payloads[1].item.type_,
            crate::content::ContentType::Block
        );
        let block = registry
            .block(crate::content::BlockId::new(plan.payloads[1].item.id))
            .expect("resolved payload block");
        assert_eq!(block.name, "copper-wall-large");
    }

    /// L08 T3: effect fields accept every upstream form
    /// (`ContentParser.java:72-82`): a named `Fx` string, an array shorthand for
    /// `MultiEffect`, and inline `MultiEffect`/`ExplosionEffect`/`WaveEffect`/
    /// `WrapEffect` objects. Bullet/weapon effect and sound fields read them.
    #[test]
    fn effects_fixture_parses_inline_and_array_effects() {
        use crate::content::registries::fx_meta::{EffectRef, InlineEffectKind, effect_by_name};

        let (registry, report) = boot_fixture("effects");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let unit = registry
            .unit_by_name("effects-effect-mech")
            .expect("modded unit");
        let weapon = &unit.weapons[0];
        let named = |name: &str| EffectRef::Named(effect_by_name(name).expect(name));

        let EffectRef::Inline(eject) = &weapon.eject_effect else {
            panic!("ejectEffect: {:?}", weapon.eject_effect);
        };
        assert_eq!(eject.kind, InlineEffectKind::MultiEffect);
        assert_eq!(
            eject.children,
            vec![named("smoke"), named("explosion")],
            "array shorthand is a MultiEffect of named effects"
        );

        let EffectRef::Inline(death) = weapon
            .shoot_on_death_effect
            .as_ref()
            .expect("shootOnDeathEffect")
        else {
            panic!("inline ExplosionEffect");
        };
        assert_eq!(death.kind, InlineEffectKind::ExplosionEffect);
        assert_eq!(death.lifetime, 22.0, "ExplosionEffect constructor default");
        assert_eq!(death.clip, 100.0);
        assert_eq!(death.wave_rad, 20.0);
        assert_eq!(death.sparks, 6);

        let bullet = registry.bullet(weapon.bullet.id).expect("inline bullet");
        let EffectRef::Inline(shoot) = &bullet.shoot_effect else {
            panic!("shootEffect: {:?}", bullet.shoot_effect);
        };
        assert_eq!(shoot.kind, InlineEffectKind::MultiEffect);
        assert_eq!(shoot.children.len(), 2);
        assert_eq!(shoot.children[0], named("shootSmall"));
        let EffectRef::Inline(wave) = &shoot.children[1] else {
            panic!("inline WaveEffect child");
        };
        assert_eq!(wave.kind, InlineEffectKind::WaveEffect);
        assert_eq!(wave.size_to, 12.0);

        let EffectRef::Inline(hit) = &bullet.hit_effect else {
            panic!("hitEffect: {:?}", bullet.hit_effect);
        };
        assert_eq!(
            hit.children,
            vec![named("hitBulletSmall"), named("explosion")]
        );

        let EffectRef::Inline(despawn) = &bullet.despawn_effect else {
            panic!("despawnEffect: {:?}", bullet.despawn_effect);
        };
        assert_eq!(despawn.kind, InlineEffectKind::WrapEffect);
        assert_eq!(despawn.children, vec![named("smoke")]);
        assert_eq!(
            despawn.color,
            crate::content::Rgba::from_hex("ff0000"),
            "WrapEffect color"
        );
        assert_eq!(
            despawn.lifetime,
            effect_by_name("smoke").expect("smoke").meta().lifetime,
            "WrapEffect.init copies the child lifetime"
        );

        let EffectRef::Inline(trail) = &bullet.trail_effect else {
            panic!("trailEffect: {:?}", bullet.trail_effect);
        };
        assert_eq!(trail.children, vec![named("smoke")]);

        // `StatusEffect.effect`/`applyEffect` and liquid
        // `particleEffect`/`vaporEffect` accept the same forms.
        let status = registry
            .status_by_name("effects-effect-aura")
            .expect("modded status");
        let EffectRef::Inline(status_effect) = &status.effect else {
            panic!("status effect: {:?}", status.effect);
        };
        assert_eq!(status_effect.kind, InlineEffectKind::MultiEffect);
        assert_eq!(status_effect.children.len(), 2);
        assert_eq!(status_effect.children[0], named("smoke"));
        let EffectRef::Inline(wrapped) = &status_effect.children[1] else {
            panic!("WrapEffect status child");
        };
        assert_eq!(wrapped.kind, InlineEffectKind::WrapEffect);
        assert_eq!(wrapped.children, vec![named("sparkShoot")]);
        let EffectRef::Inline(apply) = &status.apply_effect else {
            panic!("applyEffect: {:?}", status.apply_effect);
        };
        assert_eq!(apply.children, vec![named("explosion"), named("smoke")]);

        let liquid = registry
            .liquid_by_name("effects-effect-fluid")
            .expect("modded liquid");
        let EffectRef::Inline(particle) = &liquid.particle_effect else {
            panic!("particleEffect: {:?}", liquid.particle_effect);
        };
        assert_eq!(particle.kind, InlineEffectKind::WaveEffect);
        assert_eq!(particle.size_to, 9.0);
        let EffectRef::Inline(vapor) = &liquid.vapor_effect else {
            panic!("vaporEffect: {:?}", liquid.vapor_effect);
        };
        assert_eq!(vapor.children, vec![named("smoke"), named("explosion")]);

        assert_eq!(
            bullet.shoot_sound,
            crate::content::registries::sound_meta::SoundId::by_name("shoot").unwrap()
        );
        assert_eq!(
            bullet.hit_sound,
            crate::content::registries::sound_meta::SoundId::by_name("explosion").unwrap()
        );
        assert_eq!(bullet.hit_sound_volume, 0.5);
        assert_eq!(bullet.smoke_effect, named("smoke"));
    }

    /// L08 T3: weapon effect/sound/draw-part fields (`Weapon.java:102-180`)
    /// read from mod JSON: `parts` (`entities/part/*`), `ejectEffect`,
    /// `shootOnDeathEffect`, `shootStatus`, the sound slots, `heatColor` and the
    /// scalar weapon options.
    #[test]
    fn weapon_fx_fixture_parses_parts_effects_and_sounds() {
        use crate::content::registries::units::parts::PartProgressSpec;
        use crate::content::registries::units::parts::{BlendingKind, DrawPartKind};
        use crate::content::registries::units::weapon::WeaponKind;

        let (registry, report) = boot_fixture("weapon_fx");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let unit = registry
            .unit_by_name("weapon-fx-fx-mech")
            .expect("modded unit");
        assert_eq!(unit.weapons.len(), 1, "mirror = false");
        let weapon = &unit.weapons[0];
        assert_eq!(weapon.kind, WeaponKind::Weapon);
        assert_eq!(weapon.reload, 25.0);
        assert_eq!(weapon.recoil, 2.5);
        assert_eq!(weapon.inaccuracy, 3.0);
        assert_eq!(
            weapon.heat_color,
            crate::content::Rgba::from_hex("ff0000").unwrap()
        );
        assert!(!weapon.mirror);

        assert_eq!(
            weapon.shoot_sound,
            crate::content::registries::sound_meta::SoundId::by_name("shoot").unwrap()
        );
        assert_eq!(
            weapon.active_sound,
            crate::content::registries::sound_meta::SoundId::by_name("loopUnitBuilding").unwrap()
        );

        assert_eq!(
            weapon.eject_effect,
            crate::content::registries::fx_meta::EffectRef::Named(
                crate::content::registries::fx_meta::effect_by_name("smoke").unwrap()
            )
        );
        let death = weapon
            .shoot_on_death_effect
            .as_ref()
            .expect("shootOnDeathEffect");
        let crate::content::registries::fx_meta::EffectRef::Inline(death) = death else {
            panic!("inline ExplosionEffect");
        };
        assert_eq!(
            death.kind,
            crate::content::registries::fx_meta::InlineEffectKind::ExplosionEffect
        );
        assert_eq!(death.wave_rad, 24.0);
        assert_eq!(death.sparks, 8);

        assert_eq!(weapon.shoot_status, registry.status_id("wet").expect("wet"));
        assert_eq!(weapon.shoot_status_duration, 120.0);

        assert_eq!(weapon.parts.len(), 2);
        let barrel = &weapon.parts[0];
        assert_eq!(barrel.kind, DrawPartKind::RegionPart);
        assert_eq!(barrel.suffix, "-barrel");
        assert_eq!((barrel.x, barrel.y), (1.0, 2.0));
        assert!(barrel.mirror);
        assert_eq!(barrel.progress, PartProgressSpec::Recoil);
        assert_eq!(barrel.move_y, -2.0);
        assert_eq!(barrel.blending, BlendingKind::Additive);
        assert_eq!(barrel.color, crate::content::Rgba::from_hex("00ff00"));
        assert_eq!(barrel.heat_color, crate::content::Rgba::from_hex("ff0000"));

        let shape = &weapon.parts[1];
        assert_eq!(shape.kind, DrawPartKind::ShapePart);
        assert!(shape.circle);
        assert_eq!(shape.radius, 4.0);
        assert_eq!(
            shape.color,
            Some(crate::content::Rgba::new(1.0, 0.0, 0.0, 1.0))
        );

        let bullet = registry.bullet(weapon.bullet.id).expect("inline bullet");
        assert_eq!(
            bullet.shoot_effect,
            crate::content::registries::fx_meta::EffectRef::Named(
                crate::content::registries::fx_meta::effect_by_name("shootSmall").unwrap()
            )
        );
        assert_eq!(
            bullet.hit_effect,
            crate::content::registries::fx_meta::EffectRef::Named(
                crate::content::registries::fx_meta::effect_by_name("hitBulletSmall").unwrap()
            )
        );
    }

    /// L08 T3: `Block.drawer` parses the upstream forms — an object with a
    /// `type` (default `DrawDefault`) and fields, an array shorthand for
    /// `DrawMulti`, and a class-name string
    /// (`ContentParser.java:182-198`).
    #[test]
    fn drawer_fixture_parses_drawer_trees() {
        use crate::content::registries::blocks::drawer::DrawerKind;

        let (registry, report) = boot_fixture("drawer");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let crafter = registry
            .block_by_name("drawer-drawer-crafter")
            .expect("modded crafter");
        assert_eq!(crafter.drawer.kind, DrawerKind::Multi);
        assert_eq!(crafter.drawer.children.len(), 2);
        let region = &crafter.drawer.children[0];
        assert_eq!(region.kind, DrawerKind::Region);
        assert_eq!(region.suffix, "-glow");
        assert_eq!(region.rotate_speed, 1.5);
        assert!(region.spin_sprite);
        assert_eq!(region.layer, 10.0);
        assert_eq!(region.color, crate::content::Rgba::from_hex("00ff00"));
        let flame = &crafter.drawer.children[1];
        assert_eq!(flame.kind, DrawerKind::Flame);
        assert_eq!(flame.flame_color, crate::content::Rgba::from_hex("ff8800"));
        assert_eq!(flame.flame_radius, 5.0);
        assert_eq!(flame.x, 2.0);

        let array = registry
            .block_by_name("drawer-drawer-array")
            .expect("modded array drawer");
        assert_eq!(array.drawer.kind, DrawerKind::Multi);
        assert_eq!(array.drawer.children.len(), 2);
        assert_eq!(array.drawer.children[0].kind, DrawerKind::Default);
        assert_eq!(array.drawer.children[1].suffix, "-top");

        let plain = registry
            .block_by_name("drawer-drawer-plain")
            .expect("modded plain drawer");
        assert_eq!(plain.drawer.kind, DrawerKind::Default);
    }

    /// `ContentLoader.loadColors` (`ContentLoader.java:140-158`) runs in the
    /// live boot: a `block_colors.png` row sets floor `mapColor`s (opaque),
    /// `hasColor` and `squareSprite`, and `0`/`255` entries are skipped.
    #[test]
    fn live_boot_loads_block_colors() {
        let base = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("base");
        let stone = base.block_id("stone").expect("stone").index();
        let sand = base.block_id("sand-floor").expect("sand").index();
        let mut row = vec![0u32; stone.max(sand) + 1];
        row[stone] = 0x5a5a6eff;
        row[sand] = 0xc8a07a19;
        let (registry, _) = build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &ModBootContent::default(),
            &row,
        )
        .expect("boot");
        let stone_def = registry.block_by_name("stone").expect("stone");
        assert_eq!(
            stone_def.map_color.map(|c| c.to_rgba8888()),
            Some(0x5a5a6eff)
        );
        assert!(stone_def.has_color && stone_def.square_sprite);
        let sand_def = registry.block_by_name("sand-floor").expect("sand");
        assert_eq!(
            sand_def.map_color.map(|c| c.to_rgba8888()),
            Some(0xc8a07aff)
        );
        assert!(sand_def.has_color && !sand_def.square_sprite);
        // Air (row entry 0) keeps its registry colour.
        assert_eq!(registry.blocks()[0].map_color, base.blocks()[0].map_color);
    }

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
            &[],
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

    /// J09 T1: the `classes` fixture resolves the Java base classes through the
    /// ClassMap (`PowerGenerator` -> the port's generator kind, `GroundAI`,
    /// `EmptyBulletType`) and warns for the unsupported ability.
    #[test]
    fn classes_fixture_maps_base_classes() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../parity/mod_fixtures/classes");
        let fs = NativeFs;
        let mut mods = Mods::new(true, &dir);
        mods.load_single(&fs, &dir, &SettingsStore::new())
            .expect("load classes");
        let boot = collect_boot_content(&mut mods, &fs);
        let (registry, report) = build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &boot,
            &[],
        )
        .expect("boot with mods");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let block = registry
            .block_by_name("classes-base-generator")
            .expect("modded block");
        assert_eq!(block.kind, crate::content::BlockKind::ConsumeGenerator);
        let unit = registry
            .unit_by_name("classes-class-mech")
            .expect("modded unit");
        // `UnitType.init` duplicates a `mirror` weapon (`UnitType.java:1121-1135`),
        // so the loaded unit carries the original + its mirrored copy.
        assert_eq!(
            unit.weapons.len(),
            2,
            "weapons: {:?}",
            unit.weapons
                .iter()
                .map(|weapon| weapon.name.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(unit.weapons[0].name, unit.weapons[1].name);
        let bullet = registry
            .bullet(unit.weapons[0].bullet.id)
            .expect("inline bullet");
        assert_eq!(
            bullet.kind,
            crate::content::registries::bullets::BulletKind::Empty
        );
        assert_eq!(
            unit.abilities.len(),
            1,
            "ArmorPlateAbility warns and skips; got {:?}",
            unit.abilities
                .iter()
                .map(|ability| ability.kind.name())
                .collect::<Vec<_>>()
        );
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
        let (registry, report) = build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &boot,
            &[],
        )
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

        let (registry, report) = build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &boot,
            &[],
        )
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
