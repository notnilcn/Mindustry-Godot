// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (the `drawer = new DrawTurret`
// part lists in the `turrets` region) and
// core/src/mindustry/world/blocks/defense/turrets/Turret.java (`heatColor`).

//! Hand-ported `DrawTurret` part lists for the vanilla turrets that override
//! the default drawer (`Blocks.java` `drawer = new DrawTurret(){{
//! parts.add(new RegionPart(...)) }}`), plus the per-block `DrawTurret`
//! `basePrefix` and `Turret.heatColor` overrides.
//!
//! The part *behavior* lives in [`crate::fx::parts`]; this module owns only the
//! content data, mirroring how `content::registries::units::parts` stores the
//! unit part trees. Every part here is loaded with `part.turretShading = true`
//! (`DrawTurret.load`, `DrawTurret.java:155-165`), which selects the `-r`/`-l`
//! region pairs for mirrored parts.
//!
//! Not ported yet (no vanilla accept frame needs them; documented residual):
//! `scathe`, `smite` and `malign` use `ShapePart`/`HaloPart` trees whose halo
//! fields are not modelled by [`DrawPartSpec`], and `lustre`'s heat progress is
//! a custom lambda (`Blocks.java:5190`).

use crate::content::Rgba;
use crate::content::registries::pal;
use crate::content::registries::units::parts::{
    DrawPartSpec, InterpKind, PartMoveSpec, PartProgressSpec,
};

/// `DrawTurret` base prefix (`Blocks.java`: the Erekir turrets construct their
/// drawer as `new DrawTurret("reinforced-")`).
pub fn base_prefix(name: &str) -> &'static str {
    match name {
        "breach" | "diffuse" | "sublimate" | "titan" | "disperse" | "afflict" | "lustre"
        | "scathe" | "smite" | "malign" => "reinforced-",
        _ => "",
    }
}

/// The drawer's base region (`DrawTurret.load`, `DrawTurret.java:146-170`):
/// `<name>-base`, else the mod's `<mod>-<prefix>block-<size>`, else
/// `<prefix>block-<size>`. The port has no per-turret mod drawer prefix, so the
/// mod arm is the plain `<prefix>block-<size>` lookup the caller already walks.
pub fn base_region_candidates(name: &str, size: i32) -> [String; 2] {
    [
        format!("{name}-base"),
        format!("{}block-{}", base_prefix(name), size),
    ]
}

/// `Turret.heatColor` (`Turret.java:120`, `Pal.turretHeat`) with the vanilla
/// overrides (`Blocks.java`: `lancer`/`arc` set `Color.red`).
pub fn heat_color(name: &str) -> Rgba {
    match name {
        "lancer" | "arc" => Rgba::new(1.0, 0.0, 0.0, 1.0),
        _ => pal::TURRET_HEAT,
    }
}

/// A `RegionPart(suffix)` with `turretShading` set (`DrawTurret.load`).
fn part(suffix: &str) -> DrawPartSpec {
    let mut spec = DrawPartSpec::region(suffix);
    spec.turret_shading = true;
    spec
}

/// `PartProgress.recoil.curve(Interp.pow2In)` (titan barrel/missiles).
fn recoil_pow2_in() -> PartProgressSpec {
    PartProgressSpec::CurveInterp(Box::new(PartProgressSpec::Recoil), InterpKind::Pow2In)
}

/// `PartProgress.reload.curve(Interp.pow2In)` (titan ammo missiles).
fn reload_pow2_in() -> PartProgressSpec {
    PartProgressSpec::CurveInterp(Box::new(PartProgressSpec::Reload), InterpKind::Pow2In)
}

/// `new PartMove(progress, x, y, rot)`.
fn move_to(progress: PartProgressSpec, x: f32, y: f32, rot: f32) -> PartMoveSpec {
    PartMoveSpec {
        progress,
        x,
        y,
        gx: 0.0,
        gy: 0.0,
        rot,
    }
}

/// The `DrawTurret.parts` list for `name` (`Blocks.java` turret drawers); empty
/// for the default drawer.
pub fn parts_for(name: &str) -> Vec<DrawPartSpec> {
    match name {
        // `Blocks.java:3321-3335` (`duo`).
        "duo" => {
            let mut left = part("-barrel-l");
            left.progress = PartProgressSpec::Recoil;
            left.recoil_index = 0;
            left.under = true;
            left.move_y = -1.5;
            let mut right = part("-barrel-r");
            right.progress = PartProgressSpec::Recoil;
            right.recoil_index = 1;
            right.under = true;
            right.move_y = -1.5;
            vec![left, right]
        }
        // `Blocks.java:3407-3414` (`scatter`).
        "scatter" => {
            let mut mid = part("-mid");
            mid.progress = PartProgressSpec::Recoil;
            mid.under = false;
            mid.move_y = -1.25;
            vec![mid]
        }
        // `Blocks.java:3825-3840` (`salvo`).
        "salvo" => {
            let mut side = part("-side");
            side.progress = PartProgressSpec::Warmup;
            side.move_x = 0.6;
            side.move_rot = -15.0;
            side.mirror = true;
            side.layer_offset = 0.001;
            side.moves
                .push(move_to(PartProgressSpec::Recoil, 0.5, -0.5, -8.0));
            let mut barrel = part("-barrel");
            barrel.progress = PartProgressSpec::Recoil;
            barrel.move_y = -2.5;
            vec![side, barrel]
        }
        // `Blocks.java:4200-4213` (`cyclone`): `for(i = 3; i > 0; i--)`.
        "cyclone" => (1..=3)
            .rev()
            .map(|i| {
                let mut barrel = part(&format!("-barrel-{i}"));
                barrel.progress = PartProgressSpec::Recoil;
                barrel.recoil_index = i - 1;
                barrel.under = true;
                barrel.move_y = -2.0;
                barrel
            })
            .collect(),
        // `Blocks.java:4556-4566` (`diffuse`).
        "diffuse" => {
            let mut front = part("-front");
            front.progress = PartProgressSpec::Warmup;
            front.move_rot = -10.0;
            front.mirror = true;
            front
                .moves
                .push(move_to(PartProgressSpec::Recoil, 0.0, -3.0, -5.0));
            front.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            vec![front]
        }
        // `Blocks.java:4583-4615` (`sublimate`; `22/4f`, `-1/4f`, `6/4f`,
        // `20/4f`, `17/4f`, `8/4f`).
        "sublimate" => {
            let heat = Rgba::from_rgba8888(0xfa28_59ff);
            let mut back = part("-back");
            back.progress = PartProgressSpec::Warmup;
            back.mirror = true;
            back.move_rot = 40.0;
            back.x = 5.5;
            back.y = -0.25;
            back.move_y = 1.5;
            back.under = true;
            back.heat_color = Some(heat);
            let mut front = part("-front");
            front.progress = PartProgressSpec::Warmup;
            front.mirror = true;
            front.move_rot = 40.0;
            front.x = 5.0;
            front.y = 4.25;
            front.move_x = 1.0;
            front.move_y = 1.0;
            front.under = true;
            front.heat_color = Some(heat);
            let mut nozzle = part("-nozzle");
            nozzle.progress = PartProgressSpec::Warmup;
            nozzle.mirror = true;
            nozzle.move_x = 2.0;
            nozzle.heat_color = Some(Rgba::from_rgba8888(0xf03b_0eff));
            vec![back, front, nozzle]
        }
        // `Blocks.java:4842-4862` (`titan`).
        "titan" => {
            let mut barrel = part("-barrel");
            barrel.progress = recoil_pow2_in();
            barrel.move_y = -5.0 * 4.0 / 3.0;
            barrel.heat_color = Some(Rgba::from_rgba8888(0xf03b_0eff));
            let mut side = part("-side");
            side.heat_progress = PartProgressSpec::Warmup;
            side.progress = PartProgressSpec::Warmup;
            side.mirror = true;
            side.move_x = 2.0 * 4.0 / 3.0;
            side.move_y = -0.5;
            side.move_rot = -40.0;
            side.under = true;
            side.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            vec![barrel, side]
        }
        // `Blocks.java:5009-5030` (`disperse`).
        "disperse" => {
            let sky = Rgba::from_rgba8888(0x87ce_ebff).with_alpha(0.9);
            let mut side = part("-side");
            side.mirror = true;
            side.under = true;
            side.move_x = 1.75;
            side.move_y = -0.5;
            let mut mid = part("-mid");
            mid.under = true;
            mid.move_y = -1.5;
            mid.progress = PartProgressSpec::Recoil;
            // `PartProgress.recoil.add(0.25f).min(PartProgress.warmup)`.
            mid.heat_progress = PartProgressSpec::Min(
                Box::new(PartProgressSpec::Add(
                    Box::new(PartProgressSpec::Recoil),
                    0.25,
                )),
                Box::new(PartProgressSpec::Warmup),
            );
            mid.heat_color = Some(sky);
            let mut blade = part("-blade");
            blade.heat_progress = PartProgressSpec::Warmup;
            blade.heat_color = Some(sky);
            blade.mirror = true;
            blade.under = true;
            blade.move_y = 1.0;
            blade.move_x = 1.5;
            blade.move_rot = 8.0;
            vec![side, mid, blade]
        }
        // `Blocks.java:5132-5152` (`afflict`).
        "afflict" => {
            let heat = Rgba::from_rgba8888(0xff62_14ff);
            let mut blade = part("-blade");
            blade.progress = PartProgressSpec::Recoil;
            blade.heat_color = Some(heat);
            blade.mirror = true;
            blade.under = true;
            blade.move_x = 2.0;
            blade.move_y = -1.0;
            blade.move_rot = -7.0;
            let mut glow = part("-blade-glow");
            glow.progress = PartProgressSpec::Recoil;
            glow.heat_progress = PartProgressSpec::Warmup;
            glow.heat_color = Some(heat);
            glow.draw_region = false;
            glow.mirror = true;
            glow.under = true;
            glow.move_x = 2.0;
            glow.move_y = -1.0;
            glow.move_rot = -7.0;
            vec![blade, glow]
        }
        _ => Vec::new(),
    }
}

/// `DrawTurret.ammoParts` for `name` and the current ammo item (`titan`'s
/// missile variants, `Blocks.java:5616-5655`).
pub fn ammo_parts_for(name: &str, item: &str) -> Vec<DrawPartSpec> {
    if name != "titan" {
        return Vec::new();
    }
    let suffix = match item {
        "carbide" => "-missile",
        "phase-fabric" => "-missile-phase",
        "surge-alloy" => "-missile-surge",
        _ => return Vec::new(),
    };
    let mut missile = part(suffix);
    missile.progress = reload_pow2_in();
    missile.color_to = Some(Rgba::new(1.0, 1.0, 1.0, 0.0));
    missile.color = Some(Rgba::WHITE);
    missile.mix_color_to = Some(pal::ACCENT);
    missile.mix_color = Some(Rgba::new(1.0, 1.0, 1.0, 0.0));
    missile.outline = false;
    missile.under = true;
    missile.layer_offset = -0.01;
    missile.moves.push(move_to(
        PartProgressSpec::Inv(Box::new(PartProgressSpec::Warmup)),
        0.0,
        -4.0,
        0.0,
    ));
    vec![missile]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Blocks.java` mirror expansions: the region names a turret part loads
    /// under (mirror + turretShading selects `-r`/`-l`).
    #[test]
    fn turret_parts_cover_the_vanilla_drawers() {
        assert_eq!(base_prefix("duo"), "");
        assert_eq!(base_prefix("diffuse"), "reinforced-");
        assert_eq!(base_region_candidates("duo", 1), ["duo-base", "block-1"]);
        assert_eq!(
            base_region_candidates("diffuse", 3),
            ["diffuse-base", "reinforced-block-3"]
        );
        assert_eq!(heat_color("duo"), pal::TURRET_HEAT);
        assert_eq!(heat_color("lancer"), Rgba::new(1.0, 0.0, 0.0, 1.0));

        let names =
            |parts: &[DrawPartSpec]| parts.iter().map(|p| p.suffix.clone()).collect::<Vec<_>>();
        assert_eq!(names(&parts_for("duo")), ["-barrel-l", "-barrel-r"]);
        assert_eq!(names(&parts_for("scatter")), ["-mid"]);
        assert_eq!(names(&parts_for("salvo")), ["-side", "-barrel"]);
        assert_eq!(
            names(&parts_for("cyclone")),
            ["-barrel-3", "-barrel-2", "-barrel-1"]
        );
        assert_eq!(
            names(&parts_for("sublimate")),
            ["-back", "-front", "-nozzle"]
        );
        assert_eq!(names(&parts_for("titan")), ["-barrel", "-side"]);
        assert!(parts_for("lancer").is_empty());

        // Duo's per-barrel recoil indices (`Blocks.java:3324-3330`).
        let duo = parts_for("duo");
        assert_eq!(duo[0].recoil_index, 0);
        assert_eq!(duo[1].recoil_index, 1);
        assert_eq!(duo[0].progress, PartProgressSpec::Recoil);
        assert!(duo[0].under);
        // Every ported part enables turret shading (`DrawTurret.load`).
        assert!(duo.iter().all(|p| p.turret_shading));

        // Titan's ammo parts key on the item name.
        assert_eq!(
            names(&ammo_parts_for("titan", "surge-alloy")),
            ["-missile-surge"]
        );
        assert!(ammo_parts_for("titan", "copper").is_empty());
        assert!(ammo_parts_for("duo", "copper").is_empty());
    }
}
