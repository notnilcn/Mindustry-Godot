// SPDX-License-Identifier: GPL-3.0-only

//! Consumption builders, target priorities and light defaults for generated block specs.
//! Upstream: core/src/mindustry/world/Block.java, core/src/mindustry/world/consumers/Consume.java

use super::*;

use crate::content::color::Rgba;

/// `TargetPriority.wall`.
pub const TARGET_PRIORITY_WALL: f32 = -3.0;
/// `TargetPriority.under`.
pub const TARGET_PRIORITY_UNDER: f32 = -2.0;
/// `TargetPriority.transport`.
pub const TARGET_PRIORITY_TRANSPORT: f32 = -1.0;
/// `TargetPriority.base`.
pub const TARGET_PRIORITY_BASE: f32 = 0.0;
/// `TargetPriority.turret`.
pub const TARGET_PRIORITY_TURRET: f32 = 1.0;
/// `TargetPriority.core`.
pub const TARGET_PRIORITY_CORE: f32 = 2.0;

/// `Block.underBullets` for a vanilla block (`Block.java:120`).
///
/// Class-level assignments in the Java sources (`Conveyor.java:47`,
/// `Duct.java:47`, `OverflowDuct.java:32`, `Junction.java:21`,
/// `ItemBridge.java:51`, `Sorter.java:27`, `Router.java:19`,
/// `OverflowGate.java:18`, `DuctRouter.java:37`, `DuctBridge.java:16`,
/// `DuctJunction.java:29`, `Conduit.java:51`, `PayloadConveyor.java:39`,
/// `BeamNode.java:45`, `TargetDummy.java:40`) plus the per-instance
/// `underBullets = true` initializers in `Blocks.java:2234-2487`
/// (`duct-unloader`, `surge-router`, `liquid-router`,
/// `reinforced-liquid-router`, `reinforced-liquid-junction`,
/// `reinforced-bridge-conduit`, `power-node`) and `:6642-6651`.
pub fn block_under_bullets(kind: BlockKind, name: &str) -> bool {
    matches!(
        kind,
        BlockKind::TargetDummy
            | BlockKind::Conveyor
            | BlockKind::ArmoredConveyor
            | BlockKind::DuctBridge
            | BlockKind::Duct
            | BlockKind::DuctRouter
            | BlockKind::ItemBridge
            | BlockKind::BufferedItemBridge
            | BlockKind::Junction
            | BlockKind::OverflowDuct
            | BlockKind::OverflowGate
            | BlockKind::Router
            | BlockKind::Sorter
            | BlockKind::StackConveyor
            | BlockKind::Conduit
            | BlockKind::ArmoredConduit
            | BlockKind::PayloadConveyor
            | BlockKind::PayloadRouter
            | BlockKind::BeamNode
    ) || matches!(
        name,
        "duct-unloader"
            | "surge-router"
            | "liquid-router"
            | "reinforced-liquid-router"
            | "reinforced-liquid-junction"
            | "reinforced-bridge-conduit"
            | "power-node"
    )
}

/// Builds a spec with the class defaults for `kind` (`BlockSpec::for_kind`).
pub fn spec(name: &'static str, kind: BlockKind) -> BlockSpec {
    let mut spec = BlockSpec::for_kind(kind);
    spec.name = name;
    spec
}

/// `Color.cyan` (`arc.graphics.Color.cyan` = `#00ffff`).
pub(super) const COLOR_CYAN: Rgba = Rgba::new(0.0, 1.0, 1.0, 1.0);
/// `Color.orange` (`arc.graphics.Color.orange` = `#ffa500`).
pub(super) const COLOR_ORANGE: Rgba = Rgba::new(1.0, 165.0 / 255.0, 0.0, 1.0);
/// `Pal.accent` (`Pal.java`: `#ffd37f`).
pub(super) const PAL_ACCENT: Rgba = Rgba::new(1.0, 211.0 / 255.0, 127.0 / 255.0, 1.0);
/// `Team.crux.color` (`Team.java:35`: `#f25555`).
pub(super) const TEAM_CRUX: Rgba = Rgba::new(242.0 / 255.0, 85.0 / 255.0, 85.0 / 255.0, 1.0);

/// `Blocks.java` environment-light overrides for floors (the only non-building
/// `emitLight` blocks): returns `(radius, colour)` with Java's alpha.
///
/// * `pooled-cryofluid` — `Blocks.java:311-315`: 25, cyan `.a(0.19)`.
/// * `molten-slag` — `Blocks.java:329-333`: 40, orange `.a(0.38)`.
/// * `hotrock` — `Blocks.java:369-371`: 30, orange `.a(0.15)`.
/// * `magmarock` — `Blocks.java:379-381`: 50, orange `.a(0.3)`.
/// * `metal-tiles-6` / `metal-tiles-12` — `Blocks.java:876-879,920-922`: 30,
///   `Team.crux.color.a(0.3)`.
pub fn java_floor_light(name: &str) -> Option<(f32, Rgba)> {
    let color = |base: Rgba, alpha: f32| Rgba::new(base.r, base.g, base.b, alpha);
    Some(match name {
        "pooled-cryofluid" => (25.0, color(COLOR_CYAN, 0.19)),
        "molten-slag" => (40.0, color(COLOR_ORANGE, 0.38)),
        "hotrock" => (30.0, color(COLOR_ORANGE, 0.15)),
        "magmarock" => (50.0, color(COLOR_ORANGE, 0.3)),
        "metal-tiles-6" | "metal-tiles-12" => (30.0, color(TEAM_CRUX, 0.3)),
        _ => return None,
    })
}

/// `Block.lightRadius`/`lightColor` defaults: `Block.java:344,350` (white, 60)
/// plus the block-class and `Blocks.java` overrides (PAR-0467).
///
/// Class overrides:
/// * `ConsumeGenerator.java:69,25`: `lightRadius = baseLightRadius(65) * size`.
/// * `ImpactReactor.java:30`: 115.
/// * `LandingPad.java:71`: 90 (white; the coloured draw light is
///   `LandingPad.drawLight`'s `Pal.accent`, not the environment light).
/// * `Accelerator.java:79-80`: 70, `Pal.accent`.
/// * `MendProjector.java:45` / `OverdriveProjector.java:42`: 50.
/// * `CoreBlock.java:158`: `30 + 20 * size`.
/// * `LightBlock.java:40`: `radius * 2.5` (`illuminator` radius 140 ->
///   350; the per-tick colour comes from `LightState`).
///
/// `NuclearReactor` keeps the base 60/white environment light: its `#7f19ea`
/// radius-90 light is the `NuclearReactor.draw` heat light
/// (`NuclearReactor.java:178`), a custom draw light rather than
/// `drawEnvironmentLight`.
pub fn java_light_defaults(spec: &BlockSpec) -> (f32, Rgba) {
    if let Some((radius, color)) = java_floor_light(spec.name) {
        return (radius, color);
    }
    let size = spec.size.unwrap_or(1) as f32;
    match spec.kind {
        BlockKind::ConsumeGenerator => (65.0 * size, Rgba::WHITE),
        BlockKind::ImpactReactor => (115.0, Rgba::WHITE),
        BlockKind::LandingPad => (90.0, Rgba::WHITE),
        BlockKind::Accelerator => (70.0, PAL_ACCENT),
        BlockKind::MendProjector | BlockKind::OverdriveProjector => (50.0, Rgba::WHITE),
        BlockKind::CoreBlock => (30.0 + 20.0 * size, Rgba::WHITE),
        BlockKind::LightBlock => (350.0, Rgba::WHITE),
        _ => (60.0, Rgba::WHITE),
    }
}

/// `consumeItem`/`consumeItems` helper for generated waves.
pub fn consume_item(item: &'static str, amount: i32) -> ConsumeDef {
    ConsumeDef::Items {
        stacks: vec![StackSpec { item, amount }],
        optional: false,
        ignore: false,
        update: true,
    }
}

/// `consumeItems` helper for generated waves.
pub fn consume_items(stacks: Vec<StackSpec>) -> ConsumeDef {
    ConsumeDef::Items {
        stacks,
        optional: false,
        ignore: false,
        update: true,
    }
}

/// `consumeLiquid` helper for generated waves.
pub fn consume_liquid(liquid: &'static str, amount: f32) -> ConsumeDef {
    ConsumeDef::Liquid {
        liquid,
        amount,
        optional: false,
        update: true,
    }
}

/// `consumeLiquids` helper for generated waves.
pub fn consume_liquids(stacks: Vec<LiquidStackSpec>) -> ConsumeDef {
    ConsumeDef::Liquids {
        stacks,
        optional: false,
        update: true,
    }
}

/// `consumePower` helper for generated waves.
pub fn consume_power(usage: f32) -> ConsumeDef {
    ConsumeDef::Power {
        usage,
        buffered: 0.0,
        update: true,
    }
}

/// `consumePowerBuffered` helper for generated waves.
pub fn consume_power_buffered(capacity: f32) -> ConsumeDef {
    ConsumeDef::Power {
        usage: 0.0,
        buffered: capacity,
        update: true,
    }
}

/// `consumeCoolant` helper for generated waves.
pub fn consume_coolant(amount: f32, allow_liquid: bool, allow_gas: bool) -> ConsumeDef {
    ConsumeDef::Coolant {
        amount,
        allow_liquid,
        allow_gas,
        optional: false,
        update: true,
    }
}

/// `consume*` optional wrapper (`.optional(...)`/`.boost()`).
pub fn consume_optional(mut consume: ConsumeDef) -> ConsumeDef {
    match &mut consume {
        ConsumeDef::Items { optional, .. }
        | ConsumeDef::Liquid { optional, .. }
        | ConsumeDef::Liquids { optional, .. }
        | ConsumeDef::Coolant { optional, .. } => *optional = true,
        ConsumeDef::Power { .. } => {}
    }
    consume
}

/// `Color.valueOf(hex)` helper for generated waves.
pub fn rgba_hex(hex: &str) -> Rgba {
    Rgba::from_hex(hex).unwrap_or(Rgba::WHITE)
}
