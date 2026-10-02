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

/// One weapon + its live mount/pose, read from a unit draw state (plan 17 M5).
#[derive(Clone, Copy, Debug)]
pub struct UnitWeapon<'a> {
    /// Weapon definition.
    pub def: &'a WeaponDef,
    /// Live mount state (`UnitType.applyColor`/`cellColor` read-only).
    pub mount: &'a WeaponMount,
    /// World mount transform (`weapon_pose`).
    pub pose: WeaponPose,
}

/// `UnitType.cellColor` (`UnitType.java:1795-1798`):
/// `black.lerp(team, f + absin(Time.time, max(f*5, 1), 1 - f))`.
pub fn cell_color(health_fraction: f32, team_color: Rgba, time: f32) -> Rgba {
    let f = health_fraction.clamp(0.0, 1.0);
    // `Mathf.absin(time, scl, mag) = (sin(time / scl) * 0.5 + 0.5) * mag`.
    let osc = (time / (f * 5.0).max(1.0)).sin() * (1.0 - f) * 0.5 + (1.0 - f) * 0.5;
    let a = f + osc;
    Rgba::new(
        Rgba::BLACK.r + (team_color.r - Rgba::BLACK.r) * a,
        Rgba::BLACK.g + (team_color.g - Rgba::BLACK.g) * a,
        Rgba::BLACK.b + (team_color.b - Rgba::BLACK.b) * a,
        team_color.a,
    )
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

/// `UnitType.drawWeaponOutlines` (`UnitType.java:1744-1763`): outlines for every
/// non-`top` weapon, drawn before the unit body, at `z + weapon.layerOffset`.
/// `outline_color` is `applyColor`/`applyOutlineColor` (white unless drowning).
pub fn draw_weapon_outlines(emit: &mut PartEmit, weapons: &[UnitWeapon], outline_color: Rgba) {
    let saved_color = emit.color;
    emit.color = outline_color;
    for w in weapons {
        if !w.def.top {
            let z = emit.z;
            emit.z += w.def.layer_offset;
            draw_weapon_outline(emit, w.pose, w.def);
            emit.z = z;
        }
    }
    emit.color = saved_color;
}

/// `UnitType.drawWeapons` (`UnitType.java:1734-1742`): applies the body color
/// once, then draws every weapon (`Weapon.draw`). Top-weapon outlines are drawn
/// here (after the body) by `Weapon.draw`.
pub fn draw_unit_weapons(
    emit: &mut PartEmit,
    weapons: &[UnitWeapon],
    unit_color: Rgba,
    cell: Option<Rgba>,
    outline_color: Rgba,
) {
    let saved_color = emit.color;
    let saved_mix = emit.mix;
    emit.mix = None;
    for w in weapons {
        // `Weapon.draw` draws the top outline with the current color before
        // re-applying the body color for the region.
        emit.color = outline_color;
        draw_weapon(emit, w.pose, w.def, w.mount, unit_color, cell);
    }
    emit.color = saved_color;
    emit.mix = saved_mix;
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

    #[test]
    fn unit_weapon_layer_order_outlines_before_body_weapons_after() {
        let low = weapon();
        let mut top = weapon();
        top.name = "top-weapon".to_owned();
        top.top = true;

        let low_mount = WeaponMount::new(&low);
        let top_mount = WeaponMount::new(&top);
        let low_pose = weapon_pose(0.0, 0.0, 90.0, &low_mount, &low);
        let top_pose = weapon_pose(0.0, 0.0, 90.0, &top_mount, &top);
        let weapons = [
            UnitWeapon {
                def: &low,
                mount: &low_mount,
                pose: low_pose,
            },
            UnitWeapon {
                def: &top,
                mount: &top_mount,
                pose: top_pose,
            },
        ];

        let lookup = AllRegions::new(32.0);
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            emit.z = Layer::Block.z();
            draw_weapon_outlines(&mut emit, &weapons, Rgba::WHITE);
            // plan 16 draws the unit body/cell between the two weapon passes.
            emit.region("unit-body", 0.0, 0.0, 0.0);
            draw_unit_weapons(
                &mut emit,
                &weapons,
                Rgba::WHITE,
                Some(Rgba::BLACK),
                Rgba::WHITE,
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
            vec![
                "test-weapon-outline",
                "unit-body",
                "test-weapon",
                "test-weapon-cell",
                "top-weapon-outline",
                "top-weapon",
                "top-weapon-cell",
            ]
        );
    }

    #[test]
    fn cell_color_matches_java() {
        let team = Rgba::new(1.0, 0.0, 0.0, 1.0);
        // f = 1 collapses the oscillator: exactly the team color.
        let full = cell_color(1.0, team, 12.34);
        assert!((full.r - 1.0).abs() < 1e-6);
        assert!(full.g.abs() < 1e-6);
        // f = 0, t = 0 -> `absin` midpoint -> half-blended black/team.
        let empty = cell_color(0.0, team, 0.0);
        assert!((empty.r - 0.5).abs() < 1e-6);
        // f = 0, t = pi/2 -> `absin` peak -> full team color.
        let peak = cell_color(0.0, team, std::f32::consts::FRAC_PI_2);
        assert!((peak.r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bullet_parts_drive_from_life_fin() {
        use crate::content::registries::units::parts::PartProgressSpec as ContentProgress;
        use crate::fx::parts::draw::MapRegions;

        let mut part = DrawPartSpec::shape();
        part.circle = true;
        part.radius = 0.0;
        part.radius_to = 10.0;
        part.progress = ContentProgress::Life;
        part.color = Some(Rgba::WHITE);

        let lookup = MapRegions::new();
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            draw_bullet_parts(&mut emit, std::slice::from_ref(&part), 0.0, 0.0, 0.0, 0.5);
        }
        // `radius` lerps `0 -> 10` by `life = fin = 0.5` -> 5.
        match &program.prims[0].kind {
            PrimKind::Circle { r, .. } => assert!((r - 5.0).abs() < 1e-5),
            _ => panic!("expected circle"),
        }
    }
}
