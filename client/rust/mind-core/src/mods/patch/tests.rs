// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit tests for the data patcher.
//!
//! Upstream: none (port-only tests).

use super::*;
use crate::content::test_support::test_registry;

fn patch(json: &str) -> PatchAsset {
    PatchAsset {
        name: String::from("test"),
        json: json.to_owned(),
    }
}

/// Plan 20 M3: `patch::block_health_edit_and_unapply`.
#[test]
fn block_health_edit_and_unapply() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").health;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &[patch(r#"{"block.router.health": 999}"#)])
        .expect("apply");
    assert_eq!(registry.block(id).expect("block").health, 999);
    assert!(patcher.is_applied());
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").health, original);
    assert!(!patcher.is_applied());
}

/// #60: the patcher addresses the general `BlockDef` scalar surface (not
/// just the original ~8 fields) and `unapply` restores every value,
/// including `Option` fields whose original is `None`.
#[test]
fn block_scalar_field_surface() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").clone();
    assert_eq!(original.item_drop, None, "router drops nothing");
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{
                    "block.router.fogRadius": 7,
                    "block.router.canOverdrive": false,
                    "block.router.squareSprite": true,
                    "block.router.region": "custom-router",
                    "block.router.itemDrop": "copper",
                    "block.router.priority": 3.5,
                    "block.router.playerControllable": false,
                    "block.router.insulated": true,
                    "block.router.depositCooldown": 12.5,
                    "block.router.replaceable": false
                }"#,
            )],
        )
        .expect("apply");
    let block = registry.block(id).expect("block");
    assert_eq!(block.fog_radius, 7);
    assert!(!block.can_overdrive);
    assert!(block.square_sprite);
    assert_eq!(block.region, "custom-router");
    assert_eq!(block.item_drop, registry.item_id("copper"));
    assert_eq!(block.priority, 3.5);
    assert!(!block.player_controllable);
    assert!(block.insulated);
    assert_eq!(block.deposit_cooldown, 12.5);
    assert!(!block.replaceable);
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());

    patcher.unapply(&mut registry);
    let restored = registry.block(id).expect("block");
    assert_eq!(restored.fog_radius, original.fog_radius);
    assert_eq!(restored.can_overdrive, original.can_overdrive);
    assert_eq!(restored.square_sprite, original.square_sprite);
    assert_eq!(restored.region, original.region);
    assert_eq!(restored.item_drop, original.item_drop);
    assert_eq!(restored.priority, original.priority);
    assert_eq!(restored.player_controllable, original.player_controllable);
    assert_eq!(restored.insulated, original.insulated);
    assert_eq!(restored.deposit_cooldown, original.deposit_cooldown);
    assert_eq!(restored.replaceable, original.replaceable);
}

/// Plan 20 M3: `patch::nested_form_edits`.
#[test]
fn nested_form_edits() {
    let mut registry = test_registry();
    let id = registry.item_id("copper").expect("copper");
    let original = registry.item(id).expect("item").hardness;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"item": {"copper": {"hardness": 7}}}"#)],
        )
        .expect("apply");
    assert_eq!(registry.item(id).expect("item").hardness, 7);
    patcher.unapply(&mut registry);
    assert_eq!(registry.item(id).expect("item").hardness, original);
}

/// Plan 20 M3: `patch::no_patch_field_refused`.
#[test]
fn no_patch_field_refused() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").size;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &[patch(r#"{"block.router.size": 5}"#)])
        .expect("apply");
    assert_eq!(registry.block(id).expect("block").size, original);
    assert_eq!(patcher.warnings().len(), 1);
    assert!(patcher.warnings()[0].contains("cannot be edited"));
}

/// Plan 20 M3: `patch::unknown_fields_warn`.
#[test]
fn unknown_fields_warn() {
    let mut registry = test_registry();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.router.bogus": 1, "block.nope.health": 2}"#,
            )],
        )
        .expect("apply");
    assert_eq!(patcher.warnings().len(), 2);
}

/// Plan 20 M3: `patch::replace_only_never_stacks`.
#[test]
fn replace_only_never_stacks() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").health;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &[patch(r#"{"block.router.health": 100}"#)])
        .expect("apply");
    patcher
        .apply(&mut registry, &[patch(r#"{"block.router.health": 200}"#)])
        .expect("apply again");
    assert_eq!(registry.block(id).expect("block").health, 200);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").health, original);
}

/// PatcherTests.specificArrayRequirements: `{0: …}` + full replace.
#[test]
fn array_requirements_index_and_replace() {
    let mut registry = test_registry();
    let id = registry.block_id("scatter").expect("scatter");
    let original = registry.block(id).expect("block").requirements.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"block.scatter.requirements.0": "titanium/99"}"#)],
        )
        .expect("index edit");
    let amount = registry.block(id).expect("block").requirements[0].amount;
    assert_eq!(amount, 99);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").requirements, original);

    // Whole-array replace.
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"block.scatter.requirements": ["copper/5"]}"#)],
        )
        .expect("replace");
    assert_eq!(registry.block(id).expect("block").requirements.len(), 1);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").requirements, original);
}

/// PatcherTests.reconstructorPlansAdd: `+` append (`requirements.+`).
#[test]
fn array_requirements_append() {
    let mut registry = test_registry();
    let id = registry.block_id("scatter").expect("scatter");
    let original = registry.block(id).expect("block").requirements.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"block.scatter.requirements.+": "copper/7"}"#)],
        )
        .expect("append");
    let after = registry.block(id).expect("block").requirements.clone();
    assert_eq!(after.len(), original.len() + 1);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").requirements, original);
}

/// PatcherTests.setMultiAdd: `ObjectSet` `+` (`immunities.+`).
#[test]
fn object_set_multi_add() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").immunities.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"unit.dagger.immunities.+": "wet"}"#)],
        )
        .expect("append immunity");
    let after = registry.unit(id).expect("unit").immunities.clone();
    assert_eq!(after.len(), original.len() + 1);
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").immunities, original);
}

/// PatcherTests.consumeApply: `hasPower`, consumer count, reset.
#[test]
fn consume_apply_power() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").consumes.clone();
    let had_power = registry.block(id).expect("block").has_power;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"block.router.consumes": {"power": 3.5}}"#)],
        )
        .expect("consumes");
    let block = registry.block(id).expect("block");
    assert!(block.has_power);
    assert_eq!(block.consumes.len(), original.len() + 1);
    patcher.unapply(&mut registry);
    let block = registry.block(id).expect("block");
    assert_eq!(block.consumes, original);
    assert_eq!(block.has_power, had_power);
}

/// Plan 20 M3: `patch::load_mod_patches` applies a mod's `patches/` files.
#[test]
fn load_mod_patches_applies() {
    use crate::io::{MockFs, SettingsStore};
    use crate::mods::Mods;
    use std::path::Path;

    let fs = MockFs::new();
    fs.write(
        Path::new("/mods/patcher/mod.json"),
        br#"{"name":"Patcher","minGameVersion":"146"}"#,
    )
    .expect("mod.json");
    fs.write(
        Path::new("/mods/patcher/patches/health.json"),
        br#"{"block.router.health": 777}"#,
    )
    .expect("patch");
    let settings = SettingsStore::new();
    let mut mods = Mods::new(true, "/mods");
    mods.load_single(&fs, Path::new("/mods/patcher"), &settings)
        .expect("load_single");
    let mut registry = test_registry();
    let warnings = load_mod_patches(&mut mods, &mut registry, &fs);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(
        registry.block_by_name("router").expect("router").health,
        777
    );
}

/// PatcherTests.unitWeapons: append a weapon with an inline
/// `LightningBulletType`; `unapply` restores the list and drops the bullet.
#[test]
fn unit_weapons_append() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").weapons.clone();
    let bullets_before = registry.bullets().len();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.dagger.weapons.+": {"name":"navanax-weapon","bullet":{"type":"LightningBulletType","lightningLength":999}}}"#,
            )],
        )
        .expect("append weapon");
    let unit = registry.unit(id).expect("unit");
    assert_eq!(unit.weapons.len(), original.len() + 1);
    let created = unit.weapons.last().expect("created weapon");
    assert_eq!(created.name, "navanax-weapon");
    assert!(registry.bullets().len() > bullets_before);
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").weapons, original);
    assert_eq!(registry.bullets().len(), bullets_before);
}

/// Plan 20 C10: a bullet created by a patch runs the created-object `init()`
/// lifecycle (`DataPatcher.created`); `BulletType.init` sets `pierce` when
/// `pierceCap >= 1`, observable proof the callback ran.
#[test]
fn created_bullet_runs_init_lifecycle() {
    let mut registry = test_registry();
    let bullets_before = registry.bullets().len();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.dagger.weapons.+": {"name":"lifecycle-weapon","bullet":{"type":"LaserBulletType","pierceCap":2,"damage":5}}}"#,
            )],
        )
        .expect("append weapon");
    let created = &registry.bullets()[bullets_before..];
    assert_eq!(created.len(), 1, "one inline bullet registered");
    assert_eq!(created[0].pierce_cap, 2);
    assert!(
        created[0].pierce,
        "created bullet init() lifecycle did not run"
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.bullets().len(), bullets_before);
}

/// PatcherTests.uUnitWeaponReassign: whole-array replace, reset restores.
#[test]
fn unit_weapons_reassign() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").weapons.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.dagger.weapons": [{"name":"megapoop","bullet":{"type":"RailBulletType","lightningLength":999}}]}"#,
            )],
        )
        .expect("reassign weapons");
    let weapons = &registry.unit(id).expect("unit").weapons;
    assert_eq!(weapons.len(), 1);
    assert_eq!(weapons[0].name, "megapoop");
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").weapons, original);
}

/// PatcherTests.arrayMulti: index-keyed object replacement + `weapons.+`
/// append in one patch; unapply restores the authored list.
#[test]
fn array_multi_edit() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").weapons.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"name":"Patch0","unit":{"dagger":{
                    "weapons":{"0":{"type":"Weapon","name":"toxopid-cannon"}},
                    "weapons.+":[{"name":"sei-launcher"}]}}}"#,
            )],
        )
        .expect("array multi");
    let weapons = registry.unit(id).expect("unit").weapons.clone();
    assert_eq!(weapons.len(), original.len() + 1);
    assert_eq!(weapons[0].name, "toxopid-cannon");
    assert_eq!(weapons.last().expect("appended").name, "sei-launcher");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").weapons, original);
}

/// Plan 20 M3b: `fix_content_arrays` world half grows live building
/// `ItemModule`/`LiquidModule` arrays to the dense content counts.
#[test]
fn fix_world_content_arrays_grows_live_modules() {
    let registry = test_registry();
    let item_count = registry.items().len();
    let liquid_count = registry.liquids().len();

    let mut world = bevy_ecs::world::World::new();
    let items = world.spawn(crate::world::ItemModule::with_items(1)).id();
    let liquids = world
        .spawn(crate::world::LiquidModule::with_liquids(1))
        .id();
    world
        .get_mut::<crate::world::ItemModule>(items)
        .expect("item module")
        .add(ItemId::COPPER, 3, 10);

    fix_world_content_arrays(&mut world, &registry);

    let module = world
        .get::<crate::world::ItemModule>(items)
        .expect("module");
    assert_eq!(module.items.len(), item_count);
    assert_eq!(module.get(ItemId::COPPER), 3, "growth preserves counts");
    assert_eq!(
        world
            .get::<crate::world::LiquidModule>(liquids)
            .expect("liquid module")
            .liquids
            .len(),
        liquid_count
    );
}

/// Plan 20 C10: `fix_content_arrays` resizes a block's dense item tables to
/// the live item count (`Block.checkContentArrayCapacity` grow/shrink).
#[test]
fn fix_content_arrays_resizes_block_item_tables() {
    let mut registry = test_registry();
    let item_count = registry.items().len();
    let id = registry.block_id("router").expect("router");
    {
        let block = registry.block_mut(id).expect("block");
        block.item_costs.truncate(1);
        block.item_health_scaling.truncate(1);
    }
    fix_content_arrays(&mut registry);
    {
        let block = registry.block(id).expect("block");
        assert_eq!(block.item_costs.len(), item_count);
        assert_eq!(block.item_health_scaling.len(), item_count);
        assert_eq!(
            block.item_costs[0],
            registry.item(ItemId::new(0)).expect("item").cost
        );
    }
    // Growing past the live count shrinks back (`Arrays.copyOf` parity).
    registry
        .block_mut(id)
        .expect("block")
        .item_costs
        .push(999.0);
    fix_content_arrays(&mut registry);
    assert_eq!(
        registry.block(id).expect("block").item_costs.len(),
        item_count
    );
}

/// PatcherTests.unitFlagsArray: `targetFlags.+` array append + reset.
#[test]
fn unit_target_flags_append() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").target_flags.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.dagger.targetFlags.+": ["shield", "drill"]}"#,
            )],
        )
        .expect("append flags");
    let flags = registry.unit(id).expect("unit").target_flags.clone();
    assert_eq!(flags.len(), original.len() + 2);
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").target_flags, original);
}

/// PatcherTests.unitType: `type` changes the unit entity def; reset restores.
#[test]
fn unit_type_controller_change() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").entity_def;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &[patch(r#"{"unit.dagger.type": "legs"}"#)])
        .expect("change type");
    assert_ne!(registry.unit(id).expect("unit").entity_def, original);
    assert_eq!(
        registry.unit(id).expect("unit").entity_def,
        crate::mods::json::resolve_entity_def("legs").expect("legs")
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").entity_def, original);
}

/// PatcherTests.requiredPlanets gating (see plan 20 §3.6).
#[test]
fn required_planets_gate() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").health;
    let asset = patch(r#"{"requiredPlanets":["erekir"],"block.router.health": 4242}"#);
    // Active planet is serpulo → skipped.
    let mut patcher = DataPatcher::new();
    patcher
        .apply_with_planet(&mut registry, std::slice::from_ref(&asset), Some("serpulo"))
        .expect("gated");
    assert_eq!(registry.block(id).expect("block").health, original);
    patcher.unapply(&mut registry);
    // Active planet is erekir → applied.
    let mut patcher = DataPatcher::new();
    patcher
        .apply_with_planet(&mut registry, &[asset], Some("erekir"))
        .expect("applied");
    assert_eq!(registry.block(id).expect("block").health, 4242);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").health, original);
}

/// PatcherTests.objectFloatMap: `ObjectFloatMap<Item>` edits (object form,
/// nested-object form, dotted key) with reset.
#[test]
fn object_float_map_edit() {
    let mut registry = test_registry();
    let id = registry.block_id("mechanical-drill").expect("drill");
    let titanium = registry.item_id("titanium").expect("titanium");
    let copper = registry.item_id("copper").expect("copper");
    let surge = registry.item_id("surge-alloy").expect("surge-alloy");
    let original = registry.block(id).expect("block").drill_multipliers.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{
                    "block.mechanical-drill.drillMultipliers": {"titanium": 2.0},
                    "block.mechanical-drill": {"drillMultipliers": {"copper": 3.0}},
                    "block.mechanical-drill.drillMultipliers.surge-alloy": 10
                }"#,
            )],
        )
        .expect("drill multipliers");
    let block = registry.block(id).expect("block");
    let get = |item| {
        block
            .drill_multipliers
            .iter()
            .find(|(existing, _)| *existing == item)
            .map(|(_, value)| *value)
    };
    assert_eq!(get(titanium), Some(2.0));
    assert_eq!(get(copper), Some(3.0));
    assert_eq!(get(surge), Some(10.0));
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    patcher.unapply(&mut registry);
    assert_eq!(
        registry.block(id).expect("block").drill_multipliers,
        original
    );
}

/// PatcherTests.attributes: `Attributes` object + dotted-key edits with reset.
#[test]
fn attributes_edit() {
    let mut registry = test_registry();
    let id = registry.block_id("grass").expect("grass");
    let original = registry.block(id).expect("block").attributes.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.grass.attributes": {"oil": 99}, "block.grass.attributes.heat": 77}"#,
            )],
        )
        .expect("attributes");
    let block = registry.block(id).expect("block");
    let get = |name: &str| {
        block
            .attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| *value)
    };
    assert_eq!(get("oil"), Some(99.0));
    assert_eq!(get("heat"), Some(77.0));
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").attributes, original);
}

/// PatcherTests.customAttribute: a custom attribute name is added and removed
/// by reset (the dynamic `Attribute` registry itself is plan 02/06).
#[test]
fn custom_attribute_add_remove() {
    let mut registry = test_registry();
    let id = registry.block_id("grass").expect("grass");
    let original = registry.block(id).expect("block").attributes.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"block.grass.attributes": {"frogs": 10}}"#)],
        )
        .expect("custom attribute");
    assert!(
        registry
            .block(id)
            .expect("block")
            .attributes
            .iter()
            .any(|(key, value)| key == "frogs" && (*value - 10.0).abs() < 1e-6)
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").attributes, original);
}

/// PatcherTests.indexAccess: `weapons.0.bullet.damage` deep field edit + reset.
#[test]
fn weapon_index_field_edit() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let weapon_bullet = registry
        .unit(id)
        .expect("unit")
        .weapons
        .first()
        .expect("weapon")
        .bullet
        .id;
    let original = registry.bullet(weapon_bullet).expect("bullet").damage;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"unit.dagger.weapons.0.bullet.damage": 100}"#)],
        )
        .expect("deep bullet edit");
    assert_eq!(
        registry.bullet(weapon_bullet).expect("bullet").damage,
        100.0
    );
    patcher.unapply(&mut registry);
    assert_eq!(
        registry.bullet(weapon_bullet).expect("bullet").damage,
        original
    );
}

/// PatcherTests.unitAbilities: single `+` ability with reset.
#[test]
fn unit_abilities_single() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").abilities.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.dagger.abilities.+": {"type":"ShieldArcAbility","max":1000}}"#,
            )],
        )
        .expect("append ability");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    let abilities = registry.unit(id).expect("unit").abilities.clone();
    assert_eq!(abilities.len(), original.len() + 1);
    let added = abilities.last().expect("ability");
    assert_eq!(added.kind.name(), "ShieldArcAbility");
    assert_eq!(added.max, 1000.0);
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").abilities, original);
}

/// PatcherTests.unitAbilitiesArray: `+` with an array of abilities.
#[test]
fn unit_abilities_array() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").abilities.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.dagger.abilities.+": [
                    {"type":"ShieldArcAbility","max":1000},
                    {"type":"MoveEffectAbility","amount":10}
                ]}"#,
            )],
        )
        .expect("append abilities");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    let abilities = registry.unit(id).expect("unit").abilities.clone();
    assert_eq!(abilities.len(), original.len() + 2);
    assert_eq!(
        abilities[abilities.len() - 2].kind.name(),
        "ShieldArcAbility"
    );
    assert_eq!(
        abilities[abilities.len() - 1].kind.name(),
        "MoveEffectAbility"
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").abilities, original);
}

/// PatcherTests.unitFactoryPlans (flat): `block.ground-factory.plans.+`.
#[test]
fn unit_factory_plans_flat() {
    let mut registry = test_registry();
    let id = registry.block_id("ground-factory").expect("ground-factory");
    let flare = registry.unit_id("flare").expect("flare");
    let surge = registry.item_id("surge-alloy").expect("surge-alloy");
    let original = registry.block(id).expect("block").unit_plans.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.ground-factory.plans.+":{"unit":"flare","requirements":["surge-alloy/10"],"time":100}}"#,
            )],
        )
        .expect("add plan");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    let plan = registry
        .block(id)
        .expect("block")
        .unit_plans
        .last()
        .expect("plan");
    assert_eq!(plan.unit, flare);
    assert_eq!(plan.time, 100.0);
    assert_eq!(plan.requirements.len(), 1);
    assert_eq!(plan.requirements[0].item, surge);
    assert_eq!(plan.requirements[0].amount, 10);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").unit_plans, original);
}

/// PatcherTests.unitFactoryPlans (nested): `block.{ground-factory:{plans.+}}`.
#[test]
fn unit_factory_plans_nested() {
    let mut registry = test_registry();
    let id = registry.block_id("ground-factory").expect("ground-factory");
    let original = registry.block(id).expect("block").unit_plans.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block":{"ground-factory":{"plans.+":{"unit":"flare","requirements":["surge-alloy/10"],"time":100}}}}"#,
            )],
        )
        .expect("add plan");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    assert_eq!(
        registry.block(id).expect("block").unit_plans.len(),
        original.len() + 1
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").unit_plans, original);
}

/// PatcherTests.specificArrayRequirements (object-index form).
#[test]
fn array_requirements_object_index() {
    let mut registry = test_registry();
    let id = registry.block_id("scatter").expect("scatter");
    let original = registry.block(id).expect("block").requirements.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.scatter.requirements":{"0":"titanium/99"}}"#,
            )],
        )
        .expect("index edit");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    assert_eq!(
        registry.block(id).expect("block").requirements[0].amount,
        99
    );
    assert_eq!(
        registry.block(id).expect("block").requirements[1],
        original[1]
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").requirements, original);
}

/// PatcherTests.gibberish: malformed JSON warns, never panics.
#[test]
fn malformed_patch_warns() {
    let mut registry = test_registry();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &[patch("}[35209509()jfkjhadsf,\n,,,,[][]{")])
        .expect("no panic");
    assert_eq!(patcher.warnings().len(), 1);
}

/// PatcherTests.unitTypeObject: `{"unit.dagger": {"type": "legs"}}` form.
#[test]
fn unit_type_object_syntax() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").entity_def;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"unit.dagger": {"type": "legs"}}"#)],
        )
        .expect("object syntax");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    assert_eq!(
        registry.unit(id).expect("unit").entity_def,
        crate::mods::json::resolve_entity_def("legs").expect("legs")
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").entity_def, original);
}

/// PatcherTests.reconstructorPlans: `Seq<UnitType[]>` replace + typed
/// `consumes` items merge, both restored on unapply.
#[test]
fn reconstructor_plans_and_consumes() {
    let mut registry = test_registry();
    let id = registry
        .block_id("additive-reconstructor")
        .expect("additive-reconstructor");
    let dagger = registry.unit_id("dagger").expect("dagger");
    let flare = registry.unit_id("flare").expect("flare");
    let surge = registry.item_id("surge-alloy").expect("surge-alloy");
    let copper = registry.item_id("copper").expect("copper");
    let original = registry
        .block(id)
        .expect("block")
        .reconstructor_upgrades
        .clone();
    let original_consumes = registry.block(id).expect("block").consumes.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{
                    "block.additive-reconstructor.upgrades": [["dagger", "flare"]],
                    "block.additive-reconstructor.consumes": {
                        "remove": "items",
                        "items": ["surge-alloy/10", "copper/20"]
                    }
                }"#,
            )],
        )
        .expect("reconstructor patch");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    let block = registry.block(id).expect("block");
    assert_eq!(block.reconstructor_upgrades, vec![(dagger, flare)]);
    let items = block
        .consumes
        .iter()
        .find_map(|spec| match &spec.consume {
            Consume::Items(stacks) => Some(stacks.clone()),
            _ => None,
        })
        .expect("items consumer");
    assert_eq!(
        items,
        vec![ItemStack::new(surge, 10), ItemStack::new(copper, 20)]
    );
    patcher.unapply(&mut registry);
    let block = registry.block(id).expect("block");
    assert_eq!(block.reconstructor_upgrades, original);
    assert_eq!(block.consumes, original_consumes);
}

/// PatcherTests.reconstructorPlansEditSpecific: index form `upgrades.1`.
#[test]
fn reconstructor_index_edit() {
    let mut registry = test_registry();
    let id = registry
        .block_id("additive-reconstructor")
        .expect("additive-reconstructor");
    let dagger = registry.unit_id("dagger").expect("dagger");
    let flare = registry.unit_id("flare").expect("flare");
    let original = registry
        .block(id)
        .expect("block")
        .reconstructor_upgrades
        .clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.additive-reconstructor.upgrades.1": ["dagger", "flare"]}"#,
            )],
        )
        .expect("index edit");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    assert_eq!(
        registry.block(id).expect("block").reconstructor_upgrades[1],
        (dagger, flare)
    );
    patcher.unapply(&mut registry);
    assert_eq!(
        registry.block(id).expect("block").reconstructor_upgrades,
        original
    );
}

/// PatcherTests.reconstructorPlansAdd: `+` append form.
#[test]
fn reconstructor_append() {
    let mut registry = test_registry();
    let id = registry
        .block_id("additive-reconstructor")
        .expect("additive-reconstructor");
    let dagger = registry.unit_id("dagger").expect("dagger");
    let flare = registry.unit_id("flare").expect("flare");
    let original_len = registry
        .block(id)
        .expect("block")
        .reconstructor_upgrades
        .len();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.additive-reconstructor.upgrades.+": [["dagger", "flare"]]}"#,
            )],
        )
        .expect("append upgrade");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    let ups = registry
        .block(id)
        .expect("block")
        .reconstructor_upgrades
        .clone();
    assert_eq!(ups.len(), original_len + 1);
    assert_eq!(*ups.last().expect("upgrade"), (dagger, flare));
    patcher.unapply(&mut registry);
    assert_eq!(
        registry
            .block(id)
            .expect("block")
            .reconstructor_upgrades
            .len(),
        original_len
    );
}

/// PatcherTests.nestedArrays / nestedArrays2: object-index and dotted
/// nested forms both set one `UnitType[]` pair element.
#[test]
fn nested_array_edit_forms() {
    for json in [
        r#"{"block.ship-refabricator.upgrades.0": {"0": "dagger", "1": "mace"}}"#,
        r#"{"block": {"ship-refabricator": {"upgrades.0.0": "dagger", "upgrades.0.1": "mace"}}}"#,
    ] {
        let mut registry = test_registry();
        let id = registry
            .block_id("ship-refabricator")
            .expect("ship-refabricator");
        let dagger = registry.unit_id("dagger").expect("dagger");
        let mace = registry.unit_id("mace").expect("mace");
        let original = registry
            .block(id)
            .expect("block")
            .reconstructor_upgrades
            .clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch(json)])
            .expect("nested edit");
        assert!(
            patcher.warnings().is_empty(),
            "{json}: {:?}",
            patcher.warnings()
        );
        assert_eq!(
            registry.block(id).expect("block").reconstructor_upgrades[0],
            (dagger, mace)
        );
        patcher.unapply(&mut registry);
        assert_eq!(
            registry.block(id).expect("block").reconstructor_upgrades,
            original
        );
    }
}

/// PatcherTests.unitFlags: `targetFlags.+` single-string append + reset.
#[test]
fn unit_target_flags_single() {
    use crate::content::registries::blocks::BlockFlag;
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").target_flags.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"unit.dagger.targetFlags.+": "shield"}"#)],
        )
        .expect("append flag");
    let flags = registry.unit(id).expect("unit").target_flags.clone();
    assert_eq!(flags.len(), original.len() + 1);
    assert_eq!(*flags.last().expect("flag"), Some(BlockFlag::Shield));
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").target_flags, original);
}

/// PatcherTests.assignStringToObject: a string assigned to `weapons` warns
/// and leaves the array unchanged.
#[test]
fn string_to_object_warns() {
    let mut registry = test_registry();
    let id = registry.unit_id("dagger").expect("dagger");
    let original = registry.unit(id).expect("unit").weapons.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"unit.dagger.weapons": ["frog"]}"#)],
        )
        .expect("string to object");
    assert_eq!(patcher.warnings().len(), 1);
    assert_eq!(registry.unit(id).expect("unit").weapons, original);
}

/// PatcherTests.noIdAssign: `id` is `@NoPatch`.
#[test]
fn id_not_patchable() {
    let mut registry = test_registry();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &[patch(r#"{"block.router.id": 9231}"#)])
        .expect("id patch");
    assert_eq!(patcher.warnings().len(), 1);
}

/// PatcherTests.noResolution: an unresolvable class in the patcher parser
/// warns (the arbitrary-FQCN fallback is disabled).
#[test]
fn no_class_resolution_in_patcher() {
    let mut registry = test_registry();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"block.conveyor.lastConfig": {"class": "mindustry.ai.Pathfinder"}}"#,
            )],
        )
        .expect("no resolution");
    assert_eq!(patcher.warnings().len(), 1);
}

/// PatcherTests.singleValue: nested `{"block": {"<name>.<field>": v}}`
/// scalar form.
#[test]
fn nested_type_single_value() {
    let mut registry = test_registry();
    let id = registry.block_id("router").expect("router");
    let original = registry.block(id).expect("block").health;
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(r#"{"block": {"router.health": 9}}"#)],
        )
        .expect("single value");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    assert_eq!(registry.block(id).expect("block").health, 9);
    patcher.unapply(&mut registry);
    assert_eq!(registry.block(id).expect("block").health, original);
}

/// PatcherTests.addWeapon: append a fully-formed weapon to `flare`.
#[test]
fn weapon_append_object() {
    let mut registry = test_registry();
    let id = registry.unit_id("flare").expect("flare");
    let original = registry.unit(id).expect("unit").weapons.clone();
    let mut patcher = DataPatcher::new();
    patcher
        .apply(
            &mut registry,
            &[patch(
                r#"{"unit.flare.weapons.+": {
                    "x": 0, "y": 0, "reload": 10,
                    "bullet": {"type": "LaserBulletType", "damage": 100}
                }}"#,
            )],
        )
        .expect("append weapon");
    assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
    let weapons = registry.unit(id).expect("unit").weapons.clone();
    assert_eq!(weapons.len(), original.len() + 1);
    let added = weapons.last().expect("weapon");
    assert_eq!(
        registry.bullet(added.bullet.id).expect("bullet").damage,
        100.0
    );
    patcher.unapply(&mut registry);
    assert_eq!(registry.unit(id).expect("unit").weapons, original);
}
