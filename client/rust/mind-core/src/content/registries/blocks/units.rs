// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `units`).
//
//! Generated block metadata (`Blocks.java` region `units`).
//! Regenerate with `parity/tools/gen_blocks.py` after upstream content changes.

#![allow(unused_imports)]

use super::{
    BlockFlag, BlockKind, BlockSink, BlockSpec, BuildVisibility, assembler_plan, liquid_stack,
    payload_block, payload_unit, spec, stack, unit_plan,
};
use super::{
    BlockGroup, EnvFlag, EnvMask, TARGET_PRIORITY_BASE, TARGET_PRIORITY_CORE,
    TARGET_PRIORITY_TRANSPORT, TARGET_PRIORITY_TURRET, TARGET_PRIORITY_UNDER, TARGET_PRIORITY_WALL,
};
use super::{
    consume_coolant, consume_item, consume_items, consume_liquid, consume_liquids,
    consume_optional, consume_power, consume_power_buffered, rgba_hex,
};
use crate::content::{Category, ContentError};

/// Loads the `units` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("copper", 50),
            stack("lead", 120),
            stack("silicon", 80),
        ],
        research_cost_multiplier: Some(0.5f32),
        group: Some(BlockGroup::Units),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        unit_plans: vec![
            unit_plan(
                "dagger",
                9e+02f32,
                vec![stack("silicon", 10), stack("lead", 10)],
            ),
            unit_plan(
                "crawler",
                6e+02f32,
                vec![stack("silicon", 8), stack("coal", 10)],
            ),
            unit_plan(
                "nova",
                2.4e+03f32,
                vec![
                    stack("silicon", 30),
                    stack("lead", 20),
                    stack("titanium", 20),
                ],
            ),
        ],
        consumes: vec![consume_power(1.2f32)],
        ..spec("ground-factory", BlockKind::UnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![stack("copper", 60), stack("lead", 70), stack("silicon", 60)],
        research_cost_multiplier: Some(0.5f32),
        group: Some(BlockGroup::Units),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        unit_plans: vec![
            unit_plan("flare", 9e+02f32, vec![stack("silicon", 15)]),
            unit_plan(
                "mono",
                2.1e+03f32,
                vec![stack("silicon", 30), stack("lead", 15)],
            ),
        ],
        consumes: vec![consume_power(1.2f32)],
        ..spec("air-factory", BlockKind::UnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("copper", 150),
            stack("lead", 130),
            stack("metaglass", 120),
        ],
        group: Some(BlockGroup::Units),
        has_items: Some(true),
        has_power: Some(true),
        solid: Some(true),
        floating: Some(true),
        update: Some(true),
        configurable: Some(true),
        unit_plans: vec![
            unit_plan(
                "risso",
                2.7e+03f32,
                vec![stack("silicon", 20), stack("metaglass", 35)],
            ),
            unit_plan(
                "retusa",
                2.1e+03f32,
                vec![stack("silicon", 15), stack("titanium", 20)],
            ),
        ],
        consumes: vec![consume_power(1.2f32)],
        ..spec("naval-factory", BlockKind::UnitFactory)
    })?;

    sink.push(BlockSpec {
        size: Some(3),
        category: Some(Category::Units),
        requirements: vec![
            stack("copper", 200),
            stack("lead", 120),
            stack("silicon", 90),
        ],
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![
            ("nova", "pulsar"),
            ("dagger", "mace"),
            ("crawler", "atrax"),
            ("flare", "horizon"),
            ("mono", "poly"),
            ("risso", "minke"),
            ("retusa", "oxynoe"),
        ],
        consumes: vec![
            consume_power(3.0f32),
            consume_items(vec![stack("silicon", 40), stack("graphite", 40)]),
        ],
        ..spec("additive-reconstructor", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(5),
        category: Some(Category::Units),
        requirements: vec![
            stack("lead", 650),
            stack("silicon", 450),
            stack("titanium", 350),
            stack("thorium", 650),
        ],
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![
            ("horizon", "zenith"),
            ("mace", "fortress"),
            ("poly", "mega"),
            ("minke", "bryde"),
            ("pulsar", "quasar"),
            ("atrax", "spiroct"),
            ("oxynoe", "cyerce"),
        ],
        consumes: vec![
            consume_power(6.0f32),
            consume_items(vec![
                stack("silicon", 130),
                stack("titanium", 80),
                stack("metaglass", 40),
            ]),
        ],
        ..spec("multiplicative-reconstructor", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(7),
        category: Some(Category::Units),
        requirements: vec![
            stack("lead", 2000),
            stack("silicon", 1000),
            stack("titanium", 2000),
            stack("thorium", 750),
            stack("plastanium", 450),
            stack("phase-fabric", 600),
        ],
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![
            ("zenith", "antumbra"),
            ("spiroct", "arkyid"),
            ("fortress", "scepter"),
            ("bryde", "sei"),
            ("mega", "quad"),
            ("quasar", "vela"),
            ("cyerce", "aegires"),
        ],
        consumes: vec![
            consume_power(13.0f32),
            consume_items(vec![
                stack("silicon", 850),
                stack("titanium", 750),
                stack("plastanium", 650),
            ]),
            consume_liquid("cryofluid", 1.0f32),
        ],
        ..spec("exponential-reconstructor", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        size: Some(9),
        category: Some(Category::Units),
        requirements: vec![
            stack("lead", 4000),
            stack("silicon", 3000),
            stack("thorium", 1000),
            stack("plastanium", 600),
            stack("phase-fabric", 600),
            stack("surge-alloy", 800),
        ],
        group: Some(BlockGroup::Units),
        solid: Some(true),
        update: Some(true),
        configurable: Some(true),
        upgrades: vec![
            ("antumbra", "eclipse"),
            ("arkyid", "toxopid"),
            ("scepter", "reign"),
            ("sei", "omura"),
            ("quad", "oct"),
            ("vela", "corvus"),
            ("aegires", "navanax"),
        ],
        consumes: vec![
            consume_power(25.0f32),
            consume_items(vec![
                stack("silicon", 1000),
                stack("plastanium", 600),
                stack("surge-alloy", 500),
                stack("phase-fabric", 350),
            ]),
            consume_liquid("cryofluid", 3.0f32),
        ],
        ..spec("tetrative-reconstructor", BlockKind::Reconstructor)
    })?;

    sink.push(BlockSpec {
        category: Some(Category::Units),
        requirements: vec![stack("lead", 30), stack("copper", 30), stack("silicon", 20)],
        group: Some(BlockGroup::Projectors),
        flags: vec![BlockFlag::Repair],
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("repair-point", BlockKind::RepairTurret)
    })?;

    sink.push(BlockSpec {
        size: Some(2),
        category: Some(Category::Units),
        requirements: vec![
            stack("silicon", 90),
            stack("thorium", 80),
            stack("plastanium", 60),
        ],
        group: Some(BlockGroup::Projectors),
        flags: vec![BlockFlag::Repair],
        has_power: Some(true),
        solid: Some(true),
        update: Some(true),
        ..spec("repair-turret", BlockKind::RepairTurret)
    })?;
    let _ = sink;
    Ok(())
}
