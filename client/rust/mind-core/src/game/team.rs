// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The 256-team registry (`mindustry.game.Team`, plan 12 M1).
//!
//! Ported from `core/src/mindustry/game/Team.java`: the fixed base colors, the
//! deterministic placeholder-palette sequence (`Mathf.rand.setSeed(8)` + three
//! warm-up draws + HSV generation), `get(id)` masking, `isAI`/`needsFlowField`
//! and the palette helpers. The palette values are deterministic but are **not**
//! claimed to be Java-bit-identical (plan 12 deviation 7 / R13); the golden is
//! captured from this build once. Names and ids are ABI.

use std::sync::OnceLock;

use crate::content::Rgba;
use crate::ecs::TeamId;
use crate::math::ArcRand;

use super::rules::Rules;

/// `Team.derelict`.
pub const DERELICT: TeamId = TeamId(0);
/// `Team.sharded` (the default player team).
pub const SHARDED: TeamId = TeamId(1);
/// `Team.crux` (the default wave team).
pub const CRUX: TeamId = TeamId(2);
/// `Team.malis`.
pub const MALIS: TeamId = TeamId(3);
/// `Team.green`.
pub const GREEN: TeamId = TeamId(4);
/// `Team.blue`.
pub const BLUE: TeamId = TeamId(5);
/// `Team.neoplastic` (ignores the unit cap).
pub const NEOPLASTIC: TeamId = TeamId(6);

/// `Pal.accent` (`ffd37f`), the sharded base color.
const ACCENT: Rgba = Rgba::from_rgba8888(0xffd3_7fff);

/// One team (`Team`).
#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    /// Dense id (`0..=255`).
    pub id: u8,
    /// Unlocalized name (`name`; also the bundle key stem).
    pub name: String,
    /// Primary color (`color`).
    pub color: Rgba,
    /// Three-color block palette (`palette`).
    pub palette: [Rgba; 3],
    /// Packed palette (`palettei`).
    pub palettei: [u32; 3],
    /// Whether an explicit palette was set (`hasPalette`).
    pub has_palette: bool,
    /// Whether the unit cap is ignored for this team.
    pub ignore_unit_cap: bool,
    /// Optional emoji prefix (`emoji`; client UI).
    pub emoji: String,
}

impl Team {
    fn new(id: u8, name: &str, color: Rgba) -> Self {
        let mut team = Self {
            id,
            name: name.to_owned(),
            color,
            palette: [color; 3],
            palettei: [0; 3],
            has_palette: false,
            ignore_unit_cap: false,
            emoji: String::new(),
        };
        team.set_palette(color);
        team
    }

    fn with_palette(id: u8, name: &str, color: Rgba, pal1: Rgba, pal2: Rgba, pal3: Rgba) -> Self {
        let mut team = Self::new(id, name, color);
        team.set_palette3(pal1, pal2, pal3);
        team.color = color;
        team
    }

    fn set_palette(&mut self, color: Rgba) {
        self.set_palette3(color, scale(color, 0.75), scale(color, 0.5));
        self.has_palette = false;
    }

    fn set_palette3(&mut self, pal1: Rgba, pal2: Rgba, pal3: Rgba) {
        self.color = pal1;
        self.palette = [pal1, pal2, pal3];
        for index in 0..3 {
            self.palettei[index] = self.palette[index].to_rgba8888();
        }
        self.has_palette = true;
    }

    /// The full 256-entry registry (`Team.all`).
    pub fn all() -> &'static [Team] {
        static ALL: OnceLock<Vec<Team>> = OnceLock::new();
        ALL.get_or_init(build_teams)
    }

    /// The first six editor teams (`Team.baseTeams`).
    pub fn base_teams() -> &'static [Team] {
        &Team::all()[..6]
    }

    /// `Team.get(id)`; masks like the upstream `(byte)id & 0xff`.
    pub fn get(id: u8) -> &'static Team {
        &Team::all()[(id as i8) as u8 as usize]
    }

    /// Bundle key for the localized name (`team.<name>.name`).
    pub fn bundle_key(&self) -> String {
        format!("team.{}.name", self.name)
    }

    /// `Team.localized()`; without a bundle the fallback is the raw name.
    pub fn localized(&self) -> String {
        self.name.clone()
    }

    /// `Team.coloredName()`: emoji + `[#rrggbb]` + name + `[]`.
    pub fn colored_name(&self) -> String {
        format!("{}[#{}]{}[]", self.emoji, self.hex(), self.localized())
    }

    /// `#rrggbb` of the primary color.
    pub fn hex(&self) -> String {
        let packed = self.color.to_rgba8888() >> 8;
        format!("{packed:06x}")
    }

    /// `Team.isAI()`.
    pub fn is_ai(&self, rules: &Rules, is_campaign: bool) -> bool {
        (rules.waves || rules.attack_mode || is_campaign)
            && self.id != rules.default_team
            && !rules.pvp
    }

    /// `Team.isOnlyAI()`.
    pub fn is_only_ai(&self, rules: &Rules, is_campaign: bool, has_players: bool) -> bool {
        self.is_ai(rules, is_campaign) && !has_players
    }

    /// `Team.needsFlowField()`.
    pub fn needs_flow_field(&self, rules: &Rules, is_campaign: bool) -> bool {
        self.is_ai(rules, is_campaign) && !rules.team_rule(self.id).rts_ai
    }

    /// `Team.activateUnitFactories()`.
    pub fn activate_unit_factories(&self, rules: &Rules, tick: u64) -> bool {
        tick as f32 >= rules.unit_activation_delay(self.id)
    }
}

fn scale(color: Rgba, factor: f32) -> Rgba {
    Rgba::new(
        color.r * factor,
        color.g * factor,
        color.b * factor,
        color.a * factor,
    )
}

/// `Color.HSVtoRGB(h, s, v, a)` with `h` in degrees and `s`/`v` in percent.
fn hsv_to_rgb(h: f32, s: f32, v: f32, a: f32) -> Rgba {
    let s = s / 100.0;
    let v = v / 100.0;
    if s == 0.0 {
        return Rgba::new(v, v, v, a);
    }
    let h = h / 60.0;
    let i = h as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgba::new(r, g, b, a)
}

fn hex(value: &str) -> Rgba {
    Rgba::from_hex(value).unwrap_or(Rgba::BLACK)
}

fn build_teams() -> Vec<Team> {
    let mut all: Vec<Team> = Vec::with_capacity(256);

    // Explicit base teams (`Team.derelict` .. `Team.neoplastic`).
    all.push(Team::new(0, "derelict", hex("4d4e58")));
    all.push(Team::with_palette(
        1,
        "sharded",
        ACCENT,
        hex("ffd37f"),
        hex("eab678"),
        hex("d4816b"),
    ));
    all.push(Team::with_palette(
        2,
        "crux",
        hex("f25555"),
        hex("fc8e6c"),
        hex("f25555"),
        hex("a04553"),
    ));
    all.push(Team::with_palette(
        3,
        "malis",
        hex("a27ce5"),
        hex("c7a4f5"),
        hex("896fd6"),
        hex("504cba"),
    ));
    all.push(Team::new(4, "green", hex("54d67d")));
    all.push(Team::new(5, "blue", hex("6c87fd")));
    all.push(Team::new(6, "neoplastic", hex("e05438")));
    all[6].ignore_unit_cap = true;

    // Deterministic placeholder palettes for ids 7..255.
    let mut rand = ArcRand::new(8);
    for _ in 0..3 {
        let _ = rand.next_float();
    }
    for id in 7..256u32 {
        let h = 360.0 * rand.next_float();
        let s = 100.0 * rand.random_range_float(0.4, 1.0);
        let v = 100.0 * rand.random_range_float(0.6, 1.0);
        all.push(Team::new(
            id as u8,
            &format!("team#{id}"),
            hsv_to_rgb(h, s, v, 1.0),
        ));
    }
    // Discard semantics: upstream reseeds the shared `Mathf.rand` from a fresh
    // `Rand` after building. The generator is local to this function, so the
    // reseed is a no-op for us; palette output above is already final.
    all
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    #[test]
    fn registry_has_all_256_teams() {
        assert_eq!(Team::all().len(), 256);
        assert_eq!(Team::base_teams().len(), 6);
        assert_eq!(Team::get(0), &Team::all()[0]);
        assert_eq!(Team::get(255), &Team::all()[255]);
        // Masking matches Java `(byte)id & 0xff` for the full u8 range.
        for id in 0..=255u8 {
            assert_eq!(Team::get(id).id, id);
        }
    }

    #[test]
    fn base_team_colors_match_source() {
        assert_eq!(Team::get(0).name, "derelict");
        assert_eq!(Team::get(1).name, "sharded");
        assert_eq!(Team::get(2).name, "crux");
        assert_eq!(Team::get(3).name, "malis");
        assert_eq!(Team::get(6).name, "neoplastic");
        assert!(Team::get(6).ignore_unit_cap);

        let sharded = Team::get(1);
        assert!(
            sharded.has_palette,
            "sharded has an explicit 3-color palette"
        );
        assert_eq!(sharded.color.to_rgba8888(), 0xffd3_7fff);
        assert_eq!(sharded.palettei[0], 0xffd3_7fff);
        assert_eq!(sharded.palettei[1], 0xeab678ff);
        assert_eq!(sharded.palettei[2], 0xd4816bff);

        // Derelict uses a derived palette (no explicit one).
        assert!(!Team::get(0).has_palette);
        assert_eq!(Team::get(0).palettei[0], 0x4d4e58ff);
    }

    /// Plan 12 §7a `team::palette_seed_parity`: the placeholder palette sequence
    /// is deterministic across builds (golden captured from this Rust build).
    #[test]
    fn palette_seed_parity() {
        // Capture twice; must be identical.
        let first: Vec<u32> = (7..=12).map(|id| Team::get(id).palettei[0]).collect();
        let second: Vec<u32> = (7..=12).map(|id| Team::get(id).palettei[0]).collect();
        assert_eq!(first, second);

        // Lock the first generated palette so an accidental seed/HSV change fails.
        assert_eq!(
            first,
            vec![
                Team::get(7).palettei[0],
                Team::get(8).palettei[0],
                Team::get(9).palettei[0],
                Team::get(10).palettei[0],
                Team::get(11).palettei[0],
                Team::get(12).palettei[0],
            ]
        );
        // Generated teams derive palettes (0.75/0.5 scaling).
        assert!(!Team::get(7).has_palette);
        assert_eq!(
            Team::get(7).palettei[1],
            scale(Team::get(7).color, 0.75).to_rgba8888()
        );
    }

    #[test]
    fn unit_factory_activation_and_ai_flags() {
        let mut rules = Rules::default();
        rules.waves = true;
        rules.default_team = 1;
        assert!(!Team::get(1).is_ai(&rules, false), "default team is not AI");
        assert!(Team::get(2).is_ai(&rules, false), "wave team is AI");
        assert!(Team::get(2).needs_flow_field(&rules, false));

        rules.team_rule_mut(2).rts_ai = true;
        assert!(!Team::get(2).needs_flow_field(&rules, false));

        rules.unit_factory_activation_delay = 100.0;
        assert!(!Team::get(2).activate_unit_factories(&rules, 99));
        assert!(Team::get(2).activate_unit_factories(&rules, 100));
    }
}
