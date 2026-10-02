// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/graphics/Pal.java (constant color values used by
//         content metadata).

//! `Pal` color constants referenced by content definitions (plan 02).
//!
//! Only the constants that appear in ported content metadata live here; the
//! full palette is a plan-16/17 concern. Values are the resolved `Color.valueOf`
//! hex codes from `Pal.java`.

use crate::content::color::Rgba;

/// `Pal.accent`.
pub const ACCENT: Rgba = Rgba::from_rgba8888(0xffd37fff);
/// `Pal.bulletYellow`.
pub const BULLET_YELLOW: Rgba = Rgba::from_rgba8888(0xfff8e8ff);
/// `Pal.bulletYellowBack`.
pub const BULLET_YELLOW_BACK: Rgba = Rgba::from_rgba8888(0xf9c27aff);
/// `Pal.darkOutline`.
pub const DARK_OUTLINE: Rgba = Rgba::from_rgba8888(0x2d2f39ff);
/// `Pal.darkMetal`.
pub const DARK_METAL: Rgba = Rgba::from_rgba8888(0x6e7080ff);
/// `Pal.darkerMetal`.
pub const DARKER_METAL: Rgba = Rgba::from_rgba8888(0x565666ff);
/// `Pal.heal`.
pub const HEAL: Rgba = Rgba::from_rgba8888(0x98ffa9ff);
/// `Pal.lancerLaser`.
pub const LANCER_LASER: Rgba = Rgba::from_rgba8888(0xa9d8ffff);
/// `Pal.missileYellow`.
pub const MISSILE_YELLOW: Rgba = Rgba::from_rgba8888(0xffd2aeff);
/// `Pal.missileYellowBack`.
pub const MISSILE_YELLOW_BACK: Rgba = Rgba::from_rgba8888(0xe58956ff);
/// `Pal.neoplasmOutline`.
pub const NEOPLASM_OUTLINE: Rgba = Rgba::from_rgba8888(0x2e191dff);
/// `Pal.neoplasm1`.
pub const NEOPLASM1: Rgba = Rgba::from_rgba8888(0xf98f4aff);
/// `Pal.powerLight`.
pub const POWER_LIGHT: Rgba = Rgba::from_rgba8888(0xfbd367ff);
/// `Pal.sap`.
pub const SAP: Rgba = Rgba::from_rgba8888(0x665c9fff);
/// `Pal.sapBullet`.
pub const SAP_BULLET: Rgba = Rgba::from_rgba8888(0xbf92f9ff);
/// `Pal.sapBulletBack`.
pub const SAP_BULLET_BACK: Rgba = Rgba::from_rgba8888(0x6d56bfff);
/// `Pal.suppress` (`Pal.sap.cpy().mul(1.6f)`, pre-multiplied).
pub const SUPPRESS: Rgba = Rgba::new(
    0x66 as f32 / 255.0 * 1.6,
    0x5c as f32 / 255.0 * 1.6,
    0x9f as f32 / 255.0 * 1.6,
    1.0,
);
/// `Pal.surge`.
pub const SURGE: Rgba = Rgba::from_rgba8888(0xf3e979ff);
/// `Pal.techBlue`.
pub const TECH_BLUE: Rgba = Rgba::from_rgba8888(0x8ca9e8ff);
/// `Pal.turretHeat`.
pub const TURRET_HEAT: Rgba = Rgba::from_rgba8888(0xab3400ff);
/// `Pal.unitBack`.
pub const UNIT_BACK: Rgba = Rgba::from_rgba8888(0xd06b53ff);
/// `Pal.unitFront`.
pub const UNIT_FRONT: Rgba = Rgba::from_rgba8888(0xffa665ff);
/// `Pal.yellowBoltFront`.
pub const YELLOW_BOLT_FRONT: Rgba = Rgba::from_rgba8888(0xffd27eff);
/// `RepairBeamWeapon.laserColor` (`Color.valueOf("98ffa9")`).
pub const REPAIR_LASER: Rgba = Rgba::from_rgba8888(0x98ffa9ff);
