// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit weapon draw geometry (plan 17 M5).
//!
//! Ported from `type/Weapon.java` (`draw`/`drawOutline`) and the
//! `UnitType.drawWeapons`/`drawWeaponOutlines` order. Behavior lives in plan 10;
//! this module only reads mount state.

use crate::content::Rgba;
use crate::content::registries::units::parts::DrawPartSpec;
use crate::content::registries::units::weapon::WeaponDef;
use crate::render::draw::{Blending, PrimKind, RegionKey};
use crate::render::layer::Layer;
use crate::weapons::mount::WeaponMount;

use super::draw::PartEmit;
use super::params::PartParams;

/// `Weapon.draw` pose: world mount position, rotation and applied recoil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponPose {
    /// World x of the weapon pivot.
    pub x: f32,
    /// World y of the weapon pivot.
    pub y: f32,
    /// Weapon rotation in degrees.
    pub rotation: f32,
    /// Recoil distance after `pow(recoil, recoilPow) * recoil`.
    pub real_recoil: f32,
}

/// `Weapon.draw` geometry (`Weapon.java:223-228`).
pub fn weapon_pose(
    unit_x: f32,
    unit_y: f32,
    unit_rotation: f32,
    mount: &WeaponMount,
    weapon: &WeaponDef,
) -> WeaponPose {
    let rotation = unit_rotation - 90.0;
    let real_recoil = mount.recoil.powf(weapon.recoil_pow) * weapon.recoil;
    let weapon_rotation = rotation
        + if weapon.rotate {
            mount.rotation
        } else {
            weapon.base_rotation
        };
    let wx = unit_x
        + crate::fx::angles::trnsx_vec(rotation, weapon.x, weapon.y)
        + crate::fx::angles::trnsx_vec(weapon_rotation, 0.0, -real_recoil);
    let wy = unit_y
        + crate::fx::angles::trnsy_vec(rotation, weapon.x, weapon.y)
        + crate::fx::angles::trnsy_vec(weapon_rotation, 0.0, -real_recoil);
    WeaponPose {
        x: wx,
        y: wy,
        rotation: weapon_rotation,
        real_recoil,
    }
}

/// The mount's recoil counter for a part (`part.recoilIndex`).
pub fn part_recoil(part: &DrawPartSpec, mount: &WeaponMount) -> f32 {
    if part.recoil_index >= 0 && (part.recoil_index as usize) < mount.recoils.len() {
        mount.recoils[part.recoil_index as usize]
    } else {
        mount.recoil
    }
}

/// `-Mathf.sign(flipSprite)` (`Weapon.java:213,254`).
#[inline]
fn flip_sign(flip_sprite: bool) -> f32 {
    if flip_sprite { -1.0 } else { 1.0 }
}

/// Ports `Weapon.draw` (`Weapon.java:218-292`). `unit_color` is
/// `UnitType.applyColor`; `cell_color` is `UnitType.cellColor`.
pub fn draw_weapon(
    emit: &mut PartEmit,
    pose: WeaponPose,
    weapon: &WeaponDef,
    mount: &WeaponMount,
    unit_color: Rgba,
    cell_color: Option<Rgba>,
) {
    let z = emit.z;
    emit.z += weapon.layer_offset;

    if weapon.shadow > 0.0 {
        let saved = emit.color;
        emit.color = Rgba::new(0.0, 0.0, 0.0, 0.4);
        emit.region("circle-shadow", pose.x, pose.y, 0.0);
        emit.color = saved;
    }

    if weapon.top {
        draw_weapon_outline(emit, pose, weapon);
    }

    let mut params = base_params(pose, mount, weapon);
    for part in &weapon.parts {
        if part.under {
            params.recoil = part_recoil(part, mount);
            emit.draw_part(part, &params, &weapon.name);
        }
    }

    let prev_xscl = emit.xscl;
    emit.xscl *= flip_sign(weapon.flip_sprite);
    emit.color = unit_color;
    emit.region(&weapon.name, pose.x, pose.y, pose.rotation);

    if let Some(cell) = cell_color {
        let saved = emit.color;
        emit.color = cell;
        emit.region(
            &format!("{}-cell", weapon.name),
            pose.x,
            pose.y,
            pose.rotation,
        );
        emit.color = saved;
    }

    if mount.heat > 0.0 {
        let saved_color = emit.color;
        let saved_blend = emit.blend;
        emit.color = weapon.heat_color.with_alpha(mount.heat);
        emit.blend = Blending::Additive;
        emit.region(
            &format!("{}-heat", weapon.name),
            pose.x,
            pose.y,
            pose.rotation,
        );
        emit.color = saved_color;
        emit.blend = saved_blend;
    }
    emit.xscl = prev_xscl;

    for part in &weapon.parts {
        if !part.under {
            params.recoil = part_recoil(part, mount);
            emit.draw_part(part, &params, &weapon.name);
        }
    }

    emit.z = z;
}

/// `Weapon.drawOutline` (`Weapon.java:203-216`).
pub fn draw_weapon_outline(emit: &mut PartEmit, pose: WeaponPose, weapon: &WeaponDef) {
    let prev = emit.xscl;
    emit.xscl = flip_sign(weapon.flip_sprite);
    emit.region(
        &format!("{}-outline", weapon.name),
        pose.x,
        pose.y,
        pose.rotation,
    );
    emit.xscl = prev;
}

fn base_params(pose: WeaponPose, mount: &WeaponMount, weapon: &WeaponDef) -> PartParams {
    let mut params = PartParams::default();
    let reload = if weapon.reload != 0.0 {
        mount.reload / weapon.reload
    } else {
        0.0
    };
    params.set(
        mount.warmup,
        reload,
        mount.smooth_reload,
        mount.heat,
        mount.recoil,
        mount.charge,
        pose.x,
        pose.y,
        pose.rotation + 90.0,
    );
    params.side_multiplier = if weapon.flip_sprite { -1 } else { 1 };
    params
}

/// `UnitType.drawWeaponOutlines`: outlines for every non-`top` weapon before
/// the unit body (`UnitType.java`).
pub fn draw_weapon_outlines(emit: &mut PartEmit, weapons: &[(&WeaponDef, WeaponPose)]) {
    for (weapon, pose) in weapons {
        if !weapon.top {
            draw_weapon_outline(emit, *pose, weapon);
        }
    }
}

/// Bullet `parts` (`Weapon.java`/`BulletType.parts`): parts driven by `life`.
pub fn draw_bullet_parts(
    emit: &mut PartEmit,
    parts: &[DrawPartSpec],
    x: f32,
    y: f32,
    rotation: f32,
    fin: f32,
) {
    let mut params = PartParams::default();
    params.set(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, x, y, rotation);
    params.life = fin;
    for part in parts {
        emit.draw_part(part, &params, "");
    }
}

/// Visual-only weapon beams (plan 17 M5). `BuildWeapon`/`MineWeapon`/
/// `RepairBeamWeapon` draw overrides: pure geometry, sim logic stays in 10/11.
pub mod beams {
    use super::*;

    /// `BuildWeapon.draw`/`MineWeapon.draw` beam segment (a stroked line).
    pub fn beam(emit: &mut PartEmit, from: (f32, f32), to: (f32, f32), stroke: f32, color: Rgba) {
        let saved_color = emit.color;
        let saved_blend = emit.blend;
        emit.color = color;
        emit.push_at(
            emit.z,
            PrimKind::Line {
                x1: from.0,
                y1: from.1,
                x2: to.0,
                y2: to.1,
                stroke,
                color,
                cap: true,
            },
            Blending::Normal,
        );
        emit.color = saved_color;
        emit.blend = saved_blend;
    }

    /// `MineWeapon`: beam plus the target square (draw order: square, beam).
    pub fn mine_beam(
        emit: &mut PartEmit,
        from: (f32, f32),
        target: (f32, f32),
        stroke: f32,
        color: Rgba,
    ) {
        let saved = emit.color;
        emit.color = color;
        emit.push_at(
            emit.z,
            PrimKind::Poly {
                x: target.0,
                y: target.1,
                sides: 4,
                r: stroke * 3.0,
                rotation_deg: 45.0,
                fill: false,
                stroke,
                color,
            },
            Blending::Normal,
        );
        beam(emit, from, target, stroke, color);
        emit.color = saved;
    }

    /// `RepairBeamWeapon.drawBeam`: widened beam line + pulsing end circle.
    pub fn repair_beam(
        emit: &mut PartEmit,
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        pulse_radius: f32,
        pulse_stroke: f32,
        color: Rgba,
    ) {
        beam(emit, from, to, width, color);
        let saved = emit.color;
        emit.color = color;
        emit.push_at(
            emit.z,
            PrimKind::Circle {
                x: to.0,
                y: to.1,
                r: pulse_radius,
                fill: false,
                stroke: pulse_stroke,
                color,
            },
            Blending::Normal,
        );
        emit.color = saved;
    }
}

/// Region used by `Drawf.shadow`.
pub const SHADOW_REGION: RegionKey = RegionKey("circle-shadow");

/// `Layer` used for weapon shadows/parts ordering helpers.
pub const WEAPON_LAYER: f32 = Layer::Effect.z();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::registries::units::parts::DrawPartSpec;
    use crate::content::registries::units::weapon::WeaponSpec;
    use crate::fx::parts::draw::{AllRegions, PartEmit};
    use crate::render::draw::DrawProgram;

    fn weapon() -> WeaponDef {
        let spec = WeaponSpec {
            name: "test-weapon",
            rotate: Some(true),
            top: Some(false),
            base_rotation: Some(10.0),
            recoil: Some(4.0),
            recoil_pow: Some(2.0),
            x: Some(6.0),
            y: Some(-2.0),
            ..WeaponSpec::default()
        };
        let registry = crate::content::test_support::test_registry();
        WeaponDef::from_spec(
            spec,
            crate::content::registries::units::ResolvedBullet {
                id: crate::content::BulletId::new(0),
                range: 0.0,
                heals: false,
                kill_shooter: false,
                dps: 0.0,
            },
            &registry,
        )
        .expect("weapon")
    }

    #[test]
    fn weapon_pose_recoil_and_rotation() {
        let w = weapon();
        let mut mount = WeaponMount::new(&w);
        mount.rotation = 40.0;
        mount.recoil = 0.5;
        let pose = weapon_pose(100.0, 200.0, 90.0, &mount, &w);
        // rotation = 90 - 90 = 0; weaponRotation = 0 + mount.rotation(40) = 40
        assert_eq!(pose.rotation, 40.0);
        // realRecoil = pow(0.5, 2) * 4 = 1
        assert!((pose.real_recoil - 1.0).abs() < 1e-5);
        // wx = 100 + trnsx(0, 6, -2) + trnsx(40,0,-1)
        let expected_x = 100.0 + 6.0 + 40f32.to_radians().sin() * 1.0;
        assert!((pose.x - expected_x).abs() < 1e-4);
    }

    #[test]
    fn draw_weapon_emits_region_cell_heat_in_order() {
        let w = weapon();
        let mut mount = WeaponMount::new(&w);
        mount.heat = 0.5;
        let lookup = AllRegions::new(32.0);
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            let pose = weapon_pose(0.0, 0.0, 0.0, &mount, &w);
            draw_weapon(
                &mut emit,
                pose,
                &w,
                &mount,
                Rgba::WHITE,
                Some(Rgba::new(1.0, 0.0, 0.0, 1.0)),
            );
        }
        let names: Vec<&str> = program
            .prims
            .iter()
            .filter_map(|p| match &p.kind {
                PrimKind::Region { region, .. } => Some(region.0),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            vec!["test-weapon", "test-weapon-cell", "test-weapon-heat"]
        );
        // heat is additive
        assert_eq!(program.prims.last().unwrap().blend, Blending::Additive);
    }

    #[test]
    fn under_parts_draw_before_region_and_others_after() {
        let mut w = weapon();
        let mut under = DrawPartSpec::shape();
        under.kind = crate::content::registries::units::parts::DrawPartKind::ShapePart;
        under.under = true;
        under.color = Some(Rgba::WHITE);
        let mut top = under.clone();
        top.under = false;
        w.parts = vec![under, top];
        let lookup = AllRegions::new(32.0);
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            let mount = WeaponMount::new(&w);
            let pose = weapon_pose(0.0, 0.0, 0.0, &mount, &w);
            draw_weapon(&mut emit, pose, &w, &mount, Rgba::WHITE, None);
        }
        // [under-poly, region, top-poly]
        assert_eq!(program.len(), 3);
        assert!(matches!(program.prims[0].kind, PrimKind::Poly { .. }));
        assert!(matches!(program.prims[1].kind, PrimKind::Region { .. }));
        assert!(matches!(program.prims[2].kind, PrimKind::Poly { .. }));
    }

    #[test]
    fn beams_emit_lines_and_pulse() {
        let lookup = AllRegions::new(32.0);
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            emit.z = 122.0;
            beams::mine_beam(&mut emit, (0.0, 0.0), (32.0, 0.0), 2.0, Rgba::WHITE);
            beams::repair_beam(
                &mut emit,
                (0.0, 0.0),
                (64.0, 0.0),
                3.0,
                8.0,
                1.0,
                Rgba::WHITE,
            );
        }
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Line { .. }))
        );
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Circle { .. }))
        );
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Poly { .. }))
        );
    }

    #[test]
    fn shadow_region_constant_matches_drawf() {
        assert_eq!(SHADOW_REGION.0, "circle-shadow");
        assert_eq!(WEAPON_LAYER, 110.0);
    }
}
