// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
//
// GENERATED — do not edit. Regenerate with `mind-tools mods classmap`
// (plan 20 §3.5/§6.7). Source: `Mindustry/core/src/mindustry/mod/ClassMap.java`
// plus the ported kind manifests; block aliases are derived at runtime from
// `BlockKind::ALL` so this table only carries the non-block scopes.

//! Committed alias table for the `ClassMap` replacement (plan 20 M2/§6.7).

use super::classmap::{ClassScope, ClassTag};

/// Committed `(alias, scope, tag)` entries (simple names; FQCN aliases are
/// produced by stripping the package in [`super::classmap::ClassTagMap::resolve`]).
pub static ENTRIES: &[ClassTag] = &[
    // ---- BulletType ----
    ClassTag::new("BulletType", ClassScope::BulletType, "BulletType"),
    ClassTag::new("BasicBulletType", ClassScope::BulletType, "BasicBulletType"),
    ClassTag::new("FireBulletType", ClassScope::BulletType, "FireBulletType"),
    ClassTag::new(
        "SpaceLiquidBulletType",
        ClassScope::BulletType,
        "SpaceLiquidBulletType",
    ),
    ClassTag::new(
        "ArtilleryBulletType",
        ClassScope::BulletType,
        "ArtilleryBulletType",
    ),
    ClassTag::new(
        "MissileBulletType",
        ClassScope::BulletType,
        "MissileBulletType",
    ),
    ClassTag::new(
        "LaserBoltBulletType",
        ClassScope::BulletType,
        "LaserBoltBulletType",
    ),
    ClassTag::new("SapBulletType", ClassScope::BulletType, "SapBulletType"),
    ClassTag::new(
        "LightningBulletType",
        ClassScope::BulletType,
        "LightningBulletType",
    ),
    ClassTag::new("LaserBulletType", ClassScope::BulletType, "LaserBulletType"),
    ClassTag::new("FlakBulletType", ClassScope::BulletType, "FlakBulletType"),
    ClassTag::new(
        "ExplosionBulletType",
        ClassScope::BulletType,
        "ExplosionBulletType",
    ),
    ClassTag::new("RailBulletType", ClassScope::BulletType, "RailBulletType"),
    ClassTag::new(
        "ContinuousLaserBulletType",
        ClassScope::BulletType,
        "ContinuousLaserBulletType",
    ),
    ClassTag::new(
        "ShrapnelBulletType",
        ClassScope::BulletType,
        "ShrapnelBulletType",
    ),
    ClassTag::new(
        "LiquidBulletType",
        ClassScope::BulletType,
        "LiquidBulletType",
    ),
    ClassTag::new("EmpBulletType", ClassScope::BulletType, "EmpBulletType"),
    ClassTag::new("BombBulletType", ClassScope::BulletType, "BombBulletType"),
    // ---- Effect ----
    ClassTag::new("Effect", ClassScope::Effect, "Effect"),
    ClassTag::new("MultiEffect", ClassScope::Effect, "MultiEffect"),
    ClassTag::new("ExplosionEffect", ClassScope::Effect, "ExplosionEffect"),
    ClassTag::new("WaveEffect", ClassScope::Effect, "WaveEffect"),
    ClassTag::new("WrapEffect", ClassScope::Effect, "WrapEffect"),
    // ---- ShootPattern ----
    ClassTag::new("ShootPattern", ClassScope::ShootPattern, "ShootPattern"),
    ClassTag::new("ShootAlternate", ClassScope::ShootPattern, "ShootAlternate"),
    ClassTag::new("ShootBarrel", ClassScope::ShootPattern, "ShootBarrel"),
    ClassTag::new("ShootHelix", ClassScope::ShootPattern, "ShootHelix"),
    ClassTag::new("ShootMulti", ClassScope::ShootPattern, "ShootMulti"),
    ClassTag::new("ShootSequence", ClassScope::ShootPattern, "ShootSequence"),
    ClassTag::new("ShootSpread", ClassScope::ShootPattern, "ShootSpread"),
    // ---- UnitController (controller) ----
    ClassTag::new("AssemblerAI", ClassScope::UnitController, "AssemblerAI"),
    ClassTag::new("BuilderAI", ClassScope::UnitController, "BuilderAI"),
    ClassTag::new("CargoAI", ClassScope::UnitController, "CargoAI"),
    ClassTag::new("CommandAI", ClassScope::UnitController, "CommandAI"),
    ClassTag::new("MissileAI", ClassScope::UnitController, "MissileAI"),
    ClassTag::new("NoAI", ClassScope::UnitController, "NoAI"),
    // ---- AiController ----
    ClassTag::new("DefenderAI", ClassScope::UnitController, "DefenderAI"),
    ClassTag::new(
        "FlyingFollowAI",
        ClassScope::UnitController,
        "FlyingFollowAI",
    ),
    ClassTag::new("HugAI", ClassScope::UnitController, "HugAI"),
    ClassTag::new("SuicideAI", ClassScope::UnitController, "SuicideAI"),
    // ---- UnitType ----
    ClassTag::new("UnitType", ClassScope::UnitType, "UnitType"),
    ClassTag::new("ErekirUnitType", ClassScope::UnitType, "ErekirUnitType"),
    ClassTag::new("TankUnitType", ClassScope::UnitType, "TankUnitType"),
    ClassTag::new("MissileUnitType", ClassScope::UnitType, "MissileUnitType"),
    ClassTag::new("NeoplasmUnitType", ClassScope::UnitType, "NeoplasmUnitType"),
    // ---- Weather ----
    ClassTag::new("Weather", ClassScope::Weather, "Weather"),
    ClassTag::new("ParticleWeather", ClassScope::Weather, "ParticleWeather"),
    ClassTag::new("RainWeather", ClassScope::Weather, "RainWeather"),
    ClassTag::new("MagneticStorm", ClassScope::Weather, "MagneticStorm"),
    ClassTag::new("SolarFlare", ClassScope::Weather, "SolarFlare"),
    // ---- Liquid ----
    ClassTag::new("Liquid", ClassScope::Liquid, "Liquid"),
    ClassTag::new("CellLiquid", ClassScope::Liquid, "CellLiquid"),
    // ---- Status / Item / Sector / Planet / Team ----
    ClassTag::new("StatusEffect", ClassScope::Status, "StatusEffect"),
    ClassTag::new("Item", ClassScope::Item, "Item"),
    ClassTag::new("SectorPreset", ClassScope::Sector, "SectorPreset"),
    ClassTag::new("Planet", ClassScope::Planet, "Planet"),
    ClassTag::new("TeamEntry", ClassScope::Team, "TeamEntry"),
    // ---- Ability ----
    ClassTag::new("Ability", ClassScope::Ability, "Ability"),
    ClassTag::new(
        "ForceFieldAbility",
        ClassScope::Ability,
        "ForceFieldAbility",
    ),
    ClassTag::new("RegenAbility", ClassScope::Ability, "RegenAbility"),
    ClassTag::new(
        "RepairFieldAbility",
        ClassScope::Ability,
        "RepairFieldAbility",
    ),
    ClassTag::new(
        "ShieldRegenFieldAbility",
        ClassScope::Ability,
        "ShieldRegenFieldAbility",
    ),
    ClassTag::new(
        "StatusFieldAbility",
        ClassScope::Ability,
        "StatusFieldAbility",
    ),
    ClassTag::new(
        "LiquidExplodeAbility",
        ClassScope::Ability,
        "LiquidExplodeAbility",
    ),
    ClassTag::new(
        "LiquidRegenAbility",
        ClassScope::Ability,
        "LiquidRegenAbility",
    ),
    ClassTag::new(
        "MoveLightningAbility",
        ClassScope::Ability,
        "MoveLightningAbility",
    ),
    ClassTag::new(
        "EnergyFieldAbility",
        ClassScope::Ability,
        "EnergyFieldAbility",
    ),
    ClassTag::new("UnitSpawnAbility", ClassScope::Ability, "UnitSpawnAbility"),
    ClassTag::new(
        "BuildSpeedAbility",
        ClassScope::Ability,
        "BuildSpeedAbility",
    ),
    ClassTag::new("MineSpeedAbility", ClassScope::Ability, "MineSpeedAbility"),
    ClassTag::new("HealAbility", ClassScope::Ability, "HealAbility"),
    ClassTag::new(
        "SpawnDeathAbility",
        ClassScope::Ability,
        "SpawnDeathAbility",
    ),
    ClassTag::new(
        "MoveEffectAbility",
        ClassScope::Ability,
        "MoveEffectAbility",
    ),
    // ---- DrawPart ----
    ClassTag::new("DrawRegion", ClassScope::DrawPart, "DrawRegion"),
    ClassTag::new("DrawRegionSpin", ClassScope::DrawPart, "DrawRegionSpin"),
    ClassTag::new("DrawTurret", ClassScope::DrawPart, "DrawTurret"),
    ClassTag::new("DrawWeapon", ClassScope::DrawPart, "DrawWeapon"),
    ClassTag::new("DrawSideRegion", ClassScope::DrawPart, "DrawSideRegion"),
    ClassTag::new("DrawGlowRegion", ClassScope::DrawPart, "DrawGlowRegion"),
    ClassTag::new("DrawArc", ClassScope::DrawPart, "DrawArc"),
    ClassTag::new("DrawBlurSpin", ClassScope::DrawPart, "DrawBlurSpin"),
    ClassTag::new("DrawParticle", ClassScope::DrawPart, "DrawParticle"),
    ClassTag::new("DrawShape", ClassScope::DrawPart, "DrawShape"),
    ClassTag::new("DrawSoftShadow", ClassScope::DrawPart, "DrawSoftShadow"),
    ClassTag::new("DrawWarmupRegion", ClassScope::DrawPart, "DrawWarmupRegion"),
];
