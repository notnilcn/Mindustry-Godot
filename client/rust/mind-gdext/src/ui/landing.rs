// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/world/blocks/storage/CoreBlock.java (drawLanding
//         480-538, drawLandingThrusters 540-563, drawLaunch 427-470,
//         drawThrusters 566-573, touchdown 340-355/412/650).

//! World-space landing core sprite at the `Layer::Space` band
//! (`MindLandingCore`).
//!
//! The native port of `client/scenes/landing_core.gd`, the `World/LandingCore`
//! `Node2D` in `scenes/game.tscn`. While `MindCamera2D.landing_draw_json()`
//! reports an active landing it draws the streak rays, the dropping core
//! (scaled/rotated per the Java formulas) with its `circle-shadow` trail, the
//! four team-coloured + white flame circles and the two-pass thruster plates.
//! After touchdown the plates retract over 90 ticks (`CoreBuild.updateTile`,
//! `CoreBlock.java:650`) while the real core building shows through; the
//! touchdown itself fires `Fx.launch` plus `Effect.shake(5, 5)`, the white
//! screen fade starts when the landing begins (`CoreBlock.beginLaunch`) and the
//! sector announce fires when it ends (`Time.run(launchDuration())`).
//!
//! View-only: every read goes through the camera/fx/campaign/sim facades
//! (`get_node_or_null` + `has_method` + `call`, exactly like the GDScript) and
//! nothing here mutates simulation state. No nodes are created in code — the
//! drawn textures are atlas regions resolved through `kit::icon_texture`.
//!
//! Note: Arc is y-up; the world camera flips the canvas y axis (zoom `(z, -z)`,
//! `MindCamera2D::process`), so world coordinates and counter-clockwise angles
//! match Java numerically and [`MindLandingCore::draw_region`] mirrors each
//! texture back upright (Arc `Draw.rect`).

use godot::classes::{INode2D, Json, Node2D, Texture2D, Time};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use crate::ui::kit;

/// Region pixels -> world pixels (block regions are 4x supersampled).
const CORE_REGION_SUPERSAMPLE: f32 = 0.25;
/// `Pal.lightTrail` (`drawf.rs::LIGHT_TRAIL`).
const LIGHT_TRAIL: Color = Color::from_rgba8(0xff, 0xe2, 0xa9, 0xff);
/// `thrusterSizes` frame duration for the fade-out (`CoreBlock.java:44`).
const FADE_TICKS: f64 = 35.0 / 60.0;
/// `CoreBlock.thrusterSizes` (`CoreBlock.java:44`).
const THRUSTER_SIZES: [f64; 14] = [
    0.0, 0.0, 0.0, 0.0, 0.3, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
];
/// `CoreBlock.thrusterLength` default (`14f / 4f`, `CoreBlock.java:48`).
const THRUSTER_LENGTH: f64 = 14.0 / 4.0;
/// Per-core `thrusterLength` overrides (`Blocks.java:3166-3232`).
const THRUSTER_LENGTHS: [(&str, f64); 5] = [
    ("core-foundation", 34.0 / 4.0),
    ("core-nucleus", 40.0 / 4.0),
    ("core-bastion", 34.0 / 4.0),
    ("core-citadel", 40.0 / 4.0),
    ("core-acropolis", 48.0 / 4.0),
];
/// `Geometry.d4` (`Arc`): right, up, left, down in the y-up world (the camera
/// flips the canvas y axis, so world coordinates match Java numerically).
const D4: [Vector2; 4] = [
    Vector2::new(1.0, 0.0),
    Vector2::new(0.0, 1.0),
    Vector2::new(-1.0, 0.0),
    Vector2::new(0.0, -1.0),
];

/// `MindLandingCore` — the world-space landing core sprite.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindLandingCore {
    base: Base<Node2D>,
    /// Whether the last `process` saw an active landing (`_active`).
    active: bool,
    /// Announced once per landing (`_announced`).
    announced: bool,
    /// Dropping core block name (`<block>-thruster*` region prefix).
    block: String,
    /// Dropping core world position (`landing_draw_json` `x`/`y`).
    position: Vector2,
    /// `Cutscene.getLandTimeIn()` through the landing (`landing_draw_json`).
    fin: f64,
    /// `renderer.getDisplayScale()` during the landing (`landing_draw_json`).
    scale: f64,
    /// Player team name (`<block>-team-<team>` region).
    team: String,
    /// `CoreBuild.thrusterTime`: 1 at touchdown, `-delta / 90` per tick.
    thruster_time: f64,
    /// Player team colour for the landing flames (`team.color`, refreshed when
    /// a landing starts; `Team.sharded.color` = `Pal.accent`).
    team_color: Color,
}

#[godot_api]
impl INode2D for MindLandingCore {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            active: false,
            announced: false,
            block: String::new(),
            position: Vector2::ZERO,
            fin: 1.0,
            scale: 1.0,
            team: String::new(),
            thruster_time: 0.0,
            team_color: Color::from_rgba8(0xff, 0xd3, 0x7f, 0xff),
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        let mut active = false;
        if let Some(state) = self.landing_draw_state() {
            active = state
                .get("active")
                .and_then(|value| value.try_to_relaxed::<bool>().ok())
                .unwrap_or(false);
            self.block = state
                .get("block")
                .map(|value| value.stringify().to_string())
                .unwrap_or_default();
            self.position = Vector2::new(
                state
                    .get("x")
                    .and_then(|value| value.try_to_relaxed::<f64>().ok())
                    .unwrap_or(0.0) as f32,
                state
                    .get("y")
                    .and_then(|value| value.try_to_relaxed::<f64>().ok())
                    .unwrap_or(0.0) as f32,
            );
            self.fin = state
                .get("fin")
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or(1.0);
            self.scale = state
                .get("scale")
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or(1.0);
            self.team = state
                .get("team")
                .map(|value| value.stringify().to_string())
                .unwrap_or_default();
        }
        if active && !self.active {
            self.start_landing();
            self.refresh_team_color();
        } else if !active && self.active {
            self.finish_landing();
        }
        self.active = active;
        if self.active {
            self.base_mut().queue_redraw();
        } else if self.thruster_time > 0.0 {
            // `CoreBuild.updateTile` (`CoreBlock.java:650`): `-= delta / 90`.
            self.thruster_time = 0.0f64.max(self.thruster_time - delta * 60.0 / 90.0);
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let fin = self.fin.clamp(0.0, 1.0);
        let fout = 1.0 - fin;
        if self.active {
            // `CoreBlock.drawLaunch`/`drawLanding` (`CoreBlock.java:427-538`):
            // every landing size is multiplied by `scl = Scl.scl(4) /
            // renderer.getDisplayScale()`, so the dropping core keeps a
            // constant on-screen size while the camera zooms from
            // `landZoomFrom` to `landZoomTo`.
            let scl = 4.0 / self.scale.max(0.0001);
            // `CoreBlock.java:438-443`: seeded streak rays behind the core.
            self.draw_streaks(fin, fout, scl);
            if self.block.is_empty() {
                return;
            }
            let base = kit::icon_texture(&self.block);
            let full = kit::icon_texture(&format!("block-{}-full", self.block));
            let region = base.clone().or(full);
            let Some(region) = region else {
                return;
            };
            // `region.width * region.scl()`: world units of the unscaled sprite.
            let region_size = region.get_size() * CORE_REGION_SUPERSAMPLE;
            let size = f64::from(self.core_size());
            let core_rotation = fout.powf(2.0) * 135.0;
            // `CoreBlock.java:484-492`.
            let s = f64::from(region_size.x) * scl * 3.6 * (1.0 - (1.0 - fout).powf(2.0));
            let frame = if fin >= 0.25 { 1.0 } else { fin / 0.25 };
            let thruster_size = Self::sample(&THRUSTER_SIZES, fin);
            let position = self.position;
            let team_color = self.team_color;
            // `Draw.color(Pal.lightTrail); Draw.rect("circle-shadow", x, y, s, s)`.
            let shadow = kit::icon_texture("circle-shadow");
            if let Some(shadow) = shadow
                && s > 0.0
            {
                self.draw_region(
                    &shadow,
                    position,
                    0.0,
                    Vector2::new(s as f32, s as f32),
                    LIGHT_TRAIL,
                );
            }
            // Thruster flames (`CoreBlock.java:503-513`): team colour, then white.
            let strength = (1.0 + (size - 3.0) / 2.5)
                * scl
                * thruster_size
                * (0.95 + Self::absin(Self::time_ticks(), 2.0, 0.1));
            let offset = (size - 3.0) * 3.0 * scl;
            if strength > 0.0 {
                for i in 0..4 {
                    let v = Vector2::RIGHT
                        .rotated((i as f64 * 90.0 + core_rotation).to_radians() as f32);
                    let base_len = (size * 8.0 / 2.0 + 1.0) * scl;
                    self.base_mut().draw_circle(
                        position + v * ((base_len + strength * 2.0 + offset) as f32),
                        (6.0 * strength) as f32,
                        team_color,
                    );
                    self.base_mut().draw_circle(
                        position + v * ((base_len + strength * 0.5 + offset) as f32),
                        (3.5 * strength) as f32,
                        Color::WHITE,
                    );
                }
            }
            // `drawLandingThrusters`, `Drawf.spinSprite(region)`, the
            // `pow4In(thrusterFrame)` thruster pass, then the team region
            // (`CoreBlock.java:515-525`).
            self.draw_landing_thrusters(frame, core_rotation, 1.0, scl);
            self.spin_sprite(
                &region,
                core_rotation,
                region_size * (scl as f32),
                Color::WHITE,
            );
            self.draw_landing_thrusters(frame, core_rotation, frame.powf(4.0), scl);
            if base.is_some() {
                let mut team = kit::icon_texture(&format!("{}-team-{}", self.block, self.team));
                let mut tint = Color::WHITE;
                if team.is_none() {
                    team = kit::icon_texture(&format!("{}-team", self.block));
                    tint = team_color;
                }
                if let Some(team) = team {
                    let size = team.get_size() * CORE_REGION_SUPERSAMPLE * (scl as f32);
                    self.spin_sprite(&team, core_rotation, size, tint);
                }
            }
        } else if self.thruster_time > 0.0 {
            // Post-touchdown retraction (`CoreBuild.draw`, `CoreBlock.java:340-355`).
            self.draw_thrusters(1.0 - self.thruster_time);
        }
    }
}

#[godot_api]
impl MindLandingCore {
    /// Player-team colour for the flame circles (`team.color`, Java).
    fn refresh_team_color(&mut self) {
        let host = self.base().get_node_or_null("/root/Spine/SimHost");
        let Some(mut host) = host else {
            return;
        };
        if !host.has_method("player_team_color") {
            return;
        }
        let packed = host
            .call("player_team_color", &[])
            .try_to_relaxed::<i64>()
            .unwrap_or_default();
        self.team_color = Color::from_rgba8(
            ((packed >> 24) & 0xff) as u8,
            ((packed >> 16) & 0xff) as u8,
            ((packed >> 8) & 0xff) as u8,
            (packed & 0xff) as u8,
        );
    }

    /// `CoreBlock.beginLaunch(false)`: a white full-screen cover fading out.
    fn start_landing(&mut self) {
        self.announced = false;
        let fade = self
            .base()
            .get_node_or_null("/root/Spine/Ui/UiRoot/OverlayLayer/fade_in");
        let Some(mut fade) = fade else {
            return;
        };
        if !fade.has_method("start_white") {
            return;
        }
        let _ = fade.call("start_white", &[FADE_TICKS.to_variant()]);
    }

    /// `Time.run(launchDuration())`: announce the sector (`[accent]<name>`) and
    /// its stored resources (`[lightgray]Resources:[white]` + item emoji).
    fn finish_landing(&mut self) {
        if self.announced {
            return;
        }
        self.announced = true;
        // Touchdown (`CoreBlock.java:412,650` + `Effect.shake(5, 5)`).
        self.thruster_time = 1.0;
        let position = self.position;
        let fx = self.base().get_node_or_null("/root/Spine/MindFx");
        if let Some(mut fx) = fx
            && fx.has_method("spawn_effect")
        {
            let _ = fx.call(
                "spawn_effect",
                &[
                    GString::from("launch").to_variant(),
                    position.x.to_variant(),
                    position.y.to_variant(),
                    0.0f32.to_variant(),
                ],
            );
        }
        let shake_camera = self.base().get_node_or_null("/root/Spine/World/Camera2D");
        if let Some(mut shake_camera) = shake_camera
            && shake_camera.has_method("apply_shake_event")
        {
            let _ = shake_camera.call(
                "apply_shake_event",
                &[5.0f32.to_variant(), 5.0f32.to_variant()],
            );
        }
        self.base_mut().queue_redraw();
        let campaign = kit::campaign();
        let ui = kit::ui();
        let (Some(mut campaign), Some(mut ui)) = (campaign, ui) else {
            return;
        };
        if !campaign.has_method("get_sector_state") {
            return;
        }
        let state = campaign.call("get_sector_state", &[]);
        let Some(state) = Self::json_dict(&state) else {
            return;
        };
        // `state.isCampaign() && Vars.showSectorLandInfo && preset.showSectorLandInfo`
        // (`CoreBlock.java:414`), resolved in mind-core.
        let show_land_info = state
            .get("showLandInfo")
            .and_then(|value| value.try_to_relaxed::<bool>().ok())
            .unwrap_or(false);
        if !show_land_info {
            return;
        }
        let preset = state
            .get("preset")
            .map(|value| value.stringify().to_string())
            .unwrap_or_default();
        let mut sector_name = state
            .get("sector")
            .map(|value| value.stringify().to_string())
            .unwrap_or_default();
        if !preset.is_empty() {
            let key = format!("sector.{preset}.name");
            let localized = Self::bundle(&key);
            sector_name = if !localized.is_empty() && localized != key {
                localized
            } else {
                preset
            };
        }
        let mut text = format!("[accent]{sector_name}");
        let sector_id = state
            .get("sector")
            .and_then(|value| value.try_to_relaxed::<i64>().ok())
            .unwrap_or(-1);
        let resources = Self::sector_resources(sector_id);
        if !resources.is_empty() {
            let parts: Vec<String> = resources.iter().map(|item| format!(":{item}:")).collect();
            text = format!(
                "{text}\n[lightgray]{}[white] {}",
                Self::bundle("sectors.resources"),
                parts.join(" ")
            );
        }
        // The toast label translates raw markup (`MindRichLabel`; H03 T17).
        let _ = ui.call(
            "announce",
            &[
                GString::from(text.as_str()).to_variant(),
                5.0f32.to_variant(),
            ],
        );
    }

    /// `SectorInfo.resources` item names from the live campaign sector row.
    fn sector_resources(id: i64) -> Vec<String> {
        let campaign = kit::campaign();
        let Some(mut campaign) = campaign else {
            return Vec::new();
        };
        if !campaign.has_method("campaign_views_json") {
            return Vec::new();
        }
        let payload = campaign.call("campaign_views_json", &[GString::new().to_variant()]);
        let Some(views) = Self::json_dict(&payload) else {
            return Vec::new();
        };
        let Some(sectors) = views
            .get("sectors")
            .and_then(|value| value.try_to::<VarArray>().ok())
        else {
            return Vec::new();
        };
        for sector in sectors.iter_shared() {
            let Ok(sector) = sector.try_to::<VarDictionary>() else {
                continue;
            };
            let sector_id = sector
                .get("id")
                .and_then(|value| value.try_to_relaxed::<i64>().ok())
                .unwrap_or(-2);
            if sector_id != id {
                continue;
            }
            let stats = sector
                .get("stats")
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
            let resources = stats
                .get("resources")
                .and_then(|value| value.try_to::<VarArray>().ok())
                .unwrap_or_default();
            return resources
                .iter_shared()
                .map(|item| item.stringify().to_string())
                .collect();
        }
        Vec::new()
    }

    /// `MindAssets.bundle_get` (`Core.bundle.get`); empty without the autoload.
    fn bundle(key: &str) -> String {
        let assets = kit::node("/root/MindAssets");
        let Some(mut assets) = assets else {
            return String::new();
        };
        assets
            .call("bundle_get", &[GString::from(key).to_variant()])
            .stringify()
            .to_string()
    }

    /// `JSON.parse_string(str(value))` as a dictionary: the facades return
    /// typed dictionaries natively, the GDScript round-tripped them through
    /// JSON (this accepts either form).
    fn json_dict(value: &Variant) -> Option<VarDictionary> {
        if let Ok(dict) = value.try_to::<VarDictionary>() {
            return Some(dict);
        }
        let text = value.try_to::<GString>().ok()?;
        Json::parse_string(&text).try_to::<VarDictionary>().ok()
    }

    /// `MindCamera2D.landing_draw_json()` as a dictionary; `None` without the
    /// camera or a non-dictionary payload.
    fn landing_draw_state(&self) -> Option<VarDictionary> {
        let mut camera = self.base().get_node_or_null("/root/Spine/World/Camera2D")?;
        if !camera.has_method("landing_draw_json") {
            return None;
        }
        let payload = camera.call("landing_draw_json", &[]);
        Self::json_dict(&payload)
    }

    /// Arc `Draw.rect(region, x, y, w, h, rotation)` in the y-up world: the
    /// camera flips the canvas y axis, so the region is mirrored back (its top
    /// edge faces world `+y`) and `rotation` turns counter-clockwise on screen,
    /// both as in Java.
    fn draw_region(
        &mut self,
        texture: &Gd<Texture2D>,
        center: Vector2,
        angle: f64,
        world_size: Vector2,
        tint: Color,
    ) {
        let tex_size = texture.get_size();
        if tex_size.x <= 0.0 || tex_size.y <= 0.0 {
            return;
        }
        self.base_mut()
            .draw_set_transform_ex(center)
            .rotation(angle.to_radians() as f32)
            .scale(Vector2::new(
                world_size.x / tex_size.x,
                -world_size.y / tex_size.y,
            ))
            .done();
        self.base_mut()
            .draw_texture_ex(texture, -tex_size * 0.5)
            .modulate(tint)
            .done();
        self.base_mut().draw_set_transform(Vector2::ZERO);
    }

    /// `Drawf.spinSprite` (`Drawf.java:606-613`): the region at `r mod 90`, then
    /// again at `r - 90` with alpha `r / 90`, so a 4-fold sprite turns smoothly.
    fn spin_sprite(
        &mut self,
        texture: &Gd<Texture2D>,
        angle: f64,
        world_size: Vector2,
        tint: Color,
    ) {
        let r = angle.rem_euclid(90.0);
        let position = self.position;
        self.draw_region(texture, position, r, world_size, tint);
        let mut faded = tint;
        faded.a *= (r / 90.0) as f32;
        if faded.a > 0.0 {
            self.draw_region(texture, position, r - 90.0, world_size, faded);
        }
    }

    /// `CoreBlock.drawLaunch` streaks (`CoreBlock.java:438-443`):
    /// `Angles.randLenVectors(1, pfin, 100, 800 * scl * pfin, ...)` from Rust
    /// (`landing_streaks_json`), stroke `scl * ffin * pf * 3`, length
    /// `(ffin * 20 + 1) * scl`.
    fn draw_streaks(&mut self, fin: f64, fout: f64, scl: f64) {
        let camera = self.base().get_node_or_null("/root/Spine/World/Camera2D");
        let Some(mut camera) = camera else {
            return;
        };
        if !camera.has_method("landing_streaks_json") {
            return;
        }
        // `pfin = Interp.pow3Out(fin)`, `pf = Interp.pow2In(fout)`.
        let pfin = 1.0 - (1.0 - fin).powf(3.0);
        let pf = fout * fout;
        let payload = camera.call(
            "landing_streaks_json",
            &[pfin.to_variant(), (800.0 * scl * pfin).to_variant()],
        );
        let Ok(text) = payload.try_to::<GString>() else {
            return;
        };
        let Ok(rows) = Json::parse_string(&text).try_to::<VarArray>() else {
            return;
        };
        let position = self.position;
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarArray>() else {
                continue;
            };
            let ffin = row
                .get(3)
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or_default();
            let width = scl * ffin * pf * 3.0;
            if width <= 0.0 {
                continue;
            }
            let from = position
                + Vector2::new(
                    row.get(0)
                        .and_then(|value| value.try_to_relaxed::<f64>().ok())
                        .unwrap_or_default() as f32,
                    row.get(1)
                        .and_then(|value| value.try_to_relaxed::<f64>().ok())
                        .unwrap_or_default() as f32,
                );
            let angle = row
                .get(2)
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or_default();
            let direction = Vector2::RIGHT.rotated(angle.to_radians() as f32);
            let to = from + direction * (((ffin * 20.0 + 1.0) * scl) as f32);
            self.base_mut()
                .draw_line_ex(from, to, LIGHT_TRAIL)
                .width(width as f32)
                .done();
        }
    }

    /// `CoreBlock.drawLandingThrusters` (`CoreBlock.java:540-563`): two shading
    /// passes of four plates around the core; the second pass rotates each
    /// offset by -90 and scales the alpha by `(rotation % 90) / 90`.
    /// `base_alpha` is the caller's alpha (`pow4In(thrusterFrame)` for the
    /// second call site); `scl` is `Draw.xscl` (offsets and plate sizes).
    fn draw_landing_thrusters(&mut self, frame: f64, angle: f64, base_alpha: f64, scl: f64) {
        let length = Self::thruster_length(&self.block) * (frame - 1.0) - 0.25;
        let position = self.position;
        for pass in 0..2 {
            for i in 0..4 {
                let texture = kit::icon_texture(&format!(
                    "{}-thruster{}",
                    self.block,
                    if i >= 2 { 2 } else { 1 }
                ));
                let Some(texture) = texture else {
                    continue;
                };
                let mut rot = i as f64 * 90.0 + angle % 90.0;
                let mut offset =
                    Vector2::RIGHT.rotated(rot.to_radians() as f32) * ((length * scl) as f32);
                let mut alpha = base_alpha;
                if pass == 1 {
                    offset = offset.rotated((-90.0f64).to_radians() as f32);
                    alpha = (angle % 90.0) / 90.0 * base_alpha;
                    rot -= 90.0;
                }
                if alpha <= 0.0 {
                    continue;
                }
                let size = texture.get_size() * CORE_REGION_SUPERSAMPLE * (scl as f32);
                self.draw_region(
                    &texture,
                    position + offset,
                    rot,
                    size,
                    Color::from_rgba(1.0, 1.0, 1.0, alpha as f32),
                );
            }
        }
    }

    /// `CoreBlock.drawThrusters` (`CoreBlock.java:566-573`): four plates at
    /// `d4[i] * length`, rotation `i * 90`.
    fn draw_thrusters(&mut self, frame: f64) {
        let length = Self::thruster_length(&self.block) * (frame - 1.0) - 0.25;
        let position = self.position;
        for (i, direction) in D4.iter().enumerate() {
            let texture = kit::icon_texture(&format!(
                "{}-thruster{}",
                self.block,
                if i >= 2 { 2 } else { 1 }
            ));
            let Some(texture) = texture else {
                continue;
            };
            let offset = *direction * (length as f32);
            self.draw_region(
                &texture,
                position + offset,
                i as f64 * 90.0,
                texture.get_size() * CORE_REGION_SUPERSAMPLE,
                Color::WHITE,
            );
        }
    }

    /// Live core block size (`3` for every core; the flame lengths use it).
    fn core_size(&mut self) -> i32 {
        let host = self.base().get_node_or_null("/root/Spine/SimHost");
        let Some(mut host) = host else {
            return 3;
        };
        if !host.has_method("block_size") {
            return 3;
        }
        let mut tile = ((self.position.x as i32) / 8, (self.position.y as i32) / 8);
        if let Some(state) = self.landing_draw_state() {
            let x = state
                .get("x")
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or(0.0);
            let y = state
                .get("y")
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or(0.0);
            tile = ((x as i32) / 8, (y as i32) / 8);
        }
        host.call("block_size", &[tile.0.to_variant(), tile.1.to_variant()])
            .try_to_relaxed::<i32>()
            .unwrap_or(1)
            .max(1)
    }

    /// `Mathf.sample(values, t)`: linear interpolation over `t * (len - 1)`,
    /// `t` clamped to `0..1` (`Arc Mathf`; `CoreBlock.java:492`).
    fn sample(values: &[f64], t: f64) -> f64 {
        let Some(last) = values.len().checked_sub(1) else {
            return 0.0;
        };
        let index = t.clamp(0.0, 1.0) * last as f64;
        let low = index.floor() as usize;
        let high = (low + 1).min(last);
        let low_value = values.get(low).copied().unwrap_or_default();
        let high_value = values.get(high).copied().unwrap_or_default();
        low_value + (high_value - low_value) * (index - low as f64)
    }

    /// `CoreBlock.thrusterLengths` lookup with the `14f / 4f` default.
    fn thruster_length(block: &str) -> f64 {
        THRUSTER_LENGTHS
            .iter()
            .find_map(|(name, length)| (*name == block).then_some(*length))
            .unwrap_or(THRUSTER_LENGTH)
    }

    /// `Time.time` in ticks (Java draws `absin(2, 0.1)` on it).
    fn time_ticks() -> f64 {
        Time::singleton().get_ticks_msec() as f64 * 0.06
    }

    /// `Mathf.absin(time, scl, mag)`.
    fn absin(time: f64, scl: f64, mag: f64) -> f64 {
        ((time / (scl * 2.0)).sin() + 1.0) * 0.5 * mag
    }
}
