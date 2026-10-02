// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Ported `ConsumeGeneratorTests` (`tests/src/test/java/power/ConsumeGeneratorTests.java`).
//!
//! Plan 07 does not carry generator knobs, so the fixture attaches plan-09
//! [`GeneratorConfig`]/[`GeneratorState`] directly. `Time.delta` becomes the
//! explicit `delta` argument (deviation §2.3.1); all dynamic cases are retained.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::entities::comp::{Building, TeamComp};
use crate::world::TilePos;
use crate::world::blocks::power::generator::{
    GeneratorConfig, GeneratorFilter, GeneratorState, update_generator,
};
use crate::world::blocks::power::{PowerNodeInfo, PowerProduction};
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};

const ITEM_DURATION: f32 = 60.0;
const MAX_LIQUID_USAGE: f32 = 1.0;

struct GeneratorFixture {
    world: World,
    content: ContentRegistry,
    entity: Entity,
}

impl GeneratorFixture {
    fn new(config: GeneratorConfig) -> Self {
        let content = crate::content::test_support::test_registry();
        let mut world = World::new();
        let entity = world
            .spawn((
                Building::new(TilePos::new(0, 0), BlockId::AIR, 0),
                TeamComp { team: 0 },
                PowerModule::new(),
                PowerProduction(0.0),
                PowerNodeInfo::producer(),
                ItemModule::with_items(content.items().len()),
                LiquidModule::with_liquids(content.liquids().len()),
                config,
                GeneratorState::new(),
            ))
            .id();
        Self {
            world,
            content,
            entity,
        }
    }

    fn update(&mut self, delta: f32) {
        update_generator(&mut self.world, self.entity, delta, &self.content);
    }

    fn state(&self) -> GeneratorState {
        self.world
            .get::<GeneratorState>(self.entity)
            .copied()
            .expect("state")
    }

    fn efficiency(&self) -> f32 {
        self.world
            .get::<Building>(self.entity)
            .map(|building| building.efficiency)
            .unwrap_or(0.0)
    }

    fn add_liquid(&mut self, liquid: crate::content::LiquidId, amount: f32) {
        let mut module = self
            .world
            .get_mut::<LiquidModule>(self.entity)
            .expect("liquid module");
        module.add(liquid, amount, 100.0);
    }

    fn liquid(&self, liquid: crate::content::LiquidId) -> f32 {
        self.world
            .get::<LiquidModule>(self.entity)
            .map(|module| module.get(liquid))
            .unwrap_or(0.0)
    }

    fn add_item(&mut self, item: crate::content::ItemId, amount: i32) {
        let mut module = self
            .world
            .get_mut::<ItemModule>(self.entity)
            .expect("item module");
        module.add(item, amount, 100);
    }

    fn item(&self, item: crate::content::ItemId) -> i32 {
        self.world
            .get::<ItemModule>(self.entity)
            .map(|module| module.get(item))
            .unwrap_or(0)
    }
}

fn liquid_config() -> GeneratorConfig {
    GeneratorConfig {
        power_production: 0.1,
        item_duration: ITEM_DURATION,
        liquid_capacity: 100.0,
        filter_liquid: Some(GeneratorFilter::LiquidFlammable {
            amount: MAX_LIQUID_USAGE,
            min: 0.2,
        }),
        ..GeneratorConfig::default()
    }
}

fn item_config() -> GeneratorConfig {
    GeneratorConfig {
        power_production: 0.1,
        item_duration: ITEM_DURATION,
        filter_item: Some(GeneratorFilter::ItemFlammable { min: 0.2 }),
        ..GeneratorConfig::default()
    }
}

fn assert_close(actual: f32, expected: f32, context: &str) {
    assert!(
        (actual - expected).abs() < 0.000_01,
        "{context}: expected {expected}, got {actual}"
    );
}

/// `ConsumeGeneratorTests.generatorWorksProperlyWithLiquidInput` (deltas 2/1/0.5
/// × 4 amounts).
#[test]
fn liquid_input_deltas() {
    let oil = {
        let content = crate::content::test_support::test_registry();
        content.liquid_id("oil").expect("oil")
    };
    let flammability = {
        let content = crate::content::test_support::test_registry();
        content.liquid(oil).expect("oil def").flammability
    };
    for delta in [2.0f32, 1.0, 0.5] {
        for available in [
            0.0f32,
            MAX_LIQUID_USAGE / 4.0,
            MAX_LIQUID_USAGE,
            MAX_LIQUID_USAGE * 2.0,
        ] {
            let mut fixture = GeneratorFixture::new(liquid_config());
            fixture.add_liquid(oil, available);
            fixture.update(delta);

            let expected_consumption = (MAX_LIQUID_USAGE * delta).min(available);
            let expected_efficiency = expected_consumption / (MAX_LIQUID_USAGE * delta);
            let expected_output = expected_efficiency * flammability;
            let expected_remaining = (available - expected_consumption).max(0.0);

            assert_close(fixture.efficiency(), expected_efficiency, "base efficiency");
            assert_close(fixture.liquid(oil), expected_remaining, "remaining liquid");
            assert_close(
                fixture.state().production_efficiency,
                expected_output,
                "production efficiency",
            );
        }
    }
}

/// `ConsumeGeneratorTests.generatorWorksProperlyWithItemInput`.
#[test]
fn item_flammability_inputs() {
    let content = crate::content::test_support::test_registry();
    let cases = [
        ("coal", 3),
        ("blast-compound", 1),
        ("spore-pod", 1),
        ("pyratite", 1),
    ];
    for (name, amount) in cases {
        for supplied in [0, amount] {
            let item = content.item_id(name).expect("item");
            let flammability = content.item(item).expect("item def").flammability;
            let mut fixture = GeneratorFixture::new(item_config());
            fixture.add_item(item, supplied);
            fixture.update(0.5);

            let expected_efficiency = if supplied > 0 { flammability } else { 0.0 };
            let expected_remaining = (supplied - 1).max(0);
            assert_close(
                fixture.state().production_efficiency,
                expected_efficiency,
                name,
            );
            assert_eq!(fixture.item(item), expected_remaining, "{name} remaining");
        }
    }
}

/// `ConsumeGeneratorTests.efficiencyRemainsConstantWithinItemDuration_ItemsOnly`.
#[test]
fn efficiency_constant_within_item_duration() {
    let content = crate::content::test_support::test_registry();
    let coal = content.item_id("coal").expect("coal");
    let mut fixture = GeneratorFixture::new(item_config());
    fixture.add_item(coal, 1);
    fixture.update(0.5);
    let expected = fixture.state().production_efficiency;

    let mut current = 0.0f32;
    while {
        current += 0.5;
        current <= ITEM_DURATION
    } {
        fixture.update(0.5);
        assert_close(
            fixture.state().production_efficiency,
            expected,
            "within duration",
        );
    }
    fixture.update(0.5);
    assert_close(fixture.state().production_efficiency, 0.0, "after duration");
}

/// `PowerGenerator.getPowerProduction` (plain generator, no filter).
#[test]
fn plain_generator_production_tracks_efficiency() {
    let config = GeneratorConfig {
        power_production: 2.0,
        ..GeneratorConfig::default()
    };
    let mut fixture = GeneratorFixture::new(config);
    // No filter: `efficiency = enabled ? 1 : 0`.
    fixture.update(1.0);
    let state = fixture.state();
    assert_close(state.production_efficiency, 1.0, "efficiency");
    let production = fixture
        .world
        .get::<PowerProduction>(fixture.entity)
        .map(|production| production.0)
        .unwrap_or(0.0);
    assert_close(production, 2.0, "production");
}
