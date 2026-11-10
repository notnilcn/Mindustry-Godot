// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit tests for the JSON content parser.
//!
//! Upstream: none (port-only tests).

use super::*;
use crate::content::create_base_content;
use crate::content::{MemoryBundle, MemoryUnlockStore};

fn registry() -> ContentRegistry {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true).expect("base content");
    registry.init().expect("init");
    registry.post_init().expect("post init");
    registry
}

fn parse(
    reg: &mut ContentRegistry,
    folder: ContentType,
    stem: &str,
    json: &str,
) -> (
    ContentJsonParser<'static>,
    Result<ContentRef, ContentParseError>,
) {
    reg.set_current_mod(Some(crate::content::ModId(String::from("mods"))));
    let mut parser = ContentJsonParser::new();
    let result = parser.parse(reg, "test.json", stem, json, folder);
    (parser, result)
}

/// L02 T1: `Floor.decoration` reads as a block name in the deferred pass
/// (`ContentParser.finishParsing`, `ContentParser.java:1005`), so a floor
/// may name a prop parsed after it; mod prefix first, then the bare name
/// (`ContentParser.java:510-515`). An unknown name is an error.
#[test]
fn floor_decoration_resolves_after_parsing() {
    let mut reg = registry();
    reg.set_current_mod(Some(crate::content::ModId(String::from("mods"))));
    let mut parser = ContentJsonParser::new();
    let floor = parser.parse(
        &mut reg,
        "floor.json",
        "glow-floor",
        r#"{"type":"Floor","decoration":"glow-rock"}"#,
        ContentType::Block,
    );
    assert!(floor.is_ok(), "{floor:?}");
    let vanilla = parser.parse(
        &mut reg,
        "dust.json",
        "dust-floor",
        r#"{"type":"Floor","decoration":"sand-boulder"}"#,
        ContentType::Block,
    );
    assert!(vanilla.is_ok(), "{vanilla:?}");
    let missing = parser.parse(
        &mut reg,
        "bad.json",
        "bad-floor",
        r#"{"type":"Floor","decoration":"no-such-rock"}"#,
        ContentType::Block,
    );
    assert!(missing.is_ok(), "{missing:?}");
    let prop = parser.parse(
        &mut reg,
        "rock.json",
        "glow-rock",
        r#"{"type":"Prop"}"#,
        ContentType::Block,
    );
    assert!(prop.is_ok(), "{prop:?}");
    let errors = parser.finish_parsing(&mut reg);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("no-such-rock"));
    let decoration = |floor: &str| {
        let def = reg.block_by_name(floor).expect(floor);
        reg.block(def.decoration).map(|block| block.name.clone())
    };
    assert_eq!(
        decoration("mods-glow-floor").as_deref(),
        Some("mods-glow-rock")
    );
    assert_eq!(
        decoration("mods-dust-floor").as_deref(),
        Some("sand-boulder")
    );
    assert_eq!(decoration("mods-bad-floor").as_deref(), Some("air"));
}

/// #59: `research` on a mod content record inserts a tech node
/// (`ContentParser.java:1337-1400`): parent by content name, explicit
/// requirements, custom objectives, the implicit item Produce objective,
/// and the root/planet branch.
#[test]
fn research_inserts_tech_nodes() {
    use crate::content::tech::{ObjectiveSpec, TechNodeRef};
    let mut reg = registry();
    let mut parser = ContentJsonParser::new();
    reg.set_current_mod(Some(crate::content::ModId(String::from("mods"))));
    parser
        .parse(
            &mut reg,
            "w.json",
            "research-wall",
            r#"{"type":"Wall","health":500,"size":1,
                "research":{"parent":"router","requirements":["copper/10"],
                  "objectives":[{"type":"Research","content":"router"}]}}"#,
            ContentType::Block,
        )
        .expect("block");
    parser
        .parse(
            &mut reg,
            "i.json",
            "research-dust",
            r#"{"name":"Research Dust","research":"router"}"#,
            ContentType::Item,
        )
        .expect("item");
    parser
        .parse(
            &mut reg,
            "r.json",
            "research-root",
            r#"{"type":"Wall","health":100,
                "research":{"root":true,"name":"mod-research-root",
                  "requiresUnlock":true,"planet":"serpulo"}}"#,
            ContentType::Block,
        )
        .expect("root block");
    let errors = parser.finish_parsing(&mut reg);
    assert!(errors.is_empty(), "{errors:?}");

    let wall_id = reg.block_id("mods-research-wall").expect("wall");
    let wall = reg.block(wall_id).expect("wall def");
    let node_ref = wall.unlock.tech_node.expect("tech node");
    let node = reg.tech().node(node_ref).expect("node");
    assert_eq!(node.content, Some(ContentRef::block(wall_id)));
    assert_eq!(node.requirements.len(), 1);
    assert_eq!(node.requirements[0].amount, 10);
    assert_eq!(
        reg.item(node.requirements[0].item)
            .map(|item| item.name.as_str()),
        Some("copper")
    );
    assert!(
        node.objectives
            .iter()
            .any(|objective| matches!(objective, ObjectiveSpec::Research(_))),
        "custom objective: {:?}",
        node.objectives
    );
    let router_node = reg
        .tech()
        .nodes
        .iter()
        .position(|candidate| candidate.content_name == "router")
        .map(|index| TechNodeRef(index as u32))
        .expect("router node");
    assert_eq!(node.parent, Some(router_node));
    assert!(
        reg.tech()
            .node(router_node)
            .expect("router")
            .children
            .contains(&node_ref)
    );

    let item_id = reg.item_id("mods-research-dust").expect("item");
    let item = reg.item(item_id).expect("item def");
    let item_node = reg
        .tech()
        .node(item.unlock.tech_node.expect("item node"))
        .expect("item node record");
    assert!(
        item_node.objectives.iter().any(|objective| matches!(
            objective,
            ObjectiveSpec::Produce(c) if *c == ContentRef::item(item_id)
        )),
        "implicit produce objective: {:?}",
        item_node.objectives
    );

    let root_id = reg.block_id("mods-research-root").expect("root");
    let root_def = reg.block(root_id).expect("root def");
    let root_ref = root_def.unlock.tech_node.expect("root node");
    let root_node = reg.tech().node(root_ref).expect("root record");
    assert_eq!(root_node.name.as_deref(), Some("mod-research-root"));
    assert!(root_node.requires_unlock);
    assert!(reg.tech().roots.contains(&root_ref));
    assert_eq!(
        root_node.planet,
        reg.by_name("serpulo")
            .filter(|reference| reference.type_ == ContentType::Planet)
            .map(|reference| crate::content::PlanetId::new(reference.id))
    );
}

/// L06 T2: `Turret.playerControllable` (`Turret.java:103`) parses from
/// mod JSON and defaults to true.
#[test]
fn turret_player_controllable_parses() {
    let mut reg = registry();
    let (_, fixed) = parse(
        &mut reg,
        ContentType::Block,
        "fixed-turret",
        r#"{"type":"ItemTurret","playerControllable":false}"#,
    );
    assert!(fixed.is_ok(), "{fixed:?}");
    let (_, plain) = parse(
        &mut reg,
        ContentType::Block,
        "plain-turret",
        r#"{"type":"ItemTurret"}"#,
    );
    assert!(plain.is_ok(), "{plain:?}");
    let flag = |name: &str| reg.block_by_name(name).map(|def| def.player_controllable);
    assert_eq!(flag("mods-fixed-turret"), Some(false));
    assert_eq!(flag("mods-plain-turret"), Some(true));
    assert_eq!(flag("duo"), Some(true), "no vanilla turret clears it");
}

#[test]
fn liquid_parity() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Liquid,
        "test-fluid",
        r#"{"name":"Test Fluid","color":"ff0000","temperature":0.2,"gas":true,"effect":"wet"}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let liquid = reg.liquid_by_name("mods-test-fluid").expect("liquid");
    assert_eq!(liquid.name, "mods-test-fluid");
    assert!(liquid.effect != StatusId::NONE, "status resolved");
}

#[test]
fn status_transitions() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Status,
        "test-aura",
        r#"{"name":"Test Aura","speedMultiplier":1.5,"opposites":["wet"],"color":"00ff00"}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let status = reg.status_by_name("mods-test-aura").expect("status");
    assert!((status.speed_multiplier - 1.5).abs() < 1e-6);
    assert_eq!(status.opposites.len(), 1);
}

#[test]
fn weather_parity() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Weather,
        "test-storm",
        r#"{"name":"Test Storm","type":"ParticleWeather","duration":100,"hidden":true}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let weather = reg.weather_by_name("mods-test-storm").expect("weather");
    assert!(matches!(weather.kind, WeatherKind::Particle));
    assert!(weather.hidden);
}

#[test]
fn team_parity() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Team,
        "test-crew",
        r#"{"name":"Test Crew","team":"test-crew"}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    assert!(reg.team_by_name("mods-test-crew").is_some());
}

/// L06 T6.1: `allowCampaignRules = isVanilla()` (`Planet.java:221`):
/// vanilla planets true, mod planets false unless the JSON sets it;
/// `prebuildBase` defaults true (`Planet.java:133`).
#[test]
fn planet_campaign_rule_flags() {
    let mut reg = registry();
    assert!(
        reg.planet_by_name("serpulo")
            .expect("serpulo")
            .allow_campaign_rules
    );
    assert!(
        reg.planet_by_name("erekir")
            .expect("erekir")
            .allow_campaign_rules
    );
    let (_, plain) = parse(
        &mut reg,
        ContentType::Planet,
        "plain-world",
        r#"{"radius":1.0,"sectorSize":2}"#,
    );
    assert!(plain.is_ok(), "{plain:?}");
    let (_, ruled) = parse(
        &mut reg,
        ContentType::Planet,
        "ruled-world",
        r#"{"radius":1.0,"sectorSize":2,"allowCampaignRules":true,"prebuildBase":false}"#,
    );
    assert!(ruled.is_ok(), "{ruled:?}");
    let plain = reg.planet_by_name("mods-plain-world").expect("plain");
    assert!(!plain.allow_campaign_rules, "mod planets default false");
    assert!(plain.prebuild_base);
    let ruled = reg.planet_by_name("mods-ruled-world").expect("ruled");
    assert!(ruled.allow_campaign_rules);
    assert!(!ruled.prebuild_base);
}

#[test]
fn sector_planet_team_parity() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Planet,
        "test-world",
        r#"{"name":"Test World","radius":2.0,"sectorSize":2,"mesh":"hexSphere","accessible":true}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let planet = reg.planet_by_name("mods-test-world").expect("planet");
    assert_eq!(planet.radius, 2.0);
    assert_eq!(planet.sector_tiles, 2);
    assert_eq!(planet.sectors.len(), planet.sector_count);

    let planet_id = reg.planet_id("mods-test-world").expect("planet id");
    let json = format!(
        r#"{{"name":"Test Landing","planet":"mods-test-world","sector":{},"difficulty":1.5}}"#,
        planet_id.raw()
    );
    let (_, result) = parse(&mut reg, ContentType::Sector, "test-landing", &json);
    assert!(result.is_ok(), "{result:?}");
    let sector = reg.sector_by_name("mods-test-landing").expect("sector");
    assert!((sector.difficulty - 1.5).abs() < 1e-6);
}

/// L06 T6.4: `SectorPreset.showSectorLandInfo` (SectorPreset.java:34,
/// default true) parses from mod JSON and gates the landing announce with
/// `state.isCampaign() && Vars.showSectorLandInfo` (CoreBlock.java:414).
#[test]
fn sector_show_land_info_flag_and_gate() {
    use crate::content::registries::sectors::shows_sector_land_info;
    let mut reg = registry();
    let ground_zero = reg.sector_by_name("groundZero").expect("groundZero");
    assert!(ground_zero.show_sector_land_info, "vanilla default true");
    assert!(shows_sector_land_info(true, Some(ground_zero)));
    assert!(shows_sector_land_info(true, None), "generated sector");
    assert!(!shows_sector_land_info(false, None), "not a campaign");
    let planet = reg.planet_id("serpulo").expect("serpulo");
    let json = format!(
        r#"{{"planet":"serpulo","sector":{},"showSectorLandInfo":false}}"#,
        planet.raw() + 200
    );
    let (_, result) = parse(&mut reg, ContentType::Sector, "quiet-landing", &json);
    assert!(result.is_ok(), "{result:?}");
    let quiet = reg.sector_by_name("mods-quiet-landing").expect("sector");
    assert!(!quiet.show_sector_land_info);
    assert!(!shows_sector_land_info(true, Some(quiet)));
}

#[test]
fn block_consumes_classes_parse_with_java_defaults() {
    // J09 T1: the `consumes` array maps the `Consume*` classes with Java's
    // field defaults; unsupported classes are warned about, never dropped
    // silently (`Consume.java:16` update = true, `ConsumeCoolant.java:4-6`).
    let mut reg = registry();
    let (parser, result) = parse(
        &mut reg,
        ContentType::Block,
        "test-kit",
        r#"{"name":"Test Kit","size":2,"consumes":[
            {"type":"ConsumeItems","items":[{"item":"copper","amount":10}]},
            {"type":"ConsumeLiquid","liquid":"water","amount":0.1},
            {"type":"ConsumeLiquids","liquids":[{"liquid":"water","amount":0.05}]},
            {"type":"ConsumePower","usage":1.5,"capacity":1000,"buffered":true},
            {"type":"ConsumeCoolant"},
            {"type":"ConsumeItemDynamic"},
            {"type":"ConsumeItemExplode"}]}"#,
    );
    use crate::content::registries::blocks::Consume;

    assert!(result.is_ok(), "{result:?}");
    let block = reg.block_by_name("mods-test-kit").expect("block");
    assert_eq!(
        block.consumes.len(),
        5,
        "unsupported classes warn; the mapped five parse"
    );
    let copper = reg.item_by_name("copper").expect("copper").id;
    let water = reg.liquid_by_name("water").expect("water").id;
    let Consume::Items(items) = &block.consumes[0].consume else {
        panic!("ConsumeItems: {:?}", block.consumes[0]);
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item, copper);
    assert_eq!(items[0].amount, 10);
    assert!(!block.consumes[0].optional && block.consumes[0].update);
    let Consume::Liquid { liquid, amount } = &block.consumes[1].consume else {
        panic!("ConsumeLiquid: {:?}", block.consumes[1]);
    };
    assert_eq!(*liquid, water);
    assert_eq!(*amount, 0.1);
    let Consume::Liquids(liquids) = &block.consumes[2].consume else {
        panic!("ConsumeLiquids: {:?}", block.consumes[2]);
    };
    assert_eq!(liquids.len(), 1);
    assert_eq!(liquids[0].liquid, water);
    assert_eq!(liquids[0].amount, 0.05);
    let Consume::Power { usage, buffered } = &block.consumes[3].consume else {
        panic!("ConsumePower: {:?}", block.consumes[3]);
    };
    assert_eq!(*usage, 1.5);
    assert_eq!(*buffered, 1000.0, "`capacity` when `buffered`");
    let Consume::Coolant {
        amount,
        allow_liquid,
        allow_gas,
    } = &block.consumes[4].consume
    else {
        panic!("ConsumeCoolant: {:?}", block.consumes[4]);
    };
    assert_eq!(*amount, 1.0, "Java default `ConsumeCoolant()`");
    assert!(*allow_liquid && !*allow_gas);
    assert!(
        parser
            .warnings
            .iter()
            .any(|warning| warning.message.contains("ConsumeItemDynamic")),
        "unsupported consumer warned: {:?}",
        parser.warnings
    );
    assert!(
        parser
            .warnings
            .iter()
            .any(|warning| warning.message.contains("ConsumeItemExplode")),
        "the explode consumer names its J04 T2 dependency: {:?}",
        parser.warnings
    );
}

#[test]
fn classmap_scopes_resolve_the_t1_aliases() {
    // J09 T1: the scope tables from the plan resolve through the ClassMap
    // (bullets, AIs, block base classes, abilities, data).
    let map = classmap::ClassTagMap::committed();
    for (alias, scope, tag) in [
        ("EmptyBulletType", ClassScope::BulletType, "EmptyBulletType"),
        (
            "ContinuousFlameBulletType",
            ClassScope::BulletType,
            "ContinuousFlameBulletType",
        ),
        (
            "PointLaserBulletType",
            ClassScope::BulletType,
            "PointLaserBulletType",
        ),
        ("GroundAI", ClassScope::UnitController, "GroundAI"),
        ("MinerAI", ClassScope::UnitController, "MinerAI"),
        ("PowerGenerator", ClassScope::Block, "ConsumeGenerator"),
        ("Turret", ClassScope::Block, "ItemTurret"),
        ("ControlBlock", ClassScope::Block, "ControlBlock"),
        (
            "ArmorPlateAbility",
            ClassScope::Ability,
            "ArmorPlateAbility",
        ),
        ("DrawMulti", ClassScope::Drawer, "DrawMulti"),
        ("RegionPart", ClassScope::Part, "RegionPart"),
        ("AssemblerUnitPlan", ClassScope::Data, "AssemblerUnitPlan"),
    ] {
        assert_eq!(
            map.resolve(alias, scope),
            Some(tag),
            "{alias} ({})",
            scope.name()
        );
    }
}

#[test]
fn block_base_classes_and_unsupported_abilities() {
    // The Java block base classes map to the port's behaviour kinds; the
    // interface-only classes fail with a clear message.
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "test-generator",
        r#"{"name":"Test Generator","type":"PowerGenerator","size":2}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let block = reg.block_by_name("mods-test-generator").expect("block");
    assert_eq!(block.kind, BlockKind::ConsumeGenerator);

    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "test-control",
        r#"{"name":"Test Control","type":"ControlBlock","size":2}"#,
    );
    let error = result.expect_err("interface block class");
    assert!(
        error.message.contains("not a concrete block class"),
        "{error}"
    );

    // Ability classes: `ArmorPlateAbility` maps to a runtime kind
    // (PAR-0587) and keeps its class-field default where the JSON omits
    // `healthMultiplier`; the unknown `health` key is ignored upstream
    // (`ignoreUnknownFields = true`) and must not drop the ability.
    let (parser, result) = parse(
        &mut reg,
        ContentType::Unit,
        "test-armor-mech",
        r#"{"name":"Test Armor Mech","type":"mech","abilities":[{"type":"ArmorPlateAbility","health":100},{"type":"RegenAbility","amount":1}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-test-armor-mech").expect("unit");
    assert_eq!(unit.abilities.len(), 2, "both abilities parse");
    assert_eq!(unit.abilities[0].kind, AbilityKind::ArmorPlate);
    assert_eq!(
        unit.abilities[0].health_multiplier, 0.2,
        "`ArmorPlateAbility.healthMultiplier` class default"
    );
    assert!(
        !parser
            .warnings
            .iter()
            .any(|warning| warning.message.contains("ArmorPlateAbility")),
        "mapped ability must not warn: {:?}",
        parser.warnings
    );
}

/// PAR-0587: `ContentParser` reads any `Ability` subclass by its Java field
/// names (`ContentParser.java:355-360`). The three mod-only kinds the port
/// simulates/draws (`ArmorPlateAbility`, `MoveLightningAbility`,
/// `UnitSpawnAbility`) carry those names onto [`AbilitySpec`].
#[test]
fn mod_ability_kinds_parse_their_java_fields() {
    let mut reg = registry();
    let (parser, result) = parse(
        &mut reg,
        ContentType::Unit,
        "ability-mech",
        r#"{"name":"Ability Mech","type":"mech","health":400,
            "abilities":[
              {"type":"ArmorPlateAbility","healthMultiplier":0.35},
              {"type":"MoveLightningAbility","damage":42,"length":9,"chance":0.5,
               "y":-3.5,"minSpeed":0.4,"maxSpeed":1.6,"alternate":false},
              {"type":"UnitSpawnAbility","unit":"dagger","spawnTime":120,
               "spawnX":1.5,"spawnY":-2.5}
            ]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    assert!(
        !parser
            .warnings
            .iter()
            .any(|warning| warning.message.contains("ability")),
        "no ability warning: {:?}",
        parser.warnings
    );
    let unit = reg.unit_by_name("mods-ability-mech").expect("unit");
    assert_eq!(unit.abilities.len(), 3);

    let armor = &unit.abilities[0];
    assert_eq!(armor.kind, AbilityKind::ArmorPlate);
    assert_eq!(armor.health_multiplier, 0.35, "healthMultiplier");

    let lightning = &unit.abilities[1];
    assert_eq!(lightning.kind, AbilityKind::MoveLightning);
    assert_eq!(lightning.amount, 42.0, "damage feeds the union `amount`");
    assert_eq!(lightning.length, 9);
    assert_eq!(lightning.chance, 0.5);
    assert_eq!(lightning.spawn_offset, -3.5, "y feeds the spawn offset");
    assert_eq!(lightning.y, -3.5, "the shared `y` field keeps the value");
    assert_eq!(lightning.min_speed, 0.4);
    assert_eq!(lightning.max_speed, 1.6);
    assert!(!lightning.alternate);

    let spawn = &unit.abilities[2];
    assert_eq!(spawn.kind, AbilityKind::UnitSpawn);
    assert_eq!(spawn.unit, Some("dagger"), "unit reference resolved");
    assert_eq!(spawn.spawn_time, 120.0, "spawnTime");
    assert_eq!(spawn.spawn_x, 1.5, "spawnX");
    assert_eq!(spawn.spawn_y, -2.5, "spawnY");
}

/// PAR-0587: an ability object that omits fields keeps the Java class-field
/// initializers (`ArmorPlateAbility.java:30`, `MoveLightningAbility.java:12-25`,
/// `UnitSpawnAbility.java:19`) instead of the union defaults.
#[test]
fn mod_ability_kinds_keep_java_class_defaults() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "ability-defaults",
        r#"{"name":"Ability Defaults","type":"mech","abilities":[
            {"type":"ArmorPlateAbility"},
            {"type":"MoveLightningAbility"},
            {"type":"UnitSpawnAbility","unit":"dagger"}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-ability-defaults").expect("unit");
    assert_eq!(unit.abilities.len(), 3);
    let armor = &unit.abilities[0];
    assert_eq!(armor.kind, AbilityKind::ArmorPlate);
    assert_eq!(armor.health_multiplier, 0.2);
    let lightning = &unit.abilities[1];
    assert_eq!(lightning.kind, AbilityKind::MoveLightning);
    assert_eq!(lightning.amount, 35.0);
    assert_eq!(lightning.length, 12);
    assert_eq!(lightning.chance, 0.15);
    assert_eq!(lightning.spawn_offset, 0.0);
    assert_eq!(lightning.min_speed, 0.8);
    assert_eq!(lightning.max_speed, 1.2);
    assert!(lightning.alternate);
    let spawn = &unit.abilities[2];
    assert_eq!(spawn.kind, AbilityKind::UnitSpawn);
    assert_eq!(spawn.spawn_time, 60.0);
    assert_eq!((spawn.spawn_x, spawn.spawn_y), (0.0, 0.0));
}

/// PAR-0587: `WrapEffect.rotation` (degrees) is a plain `float` field read by
/// `ContentParser` (`WrapEffect.java:18`); the wrapper creates its child at
/// that rotation (`:43-46`) while `init()` copies the child lifetime
/// (`:29-32`).
#[test]
fn wrap_effect_rotation_parses() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "wrap-mech",
        r#"{"name":"Wrap Mech","type":"mech",
            "deathExplosionEffect":{"type":"WrapEffect","effect":"smoke",
                                    "color":"ff0000","rotation":40}}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-wrap-mech").expect("unit");
    let EffectRef::Inline(wrap) = &unit.death_explosion_effect else {
        panic!(
            "deathExplosionEffect not inline: {:?}",
            unit.death_explosion_effect
        );
    };
    assert_eq!(wrap.kind, InlineEffectKind::WrapEffect);
    assert_eq!(wrap.rotation, 40.0);
    assert_eq!(wrap.color, Rgba::from_hex("ff0000"));
    assert_eq!(
        wrap.lifetime,
        effect_by_name("smoke").expect("smoke").meta().lifetime,
        "`WrapEffect.init()` copies the child lifetime"
    );
}

#[test]
fn unit_weapons_nested() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "test-mech",
        r#"{"name":"Test Mech","type":"mech","health":500,"speed":0.6,
            "weapons":[{"name":"test-gun","mirror":true,"reload":30,
              "bullet":{"type":"LaserBulletType","damage":10,"speed":5}}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-test-mech").expect("unit");
    assert_eq!(unit.health, 500.0);
    assert_eq!(unit.weapons.len(), 1);
    assert_eq!(unit.weapons[0].name, "test-gun");
    assert!(unit.weapons[0].bullet.id.raw() >= 112);
}

/// #59: `BlockSpec`'s extended `Block`/`Floor` field surface reads from
/// mod JSON (the parser used to stop at ~30 fields).
#[test]
fn block_extended_field_surface() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-wall",
        r#"{"type":"Wall","health":100,"size":1,
            "fogRadius":9,"canOverdrive":false,"squareSprite":true,
            "insulated":true,"absorbLasers":true,"replaceable":false,
            "priority":2.5,"unitCapModifier":3,"region":"mod-wall-region",
            "itemDrop":"copper","mapColor":"3366ff","drawTeamOverlay":true,
            "buildCostMultiplier":0.5,"buildTime":40,"albedo":0.25,
            "group":"walls","cacheLayer":"walls"}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let block = reg.block_by_name("mods-mod-wall").expect("block");
    assert_eq!(block.fog_radius, 9);
    assert!(!block.can_overdrive);
    assert!(block.square_sprite);
    assert!(block.insulated);
    assert!(block.absorb_lasers);
    assert!(!block.replaceable);
    assert_eq!(block.priority, 2.5);
    assert_eq!(block.unit_cap_modifier, 3);
    assert_eq!(block.region, "mod-wall-region");
    assert_eq!(
        block
            .item_drop
            .and_then(|id| reg.item(id))
            .map(|i| i.name.as_str()),
        Some("copper")
    );
    assert_eq!(
        block.map_color,
        Some(crate::content::Rgba::new(0.2, 0.4, 1.0, 1.0))
    );
    assert!(block.draw_team_overlay);
    assert_eq!(block.build_cost_multiplier, 0.5);
    assert_eq!(block.build_time, 40.0);
    assert_eq!(block.albedo, 0.25);
    assert_eq!(
        block.group,
        crate::content::registries::blocks::BlockGroup::Walls
    );
    assert_eq!(block.cache_layer, crate::render::layer::CacheLayerId::Walls);
}

/// GAPS #56 follow-up: the production-family content fields
/// (`GenericCrafter.java:22-37`, `Separator.java:20`) read from mod JSON —
/// item/liquid outputs, output directions, liquid fullness, results.
#[test]
fn block_crafter_output_fields_parse() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-crafter",
        r#"{"type":"GenericCrafter","size":3,"health":200,
            "outputItems":[{"item":"copper","amount":2},{"item":"silicon","amount":3}],
            "outputLiquids":[{"liquid":"water","amount":0.1},
                             {"liquid":"slag","amount":0.05}],
            "liquidOutputDirections":[1,3],
            "ignoreLiquidFullness":true,
            "results":[{"item":"lead","amount":4}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let block = reg.block_by_name("mods-mod-crafter").expect("block");
    let items: Vec<(&str, i32)> = block
        .output_items
        .iter()
        .map(|stack| {
            let item = reg.item(stack.item).expect("item");
            (item.name.as_str(), stack.amount)
        })
        .collect();
    assert_eq!(items, vec![("copper", 2), ("silicon", 3)]);
    let liquids: Vec<(String, f32)> = block
        .output_liquids
        .iter()
        .map(|(liquid, amount)| (reg.liquid(*liquid).expect("liquid").name.clone(), *amount))
        .collect();
    assert_eq!(liquids.len(), 2);
    assert_eq!(liquids[0].0, "water");
    assert!((liquids[0].1 - 0.1).abs() < 1e-6);
    assert_eq!(liquids[1].0, "slag");
    assert_eq!(block.liquid_output_directions, vec![1, 3]);
    assert!(block.ignore_liquid_fullness);
    assert_eq!(block.separator_results.len(), 1);
    assert_eq!(block.separator_results[0].amount, 4);

    // `GenericCrafter.init` single-form write-back (`:125-130`).
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-single",
        r#"{"type":"GenericCrafter","size":2,
            "outputItem":{"item":"lead","amount":5},
            "outputLiquid":{"liquid":"oil","amount":0.25}}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let single = reg.block_by_name("mods-mod-single").expect("block");
    assert_eq!(single.output_items.len(), 1);
    assert_eq!(single.output_items[0].amount, 5);
    assert_eq!(single.output_liquids.len(), 1);
    assert!((single.output_liquids[0].1 - 0.25).abs() < 1e-6);

    // A non-array `liquidOutputDirections` is a parse error.
    let (_, bad) = parse(
        &mut reg,
        ContentType::Block,
        "mod-bad-directions",
        r#"{"type":"GenericCrafter","size":2,"liquidOutputDirections":1}"#,
    );
    let error = bad.expect_err("directions must be an array");
    assert!(error.message.contains("liquidOutputDirections"), "{error}");
}

/// GAPS #56 follow-up: the drill family fields (`Drill.java:26-42`,
/// `BeamDrill.java:38-40`, `BurstDrill.java:26`) read from mod JSON.
#[test]
fn block_drill_fields_parse() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-drill",
        r#"{"type":"Drill","size":2,"health":240,
            "drillTime":123,"tier":5,"hardnessDrillMultiplier":0.5,
            "blockedItems":["copper","lead"],"blockedItem":"sand",
            "drillMultipliers":{"copper":2.5,"titanium":1.25},
            "liquidBoostIntensity":1.25,"warmupSpeed":0.02,
            "invertedTime":180,"range":7}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let block = reg.block_by_name("mods-mod-drill").expect("block");
    assert_eq!(block.drill_time, Some(123.0));
    assert_eq!(block.drill_tier, Some(5));
    assert_eq!(block.hardness_drill_multiplier, Some(0.5));
    let blocked: Vec<&str> = block
        .blocked_items
        .iter()
        .map(|id| reg.item(*id).expect("item").name.as_str())
        .collect();
    assert_eq!(blocked, vec!["copper", "lead", "sand"]);
    let multipliers: Vec<(&str, f32)> = block
        .drill_multipliers
        .iter()
        .map(|(item, value)| (reg.item(*item).expect("item").name.as_str(), *value))
        .collect();
    assert_eq!(multipliers.len(), 2);
    assert!(
        multipliers
            .iter()
            .any(|(name, value)| *name == "copper" && *value == 2.5)
    );
    assert_eq!(block.liquid_boost_intensity, Some(1.25));
    assert_eq!(block.warmup_speed, Some(0.02));
    assert_eq!(block.inverted_time, Some(180.0));
    assert_eq!(block.beam_range, Some(7));
}

/// GAPS #56 follow-up: pump / wall-crafter / attribute-crafter fields
/// (`Pump.java:20`, `SolidPump.java:22-26`, `Fracker.java:8`,
/// `WallCrafter.java:30-46`, `AttributeCrafter.java:17-31`).
#[test]
fn block_pump_and_attribute_fields_parse() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-pump",
        r#"{"type":"SolidPump","size":2,"health":200,
            "pumpAmount":0.4,"itemUseTime":80,"result":"cryofluid",
            "attribute":"spores","baseEfficiency":0.5,
            "output":"titanium","itemBoostIntensity":1.25,
            "boostItemUseTime":90,"boostScale":0.75,"minEfficiency":0.1,
            "maxBoost":2.5,"outputScale":0.5,
            "scaleLiquidConsumption":true,"displayEfficiency":false,
            "displayScaledOutput":true}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let block = reg.block_by_name("mods-mod-pump").expect("block");
    assert_eq!(block.pump_amount, Some(0.4));
    assert_eq!(block.item_use_time, Some(80.0));
    assert_eq!(
        block
            .pump_liquid
            .and_then(|id| reg.liquid(id))
            .map(|liquid| liquid.name.as_str()),
        Some("cryofluid")
    );
    assert_eq!(block.attribute.as_deref(), Some("spores"));
    assert_eq!(block.base_efficiency, Some(0.5));
    assert_eq!(
        block
            .output_item
            .and_then(|id| reg.item(id))
            .map(|item| item.name.as_str()),
        Some("titanium")
    );
    assert_eq!(block.item_boost_intensity, Some(1.25));
    assert_eq!(block.boost_item_use_time, Some(90.0));
    assert_eq!(block.boost_scale, Some(0.75));
    assert_eq!(block.min_efficiency, Some(0.1));
    assert_eq!(block.max_boost, Some(2.5));
    assert_eq!(block.output_scale, Some(0.5));
    assert_eq!(block.scale_liquid_consumption, Some(true));
    assert_eq!(block.display_efficiency, Some(false));
    assert_eq!(block.display_scaled_output, Some(true));

    // `Attribute` is an enum upstream: an unknown name warns.
    let (parser, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-bad-attribute",
        r#"{"type":"AttributeCrafter","size":2,"attribute":"nope"}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    assert!(
        parser
            .warnings
            .iter()
            .any(|warning| warning.message.contains("unknown attribute")),
        "{:?}",
        parser.warnings
    );
}

/// GAPS #56 follow-up: the environment family fields (`Block.variants`,
/// `TiledFloor.java:17/21`, `TiledWall.java:16`, `WobbleProp.java:9`).
#[test]
fn block_environment_family_fields_parse() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-tiled",
        r#"{"type":"TiledFloor","variants":4,"maxSize":5}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let tiled = reg.block_by_name("mods-mod-tiled").expect("tiled floor");
    assert_eq!(tiled.variants, 4);
    assert_eq!(tiled.max_size, 5);

    let (_, result) = parse(
        &mut reg,
        ContentType::Block,
        "mod-wobble",
        r#"{"type":"WobbleProp","wscl":30,"wmag":0.25,"wtscl":2,"wmag2":0.5}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let wobble = reg.block_by_name("mods-mod-wobble").expect("wobble prop");
    assert_eq!(wobble.wobble_scale, Some(30.0));
    assert_eq!(wobble.wobble_magnitude, Some(0.25));
    assert_eq!(wobble.wobble_time_scale, Some(2.0));
    assert_eq!(wobble.wobble_magnitude2, Some(0.5));
}

/// #59: the extended `UnitType` field surface (stats, sounds, effects,
/// target flags, commands/stances/defaultCommand).
#[test]
fn unit_extended_field_surface() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "mod-mech",
        r#"{"name":"Mod Mech","type":"mech","health":500,
            "boostMultiplier":1.5,"payloadCapacity":64,"outlineRadius":4,
            "outlineColor":"ff0000","deathSound":"explosion","deathSoundVolume":0.5,
            "mineWalls":true,"hittable":false,"internal":true,
            "targetFlags":["core","storage"],"defaultCommand":"move",
            "commands":["move","repair"],"stances":["stop"]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-mod-mech").expect("unit");
    assert_eq!(unit.boost_multiplier, 1.5);
    assert_eq!(unit.payload_capacity, 64.0);
    assert_eq!(unit.outline_radius, 4);
    assert_eq!(
        unit.outline_color,
        crate::content::Rgba::new(1.0, 0.0, 0.0, 1.0)
    );
    assert_eq!(
        unit.death_sound,
        crate::content::registries::sound_meta::SoundId::by_name("explosion")
            .expect("explosion sound")
    );
    assert_eq!(unit.death_sound_volume, 0.5);
    assert!(unit.mine_walls);
    assert!(!unit.hittable);
    assert!(unit.internal);
    assert_eq!(
        unit.target_flags,
        vec![
            Some(crate::content::registries::blocks::BlockFlag::Core),
            Some(crate::content::registries::blocks::BlockFlag::Storage),
        ]
    );
    assert_eq!(unit.commands.len(), 2);
    assert_eq!(unit.stances.len(), 1);
    assert!(unit.default_command.is_some());
}

/// #59: inline `ParticleEffect`/`RadialEffect`/`SeqEffect`/`SoundEffect`/
/// `NoiseEffect` records parse through the `Effect` field parser
/// (`ContentParser.java:72-82`); the default inline class is
/// `ParticleEffect` when `type` is omitted.
#[test]
fn inline_effect_classes_parse() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "fx-mech",
        r#"{"name":"Fx Mech","type":"mech","weapons":[{"name":"fx-gun","bullet":{
            "type":"BasicBulletType","damage":1,
            "hitEffect":{"type":"ParticleEffect","particles":12,"region":"spark",
                         "colorFrom":"ff0000","cone":90,"lightColor":"00ff00",
                         "interp":"pow2In"},
            "despawnEffect":{"type":"RadialEffect","amount":3,"rotationSpacing":45,
                             "effect":{"type":"ParticleEffect","particles":2}},
            "shootEffect":{"type":"SeqEffect","effects":[
                {"type":"ParticleEffect","length":8},
                {"type":"SoundEffect","sound":"shoot","minPitch":0.5,
                 "effect":{"type":"NoiseEffect","layers":2,"color":"00ff00"}}]},
            "smokeEffect":{"particles":3}
        }}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-fx-mech").expect("unit");
    let bullet = reg
        .bullet(unit.weapons[0].bullet.id)
        .expect("bullet registered");
    let EffectRef::Inline(hit) = &bullet.hit_effect else {
        panic!("hitEffect not inline: {:?}", bullet.hit_effect);
    };
    assert_eq!(hit.kind, InlineEffectKind::ParticleEffect);
    assert_eq!(hit.particles, 12);
    assert_eq!(hit.region, "spark");
    assert_eq!(hit.cone, 90.0);
    assert_eq!(
        hit.particle_color_from,
        Some(crate::content::Rgba::new(1.0, 0.0, 0.0, 1.0))
    );
    assert_eq!(
        hit.light_color,
        Some(crate::content::Rgba::new(0.0, 1.0, 0.0, 1.0))
    );
    assert_eq!(hit.interp, InterpKind::Pow2In);
    assert!(hit.rand_length && hit.use_rotation && hit.cap);

    let EffectRef::Inline(radial) = &bullet.despawn_effect else {
        panic!("despawnEffect not inline");
    };
    assert_eq!(radial.kind, InlineEffectKind::RadialEffect);
    assert_eq!(radial.amount, 3);
    assert_eq!(radial.rotation_spacing, 45.0);
    assert_eq!(radial.clip, 100.0, "RadialEffect() sets clip 100");
    let EffectRef::Inline(radial_child) = &radial.children[0] else {
        panic!("radial child not inline");
    };
    assert_eq!(radial_child.particles, 2);

    let EffectRef::Inline(seq) = &bullet.shoot_effect else {
        panic!("shootEffect not inline");
    };
    assert_eq!(seq.kind, InlineEffectKind::SeqEffect);
    assert_eq!(seq.children.len(), 2);
    // `SeqEffect.init()`: lifetime is the sum of the children.
    assert_eq!(
        seq.lifetime,
        seq.children[0].lifetime() + seq.children[1].lifetime()
    );
    let EffectRef::Inline(sound) = &seq.children[1] else {
        panic!("sound child not inline");
    };
    assert_eq!(sound.kind, InlineEffectKind::SoundEffect);
    assert_eq!(sound.min_pitch, 0.5);
    assert!(sound.sound.is_some());
    let EffectRef::Inline(noise) = &sound.children[0] else {
        panic!("noise child not inline");
    };
    assert_eq!(noise.kind, InlineEffectKind::NoiseEffect);
    assert_eq!(noise.layers, 2);
    assert_eq!(noise.clip, f32::MAX, "NoiseEffect is never clipped");
    assert_eq!(
        noise.color,
        Some(crate::content::Rgba::new(0.0, 1.0, 0.0, 1.0))
    );

    // No `type` -> upstream's `ParticleEffect` default class.
    let EffectRef::Inline(smoke) = &bullet.smoke_effect else {
        panic!("smokeEffect not inline");
    };
    assert_eq!(smoke.kind, InlineEffectKind::ParticleEffect);
    assert_eq!(smoke.particles, 3);
}

/// #59: `BulletType.spawnUnit`/`despawnUnit` name a registered unit
/// (`ContentParser` field read), the despawn-unit knobs carry over, and a
/// string `bullet` value resolves a named `Bullets.<field>` reference.
#[test]
fn bullet_unit_refs_and_named_bullets() {
    use crate::content::registries::bullets::FIREBALL;
    let mut reg = registry();
    let (_, scout) = parse(
        &mut reg,
        ContentType::Unit,
        "scout",
        r#"{"name":"Scout","type":"mech","health":100,"speed":1.5,"lifetime":90}"#,
    );
    assert!(scout.is_ok(), "{scout:?}");
    let (_, spawner) = parse(
        &mut reg,
        ContentType::Unit,
        "spawner",
        r#"{"name":"Spawner","type":"mech","weapons":[
            {"name":"spawn-gun","bullet":{"type":"BasicBulletType","damage":1,
             "spawnUnit":"scout","despawnUnit":"scout","despawnUnitCount":3,
             "despawnUnitChance":0.5,"despawnUnitRadius":2,"faceOutwards":true}},
            {"name":"fire-gun","bullet":"fireball"}]}"#,
    );
    assert!(spawner.is_ok(), "{spawner:?}");
    let unit = reg.unit_by_name("mods-spawner").expect("unit");
    assert_eq!(unit.weapons.len(), 2);

    let spawn_bullet = reg.bullet(unit.weapons[0].bullet.id).expect("spawn bullet");
    let scout_id = reg.unit_id("mods-scout").expect("scout");
    assert_eq!(spawn_bullet.spawn_unit, Some(scout_id));
    assert_eq!(spawn_bullet.despawn_unit, Some(scout_id));
    assert_eq!(spawn_bullet.despawn_unit_count, 3);
    assert!((spawn_bullet.despawn_unit_chance - 0.5).abs() < 1e-6);
    assert!((spawn_bullet.despawn_unit_radius - 2.0).abs() < 1e-6);
    assert!(spawn_bullet.face_outwards);
    // `BulletType.range` for a spawn-unit bullet is `lifetime * speed`.
    let scout_def = reg.unit(scout_id).expect("scout def");
    assert_eq!(
        spawn_bullet.spawn_unit_range,
        Some((scout_def.lifetime, scout_def.speed))
    );

    assert_eq!(unit.weapons[1].bullet.id, FIREBALL);
}

#[test]
fn unit_abilities_nested() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "test-ability-mech",
        r#"{"name":"Test Ability Mech","type":"mech","health":100,
            "abilities":[
                {"type":"ShieldArcAbility","max":1000},
                {"type":"MoveEffectAbility","amount":10}
            ]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-test-ability-mech").expect("unit");
    assert_eq!(unit.abilities.len(), 2);
    assert_eq!(unit.abilities[0].kind.name(), "ShieldArcAbility");
    assert_eq!(unit.abilities[0].max, 1000.0);
    assert_eq!(unit.abilities[1].kind.name(), "MoveEffectAbility");
    assert_eq!(unit.abilities[1].amount, 10.0);
}

#[test]
fn weapon_shoot_pattern() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "test-shooter",
        r#"{"name":"Test Shooter","type":"mech","weapons":[
            {"name":"w","shoot":{"type":"ShootAlternate","spread":3.5},
             "bullet":{"type":"BasicBulletType","damage":1}}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-test-shooter").expect("unit");
    assert_eq!(unit.weapons.len(), 1);
    assert_eq!(unit.weapons[0].shoot.kind, ShootPatternKind::ShootAlternate);
    assert!((unit.weapons[0].shoot.spread - 3.5).abs() < 1e-6);
}

/// L08 T3: `PartProgress` JSON (`ContentParser.parseProgressOp`): the
/// constant-name form, the numeric `constant(...)` form and chained
/// `operation`/`operations` builders map onto the ported expression tree.
#[test]
fn draw_part_progress_expressions_parse() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "test-parts",
        r#"{"name":"Test Parts","type":"mech","weapons":[
            {"name":"w","parts":[
                {"type":"RegionPart","progress":{"type":"reload","operations":[
                    {"op":"inv"},{"op":"add","amount":0.5}]}},
                {"type":"ShapePart","progress":{"type":"warmup","operation":"sin","scl":10,"mag":2}},
                {"type":"HoverPart","progress":0.25}
            ],"bullet":{"type":"BasicBulletType","damage":1}}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-test-parts").expect("unit");
    let parts = &unit.weapons[0].parts;
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0].progress,
        PartProgressSpec::Add(
            Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
            0.5
        )
    );
    assert_eq!(
        parts[1].progress,
        PartProgressSpec::Sin(Box::new(PartProgressSpec::Warmup), 0.0, 10.0, 2.0)
    );
    assert_eq!(parts[2].progress, PartProgressSpec::Constant(0.25));
}

#[test]
fn bullet_frag_and_interval() {
    let mut reg = registry();
    let (_, result) = parse(
        &mut reg,
        ContentType::Unit,
        "test-frag",
        r#"{"name":"Test Frag","type":"mech","weapons":[
            {"name":"w","bullet":{"type":"BasicBulletType","damage":1,
                "frag":{"type":"BasicBulletType","damage":5},
                "intervalBullet":{"type":"BasicBulletType","damage":2}}}]}"#,
    );
    assert!(result.is_ok(), "{result:?}");
    let unit = reg.unit_by_name("mods-test-frag").expect("unit");
    let bullet_id = unit.weapons[0].bullet.id;
    let bullet = reg.bullet(bullet_id).expect("bullet");
    assert!(bullet.frag_bullet.is_some(), "frag registered");
    assert!(bullet.interval_bullet.is_some(), "interval registered");
    let frag = reg
        .bullet(bullet.frag_bullet.unwrap())
        .expect("frag bullet");
    assert_eq!(frag.damage, 5.0);
}

#[test]
fn null_field_refused() {
    let mut reg = registry();
    for folder in [
        ContentType::Item,
        ContentType::Block,
        ContentType::Liquid,
        ContentType::Status,
        ContentType::Unit,
        ContentType::Weather,
        ContentType::Sector,
        ContentType::Planet,
        ContentType::Team,
    ] {
        let (_, result) = parse(&mut reg, folder, "bad-null", r#"{"name":null}"#);
        let error = result.expect_err("null name refused");
        assert!(error.message.contains("cannot be null"), "{error}");
    }
}
