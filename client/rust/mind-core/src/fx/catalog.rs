// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The vanilla `Fx.java` catalogue (267 entries, `Effect.all` order).
//!
//! Names/lifetimes/clips come verbatim from the plan-02 seed table
//! ([`crate::content::registries::fx_meta::EFFECTS`]); this module classifies
//! each entry into an [`EffectKind`] and records layer overrides. Entries not
//! yet ported are [`EffectKind::Unported`] and tracked by the `fx` ledger/audit
//! (`mind-headless fx audit`). Catalogue waves F1–F6 fill these in incrementally
//! without ever renumbering ids (append-only ABI).

use crate::content::EffectId;
use crate::content::registries::fx_meta::EFFECTS;
use crate::render::layer::Layer;

use super::custom::CustomFxId;
use super::def::{CustomParams, EffectDef, EffectKind};

/// Classifies one upstream field name.
fn kind_for(name: &str) -> EffectKind {
    let custom = |id| EffectKind::Custom(id, CustomParams::default());
    match name {
        "none" => EffectKind::None,
        "blockCrash" => custom(CustomFxId::BlockCrash),
        "trailFade" => custom(CustomFxId::TrailFade),
        "unitSpawn" => custom(CustomFxId::UnitSpawn),
        "unitControl" => custom(CustomFxId::UnitControl),
        "unitDespawn" => custom(CustomFxId::UnitDespawn),
        "unitSpirit" => custom(CustomFxId::UnitSpirit),
        "itemTransfer" => custom(CustomFxId::ItemTransfer),
        "pointBeam" => custom(CustomFxId::PointBeam),
        "pointHit" => custom(CustomFxId::PointHit),
        "hitScepterSecondary" => custom(CustomFxId::HitScepterSecondary),
        "lightning" => custom(CustomFxId::Lightning),
        "coreBuildShockwave" => custom(CustomFxId::CoreBuildShockwave),
        "coreBuildBlock" => custom(CustomFxId::CoreBuildBlock),
        "pointShockwave" => custom(CustomFxId::PointShockwave),
        "moveCommand" => custom(CustomFxId::MoveCommand),
        "attackCommand" => custom(CustomFxId::AttackCommand),
        "placeBlock" => custom(CustomFxId::PlaceBlock),
        "tapBlock" => custom(CustomFxId::TapBlock),
        "breakBlock" => custom(CustomFxId::BreakBlock),
        "payloadDeposit" => custom(CustomFxId::PayloadDeposit),
        "select" => custom(CustomFxId::Select),
        "hitBulletSmall" => custom(CustomFxId::HitBulletSmall),
        "hitBulletColor" => custom(CustomFxId::HitBulletColor),
        "hitBulletBig" => custom(CustomFxId::HitBulletBig),
        "hitFlameSmall" => custom(CustomFxId::HitFlameSmall),
        "hitLiquid" => custom(CustomFxId::HitLiquid),
        "shootSmall" => custom(CustomFxId::ShootSmall),
        "shootBig" => custom(CustomFxId::ShootBig),
        "casing1" => custom(CustomFxId::Casing1),
        "healWave" => custom(CustomFxId::HealWave),
        "shockwave" => custom(CustomFxId::Shockwave),
        "smoke" => custom(CustomFxId::Smoke),
        "explosion" => custom(CustomFxId::Explosion),
        "hitLaser" => custom(CustomFxId::HitLaser),
        "commandSend" => custom(CustomFxId::CommandSend),
        "upgradeCoreBloom" => custom(CustomFxId::UpgradeCoreBloom),
        "coreLaunchConstruct" => custom(CustomFxId::CoreLaunchConstruct),
        "fallSmoke" => custom(CustomFxId::FallSmoke),
        "rocketSmoke" => custom(CustomFxId::RocketSmoke),
        "rocketSmokeLarge" => custom(CustomFxId::RocketSmokeLarge),
        "magmasmoke" => custom(CustomFxId::MagmaSmoke),
        "spawn" => custom(CustomFxId::Spawn),
        "padlaunch" => custom(CustomFxId::Padlaunch),
        "breakProp" => custom(CustomFxId::BreakProp),
        "unitDrop" => custom(CustomFxId::UnitDrop),
        "unitLand" => custom(CustomFxId::UnitLand),
        "unitDust" => custom(CustomFxId::UnitDust),
        "unitLandSmall" => custom(CustomFxId::UnitLandSmall),
        "unitPickup" => custom(CustomFxId::UnitPickup),
        "crawlDust" => custom(CustomFxId::CrawlDust),
        "landShock" => custom(CustomFxId::LandShock),
        "pickup" => custom(CustomFxId::Pickup),
        "sparkExplosion" => custom(CustomFxId::SparkExplosion),
        "titanExplosion" => custom(CustomFxId::TitanExplosion),
        "titanExplosionLarge" => custom(CustomFxId::TitanExplosionLarge),
        "titanExplosionSmall" => custom(CustomFxId::TitanExplosionSmall),
        "titanExplosionFrag" => custom(CustomFxId::TitanExplosionFrag),
        "coreExplosion" => custom(CustomFxId::CoreExplosion),
        "smokeAoeCloud" => custom(CustomFxId::SmokeAoeCloud),
        "scatheExplosion" => custom(CustomFxId::ScatheExplosion),
        "scatheExplosionSmall" => custom(CustomFxId::ScatheExplosionSmall),
        "scatheLight" => custom(CustomFxId::ScatheLight),
        "scatheLightSmall" => custom(CustomFxId::ScatheLightSmall),
        "titanLightSmall" => custom(CustomFxId::TitanLightSmall),
        "scatheSlash" => custom(CustomFxId::ScatheSlash),
        "dynamicSpikes" => custom(CustomFxId::DynamicSpikes),
        "greenBomb" => custom(CustomFxId::GreenBomb),
        "greenLaserCharge" => custom(CustomFxId::GreenLaserCharge),
        "greenLaserChargeSmall" => custom(CustomFxId::GreenLaserChargeSmall),
        "greenCloud" => custom(CustomFxId::GreenCloud),
        "healWaveDynamic" => custom(CustomFxId::HealWaveDynamic),
        "heal" => custom(CustomFxId::Heal),
        "dynamicWave" => custom(CustomFxId::DynamicWave),
        "shieldWave" => custom(CustomFxId::ShieldWave),
        "shieldApply" => custom(CustomFxId::ShieldApply),
        "hitSquaresColor" => custom(CustomFxId::HitSquaresColor),
        "hitFuse" => custom(CustomFxId::HitFuse),
        "hitFlamePlasma" => custom(CustomFxId::HitFlamePlasma),
        "hitLaserBlast" => custom(CustomFxId::HitLaserBlast),
        "hitEmpSpark" => custom(CustomFxId::HitEmpSpark),
        "hitLancer" => custom(CustomFxId::HitLancer),
        "hitLancerLow" => custom(CustomFxId::HitLancerLow),
        "hitBeam" => custom(CustomFxId::HitBeam),
        "hitFlameBeam" => custom(CustomFxId::HitFlameBeam),
        "hitMeltdown" => custom(CustomFxId::HitMeltdown),
        "hitMeltHeal" => custom(CustomFxId::HitMeltHeal),
        "instBomb" => custom(CustomFxId::InstBomb),
        "instTrail" => custom(CustomFxId::InstTrail),
        "instShoot" => custom(CustomFxId::InstShoot),
        "instHit" => custom(CustomFxId::InstHit),
        _ => EffectKind::Unported,
    }
}

/// `Effect.layer(...)` overrides for the catalogue.
fn layer_for(name: &str) -> f32 {
    match name {
        "hitScepterSecondary" => Layer::Bullet.z() - 1.0,
        "coreBuildBlock" | "upgradeCore" => Layer::Turret.z() - 5.0,
        "moveCommand" | "attackCommand" | "commandSend" => Layer::OverlayUi.z(),
        "payloadDeposit" => Layer::FlyingUnitLow.z() - 5.0,
        "breakProp" | "unitDrop" | "unitLand" | "unitDust" | "unitLandSmall" | "unitPickup"
        | "crawlDust" | "landShock" => Layer::Debris.z(),
        "casing1" | "casing2" | "casing3" | "casing4" | "casing2Double" | "casing3Double" => {
            Layer::Bullet.z()
        }
        "shootScepterSecondary" => Layer::Effect.z() + 1.0,
        "unitAssemble" => Layer::FlyingUnit.z() + 5.0,
        "scatheLight" | "scatheLightSmall" | "titanLightSmall" => Layer::Bullet.z() + 2.0,
        _ => Layer::Effect.z(),
    }
}

/// Builds the registry in `Effect.all` declaration order.
pub fn build_registry() -> Vec<EffectDef> {
    EFFECTS
        .iter()
        .enumerate()
        .map(|(index, meta)| {
            let id = EffectId(index as u16);
            let mut def = EffectDef::blank(id, meta.name, meta.lifetime, meta.clip);
            def.kind = kind_for(meta.name);
            def.layer = layer_for(meta.name);
            def
        })
        .collect()
}

/// Counts of catalogue classification (used by `fx audit`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CatalogCounts {
    /// Number of `EffectKind::None` entries.
    pub none: usize,
    /// Number of declarative kinds (Particle/Explosion/…).
    pub declarative: usize,
    /// Number of composites (Multi/Seq/Radial/Wrap/Sound).
    pub composite: usize,
    /// Number of custom bodies.
    pub custom: usize,
    /// Number of unported entries.
    pub unported: usize,
}

impl CatalogCounts {
    /// Total classified entries.
    pub fn total(&self) -> usize {
        self.none + self.declarative + self.composite + self.custom + self.unported
    }
}

/// Tallies the current catalogue classification.
pub fn counts(registry: &super::def::EffectRegistry) -> CatalogCounts {
    let mut counts = CatalogCounts::default();
    for def in registry.iter() {
        match &def.kind {
            EffectKind::None => counts.none += 1,
            EffectKind::Particle(_)
            | EffectKind::Explosion(_)
            | EffectKind::Wave(_)
            | EffectKind::Triangle(_)
            | EffectKind::Noise(_) => counts.declarative += 1,
            EffectKind::Multi(_)
            | EffectKind::Seq(_)
            | EffectKind::Radial(_)
            | EffectKind::Wrap(_)
            | EffectKind::Sound(_) => counts.composite += 1,
            EffectKind::Custom(..) => counts.custom += 1,
            EffectKind::Unported => counts.unported += 1,
        }
    }
    counts
}

/// `fnv1a` hash of the id/name order, for the `fx_order.txt` drift gate.
pub fn order_hash(registry: &super::def::EffectRegistry) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for def in registry.iter() {
        for &b in def.name.as_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        hash ^= 10;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::def::registry;

    #[test]
    fn catalog_builds_all_entries() {
        let defs = build_registry();
        assert_eq!(defs.len(), crate::content::EFFECT_COUNT);
        assert_eq!(defs[0].name, "none");
        assert_eq!(defs[266].name, "debugRect");
    }

    #[test]
    fn counts_total_matches() {
        let reg = registry();
        let c = counts(reg);
        assert_eq!(c.total(), crate::content::EFFECT_COUNT);
        assert_eq!(c.none, 1);
        assert!(c.custom >= 30);
        // Ids are dense and named consistently.
        for (i, def) in build_registry().iter().enumerate() {
            assert_eq!(def.id.raw() as usize, i);
            assert!(!def.name.is_empty());
        }
    }

    #[test]
    fn order_hash_is_stable() {
        let a = order_hash(registry());
        let b = order_hash(registry());
        assert_eq!(a, b);
    }
}
