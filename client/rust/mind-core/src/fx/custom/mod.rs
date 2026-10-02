// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Hand-ported effect bodies (`Fx.java` one-offs) and the [`FxEmit`] builder.
//!
//! Custom bodies are Rust functions in `mind-core` (plan 17 §2.4 #4): testable
//! headlessly and compiled to [`DrawPrim`]s. The bodies mirror the Java
//! `Cons<EffectContainer>` bodies 1:1 using the Arc primitive vocabulary.

pub mod basic;
mod emit;

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
    }
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
