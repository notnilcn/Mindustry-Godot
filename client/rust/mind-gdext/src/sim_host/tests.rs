// SPDX-License-Identifier: GPL-3.0-only

//! `MindSimHost` unit tests (moved out of `sim_host.rs`).
//! Upstream: none (port-only; test module)

use super::bar_rows::{
    BarExtras, BarRow, GeneratorBars, PowerBars, TurretBars, UnitCapBars, building_bar_rows,
};
use super::campaign_link::core_items_json;
use super::command_panel::command_panel;
use super::config::block_items_json;
use super::player::{editor_team_choice, player_near_drop_zone_predicate};
use super::render_light::{bullet_circle_light, environment_light, floor_light_sources};
use super::render_units::{factory_preview, reconstructor_preview};
use super::world_query::first_core_position;
use super::*;
use mind_core::content::create_base_content;
use mind_core::world::TilePos;

fn base_registry() -> ContentRegistry {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    match create_base_content(&bundle, &store, true) {
        Ok(mut registry) => {
            if let Err(error) = registry.init() {
                panic!("registry init failed: {error}");
            }
            if let Err(error) = registry.post_init() {
                panic!("registry post_init failed: {error}");
            }
            registry
        }
        Err(error) => panic!("content boot failed: {error}"),
    }
}

/// PAR-0556: `HudFragment.setPlayerTeamEditor` only acts in editor mode and
/// only for a real team id (`HudFragment.java:714-718`).
#[test]
fn editor_team_change_requires_editor_mode_and_a_valid_team() {
    use mind_core::game::team::Team;
    let teams = Team::all().len();
    assert_eq!(editor_team_choice(true, 1, teams), Some(1));
    assert_eq!(editor_team_choice(true, 0, teams), Some(0));
    assert_eq!(
        editor_team_choice(true, teams as i64 - 1, teams),
        Some((teams - 1) as u8)
    );
    // Outside the editor the call is a no-op (`state.isEditor()`).
    assert_eq!(editor_team_choice(false, 1, teams), None);
    // Out-of-range ids are rejected instead of panicking.
    assert_eq!(editor_team_choice(true, -1, teams), None);
    assert_eq!(editor_team_choice(true, teams as i64, teams), None);
}

/// PAR-0467: the host's light source reads `BlockDef.light_radius` /
/// `light_color` instead of the old hardcoded 60/white
/// (`Block.drawEnvironmentLight`, `Block.java:493-495`).
#[test]
fn block_light_sources_use_the_block_light_fields() {
    let content = base_registry();
    let get = |name: &str| {
        content
            .block_by_name(name)
            .unwrap_or_else(|| panic!("block {name}"))
    };

    // `ImpactReactor.java:30`: a placed impact reactor lights at 115.
    let impact = environment_light(get("impact-reactor"), 100.0, 200.0);
    assert_eq!((impact.x, impact.y), (100.0, 200.0));
    assert_eq!(impact.radius, 115.0);
    assert_eq!(impact.color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(impact.opacity, 1.0, "white alpha");

    // `LandingPad.java:71`: 90 white (`#eab678` is LaunchPad's custom draw
    // colour, not the environment light).
    let landing = environment_light(get("landing-pad"), 0.0, 0.0);
    assert_eq!(landing.radius, 90.0);
    assert_eq!(landing.color, [1.0, 1.0, 1.0, 1.0]);

    // `Accelerator.java:79-80`: 70 `Pal.accent`.
    let accelerator = environment_light(get("interplanetary-accelerator"), 0.0, 0.0);
    assert_eq!(accelerator.radius, 70.0);
    assert!((accelerator.color[1] - 211.0 / 255.0).abs() < 1e-6);
    assert!((accelerator.color[2] - 127.0 / 255.0).abs() < 1e-6);

    // `NuclearReactor` keeps the base 60 white environment light.
    let reactor = environment_light(get("thorium-reactor"), 0.0, 0.0);
    assert_eq!(reactor.radius, 60.0);
    assert_eq!(reactor.color, [1.0, 1.0, 1.0, 1.0]);

    // Floor alpha becomes the stamped opacity (`drawEnvironmentLight`
    // passes `lightColor.a`).
    let cryo = environment_light(get("pooled-cryofluid"), 0.0, 0.0);
    assert_eq!(cryo.radius, 25.0);
    assert!((cryo.opacity - 0.19).abs() < 1e-6);
}

/// GAPS #49 follow-up: the four beam kinds override `BulletType.drawLight`
/// with an empty body and draw a line light in `draw` (already stamped by
/// `render::light::bullet_line_light`), so the host must not also emit the
/// default circle. Every other kind keeps the default circle, now gated
/// and coloured by the def's own light fields
/// (`BulletType.java:708-711,862-863`).
#[test]
fn beam_bullet_kinds_skip_the_default_circle_light() {
    use mind_core::content::Rgba;
    use mind_core::content::registries::bullets::{BulletDef, BulletKind};

    // A plain bullet with the class-default light fields (`BulletType`
    // `init` derives `max(18, hitSize * 5)`; the radius is set here so the
    // helper's gate has a value to read).
    let plain = |radius: f32, opacity: f32| {
        let mut def = BulletDef::new(BulletKind::Plain);
        def.light_radius = radius;
        def.light_opacity = opacity;
        def.light_color = Rgba::new(1.0, 0.5, 0.25, 1.0);
        def
    };
    assert_eq!(
        bullet_circle_light(&plain(30.0, 0.3)),
        Some((30.0, [1.0, 0.5, 0.25, 1.0], 0.3))
    );
    // `if(lightOpacity <= 0f || lightRadius <= 0f) return;`
    assert_eq!(bullet_circle_light(&plain(30.0, 0.0)), None);
    assert_eq!(bullet_circle_light(&plain(30.0, -1.0)), None);
    assert_eq!(bullet_circle_light(&plain(0.0, 0.3)), None);
    assert_eq!(bullet_circle_light(&plain(-1.0, 0.3)), None);

    // The beam kinds never emit the default circle, even with live-looking
    // light fields (Laser carries `colors[0]`; ContinuousLaser is orange in
    // item-4's def; ContinuousFlame is `hitColor`; Sap is `Pal.sap`).
    for kind in [
        BulletKind::Laser,
        BulletKind::ContinuousLaser,
        BulletKind::ContinuousFlame,
        BulletKind::Sap,
    ] {
        let mut def = BulletDef::new(kind);
        def.light_radius = 40.0;
        def.light_opacity = 0.7;
        assert_eq!(
            bullet_circle_light(&def),
            None,
            "{kind:?} must not double-light with a circle"
        );
    }

    // Base content: every registered beam-kind def is skipped, and the
    // non-beam kinds still produce lights.
    let content = base_registry();
    let mut beam_defs = 0usize;
    for def in content.bullets() {
        if matches!(
            def.kind,
            BulletKind::Laser
                | BulletKind::ContinuousLaser
                | BulletKind::ContinuousFlame
                | BulletKind::Sap
        ) {
            beam_defs += 1;
            assert!(
                bullet_circle_light(def).is_none(),
                "registered beam bullet #{} must be skipped",
                def.id.raw()
            );
        }
    }
    assert!(beam_defs > 0, "base content has beam bullets");
    // `shrapnel` also draws a line light in Java (`ShrapnelBulletType:69`)
    // but keeps the default `drawLight`, so it must still cast a circle.
    if let Some(shrapnel) = content
        .bullets()
        .iter()
        .find(|def| def.kind == BulletKind::Shrapnel && def.light_radius > 0.0)
    {
        assert!(bullet_circle_light(shrapnel).is_some());
    }
}

/// PAR-0467: `BlockRenderer.java:846-855` floor/overlay lights from a
/// placed floor tile (Java positions them at `Tile.worldx()/worldy()`).
#[test]
fn floor_light_sources_follow_the_java_table() {
    let content = base_registry();
    let mut grid = WorldGrid::new(6, 6);
    let Some(cryofluid) = content.block_id("pooled-cryofluid") else {
        panic!("cryofluid");
    };
    grid.tiles.get_mut(2, 3).floor = cryofluid;
    let view = CameraView {
        x: 24.0,
        y: 24.0,
        w: 48.0,
        h: 48.0,
        zoom: 1.0,
        team: 0,
    };
    let lights = floor_light_sources(&grid, &content, &view);
    assert_eq!(lights.len(), 1, "only the cryofluid tile emits");
    let light = &lights[0];
    assert_eq!((light.x, light.y), (16.0, 24.0), "tile centre x*8, y*8");
    assert_eq!(light.radius, 25.0);
    assert!((light.opacity - 0.19).abs() < 1e-6);
    assert!(light.color[0] < 1e-6 && (light.color[1] - 1.0).abs() < 1e-6);

    // A solid block on the tile blocks the floor light (`!obstructsLight`).
    let Some(wall) = content.block_id("copper-wall") else {
        panic!("wall");
    };
    grid.tiles.get_mut(2, 3).block = wall;
    assert!(floor_light_sources(&grid, &content, &view).is_empty());
}

fn bar_rows(def_name: &str, extras: &BarExtras, content: &ContentRegistry) -> Vec<BarRow> {
    let Some(def) = content.block_by_name(def_name) else {
        panic!("block {def_name}");
    };
    building_bar_rows(def, 1.0, Some(0.5), Some(0), Some(&[]), &[], extras)
}

#[test]
fn bar_rows_follow_the_java_overrides() {
    // J05 T4: `Block.setBars` + the `addBar`/`removeBar` calls in `world/**`.
    let content = base_registry();
    let extras = BarExtras {
        core_capacity: Some((4000.0, 16.0)),
        drill: Some((12.5, 0.5)),
        heat: Some((5.0, 10.0)),
        time_scale: 1.0,
        factory: Some((450.0, 0)),
        ..BarExtras::default()
    };
    // `CoreBlock.java:148-152`: `bar.capacity` appended after the defaults.
    let core = bar_rows("core-shard", &extras, &content);
    let Some(capacity) = core.iter().find(|row| row.key == "capacity") else {
        panic!("core capacity bar missing");
    };
    assert_eq!(capacity.label_key, "bar.capacity");
    assert_eq!(capacity.color, 0x2ea7_56ff);
    assert_eq!(capacity.value, 0.0, "0 items over 4000 * 16");
    assert_eq!(
        capacity.label_args.first().map(String::as_str),
        Some(
            mind_core::ui::text::format_amount_default(4000)
                .replace("[gray]", "")
                .replace("[]", "")
                .as_str()
        ),
        "`UI.formatAmount(storageCapacity)` with the markup stripped"
    );
    // `ItemSource.java:41` / `Unloader.java:74` / `DirectionalUnloader.java:79`.
    for name in ["item-source", "unloader", "duct-unloader"] {
        assert!(
            !bar_rows(name, &extras, &content)
                .iter()
                .any(|row| row.key == "items"),
            "{name} removes the item bar"
        );
    }
    // `LiquidSource.java:45` / `LiquidVoid.java:23` / `LiquidJunction.java:29`.
    for name in ["liquid-source", "liquid-void", "liquid-junction"] {
        assert!(
            !bar_rows(name, &extras, &content)
                .iter()
                .any(|row| row.key == "liquid"),
            "{name} removes the liquid bar"
        );
    }
    // `Drill.java:117-119`: `bar.drillspeed` = speed * 60 * timeScale.
    let drill = bar_rows("mechanical-drill", &extras, &content);
    let Some(speed) = drill.iter().find(|row| row.key == "drillspeed") else {
        panic!("drill speed bar missing");
    };
    assert_eq!(speed.label_key, "bar.drillspeed");
    assert_eq!(speed.label_args.first().map(String::as_str), Some("750.00"));
    assert_eq!(speed.value, 0.5, "warmup");
    assert_eq!(speed.color, 0xff89_47ff);
    // `HeatProducer.java:40` / `HeatConductor.java:34`: `bar.heat`, heat/output.
    let heat_block = bar_rows("heat-source", &extras, &content);
    assert!(
        heat_block.iter().any(|row| row.key == "heat"),
        "heat-source has a heat bar"
    );
    // `UnitFactory.java:121-125`: `bar.progress` = `fraction()`.
    let factory = bar_rows("ground-factory", &extras, &content);
    let Some(progress) = factory.iter().find(|row| row.key == "progress") else {
        panic!("factory progress bar missing");
    };
    assert_eq!(progress.value, 0.5, "450 / dagger's 900 ticks");
    assert_eq!(progress.label_key, "bar.progress");
    // A plain block keeps exactly the default rows (`Block.java:709-760`):
    // health only (the helper's synthetic power status is not fed here).
    let Some(router_def) = content.block_by_name("router") else {
        panic!("router");
    };
    let router = building_bar_rows(router_def, 1.0, None, None, None, &[], &extras);
    assert_eq!(
        router.iter().map(|row| row.key).collect::<Vec<_>>(),
        ["health"]
    );
    let duo = bar_rows("duo", &extras, &content);
    assert_eq!(
        duo.iter().map(|row| row.key).collect::<Vec<_>>(),
        ["health", "power", "liquid"]
    );
}

/// PAR-0474: `WallCrafter.setBars` (`WallCrafter.java:68-72`) adds a
/// `drillspeed` bar showing `lastEfficiency * 60 / drillTime` (no extra
/// `timeScale` factor — it is already folded into `lastEfficiency`).
#[test]
fn wall_crafter_bar_uses_last_efficiency_over_drill_time() {
    let content = base_registry();
    // 6.4 * 60 / 48 = 8.00 items/s.
    let extras = BarExtras {
        wall_crafter: Some((6.4, 0.5, 48.0)),
        ..BarExtras::default()
    };
    let Some(crusher) = content.block_by_name("large-cliff-crusher") else {
        panic!("large-cliff-crusher");
    };
    let rows = building_bar_rows(crusher, 950.0, Some(1.0), Some(0), Some(&[]), &[], &extras);
    let Some(row) = rows.iter().find(|row| row.key == "drillspeed") else {
        panic!("wall crafter drill speed bar missing");
    };
    assert_eq!(row.label_key, "bar.drillspeed");
    assert_eq!(row.label_args.first().map(String::as_str), Some("8.00"));
    assert_eq!(row.value, 0.5, "warmup");
    assert_eq!(row.color, 0xff89_47ff);
    // No fitted `DrillState` values => no bar.
    assert!(
        !building_bar_rows(
            crusher,
            950.0,
            Some(1.0),
            Some(0),
            Some(&[]),
            &[],
            &BarExtras::default()
        )
        .iter()
        .any(|row| row.key == "drillspeed")
    );
}

/// L05 T1: the remaining `Block.setBars` families (turrets, power,
/// generators/reactors, projectors, pumps, assemblers, pads, accelerator).
#[test]
fn bar_rows_for_the_remaining_families() {
    use mind_core::content::BlockKind;
    let content = base_registry();
    let def = |name: &str| {
        content
            .block_by_name(name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let rows_for = |name: &str, extras: &BarExtras| {
        building_bar_rows(def(name), 1.0, None, Some(0), None, &[], extras)
    };
    let keys = |rows: &[BarRow]| rows.iter().map(|row| row.key).collect::<Vec<_>>();

    // `ItemTurret.java:61-67`, `Turret.java:201-207`,
    // `BaseTurret.java:104-110`.
    let turret = BarExtras {
        turret: Some(TurretBars {
            total_ammo: 15,
            max_ammo: 30,
            heat_req: 2.0,
            heat_requirement: 4.0,
            max_heat_efficiency: 1.0,
            activation_timer: 0.0,
            activation_time: 0.0,
        }),
        ..BarExtras::default()
    };
    let duo = rows_for("duo", &turret);
    let Some(ammo) = duo.iter().find(|row| row.key == "ammo") else {
        panic!("ammo bar");
    };
    assert_eq!(ammo.label_key, "stat.ammo");
    assert_eq!(ammo.value, 0.5);
    let Some(heat) = duo.iter().find(|row| row.key == "heat") else {
        panic!("heat bar");
    };
    assert_eq!(heat.label_key, "bar.heatpercent");
    assert_eq!(heat.label_args, ["2", "50"]);

    // `PowerNode.java:115-122`: power, batteries, connections.
    let power = BarExtras {
        power: Some(PowerBars {
            balance: 10.0,
            produced: 30.0,
            needed: 60.0,
            stored: 500.0,
            capacity: 1000.0,
            links: 2,
            max_nodes: 3,
            back: None,
            front: None,
        }),
        ..BarExtras::default()
    };
    let node = rows_for("power-node", &power);
    assert_eq!(keys(&node), ["health", "power", "batteries", "connections"]);
    assert_eq!(node[1].label_args, ["+600"]);
    assert_eq!(node[1].value, 0.5);
    assert_eq!(
        node[2].label_args,
        [
            mind_core::ui::text::format_amount_default(500)
                .replace("[gray]", "")
                .replace("[]", ""),
            mind_core::ui::text::format_amount_default(1000)
                .replace("[gray]", "")
                .replace("[]", "")
        ]
    );
    assert_eq!(node[3].label_args, ["2", "3"]);

    // `PowerDiode.java:33-48`: `bar.input`/`bar.output`, no balance rows.
    let diode = rows_for(
        "diode",
        &BarExtras {
            power: Some(PowerBars {
                back: Some(0.25),
                front: Some(0.75),
                ..PowerBars::default()
            }),
            ..BarExtras::default()
        },
    );
    assert_eq!(keys(&diode), ["health", "back", "front"]);
    assert_eq!(diode[1].label_key, "bar.input");
    assert_eq!(diode[2].label_key, "bar.output");
    assert_eq!((diode[1].value, diode[2].value), (0.25, 0.75));
    // Java always adds both rows; `bar(Building)` returns 0 for an
    // unconnected diode (`PowerDiode.java:33-48`).
    let unconnected = rows_for("diode", &BarExtras::default());
    assert_eq!(keys(&unconnected), ["health", "back", "front"]);
    assert_eq!((unconnected[1].value, unconnected[2].value), (0.0, 0.0));

    // `PayloadLoader.java:60-66`: the progress bar exists even without a
    // payload (`bar.loadprogress`, 0).
    let loader = rows_for("payload-loader", &BarExtras::default());
    assert_eq!(keys(&loader), ["health", "liquid", "progress"]);
    assert_eq!(loader[2].label_key, "bar.loadprogress");
    assert_eq!(loader[2].value, 0.0);

    // `PowerGenerator.java:88-96`.
    let generator = rows_for(
        "combustion-generator",
        &BarExtras {
            generator: Some(GeneratorBars {
                production: 1.0,
                usage: 0.0,
                efficiency: 0.5,
            }),
            time_scale: 1.0,
            ..BarExtras::default()
        },
    );
    let Some(output) = generator.iter().find(|row| row.key == "power") else {
        panic!("power output bar");
    };
    assert_eq!(output.label_key, "bar.poweroutput");
    assert_eq!(output.label_args, ["60.0"]);
    assert_eq!(output.value, 0.5);

    // `HeaterGenerator.java:38-42`: the power row plus `bar.heat`
    // (`Pal.lightOrange`, `entity.heat / heatOutput`).
    let heater = BarExtras {
        generator: Some(GeneratorBars {
            production: 1.0,
            usage: 0.0,
            efficiency: 0.5,
        }),
        heat: Some((2.5, 10.0)),
        time_scale: 1.0,
        ..BarExtras::default()
    };
    let neoplasia = rows_for("neoplasia-reactor", &heater);
    assert_eq!(def("neoplasia-reactor").kind, BlockKind::HeaterGenerator);
    assert_eq!(keys(&neoplasia), ["health", "liquid", "power", "heat"]);
    let Some(heat_bar) = neoplasia.iter().find(|row| row.key == "heat") else {
        panic!("heater heat bar");
    };
    assert_eq!(heat_bar.label_key, "bar.heat");
    assert!(heat_bar.label_args.is_empty());
    assert_eq!(heat_bar.value, 0.25);
    assert_eq!(heat_bar.color, 0xf680_21ff);
    // Only `HeaterGenerator` carries `HeatState`, so a plain
    // `ConsumeGenerator` gets `extras.heat == None` and stays power-only.
    let steam = rows_for(
        "steam-generator",
        &BarExtras {
            heat: None,
            ..heater
        },
    );
    assert!(!steam.iter().any(|row| row.key == "heat"));
    assert!(steam.iter().any(|row| row.key == "power"));

    // `NuclearReactor.java:103`, `VariableReactor.java:57-63`.
    let reactor = rows_for(
        "thorium-reactor",
        &BarExtras {
            reactor_heat: Some(0.25),
            ..BarExtras::default()
        },
    );
    assert_eq!(
        reactor
            .iter()
            .find(|row| row.key == "heat")
            .map(|row| row.value),
        Some(0.25)
    );
    let flux = rows_for(
        "flux-reactor",
        &BarExtras {
            variable_reactor: Some((30.0, 150.0, 0.4)),
            ..BarExtras::default()
        },
    );
    let Some(instability) = flux.iter().find(|row| row.key == "instability") else {
        panic!("instability bar");
    };
    assert_eq!(instability.value, 0.4);
    assert_eq!(instability.color, 0x665c_9fff);

    // `OverdriveProjector.java:80`, `ForceProjector.java:99`.
    let overdrive = rows_for(
        "overdrive-projector",
        &BarExtras {
            overdrive: Some((2.25, 2.25)),
            ..BarExtras::default()
        },
    );
    let Some(boost) = overdrive.iter().find(|row| row.key == "boost") else {
        panic!("boost bar");
    };
    assert_eq!(boost.label_args, ["125"]);
    assert_eq!(boost.value, 1.0);
    let shield = rows_for(
        "force-projector",
        &BarExtras {
            shield: Some(0.5),
            ..BarExtras::default()
        },
    );
    assert_eq!(
        shield
            .iter()
            .find(|row| row.key == "shield")
            .map(|row| row.value),
        Some(0.5)
    );

    // `SolidPump.java:51-55`.
    let pump = rows_for(
        "water-extractor",
        &BarExtras {
            pump: Some((0.25, 0.5, 0.5)),
            ..BarExtras::default()
        },
    );
    let Some(efficiency) = pump.iter().find(|row| row.key == "efficiency") else {
        panic!("pump speed bar");
    };
    assert_eq!(efficiency.label_args, ["15.0"]);
    assert_eq!(efficiency.value, 0.25);

    // `UnitAssembler.java:128-143` `units` bar is the 3-argument form.
    let assembler = rows_for(
        "tank-assembler",
        &BarExtras {
            assembler: Some((
                0.5,
                100.0,
                Some(UnitCapBars {
                    name: String::from("vanquish"),
                    count: 3,
                    cap: Some(8),
                    use_cap: true,
                    has_unit: true,
                }),
            )),
            ..BarExtras::default()
        },
    );
    let Some(units) = assembler.iter().find(|row| row.key == "units") else {
        panic!("assembler units bar");
    };
    assert_eq!(units.label_key, "bar.unitcap");
    assert_eq!(units.label_args, [":vanquish:", "3", "8"]);
    assert_eq!(units.value, 3.0 / 8.0);
    assert_eq!(units.color, 0xec7b_4cff);

    // `LaunchPad.java:69-70`, `LandingPad.java:113-115`.
    let launch = rows_for(
        "launch-pad",
        &BarExtras {
            launch: Some((50.0, 100.0)),
            ..BarExtras::default()
        },
    );
    assert_eq!(keys(&launch), ["health", "items", "progress"]);
    assert_eq!(launch[2].label_key, "bar.launchcooldown");
    assert_eq!(launch[2].value, 0.5);
    let landing = rows_for(
        "landing-pad",
        &BarExtras {
            landing_cooldown: Some(0.25),
            ..BarExtras::default()
        },
    );
    assert_eq!(
        landing
            .iter()
            .find(|row| row.key == "cooldown")
            .map(|row| row.value),
        Some(0.25)
    );

    // `Accelerator.java:106-110`.
    let accelerator = rows_for(
        "interplanetary-accelerator",
        &BarExtras {
            accelerator: Some((250_000.0, 1_000_000.0)),
            ..BarExtras::default()
        },
    );
    let Some(buffer) = accelerator
        .iter()
        .find(|row| row.key == "powerBufferRequirement")
    else {
        panic!("power buffer bar");
    };
    assert_eq!(buffer.label_key, "bar.powerbuffer");
    assert_eq!(
        buffer.label_args,
        [
            mind_core::ui::text::format_amount_default(250_000)
                .replace("[gray]", "")
                .replace("[]", ""),
            mind_core::ui::text::format_amount_default(1_000_000)
                .replace("[gray]", "")
                .replace("[]", "")
        ]
    );
    assert_eq!(buffer.value, 0.25);

    // `TargetDummy.java:64-73`: health removed, `dps` over the reset timer.
    let dummy = rows_for(
        "target-dummy",
        &BarExtras {
            target_dummy: Some((30.0, 120.0)),
            ..BarExtras::default()
        },
    );
    assert_eq!(keys(&dummy), ["dps"]);
    assert_eq!(dummy[0].value, 0.75);

    // `GenericCrafter.java:81-98`: output liquids replace the dynamic bar.
    let electrolyzer = def("electrolyzer");
    assert_eq!(electrolyzer.kind, BlockKind::GenericCrafter);
    let crafter = building_bar_rows(
        electrolyzer,
        1.0,
        None,
        Some(0),
        Some(&[]),
        &[],
        &BarExtras {
            output_liquids: vec![
                (String::from("ozone"), 0xaabb_ccff, 10.0, 20.0),
                (String::from("hydrogen"), 0xddee_ffff, 5.0, 20.0),
            ],
            ..BarExtras::default()
        },
    );
    let liquids: Vec<_> = crafter.iter().filter(|row| row.key == "liquid").collect();
    assert_eq!(liquids.len(), 2);
    assert_eq!(liquids[0].label_key, "liquid.ozone.name");
    assert_eq!((liquids[0].value, liquids[1].value), (0.5, 0.25));
}

#[test]
fn best_core_position_uses_the_anchor_tile_corner() {
    let registry = base_registry();
    let Some(core) = registry.block_id("core-shard") else {
        panic!("core-shard missing");
    };
    let mut grid = WorldGrid::new(8, 8);
    grid.fill(BlockId::AIR, BlockId::AIR);
    if grid.set_block(TilePos::new(3, 5), core, 0, 0).is_err() {
        panic!("set_block failed");
    }
    let unit = mind_core::config::TILESIZE as f32;
    // Java's spawn is `tile.worldx()/worldy()` of the anchor tile.
    assert_eq!(
        first_core_position(&grid, &registry),
        Some((3.0 * unit, 5.0 * unit))
    );
}

#[test]
fn best_core_position_is_none_without_cores() {
    let registry = base_registry();
    let mut grid = WorldGrid::new(4, 4);
    grid.fill(BlockId::AIR, BlockId::AIR);
    assert_eq!(first_core_position(&grid, &registry), None);
}

/// PAR-0037: Java spawns the player at the core's anchor-tile corner
/// (`core.pos()` -> `tile.worldx/worldy()`), so a 3x3 `core-shard` placed
/// with its centre at (129, 55) must report `(129*8, 55*8)` - the
/// reference Ground Zero `viewpos` is `(1032.0, 440.3)`.
#[test]
fn core_position_matches_the_java_spawn_anchor() {
    let registry = base_registry();
    let Some(core) = registry.block_id("core-shard") else {
        panic!("core-shard missing");
    };
    let mut sim = Sim::new(1, 256, 256, BlockId::AIR, BlockId::AIR);
    if let Err(error) = sim.apply(Command::Place {
        x: 129,
        y: 55,
        block: core,
    }) {
        panic!("place core failed: {error}");
    }
    assert_eq!(
        first_core_position(&sim.grid, &registry),
        Some((1032.0, 440.0))
    );
}

/// The map-loader convention: a 3x3 footprint filled across tiles
/// 128-130 x 54-56 (min corner first in row-major) must resolve to the
/// same anchor `(129, 55)` -> `(1032, 440)`.
#[test]
fn map_loaded_footprint_resolves_to_the_anchor_corner() {
    let registry = base_registry();
    let Some(core) = registry.block_id("core-shard") else {
        panic!("core-shard missing");
    };
    let mut grid = WorldGrid::new(256, 256);
    grid.fill(BlockId::AIR, BlockId::AIR);
    for y in 54..=56 {
        for x in 128..=130 {
            if grid.set_block(TilePos::new(x, y), core, 0, 0).is_err() {
                panic!("set_block failed at ({x},{y})");
            }
        }
    }
    assert_eq!(first_core_position(&grid, &registry), Some((1032.0, 440.0)));
}

#[test]
fn core_items_json_reports_the_core_inventory() {
    let registry = base_registry();
    let Some(core) = registry.block_id("core-shard") else {
        panic!("core-shard missing");
    };
    let Some(copper) = registry.item_id("copper") else {
        panic!("copper missing");
    };
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    if let Err(error) = sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: core,
    }) {
        panic!("place core failed: {error}");
    }
    let Some(entity) = sim.grid.tile(4, 4).build else {
        panic!("core building entity missing");
    };
    {
        // `Sim::apply` spawns the base building only; the plan-08 building
        // path attaches the item module, so attach the fixture module here.
        let mut module = ItemModule::with_items(registry.items().len());
        module.add(copper, 25, 1000);
        sim.ecs.0.entity_mut(entity).insert(module);
    }
    let team = sim
        .ecs
        .0
        .get::<BuildingComp>(entity)
        .map(|comp| comp.team.0)
        .unwrap_or(mind_core::ecs::TeamId::SHARDED.0);
    let json = core_items_json(&sim, &registry, team);
    assert!(json.contains("\"copper\":25"), "unexpected JSON: {json}");
}

/// PAR-0485 / CG-ECON-02: `CoreItemsDisplay` reads `player.team().core()`
/// (`CoreItemsDisplay.java:33`, `Team.java:83-85`), never "the first
/// CoreBlock in ECS order". A derelict/enemy core placed earlier must not
/// supply the HUD inventory.
#[test]
fn core_items_json_uses_the_player_team_core() {
    let registry = base_registry();
    let Some(core) = registry.block_id("core-shard") else {
        panic!("core-shard missing");
    };
    let Some(copper) = registry.item_id("copper") else {
        panic!("copper missing");
    };
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    // Enemy/derelict core first in entity order, stocked differently.
    if let Err(error) = sim.apply(Command::Place {
        x: 4,
        y: 4,
        block: core,
    }) {
        panic!("place enemy core failed: {error}");
    }
    let Some(enemy) = sim.grid.tile(4, 4).build else {
        panic!("enemy core entity missing");
    };
    {
        let mut module = ItemModule::with_items(registry.items().len());
        module.add(copper, 10, 1000);
        sim.ecs.0.entity_mut(enemy).insert(module);
    }
    if let Some(mut comp) = sim.ecs.0.get_mut::<BuildingComp>(enemy) {
        comp.team = mind_core::ecs::TeamId(2);
    }
    // The player's core, second in entity order.
    if let Err(error) = sim.apply(Command::Place {
        x: 12,
        y: 12,
        block: core,
    }) {
        panic!("place player core failed: {error}");
    }
    let Some(player_core) = sim.grid.tile(12, 12).build else {
        panic!("player core entity missing");
    };
    {
        let mut module = ItemModule::with_items(registry.items().len());
        module.add(copper, 25, 1000);
        sim.ecs.0.entity_mut(player_core).insert(module);
    }
    let team = sim
        .ecs
        .0
        .get::<BuildingComp>(player_core)
        .map(|comp| comp.team.0)
        .unwrap_or(mind_core::ecs::TeamId::SHARDED.0);
    let json = core_items_json(&sim, &registry, team);
    assert_eq!(json, "{\"copper\":25}", "player core inventory only");
    // A team with no core reads empty (never another team's core).
    assert_eq!(core_items_json(&sim, &registry, 77), "{}");
}

#[test]
fn block_items_json_gates_on_the_item_total() {
    let registry = base_registry();
    let Some(conveyor) = registry.block_id("conveyor") else {
        panic!("conveyor missing");
    };
    let Some(copper) = registry.item_id("copper") else {
        panic!("copper missing");
    };
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    if let Err(error) = sim.apply(Command::Place {
        x: 2,
        y: 2,
        block: conveyor,
    }) {
        panic!("place conveyor failed: {error}");
    }
    let Some(entity) = sim.grid.tile(2, 2).build else {
        panic!("conveyor building entity missing");
    };
    // `Sim::apply` spawns the base building only; the plan-08 building path
    // attaches the item module, so attach the fixture module here.
    sim.ecs
        .0
        .entity_mut(entity)
        .insert(ItemModule::with_items(registry.items().len()));
    // Zero stored items opens no inventory (`InputHandler.java:2017`).
    assert_eq!(block_items_json(&sim, &registry, 2, 2), None);
    if let Some(mut module) = sim.ecs.0.get_mut::<ItemModule>(entity) {
        module.add(copper, 3, 1000);
    }
    let json = match block_items_json(&sim, &registry, 2, 2) {
        Some(json) => json,
        None => panic!("items JSON missing"),
    };
    assert!(json.contains("\"copper\":3"), "unexpected JSON: {json}");
}

#[test]
fn building_bars_items_row() {
    // Java gates `bar.items` on `hasItems && configurable`
    // (`Block.java:724`); the launch pad satisfies both.
    let registry = base_registry();
    let Some(block) = registry.block_id("launch-pad") else {
        panic!("launch-pad missing");
    };
    let Some(def) = registry.block(block) else {
        panic!("launch-pad def missing");
    };
    assert!(def.has_items && def.configurable && def.item_capacity > 0);
    let rows = building_bar_rows(def, 1.0, None, Some(5), None, &[], &BarExtras::default());
    assert_eq!(rows.len(), 2, "rows: {:?}", rows.len());
    assert_eq!(rows[0].key, "health");
    assert_eq!(rows[1].key, "items");
    assert_eq!(rows[1].label_key, "bar.items");
    assert_eq!(rows[1].label_args.first().map(String::as_str), Some("5"));
    let expected = 5.0 / def.item_capacity as f32;
    assert!((rows[1].value - expected).abs() < 1e-4);
}

#[test]
fn building_bars_conduit_dynamic_liquid() {
    // Conduit: `has_liquids` with no liquid consumer -> dynamic max bar.
    let registry = base_registry();
    let Some(conduit) = registry.block_id("conduit") else {
        panic!("conduit missing");
    };
    let Some(def) = registry.block(conduit) else {
        panic!("conduit def missing");
    };
    assert!(def.has_liquids && def.liquid_capacity > 0.0);
    let liquid_info: Vec<(String, u32)> = registry
        .liquids()
        .iter()
        .map(|liquid| (liquid.name.clone(), 0))
        .collect();
    let capacity = def.liquid_capacity;
    let liquids = vec![0.0, capacity * 0.4];
    let rows = building_bar_rows(
        def,
        1.0,
        None,
        None,
        Some(&liquids),
        &liquid_info,
        &BarExtras::default(),
    );
    let row = rows
        .iter()
        .find(|row| row.key == "liquid")
        .unwrap_or_else(|| panic!("liquid row"));
    assert!((row.value - 0.4).abs() < 1e-3, "value: {}", row.value);
    assert_eq!(row.label_key, format!("liquid.{}.name", liquid_info[1].0));
}

#[test]
fn drop_zone_predicate_uses_the_radius() {
    // Spawn tile (10,10) -> world (80,80); radius 300 (G03 T9).
    let spawns = [(10, 10)];
    assert!(player_near_drop_zone_predicate(
        &spawns,
        Some((80.0 + 299.0, 80.0)),
        1,
        2,
        300.0
    ));
    assert!(!player_near_drop_zone_predicate(
        &spawns,
        Some((80.0 + 301.0, 80.0)),
        1,
        2,
        300.0
    ));
    // No spawns / same team / dead player.
    assert!(!player_near_drop_zone_predicate(
        &[],
        Some((80.0, 80.0)),
        1,
        2,
        300.0
    ));
    assert!(!player_near_drop_zone_predicate(
        &spawns,
        Some((80.0, 80.0)),
        2,
        2,
        300.0
    ));
    assert!(!player_near_drop_zone_predicate(&spawns, None, 1, 2, 300.0));
}

/// H03 T3: types sort by id, command rows are the first-seen union without
/// duplicates, and each type's default command is reported active.
#[test]
fn command_panel_orders_types_and_unions_commands() {
    use mind_core::ai::default_command;
    let content = base_registry();
    let (Some(dagger), Some(nova)) = (content.unit_by_name("dagger"), content.unit_by_name("nova"))
    else {
        panic!("dagger/nova missing from content");
    };
    let (first, second) = if dagger.id < nova.id {
        (dagger, nova)
    } else {
        (nova, dagger)
    };
    let units = vec![
        (second.id.get() as i32, None, StanceBits::new()),
        (first.id.get() as i32, None, StanceBits::new()),
        (first.id.get() as i32, None, StanceBits::new()),
    ];
    let panel = command_panel(&content, &units);
    let Some(types) = panel["types"].as_array() else {
        panic!("types not an array: {panel}");
    };
    assert_eq!(types.len(), 2);
    assert_eq!(types[0]["name"], serde_json::json!(first.name));
    assert_eq!(types[0]["count"], serde_json::json!(2));
    assert_eq!(types[1]["name"], serde_json::json!(second.name));
    assert_eq!(types[1]["count"], serde_json::json!(1));
    // Expected union: the lower id's list, then the other type's extras.
    let mut expected: Vec<String> = Vec::new();
    for command in first.commands.iter().chain(second.commands.iter()) {
        let Some(def) = content.unit_command(*command) else {
            panic!("command {command:?} missing");
        };
        let name = def.name.clone();
        if !expected.contains(&name) {
            expected.push(name);
        }
    }
    let Some(commands) = panel["commands"].as_array() else {
        panic!("commands not an array: {panel}");
    };
    if expected.len() > 1 {
        let names: Vec<String> = commands
            .iter()
            .map(|row| row["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(names, expected);
        for unit in [first, second] {
            let Some(default) = default_command(unit, &content) else {
                panic!("{} has no default command", unit.name);
            };
            let default = default.name.clone();
            let row = commands
                .iter()
                .find(|row| row["name"] == serde_json::json!(default));
            assert!(
                row.is_some_and(|row| row["active"] == serde_json::json!(true)),
                "default command {default} not active"
            );
        }
    } else {
        assert!(commands.is_empty());
    }
}

/// H03 T9: the factory preview fraction is `progress / plan.time` on a
/// live placed `ground-factory` (half progress -> 0.5), and the
/// reconstructor preview resolves the upgrade target with
/// `progress / constructTime`.
#[test]
fn unit_build_preview_fractions() {
    use mind_core::content::BlockId;
    use mind_core::determinism::SimCommand;
    use mind_core::world::TilePos;
    use mind_core::world::blocks::units::UnitFactoryBuild;

    fn some<T>(value: Option<T>, what: &str) -> T {
        match value {
            Some(value) => value,
            None => panic!("{what} missing"),
        }
    }

    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(some(sim.world_block_id("stone"), "stone"));
    sim.grid.fill(stone, BlockId::AIR);
    if let Err(error) = sim.install_block_runtime() {
        panic!("runtime: {error}");
    }
    let factory = some(sim.world_block_id("ground-factory"), "ground-factory");
    if let Err(error) = sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: factory,
        rotation: 0,
        team: 1,
        player: None,
    }) {
        panic!("place: {error}");
    }
    let entity = some(sim.grid.entity_at(TilePos::new(10, 10)), "entity");
    let def = some(
        sim.block_runtime().map(|runtime| runtime.content()),
        "runtime",
    );
    let def = some(def.block(BlockId::new(factory)), "def");
    let plans = def.unit_plans.clone();
    let plan_time = some(plans.first(), "plan").time;
    {
        let mut state = some(sim.ecs.0.get_mut::<UnitFactoryBuild>(entity), "state");
        state.current_plan = 0;
        state.progress = plan_time / 2.0;
    }
    let state = some(sim.ecs.0.get::<UnitFactoryBuild>(entity), "state");
    let (unit, fraction) = factory_preview(&plans, state.current_plan, state.progress);
    assert_eq!(unit, plans.first().map(|plan| plan.unit));
    assert!((fraction - 0.5).abs() < 1e-6, "fraction {fraction}");
    let Some(unit) = unit else {
        panic!("plan unit missing");
    };

    // Reconstructor: `(from -> to)` upgrades, payload arrived at half
    // progress.
    let nova = some(
        sim.block_runtime()
            .and_then(|runtime| runtime.content().unit_id("nova")),
        "nova",
    );
    let (target, fraction, arrived) =
        reconstructor_preview(&[(unit, nova)], Some(unit), 60.0, 120.0, 1.0);
    assert_eq!(target, Some(nova));
    assert!((fraction - 0.5).abs() < 1e-6, "fraction {fraction}");
    assert!(arrived);
}
