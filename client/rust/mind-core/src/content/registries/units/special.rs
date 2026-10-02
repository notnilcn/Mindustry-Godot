// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (wave `special`).
//
//! Generated unit metadata (`UnitTypes.java`, wave `special`).
//! Regenerate with `parity/tools/gen_units.py` after upstream content changes.

#![allow(unused_imports)]

use super::ability::{AbilityKind, AbilitySpec};
use super::parts::{BlendingKind, DrawPartSpec, InterpKind, PartMoveSpec, PartProgressSpec};
use super::weapon::{BulletRef, ShootPatternSpec, WeaponKind, WeaponSpec};
use super::{
    AiControllerKind, ControllerKind, EngineSpec, TreadRect, UnitKind, UnitSink, entity, spec,
};
use crate::content::ContentError;
use crate::content::color::Rgba;
use crate::content::registries::blocks::{BlockFlag, EnvMask};
use crate::content::registries::bullets::{BulletKind, BulletSpec};
use crate::content::registries::fx_meta::{EffectId, EffectRef, EffectSpec};
use crate::content::registries::planets::EnvFlag;
use crate::content::registries::sound_meta::SoundId;

/// Loads the `special` wave in upstream order.
pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    block(sink)?;
    manifold(sink)?;
    assembly_drone(sink)?;
    dummy(sink)?;
    Ok(())
}

fn block(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("block", UnitKind::UnitType, entity::BLOCK);
        unit.speed = Some(0.0);
        unit.hit_size = Some(0.0);
        unit.health = Some(1.0);
        unit.rotate_speed = Some(360.0);
        unit.item_capacity = Some(0);
        unit.hidden = Some(true);
        unit.internal = Some(true);
        unit
    })
}

fn manifold(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("manifold", UnitKind::ErekirUnitType, entity::TETHER);
        unit.controller = Some(ControllerKind::Cargo);
        unit.is_enemy = Some(false);
        unit.allowed_in_payloads = Some(false);
        unit.logic_controllable = Some(false);
        unit.player_controllable = Some(false);
        unit.env_disabled = Some(EnvMask::none());
        unit.payload_capacity = Some(0.0);
        unit.low_altitude = Some(false);
        unit.flying = Some(true);
        unit.drag = Some(0.06);
        unit.speed = Some(3.5);
        unit.rotate_speed = Some(9.0);
        unit.accel = Some(0.1);
        unit.item_capacity = Some(100);
        unit.health = Some(200.0);
        unit.hit_size = Some(11.0);
        unit.engine_size = Some(2.3);
        unit.engine_offset = Some(6.5);
        unit.hidden = Some(true);
        unit.engines_mirror
            .push(EngineSpec::new(6.0, -6.0, 2.3, 315.0));
        unit
    })
}

fn assembly_drone(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("assembly-drone", UnitKind::ErekirUnitType, entity::TETHER);
        unit.controller = Some(ControllerKind::Assembler);
        unit.flying = Some(true);
        unit.drag = Some(0.06);
        unit.accel = Some(0.11);
        unit.speed = Some(1.3);
        unit.health = Some(90.0);
        unit.engine_size = Some(2.0);
        unit.engine_offset = Some(6.5);
        unit.payload_capacity = Some(0.0);
        unit.targetable = Some(false);
        unit.bounded = Some(false);
        unit.outline_color = Some(Rgba::new(0.1764706, 0.18431373, 0.22352941, 1.0));
        unit.is_enemy = Some(false);
        unit.hidden = Some(true);
        unit.use_unit_cap = Some(false);
        unit.logic_controllable = Some(false);
        unit.player_controllable = Some(false);
        unit.allowed_in_payloads = Some(false);
        unit.create_wreck = Some(false);
        unit.env_enabled = Some(EnvMask::any());
        unit.env_disabled = Some(EnvMask::none());
        unit
    })
}

fn dummy(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("dummy", UnitKind::UnitType, entity::DUMMY);
        unit.controller = Some(ControllerKind::No);
        unit.env_enabled = Some(EnvMask::any());
        unit.env_disabled = Some(EnvMask::none());
        unit.is_enemy = Some(false);
        unit.allowed_in_payloads = Some(false);
        unit.logic_controllable = Some(false);
        unit.player_controllable = Some(false);
        unit.hidden = Some(true);
        unit.hoverable = Some(false);
        unit.can_boost = Some(true);
        unit.use_unit_cap = Some(false);
        unit.killable = Some(false);
        unit.physics = Some(false);
        unit.internal = Some(true);
        unit.internal_generate_sprites = Some(true);
        unit.flying_layer = Some(114.0);
        unit.drag = Some(0.33);
        unit.hit_size = Some(12.0);
        unit.hide_details = Some(false);
        unit.engine_offset = Some(7.0);
        unit.engine_size = Some(2.0);
        unit
    })
}
