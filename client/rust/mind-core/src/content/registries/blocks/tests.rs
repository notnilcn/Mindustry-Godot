// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the block metadata registry.
//! Upstream: none (port-only; registry tests)

use super::*;
use crate::content::ContentError;
use crate::content::category::Category;
use crate::content::color::Rgba;
use crate::content::id::BlockId;
use crate::content::stacks::ItemStack;
use crate::content::test_support::test_registry;

/// Plan 02 §7a: `blocks::construct_block_present` (`build2` exists).
///
/// Ported from `tests/src/test/java/ApplicationTests.java` bootstrap
/// assertion that `Blocks.build2` is registered.
#[test]
fn construct_block_present() {
    let registry = test_registry();
    let build2 = registry
        .block_by_name("build2")
        .expect("build2 must be registered");
    assert_eq!(build2.kind, BlockKind::ConstructBlock);
    assert_eq!(build2.size, 2);
    assert_eq!(build2.health, 10);
    assert!(!build2.in_editor);
    assert_eq!(build2.id, BlockId::new(6));
}

/// `stone-wall` keeps its upstream identity (`air = 0`, `stone-wall = 79`).
#[test]
fn stone_wall_keeps_upstream_id() {
    let registry = test_registry();
    let stone_wall = registry
        .block(BlockId::STONE_WALL)
        .expect("stone-wall id must resolve");
    assert_eq!(stone_wall.name, "stone-wall");
    assert_eq!(stone_wall.kind, BlockKind::StaticWall);
    assert!(stone_wall.solid);

    let blocks = Blocks::new();
    assert_eq!(blocks.id("stone-wall").unwrap(), BlockId::STONE_WALL);
    assert_eq!(blocks.name(BlockId::STONE_WALL).unwrap(), "stone-wall");
    assert_eq!(blocks.len(), 447, "M4 block count + M7 JVM-golden fix");
}

/// Plan 02 §7a: `blocks::health_and_buildtime_derivation` — `Block.init()`
/// health/offset/build-time derivations (`world/Block.java:1357+`).
#[test]
fn health_and_buildtime_derivation() {
    let registry = test_registry();

    let press = registry.block_by_name("graphite-press").unwrap();
    // size 2, no healthScaling items -> round(2*2*40, 5) = 160.
    assert_eq!(press.size, 2);
    assert_eq!(press.health, 160);
    assert_eq!(press.offset, 4.0);
    assert_eq!(press.size_offset, 0);
    // buildTime = 75 * copper.cost + 30 * lead.cost (no multiplier).
    let copper = registry.item(registry.item_id("copper").unwrap()).unwrap();
    let lead = registry.item(registry.item_id("lead").unwrap()).unwrap();
    assert_eq!(press.build_time, 75.0 * copper.cost + 30.0 * lead.cost);
    assert_eq!(press.build_time, 58.5);

    // size 3 + carbide(1.1)/thorium(0.2)/tungsten(0.8) health scaling
    // -> 40 * 3.1 = 124 per tile -> round(9 * 124, 5) = 1115.
    let synth = registry.block_by_name("phase-synthesizer").unwrap();
    assert_eq!(synth.size, 3);
    assert!((synth.scaled_health - 124.0).abs() < 0.001);
    assert_eq!(synth.health, 1115);

    // Walls have explicit health and a 6x build time multiplier.
    let wall = registry.block_by_name("copper-wall").unwrap();
    assert_eq!(wall.health, 320);
    assert_eq!(wall.build_cost_multiplier, 6.0);
    assert_eq!(wall.build_time, 6.0 * copper.cost * 6.0);
    assert_eq!(wall.offset, 0.0);
    assert_eq!(wall.size_offset, 0);
}

/// PAR-0467: `Block.lightRadius`/`lightColor` (`Block.java:344,350`) plus
/// the per-class (`ConsumeGenerator.java:69`, `ImpactReactor.java:30`,
/// `LandingPad.java:71`, `Accelerator.java:79-80`, `MendProjector.java:45`,
/// `OverdriveProjector.java:42`, `CoreBlock.java:158`,
/// `LightBlock.java:39`) and `Blocks.java` floor overrides.
#[test]
fn light_radius_and_color_follow_the_java_table() {
    let registry = test_registry();
    let get = |name: &str| {
        registry
            .block_by_name(name)
            .unwrap_or_else(|| panic!("block {name}"))
    };
    let near = |a: f32, b: f32| (a - b).abs() < 1e-6;

    // `Block.java:344,350`: white, 60 — the `NuclearReactor` environment
    // light (its `#7f19ea` radius-90 light is `NuclearReactor.drawLight`,
    // `NuclearReactor.java:175-179`, not `drawEnvironmentLight`).
    let reactor = get("thorium-reactor");
    assert_eq!(reactor.light_radius, 60.0);
    assert_eq!(reactor.light_color, Rgba::WHITE);
    assert!(reactor.emit_light);

    // `ImpactReactor.java:30`: 115 white.
    let impact = get("impact-reactor");
    assert_eq!(impact.light_radius, 115.0);
    assert_eq!(impact.light_color, Rgba::WHITE);

    // `ConsumeGenerator.java:25,69`: `65 * size` (steam size 2 -> 130,
    // differential size 3 -> 195). `ThermalGenerator extends
    // PowerGenerator` (not `ConsumeGenerator`), so it keeps 60.
    assert_eq!(get("steam-generator").light_radius, 130.0);
    assert_eq!(get("thermal-generator").light_radius, 60.0);
    assert_eq!(get("differential-generator").light_radius, 195.0);

    // `LandingPad.java:71`: 90 white; `#eab678` is `LaunchPad.lightColor`
    // (`LaunchPad.java:39`), a custom draw light.
    let landing = get("landing-pad");
    assert_eq!(landing.light_radius, 90.0);
    assert_eq!(landing.light_color, Rgba::WHITE);

    // `Accelerator.java:79-80`: 70, `Pal.accent` (`#ffd37f`).
    let accelerator = get("interplanetary-accelerator");
    assert_eq!(accelerator.light_radius, 70.0);
    assert!(near(accelerator.light_color.r, 1.0));
    assert!(near(accelerator.light_color.g, 211.0 / 255.0));
    assert!(near(accelerator.light_color.b, 127.0 / 255.0));
    assert!(near(accelerator.light_color.a, 1.0));

    // `MendProjector.java:45` / `OverdriveProjector.java:42`: 50 white
    // (RegenProjector/ThermalGenerator keep the 60 class default).
    assert_eq!(get("mend-projector").light_radius, 50.0);
    assert_eq!(get("overdrive-projector").light_radius, 50.0);
    assert_eq!(get("regen-projector").light_radius, 60.0);

    // `CoreBlock.java:158`: `30 + 20 * size` (3 -> 90, 4 -> 110, 5 -> 130)
    // and the derived `fogRadius` (`CoreBlock.java:159`) now reads the
    // unified field: (int)(90 / 8 * 3) + 13 = 46.
    assert_eq!(get("core-shard").light_radius, 90.0);
    assert_eq!(get("core-foundation").light_radius, 110.0);
    assert_eq!(get("core-nucleus").light_radius, 130.0);
    assert_eq!(get("core-shard").fog_radius, 46);

    // `LightBlock.java:39,40`: `radius * 2.5` (`illuminator` 140 -> 350).
    let illuminator = get("illuminator");
    assert_eq!(illuminator.light_radius, 350.0);
    assert!(illuminator.emit_light);

    // Floors (`Blocks.java:311-315,329-333,369-381,876-879,920-922`).
    let cryo = get("pooled-cryofluid");
    assert_eq!(cryo.light_radius, 25.0);
    assert!(near(cryo.light_color.r, 0.0) && near(cryo.light_color.g, 1.0));
    assert!(near(cryo.light_color.b, 1.0) && near(cryo.light_color.a, 0.19));
    assert!(cryo.emit_light && !cryo.obstructs_light);

    let slag = get("molten-slag");
    assert_eq!(slag.light_radius, 40.0);
    assert!(near(slag.light_color.r, 1.0) && near(slag.light_color.g, 165.0 / 255.0));
    assert!(near(slag.light_color.b, 0.0) && near(slag.light_color.a, 0.38));

    let hotrock = get("hotrock");
    assert_eq!(hotrock.light_radius, 30.0);
    assert!(near(hotrock.light_color.a, 0.15));
    let magmarock = get("magmarock");
    assert_eq!(magmarock.light_radius, 50.0);
    assert!(near(magmarock.light_color.a, 0.3));
    // `Team.crux.color` (`Team.java:35`) at alpha 0.3.
    let metal = get("metal-tiles-6");
    assert_eq!(metal.light_radius, 30.0);
    assert!(near(metal.light_color.r, 242.0 / 255.0) && near(metal.light_color.a, 0.3));
    // A non-emitting floor/floor-light defaults: white 60, no light.
    let stone = get("stone");
    assert_eq!(stone.light_radius, 60.0);
    assert!(!stone.emit_light && !stone.obstructs_light);

    // `Floor.java:117`: floors clear `obstructsLight`; walls keep it.
    assert!(!registry.block(BlockId::AIR).unwrap().obstructs_light);
    let wall = get("copper-wall");
    assert!(wall.obstructs_light);
}

/// Plan 02 §7a: `blocks::research_requirements_formula`
/// (`Block.researchRequirements()`, `Block.java:1284-1295`).
#[test]
fn research_requirements_formula() {
    let registry = test_registry();
    let copper = registry.item_id("copper").unwrap();
    let lead = registry.item_id("lead").unwrap();

    let press = registry.block_by_name("graphite-press").unwrap();
    // Arc `Mathf.round` truncates: round(60 + 75^1.11*20, 10) = 2470,
    // then `UI.roundAmount` truncates to 2400 (JVM golden confirms).
    // round(60 + 30^1.11*20, 10) = 930 -> `roundAmount` 900.
    assert_eq!(
        press.research_requirements(),
        vec![ItemStack::new(copper, 2400), ItemStack::new(lead, 900)]
    );

    // Explicit `researchCost` overrides the formula (radar).
    let radar = registry.block_by_name("radar").unwrap();
    let silicon = registry.item_id("silicon").unwrap();
    let graphite = registry.item_id("graphite").unwrap();
    assert_eq!(
        radar.research_requirements(),
        vec![ItemStack::new(silicon, 70), ItemStack::new(graphite, 70)]
    );

    // `researchCostMultiplier <= 0` yields an empty list.
    let mut zeroed = press.clone();
    zeroed.research_cost = None;
    zeroed.research_cost_multiplier = 0.0;
    assert!(zeroed.research_requirements().is_empty());
}

/// Plan 02 §7a (M4 metadata half, exercised for M3 coverage):
/// every ported block has valid metadata.
#[test]
fn all_metadata_valid() {
    let registry = test_registry();
    assert_eq!(registry.blocks().len(), 447);
    for block in registry.blocks() {
        assert!(block.health > 0, "{} has no health", block.name);
        assert!(
            (1..=16).contains(&block.size),
            "{} has invalid size {}",
            block.name,
            block.size
        );
        assert_eq!(
            block.offset,
            ((block.size + 1) % 2) as f32 * TILE_SIZE / 2.0,
            "{} offset",
            block.name
        );
        assert_eq!(block.size_offset, -((block.size - 1) / 2), "{}", block.name);
        assert!(block.build_time > 0.0, "{} build time", block.name);
        assert!(
            block.consumer_partitions_consistent(),
            "{} consumer partitions",
            block.name
        );
        assert!(!block.region.is_empty(), "{} region", block.name);
        assert!(!block.name.is_empty());
    }
}

/// Plan 02 §7a: `blocks::ergonomic_flags_match_golden` — behavior-facing
/// flags/groups/priorities for representative B1–B6 blocks match the
/// upstream constructors (source-derived golden; JVM golden pending NUD-10).
#[test]
fn ergonomic_flags_match_golden() {
    let registry = test_registry();
    let get = |name: &str| registry.block_by_name(name).expect("block present");

    let conveyor = get("conveyor");
    assert_eq!(conveyor.kind, BlockKind::Conveyor);
    assert_eq!(conveyor.group, BlockGroup::Transportation);
    assert!(conveyor.update && conveyor.has_items && !conveyor.solid);
    assert!(conveyor.accepts_items);
    // `Block.init` overrides the transport priority to `under`
    // (Block.java:1386-1387; `Conveyor.java:45` sets transport first).
    assert!(conveyor.under_bullets);
    assert_eq!(conveyor.priority, TARGET_PRIORITY_UNDER);

    let router = get("router");
    assert_eq!(router.group, BlockGroup::Transportation);
    assert!(router.update && router.has_items && !router.solid);
    assert_eq!(router.item_capacity, 1);

    let sorter = get("sorter");
    assert!(!sorter.update && sorter.destructible && sorter.configurable && sorter.save_config);
    assert_eq!(sorter.group, BlockGroup::Transportation);

    let drill = get("mechanical-drill");
    assert!(drill.flags.contains(&BlockFlag::Drill));
    assert!(drill.update && drill.solid && drill.has_items && drill.has_liquids);
    assert_eq!(drill.group, BlockGroup::Drills);
    assert_eq!(drill.category, Category::Production);

    let generator = get("combustion-generator");
    assert!(generator.outputs_power && !generator.consumes_power && generator.has_power);
    assert!(generator.update && generator.solid);
    assert_eq!(generator.group, BlockGroup::Power);

    let battery = get("battery");
    assert!(battery.flags.contains(&BlockFlag::Battery));
    assert!(battery.outputs_power && battery.consumes_power && battery.has_power);
    assert!(battery.destructible && !battery.update);

    let core = get("core-shard");
    assert!(core.flags.contains(&BlockFlag::Core));
    assert_eq!(core.priority, TARGET_PRIORITY_CORE);
    assert_eq!(core.unit_cap_modifier, 8);
    assert!(core.solid && core.update && core.has_items);
    // `coreShard.alwaysUnlocked = true` (Blocks.java:3147).
    assert!(core.unlock.always_unlocked && core.unlock.unlocked());

    let duo = get("duo");
    assert!(duo.flags.contains(&BlockFlag::Turret));
    assert_eq!(duo.group, BlockGroup::Turrets);
    assert_eq!(duo.priority, TARGET_PRIORITY_TURRET);
    assert!(duo.has_items && duo.solid && duo.update);

    let factory = get("ground-factory");
    assert_eq!(factory.group, BlockGroup::Units);
    assert!(factory.update && factory.solid && factory.has_items && factory.has_power);
    assert!(factory.configurable);

    let payload = get("payload-conveyor");
    assert_eq!(payload.group, BlockGroup::Payloads);
    assert_eq!(payload.size, 3);
    assert!(payload.update);
    assert!(payload.under_bullets);
    assert_eq!(payload.priority, TARGET_PRIORITY_UNDER);

    let power_source = get("power-source");
    assert!(power_source.outputs_power && !power_source.consumes_power);
    assert_eq!(power_source.build_visibility, BuildVisibility::SandboxOnly);
    assert!(power_source.destructible && power_source.configurable);
    // Sandbox/logic blocks are `alwaysUnlocked` upstream (Blocks.java:6750+).
    assert!(power_source.unlock.always_unlocked);
    assert!(get("target-dummy").unlock.always_unlocked);
    assert!(get("heat-source").unlock.always_unlocked);

    // J08 T1: `Block.rotate`/`rotateDraw`/`rotateDrawEditor`/
    // `visualRotationOffset` (`Block.java:124-130`) — the port derives
    // `rotate` from the kind and takes the three draw fields from the spec.
    let conveyor = get("conveyor");
    assert!(conveyor.rotate && conveyor.rotate_draw && conveyor.rotate_draw_editor);
    assert_eq!(conveyor.visual_rotation_offset, 0.0);
    assert!(!get("copper-wall").rotate, "walls are not rotatable");
    let centrifuge = get("coal-centrifuge");
    assert!(
        !centrifuge.rotate && !centrifuge.rotate_draw,
        "Blocks.java:1318-1320 sets rotateDraw = false (a GenericCrafter is \
         not rotatable, so the flag only matters for the editor draw)"
    );

    let processor = get("world-processor");
    assert_eq!(processor.group, BlockGroup::Logic);
    assert_eq!(
        processor.build_visibility,
        BuildVisibility::WorldProcessorOnly
    );
    assert!(processor.update && processor.solid && processor.configurable);

    let launch = get("launch-pad");
    assert!(launch.flags.contains(&BlockFlag::LaunchPad));
    assert!(launch.has_items && launch.solid && launch.update && launch.configurable);

    let message = get("message");
    assert_eq!(message.group, BlockGroup::Logic);
    assert!(message.solid && message.destructible && message.configurable);

    // PAR-0446: `update = solid = true` (`UnitCargoUnloadPoint.java:26`,
    // `UnitAssembler.java:62`).
    assert!(get("unit-cargo-unload-point").solid);
    for name in ["tank-assembler", "ship-assembler", "mech-assembler"] {
        assert!(get(name).solid, "{name} is solid (UnitAssembler.java:62)");
    }
}

/// Generated item references resolve and `get_dependencies` covers inputs.
#[test]
fn dependencies_and_items_resolve() {
    let registry = test_registry();
    let press = registry.block_by_name("graphite-press").unwrap();
    let deps = press.get_dependencies();
    assert!(deps.contains(&registry.item_id("copper").unwrap()));
    assert!(deps.contains(&registry.item_id("lead").unwrap()));
    assert!(deps.contains(&registry.item_id("coal").unwrap()));

    // Every requirement/drop/consume reference resolves to a real item.
    for block in registry.blocks() {
        for stack in &block.requirements {
            assert!(
                registry.item(stack.item).is_some(),
                "{} requirement",
                block.name
            );
        }
        if let Some(drop) = block.item_drop {
            assert!(registry.item(drop).is_some(), "{} drop", block.name);
        }
    }

    // Ore blocks carry `itemDrop` and minable metadata (`OreBlock` ctor).
    let ore = registry.block_by_name("ore-copper").unwrap();
    assert_eq!(ore.kind, BlockKind::OreBlock);
    assert_eq!(ore.item_drop, registry.item_id("copper"));
    assert_eq!(ore.region, "ore-copper");
}

/// GAPS #56 follow-up: every production/environment family knob that a mod
/// JSON block def can set (the `ContentParser` field surface) survives
/// `BlockSpec` → [`BlockDef::from_spec`] with the same value; the record is
/// the source of truth `BlockKindData::from_def` copies.
#[test]
fn block_family_fields_round_trip_from_spec() {
    let registry = test_registry();
    let bundle = crate::content::MemoryBundle::new();
    let store = crate::content::MemoryUnlockStore::new();
    let mut spec = BlockSpec {
        name: "mod-family-knobs",
        kind: BlockKind::GenericCrafter,
        ..BlockSpec::default()
    };
    // `GenericCrafter.java:22-39`.
    spec.output_items = vec![stack("copper", 2), stack("silicon", 3)];
    spec.output_liquids = vec![liquid_stack("water", 0.2), liquid_stack("oil", 0.1)];
    spec.liquid_output_directions = vec![1, 3];
    spec.ignore_liquid_fullness = Some(true);
    spec.separator_results = vec![stack("lead", 4)];
    // `Drill.java:26-42` / `BurstDrill.java:26` / `BeamDrill.java:38-41`.
    spec.drill_time = Some(123.0);
    spec.drill_tier = Some(5);
    spec.hardness_drill_multiplier = Some(0.0);
    spec.blocked_items = vec!["thorium"];
    spec.liquid_boost_intensity = Some(1.75);
    spec.warmup_speed = Some(0.01);
    spec.inverted_time = Some(140.0);
    // `BurstDrill.java:15,26` (`Blocks.java:3095-3096`).
    spec.burst_shake = Some(4.0);
    spec.burst_drill_effect = Some(crate::fx::inline::multi_of(&[
        crate::content::EffectId::MINE_IMPACT,
        crate::content::EffectId::DRILL_STEAM,
    ]));
    spec.beam_range = Some(7);
    spec.drill_multipliers = vec![("beryllium", 2.0)];
    // `Pump.java:20` / `SolidPump.java:22-26` / `Fracker.java:8` /
    // `WallCrafter.java:28-49` / `AttributeCrafter.java:15-31`.
    spec.pump_amount = Some(0.25);
    spec.item_use_time = Some(60.0);
    spec.pump_liquid = Some("oil");
    spec.base_efficiency = Some(0.5);
    spec.attribute = Some("sand");
    spec.output_item = Some("sand");
    spec.item_boost_intensity = Some(1.7);
    spec.boost_item_use_time = Some(80.0);
    spec.boost_scale = Some(0.15);
    spec.min_efficiency = Some(9.0 - 0.0001);
    spec.max_boost = Some(2.0);
    spec.output_scale = Some(0.15);
    spec.scale_liquid_consumption = Some(true);
    spec.display_efficiency = Some(false);
    spec.display_scaled_output = Some(false);
    // `Block.java:136` / `TiledFloor.java:17-22` / `WobbleProp.java:9`.
    spec.variants = Some(4);
    spec.max_size = Some(2);
    spec.wobble_scale = Some(30.0);
    spec.wobble_magnitude = Some(0.5);
    spec.wobble_time_scale = Some(2.0);
    spec.wobble_magnitude2 = Some(0.75);

    let def = BlockDef::from_spec(spec, &registry, &bundle, &store).expect("block def");
    let item = |name: &str| registry.item_id(name).expect("item");
    let liquid = |name: &str| registry.liquid_id(name).expect("liquid");

    assert_eq!(
        def.output_items,
        vec![
            ItemStack::new(item("copper"), 2),
            ItemStack::new(item("silicon"), 3)
        ]
    );
    assert_eq!(
        def.output_liquids,
        vec![(liquid("water"), 0.2), (liquid("oil"), 0.1)]
    );
    assert_eq!(def.liquid_output_directions, vec![1, 3]);
    assert!(def.ignore_liquid_fullness);
    assert_eq!(def.separator_results, vec![ItemStack::new(item("lead"), 4)]);
    assert_eq!(def.drill_time, Some(123.0));
    assert_eq!(def.drill_tier, Some(5));
    assert_eq!(def.hardness_drill_multiplier, Some(0.0));
    assert_eq!(def.blocked_items, vec![item("thorium")]);
    assert_eq!(def.liquid_boost_intensity, Some(1.75));
    assert_eq!(def.warmup_speed, Some(0.01));
    assert_eq!(def.inverted_time, Some(140.0));
    assert_eq!(def.burst_shake, Some(4.0));
    assert_eq!(
        def.burst_drill_effect,
        Some(crate::fx::inline::multi_of(&[
            crate::content::EffectId::MINE_IMPACT,
            crate::content::EffectId::DRILL_STEAM,
        ]))
    );
    assert!(!def.ignore_resize_config, "GenericCrafter does not set it");
    assert_eq!(def.beam_range, Some(7));
    assert_eq!(def.drill_multipliers, vec![(item("beryllium"), 2.0)]);
    assert_eq!(def.pump_amount, Some(0.25));
    assert_eq!(def.item_use_time, Some(60.0));
    assert_eq!(def.pump_liquid, Some(liquid("oil")));
    assert_eq!(def.base_efficiency, Some(0.5));
    assert_eq!(def.attribute.as_deref(), Some("sand"));
    assert_eq!(def.output_item, Some(item("sand")));
    assert_eq!(def.item_boost_intensity, Some(1.7));
    assert_eq!(def.boost_item_use_time, Some(80.0));
    assert_eq!(def.boost_scale, Some(0.15));
    assert!((def.min_efficiency.unwrap() - (9.0 - 0.0001)).abs() < 1e-6);
    assert_eq!(def.max_boost, Some(2.0));
    assert_eq!(def.output_scale, Some(0.15));
    assert_eq!(def.scale_liquid_consumption, Some(true));
    assert_eq!(def.display_efficiency, Some(false));
    assert_eq!(def.display_scaled_output, Some(false));
    assert_eq!(def.variants, 4);
    assert_eq!(def.max_size, 2);
    assert_eq!(def.wobble_scale, Some(30.0));
    assert_eq!(def.wobble_magnitude, Some(0.5));
    assert_eq!(def.wobble_time_scale, Some(2.0));
    assert_eq!(def.wobble_magnitude2, Some(0.75));

    // An empty spec leaves every knob unset so `BlockKindData::from_def`
    // applies the Java class default.
    let plain = BlockDef::from_spec(
        BlockSpec {
            name: "mod-plain-drill",
            kind: BlockKind::Drill,
            ..BlockSpec::default()
        },
        &registry,
        &bundle,
        &store,
    )
    .expect("plain def");
    assert!(plain.output_items.is_empty());
    assert!(plain.output_liquids.is_empty());
    assert!(plain.liquid_output_directions.is_empty());
    assert_eq!(plain.drill_time, None);
    assert_eq!(plain.drill_tier, None);
    assert_eq!(plain.hardness_drill_multiplier, None);
    assert_eq!(plain.burst_shake, None);
    assert_eq!(plain.burst_drill_effect, None);
    assert!(plain.blocked_items.is_empty());
    assert_eq!(plain.pump_amount, None);
    assert_eq!(plain.item_use_time, None);
    assert_eq!(plain.pump_liquid, None);
    assert_eq!(plain.attribute, None);
    assert_eq!(plain.max_size, 0);
    assert_eq!(plain.wobble_scale, None);
}

/// An unresolvable content name in a family list is an `UnknownName` error,
/// not a silently dropped value (the parser reports the file).
#[test]
fn block_family_fields_reject_unknown_names() {
    let registry = test_registry();
    let bundle = crate::content::MemoryBundle::new();
    let store = crate::content::MemoryUnlockStore::new();
    let base = BlockSpec {
        name: "mod-bad-family",
        kind: BlockKind::Drill,
        ..BlockSpec::default()
    };

    let mut bad_blocked = base.clone();
    bad_blocked.blocked_items = vec!["not-an-item"];
    assert_eq!(
        BlockDef::from_spec(bad_blocked, &registry, &bundle, &store),
        Err(ContentError::UnknownName(String::from("not-an-item")))
    );

    let mut bad_liquid = base.clone();
    bad_liquid.pump_liquid = Some("not-a-liquid");
    assert_eq!(
        BlockDef::from_spec(bad_liquid, &registry, &bundle, &store),
        Err(ContentError::UnknownName(String::from("not-a-liquid")))
    );

    let mut bad_output = base;
    bad_output.output_items = vec![stack("not-an-item", 1)];
    assert_eq!(
        BlockDef::from_spec(bad_output, &registry, &bundle, &store),
        Err(ContentError::UnknownName(String::from("not-an-item")))
    );
}

/// GAPS #56 follow-up: the mod-only environment classes resolve through the
/// `ClassMap` replacement (`ClassMap.java:287-290`) and classify as floors /
/// walls / props so the environment behaviour and renderer dispatch hold.
#[test]
fn tiled_and_wobble_kinds_classify() {
    use crate::mods::json::classmap::{ClassScope, ClassTagMap};
    use crate::mods::json::resolve_block_kind;

    for (class, kind, floor) in [
        ("TiledFloor", BlockKind::TiledFloor, true),
        ("TiledWall", BlockKind::TiledWall, false),
        ("WobbleProp", BlockKind::WobbleProp, false),
    ] {
        assert_eq!(kind.name(), class);
        assert_eq!(resolve_block_kind(class), Some(kind));
        assert_eq!(
            resolve_block_kind(&format!("mindustry.world.blocks.environment.{class}")),
            Some(kind),
            "{class} FQCN resolves"
        );
        assert_eq!(is_floor_kind(kind), floor, "{class} floor classification");
        assert_eq!(
            ClassTagMap::committed().resolve(class, ClassScope::Block),
            Some(kind.name()),
            "{class} classmap entry"
        );
    }
    // `TiledFloor` is drawn as a floor but not as an overlay (`Floor`
    // `drawMain`, not `OverlayFloor.drawBase`).
    assert!(!is_overlay_floor_kind(BlockKind::TiledFloor));
    // The classmap still builds and audits clean with the new kinds.
    let map = ClassTagMap::committed();
    assert_eq!(map.audit(), Vec::<String>::new());
    assert!(map.resolve("TiledFloor", ClassScope::Block).is_some());
    // The `BlockKind` ABI stays dense (ordinal == index in `ALL`).
    assert_eq!(BlockKind::ALL.len(), 140);
    for (index, kind) in BlockKind::ALL.iter().enumerate() {
        assert_eq!(kind.ordinal(), index, "{} ordinal", kind.name());
    }
    assert_eq!(BlockKind::TiledFloor.ordinal(), 137);
    assert_eq!(BlockKind::TiledWall.ordinal(), 138);
    assert_eq!(BlockKind::WobbleProp.ordinal(), 139);
}

/// `Block.ignoreResizeConfig` (`Block.java:264-265`, `MapEditor.java:342-345`)
/// is set by three class constructors — `LogicBlock` (`LogicBlock.java:58`),
/// `ItemBridge` (`ItemBridge.java:61`) and `PowerNode` (`PowerNode.java:45`)
/// — with the subclasses inheriting it. The editor resize uses this to skip
/// the `BuildPlan.pointConfig` shift.
#[test]
fn ignore_resize_config_matches_the_java_constructors() {
    for kind in [
        BlockKind::LogicBlock,
        BlockKind::ItemBridge,
        BlockKind::BufferedItemBridge,
        BlockKind::LiquidBridge,
        BlockKind::PowerNode,
        BlockKind::LongPowerNode,
    ] {
        assert!(kind_ignores_resize_config(kind), "{}", kind.name());
    }
    for kind in [
        BlockKind::LogicDisplay,
        BlockKind::TileableLogicDisplay,
        BlockKind::MemoryBlock,
        BlockKind::CanvasBlock,
        BlockKind::DuctBridge,
        BlockKind::DirectionLiquidBridge,
        BlockKind::BeamNode,
        BlockKind::Drill,
    ] {
        assert!(!kind_ignores_resize_config(kind), "{}", kind.name());
    }

    // The vanilla blocks resolve the flag through `BlockDef::from_spec`.
    let registry = test_registry();
    let flag = |name: &str| {
        registry
            .block_by_name(name)
            .unwrap_or_else(|| panic!("{name}"))
            .ignore_resize_config
    };
    for name in [
        "micro-processor",
        "world-processor",
        "phase-conveyor",
        "bridge-conveyor",
        "bridge-conduit",
        "phase-conduit",
        "power-node",
        "power-node-large",
        "surge-tower",
        "beam-link",
    ] {
        assert!(flag(name), "{name}");
    }
    for name in [
        "logic-display",
        "large-logic-display",
        "memory-cell",
        "memory-bank",
        "canvas",
        "beam-node",
        "beam-tower",
        "duct-bridge",
        "mechanical-drill",
    ] {
        assert!(!flag(name), "{name}");
    }
}
