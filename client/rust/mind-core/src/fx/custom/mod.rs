// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Hand-ported effect bodies (`Fx.java` one-offs) and the [`FxEmit`] builder.
//!
//! Custom bodies are Rust functions in `mind-core` (plan 17 §2.4 #4): testable
//! headlessly and compiled to [`DrawPrim`]s. The bodies mirror the Java
//! `Cons<EffectContainer>` bodies 1:1 using the Arc primitive vocabulary.

pub mod basic;
mod emit;
pub mod wave_f56;

pub use emit::FxEmit;

use super::container::EffectContainer;
use super::data::ViewSnapshot;

/// One hand-ported body id. Extend append-only as catalogue waves land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum CustomFxId {
    /// `Fx.blockCrash`.
    BlockCrash,
    /// `Fx.trailFade`.
    TrailFade,
    /// `Fx.unitSpawn`.
    UnitSpawn,
    /// `Fx.unitControl`.
    UnitControl,
    /// `Fx.unitDespawn`.
    UnitDespawn,
    /// `Fx.unitSpirit`.
    UnitSpirit,
    /// `Fx.itemTransfer`.
    ItemTransfer,
    /// `Fx.pointBeam`.
    PointBeam,
    /// `Fx.pointHit`.
    PointHit,
    /// `Fx.hitScepterSecondary`.
    HitScepterSecondary,
    /// `Fx.lightning`.
    Lightning,
    /// `Fx.coreBuildShockwave`.
    CoreBuildShockwave,
    /// `Fx.coreBuildBlock`.
    CoreBuildBlock,
    /// `Fx.pointShockwave`.
    PointShockwave,
    /// `Fx.moveCommand`.
    MoveCommand,
    /// `Fx.attackCommand`.
    AttackCommand,
    /// `Fx.placeBlock`.
    PlaceBlock,
    /// `Fx.tapBlock`.
    TapBlock,
    /// `Fx.breakBlock`.
    BreakBlock,
    /// `Fx.payloadDeposit`.
    PayloadDeposit,
    /// `Fx.select`.
    Select,
    /// `Fx.hitBulletSmall`.
    HitBulletSmall,
    /// `Fx.hitBulletColor`.
    HitBulletColor,
    /// `Fx.hitBulletBig`.
    HitBulletBig,
    /// `Fx.hitFlameSmall`.
    HitFlameSmall,
    /// `Fx.hitLiquid`.
    HitLiquid,
    /// `Fx.shootSmall`.
    ShootSmall,
    /// `Fx.shootBig`.
    ShootBig,
    /// `Fx.casing1`.
    Casing1,
    /// `Fx.healWave`.
    HealWave,
    /// `Fx.shockwave`.
    Shockwave,
    /// `Fx.smoke`.
    Smoke,
    /// `Fx.explosion`.
    Explosion,
    /// `Fx.hitLaser`.
    HitLaser,
    /// `Fx.commandSend`.
    CommandSend,
    /// `Fx.upgradeCoreBloom`.
    UpgradeCoreBloom,
    /// `Fx.coreLaunchConstruct`.
    CoreLaunchConstruct,
    /// `Fx.fallSmoke`.
    FallSmoke,
    /// `Fx.rocketSmoke`.
    RocketSmoke,
    /// `Fx.rocketSmokeLarge`.
    RocketSmokeLarge,
    /// `Fx.magmasmoke`.
    MagmaSmoke,
    /// `Fx.spawn`.
    Spawn,
    /// `Fx.padlaunch`.
    Padlaunch,
    /// `Fx.breakProp`.
    BreakProp,
    /// `Fx.unitDrop`.
    UnitDrop,
    /// `Fx.unitLand`.
    UnitLand,
    /// `Fx.unitDust`.
    UnitDust,
    /// `Fx.unitLandSmall`.
    UnitLandSmall,
    /// `Fx.unitPickup`.
    UnitPickup,
    /// `Fx.crawlDust`.
    CrawlDust,
    /// `Fx.landShock`.
    LandShock,
    /// `Fx.pickup`.
    Pickup,
    /// `Fx.sparkExplosion`.
    SparkExplosion,
    /// `Fx.titanExplosion`.
    TitanExplosion,
    /// `Fx.titanExplosionLarge`.
    TitanExplosionLarge,
    /// `Fx.titanExplosionSmall`.
    TitanExplosionSmall,
    /// `Fx.titanExplosionFrag`.
    TitanExplosionFrag,
    /// `Fx.coreExplosion`.
    CoreExplosion,
    /// `Fx.smokeAoeCloud`.
    SmokeAoeCloud,
    /// `Fx.scatheExplosion`.
    ScatheExplosion,
    /// `Fx.scatheExplosionSmall`.
    ScatheExplosionSmall,
    /// `Fx.scatheLight`.
    ScatheLight,
    /// `Fx.scatheLightSmall`.
    ScatheLightSmall,
    /// `Fx.titanLightSmall`.
    TitanLightSmall,
    /// `Fx.scatheSlash`.
    ScatheSlash,
    /// `Fx.dynamicSpikes`.
    DynamicSpikes,
    /// `Fx.greenBomb`.
    GreenBomb,
    /// `Fx.greenLaserCharge`.
    GreenLaserCharge,
    /// `Fx.greenLaserChargeSmall`.
    GreenLaserChargeSmall,
    /// `Fx.greenCloud`.
    GreenCloud,
    /// `Fx.healWaveDynamic`.
    HealWaveDynamic,
    /// `Fx.heal`.
    Heal,
    /// `Fx.dynamicWave`.
    DynamicWave,
    /// `Fx.shieldWave`.
    ShieldWave,
    /// `Fx.shieldApply`.
    ShieldApply,
    /// `Fx.hitSquaresColor`.
    HitSquaresColor,
    /// `Fx.hitFuse`.
    HitFuse,
    /// `Fx.hitFlamePlasma`.
    HitFlamePlasma,
    /// `Fx.hitLaserBlast`.
    HitLaserBlast,
    /// `Fx.hitEmpSpark`.
    HitEmpSpark,
    /// `Fx.hitLancer`.
    HitLancer,
    /// `Fx.hitLancerLow`.
    HitLancerLow,
    /// `Fx.hitBeam`.
    HitBeam,
    /// `Fx.hitFlameBeam`.
    HitFlameBeam,
    /// `Fx.hitMeltdown`.
    HitMeltdown,
    /// `Fx.hitMeltHeal`.
    HitMeltHeal,
    /// `Fx.instBomb`.
    InstBomb,
    /// `Fx.instTrail`.
    InstTrail,
    /// `Fx.instShoot`.
    InstShoot,
    /// `Fx.instHit`.
    InstHit,
    /// Wave F5–F6 dispatcher: the body is selected by effect name at render
    /// time (`wave_f56::dispatch`). Keeps the registry id/name ABI while sharing
    /// one function-pointer entry for the remaining one-off bodies.
    Catalogue,
}

/// A custom body function.
pub type CustomBody = fn(&mut FxEmit, &EffectContainer, &dyn ViewSnapshot);

/// Resolves a [`CustomFxId`] to its body.
pub fn dispatch(id: CustomFxId) -> CustomBody {
    match id {
        CustomFxId::BlockCrash => basic::block_crash,
        CustomFxId::TrailFade => basic::trail_fade,
        CustomFxId::UnitSpawn => basic::unit_spawn,
        CustomFxId::UnitControl => basic::unit_control,
        CustomFxId::UnitDespawn => basic::unit_despawn,
        CustomFxId::UnitSpirit => basic::unit_spirit,
        CustomFxId::ItemTransfer => basic::item_transfer,
        CustomFxId::PointBeam => basic::point_beam,
        CustomFxId::PointHit => basic::point_hit,
        CustomFxId::HitScepterSecondary => basic::hit_scepter_secondary,
        CustomFxId::Lightning => basic::lightning,
        CustomFxId::CoreBuildShockwave => basic::core_build_shockwave,
        CustomFxId::CoreBuildBlock => basic::core_build_block,
        CustomFxId::PointShockwave => basic::point_shockwave,
        CustomFxId::MoveCommand => basic::move_command,
        CustomFxId::AttackCommand => basic::attack_command,
        CustomFxId::PlaceBlock => basic::place_block,
        CustomFxId::TapBlock => basic::tap_block,
        CustomFxId::BreakBlock => basic::break_block,
        CustomFxId::PayloadDeposit => basic::payload_deposit,
        CustomFxId::Select => basic::select,
        CustomFxId::HitBulletSmall => basic::hit_bullet_small,
        CustomFxId::HitBulletColor => basic::hit_bullet_color,
        CustomFxId::HitBulletBig => basic::hit_bullet_big,
        CustomFxId::HitFlameSmall => basic::hit_flame_small,
        CustomFxId::HitLiquid => basic::hit_liquid,
        CustomFxId::ShootSmall => basic::shoot_small,
        CustomFxId::ShootBig => basic::shoot_big,
        CustomFxId::Casing1 => basic::casing1,
        CustomFxId::HealWave => basic::heal_wave,
        CustomFxId::Shockwave => basic::shockwave,
        CustomFxId::Smoke => basic::smoke,
        CustomFxId::Explosion => basic::explosion,
        CustomFxId::HitLaser => basic::hit_laser,
        CustomFxId::CommandSend => basic::command_send,
        CustomFxId::UpgradeCoreBloom => basic::upgrade_core_bloom,
        CustomFxId::CoreLaunchConstruct => basic::core_launch_construct,
        CustomFxId::FallSmoke => basic::fall_smoke,
        CustomFxId::RocketSmoke => basic::rocket_smoke,
        CustomFxId::RocketSmokeLarge => basic::rocket_smoke_large,
        CustomFxId::MagmaSmoke => basic::magma_smoke,
        CustomFxId::Spawn => basic::spawn,
        CustomFxId::Padlaunch => basic::padlaunch,
        CustomFxId::BreakProp => basic::break_prop,
        CustomFxId::UnitDrop => basic::unit_drop,
        CustomFxId::UnitLand => basic::unit_land,
        CustomFxId::UnitDust => basic::unit_dust,
        CustomFxId::UnitLandSmall => basic::unit_land_small,
        CustomFxId::UnitPickup => basic::unit_pickup,
        CustomFxId::CrawlDust => basic::crawl_dust,
        CustomFxId::LandShock => basic::land_shock,
        CustomFxId::Pickup => basic::pickup,
        CustomFxId::SparkExplosion => basic::spark_explosion,
        CustomFxId::TitanExplosion => basic::titan_explosion,
        CustomFxId::TitanExplosionLarge => basic::titan_explosion_large,
        CustomFxId::TitanExplosionSmall => basic::titan_explosion_small,
        CustomFxId::TitanExplosionFrag => basic::titan_explosion_frag,
        CustomFxId::CoreExplosion => basic::core_explosion,
        CustomFxId::SmokeAoeCloud => basic::smoke_aoe_cloud,
        CustomFxId::ScatheExplosion => basic::scathe_explosion,
        CustomFxId::ScatheExplosionSmall => basic::scathe_explosion_small,
        CustomFxId::ScatheLight => basic::scathe_light,
        CustomFxId::ScatheLightSmall => basic::scathe_light_small,
        CustomFxId::TitanLightSmall => basic::titan_light_small,
        CustomFxId::ScatheSlash => basic::scathe_slash,
        CustomFxId::DynamicSpikes => basic::dynamic_spikes,
        CustomFxId::GreenBomb => basic::green_bomb,
        CustomFxId::GreenLaserCharge => basic::green_laser_charge,
        CustomFxId::GreenLaserChargeSmall => basic::green_laser_charge_small,
        CustomFxId::GreenCloud => basic::green_cloud,
        CustomFxId::HealWaveDynamic => basic::heal_wave_dynamic,
        CustomFxId::Heal => basic::heal,
        CustomFxId::DynamicWave => basic::dynamic_wave,
        CustomFxId::ShieldWave => basic::shield_wave,
        CustomFxId::ShieldApply => basic::shield_apply,
        CustomFxId::HitSquaresColor => basic::hit_squares_color,
        CustomFxId::HitFuse => basic::hit_fuse,
        CustomFxId::HitFlamePlasma => basic::hit_flame_plasma,
        CustomFxId::HitLaserBlast => basic::hit_laser_blast,
        CustomFxId::HitEmpSpark => basic::hit_emp_spark,
        CustomFxId::HitLancer => basic::hit_lancer,
        CustomFxId::HitLancerLow => basic::hit_lancer_low,
        CustomFxId::HitBeam => basic::hit_beam,
        CustomFxId::HitFlameBeam => basic::hit_flame_beam,
        CustomFxId::HitMeltdown => basic::hit_meltdown,
        CustomFxId::HitMeltHeal => basic::hit_melt_heal,
        CustomFxId::InstBomb => basic::inst_bomb,
        CustomFxId::InstTrail => basic::inst_trail,
        CustomFxId::InstShoot => basic::inst_shoot,
        CustomFxId::InstHit => basic::inst_hit,
        CustomFxId::Catalogue => catalogue_body,
    }
}

/// Dispatches a wave F5–F6 body by the live effect's catalogue name.
pub fn catalogue_body(emit: &mut FxEmit, e: &EffectContainer, snap: &dyn ViewSnapshot) {
    let name = super::def::registry().get(e.id).name;
    wave_f56::dispatch(name, emit, e, snap);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_custom_id_has_a_body() {
        for id in [
            CustomFxId::BlockCrash,
            CustomFxId::TrailFade,
            CustomFxId::UnitSpawn,
            CustomFxId::HitBulletSmall,
            CustomFxId::Explosion,
        ] {
            let body = dispatch(id);
            let _ = body as usize;
        }
    }
}
