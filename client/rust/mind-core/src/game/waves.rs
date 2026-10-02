// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Waves` — the built-in survival wave table and `generate` algorithm
//! (plan 11 §3.10).
//!
//! Ported from `core/src/mindustry/game/Waves.java` (`waveVersion = 7`, the
//! Serpulo survival prescription and the seeded procedural generator). The
//! generator is bit-exact via [`crate::math::rand_arc::ArcRand`]; the JVM golden
//! lives at `tests/golden/units/wave_generate.json` (produced by
//! `parity/java/DumpWaves.java`).

use crate::content::ContentRegistry;
use crate::game::spawn_group::{ItemStack, NEVER, SpawnGroup};
use crate::math::rand_arc::ArcRand;

/// Wave format version (`Waves.waveVersion`).
pub const WAVE_VERSION: i32 = 7;

/// The built-in wave table (`Waves.get()`).
#[derive(Debug, Default)]
pub struct Waves {
    spawns: Option<Vec<SpawnGroup>>,
}

impl Waves {
    /// Creates an empty table (lazily populated by [`Waves::get`]).
    pub fn new() -> Self {
        Self { spawns: None }
    }

    /// The vanilla wave table (`Waves.get()`; 28 groups, verbatim upstream order).
    pub fn get(&mut self) -> &[SpawnGroup] {
        if self.spawns.is_none() {
            self.spawns = Some(build_table());
        }
        self.spawns.as_deref().unwrap_or(&[])
    }

    /// `Waves.generate(difficulty)` — applies the 1.12 power curve and a fresh
    /// `ArcRand` (callers that need determinism use [`Waves::generate_seeded`]).
    pub fn generate(difficulty: f32) -> Vec<SpawnGroup> {
        Self::generate_with(
            difficulty.powf(1.12),
            &mut ArcRand::new(0),
            false,
            false,
            false,
        )
    }

    /// Deterministic `generate` seeded with `seed`.
    pub fn generate_seeded(seed: u64, difficulty: f32, attack: bool) -> Vec<SpawnGroup> {
        Self::generate_with(
            difficulty.powf(1.12),
            &mut ArcRand::new(seed),
            attack,
            false,
            false,
        )
    }

    /// `Waves.generate(difficulty, rand, attack, airOnly, naval)`.
    pub fn generate_with(
        difficulty: f32,
        rand: &mut ArcRand,
        attack: bool,
        air_only: bool,
        naval: bool,
    ) -> Vec<SpawnGroup> {
        // Shape-identical (6x5) species table. Row 5 consumes the two `chance`
        // draws during construction, exactly like upstream `new UnitType[][]{...}`.
        let mut species: Vec<[&'static str; 5]> = vec![
            ["dagger", "mace", "fortress", "scepter", "reign"],
            ["nova", "pulsar", "quasar", "vela", "corvus"],
            ["crawler", "atrax", "spiroct", "arkyid", "toxopid"],
            ["risso", "minke", "bryde", "sei", "omura"],
            ["retusa", "oxynoe", "cyerce", "aegires", "navanax"],
            [
                "flare",
                "horizon",
                "zenith",
                if rand.chance(0.5) { "quad" } else { "antumbra" },
                if rand.chance(0.1) { "quad" } else { "eclipse" },
            ],
        ];
        let flying = [false, false, false, false, false, true];
        let naval_flag = [false, false, false, true, true, false];

        let keep: Vec<bool> = (0..species.len())
            .map(|i| {
                if air_only && !flying[i] {
                    return false;
                }
                if naval {
                    flying[i] || naval_flag[i]
                } else {
                    !naval_flag[i]
                }
            })
            .collect();
        let mut filtered: Vec<[&'static str; 5]> = Vec::new();
        for (row, keep_row) in species.drain(..).zip(keep) {
            if keep_row {
                filtered.push(row);
            }
        }
        let fspec = filtered;

        let mut out: Vec<SpawnGroup> = Vec::new();
        let cap = 150;
        let shield_start = 30.0f32;
        let shields_per_wave = 20.0 + difficulty * 30.0;
        let scaling = [1.0f32, 2.0, 3.0, 4.0, 5.0];

        // `createProgression` closure state.
        let mut cur_species: [&'static str; 5] = fspec[0];
        let mut cur_tier: i32 = 0;

        let create_progression = |start: i32,
                                  out: &mut Vec<SpawnGroup>,
                                  cur_species: &mut [&'static str; 5],
                                  cur_tier: &mut i32,
                                  rand: &mut ArcRand| {
            *cur_species = fspec[rand.random_inclusive(fspec.len() as i32 - 1) as usize];
            *cur_tier = 0;
            let mut i = start;
            while i < cap {
                let f = i;
                let next = rand.random_range_int(8, 16)
                    + lerp(5.0, 0.0, difficulty) as i32
                    + *cur_tier * 4;
                let shield_amount = ((i - shield_start as i32) as f32 * shields_per_wave).max(0.0);
                let space = if start == 0 {
                    1
                } else {
                    rand.random_range_int(1, 2)
                };
                let ctier = *cur_tier;

                let unit = cur_species[(*cur_tier).min(4) as usize];
                let mut main = SpawnGroup::new(unit);
                main.unit_amount = if f == start {
                    1
                } else {
                    6 / scaling[ctier as usize] as i32
                };
                main.begin = f;
                main.end = if f + next >= cap { NEVER } else { f + next };
                main.max = 13;
                main.unit_scaling = (if difficulty < 0.4 {
                    rand.random_range_float(2.5, 5.0)
                } else {
                    rand.random_range_float(1.0, 4.0)
                }) * scaling[ctier as usize];
                main.shields = shield_amount;
                main.shield_scaling = shields_per_wave;
                main.spacing = space;
                out.push(main);

                let mut extra = SpawnGroup::new(unit);
                extra.unit_amount = 3 / scaling[ctier as usize] as i32;
                extra.begin = f + next - 1;
                extra.end = f + next + rand.random_range_int(6, 10);
                extra.max = 6;
                extra.unit_scaling = rand.random_range_float(2.0, 4.0);
                extra.spacing = rand.random_range_int(2, 4);
                extra.shields = shield_amount / 2.0;
                extra.shield_scaling = shields_per_wave;
                out.push(extra);

                i += next + 1;
                if *cur_tier < 3 || (rand.chance(0.05) && difficulty > 0.8) {
                    *cur_tier += 1;
                }
                *cur_tier = (*cur_tier).min(3);
                if rand.chance(0.3) {
                    *cur_species = fspec[rand.random_inclusive(fspec.len() as i32 - 1) as usize];
                }
            }
        };

        create_progression(0, &mut out, &mut cur_species, &mut cur_tier, rand);

        let mut step = 5 + rand.random_inclusive(5);
        while step <= cap {
            create_progression(step, &mut out, &mut cur_species, &mut cur_tier, rand);
            step += (rand.random_range_int(15, 30) as f32 * lerp(1.0, 0.5, difficulty)) as i32;
        }

        let boss_wave = (rand.random_range_int(50, 70) as f32 * lerp(1.0, 0.5, difficulty)) as i32;
        let boss_spacing =
            (rand.random_range_int(25, 40) as f32 * lerp(1.0, 0.5, difficulty)) as i32;
        let boss_tier = if difficulty < 0.6 { 3 } else { 4 } as usize;

        // Main boss progression.
        let boss_unit = pick_species(rand, &fspec, boss_tier);
        let mut main_boss = SpawnGroup::new(boss_unit);
        main_boss.unit_amount = 1;
        main_boss.begin = boss_wave;
        main_boss.spacing = boss_spacing;
        main_boss.end = NEVER;
        main_boss.max = 16;
        main_boss.unit_scaling = boss_spacing as f32;
        main_boss.shield_scaling = shields_per_wave;
        main_boss.effect = Some("boss".to_owned());
        out.push(main_boss);

        // Alt boss progression.
        let boss_unit = pick_species(rand, &fspec, boss_tier);
        let mut alt_boss = SpawnGroup::new(boss_unit);
        alt_boss.unit_amount = 1;
        alt_boss.begin = boss_wave + rand.random_range_int(3, 5) * boss_spacing;
        alt_boss.spacing = boss_spacing;
        alt_boss.end = NEVER;
        alt_boss.max = 16;
        alt_boss.unit_scaling = boss_spacing as f32;
        alt_boss.shield_scaling = shields_per_wave;
        alt_boss.effect = Some("boss".to_owned());
        out.push(alt_boss);

        let final_boss_start = 120 + rand.random_inclusive(30);

        // Final boss waves.
        let boss_unit = pick_species(rand, &fspec, boss_tier);
        let mut final_boss = SpawnGroup::new(boss_unit);
        final_boss.unit_amount = 1;
        final_boss.begin = final_boss_start;
        final_boss.spacing = boss_spacing / 2;
        final_boss.end = NEVER;
        final_boss.max = 16;
        final_boss.unit_scaling = boss_spacing as f32;
        final_boss.shields = 500.0;
        final_boss.shield_scaling = shields_per_wave * 4.0;
        final_boss.effect = Some("boss".to_owned());
        out.push(final_boss);

        // Final boss waves (alt).
        let boss_unit = pick_species(rand, &fspec, boss_tier);
        let mut final_boss_alt = SpawnGroup::new(boss_unit);
        final_boss_alt.unit_amount = 1;
        final_boss_alt.begin = final_boss_start + 15;
        final_boss_alt.spacing = boss_spacing / 2;
        final_boss_alt.end = NEVER;
        final_boss_alt.max = 16;
        final_boss_alt.unit_scaling = boss_spacing as f32;
        final_boss_alt.shields = 500.0;
        final_boss_alt.shield_scaling = shields_per_wave * 4.0;
        final_boss_alt.effect = Some("boss".to_owned());
        out.push(final_boss_alt);

        // Add megas to heal the base.
        if attack && difficulty >= 0.5 {
            let amount = rand.random_range_int(1, 3 + (difficulty * 2.0) as i32);
            for _ in 0..amount {
                let wave = rand.random_range_int(3, 20);
                let mut mega = SpawnGroup::new("mega");
                mega.unit_amount = 1;
                mega.begin = wave;
                mega.end = wave;
                mega.max = 16;
                out.push(mega);
            }
        }

        // Shift back waves on higher difficulty for a harder start.
        let shift = ((difficulty * 14.0 - 5.0) as i32).max(0);
        for group in &mut out {
            group.begin -= shift;
            group.end -= shift;
        }

        out
    }
}

/// `Structs.random(rand, species)[tier]`.
fn pick_species(rand: &mut ArcRand, species: &[[&'static str; 5]], tier: usize) -> &'static str {
    species[rand.random_inclusive(species.len() as i32 - 1) as usize][tier]
}

/// `Mathf.lerp`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Builds the verbatim `Waves.get()` table (`Waves.java:19-253`).
fn build_table() -> Vec<SpawnGroup> {
    let mut out: Vec<SpawnGroup> = Vec::with_capacity(28);

    // 1
    let mut g = SpawnGroup::new("dagger");
    g.end = 10;
    g.unit_scaling = 2.0;
    g.max = 30;
    out.push(g);
    // 2
    let mut g = SpawnGroup::new("crawler");
    g.begin = 4;
    g.end = 13;
    g.unit_amount = 2;
    g.unit_scaling = 1.5;
    out.push(g);
    // 3
    let mut g = SpawnGroup::new("flare");
    g.begin = 12;
    g.end = 16;
    g.unit_scaling = 1.0;
    out.push(g);
    // 4
    let mut g = SpawnGroup::new("dagger");
    g.begin = 11;
    g.unit_scaling = 1.7;
    g.spacing = 2;
    g.max = 4;
    g.shield_scaling = 25.0;
    out.push(g);
    // 5
    let mut g = SpawnGroup::new("pulsar");
    g.begin = 13;
    g.spacing = 3;
    g.unit_scaling = 0.5;
    g.max = 25;
    out.push(g);
    // 6
    let mut g = SpawnGroup::new("mace");
    g.begin = 7;
    g.spacing = 3;
    g.unit_scaling = 2.0;
    g.end = 30;
    out.push(g);
    // 7
    let mut g = SpawnGroup::new("dagger");
    g.begin = 12;
    g.unit_scaling = 1.0;
    g.unit_amount = 4;
    g.spacing = 2;
    g.shield_scaling = 20.0;
    g.max = 14;
    out.push(g);
    // 8
    let mut g = SpawnGroup::new("mace");
    g.begin = 28;
    g.spacing = 3;
    g.unit_scaling = 1.0;
    g.end = 40;
    g.shield_scaling = 20.0;
    out.push(g);
    // 9
    let mut g = SpawnGroup::new("spiroct");
    g.begin = 45;
    g.spacing = 3;
    g.unit_scaling = 1.0;
    g.max = 10;
    g.shield_scaling = 30.0;
    g.shields = 100.0;
    g.effect = Some("overdrive".to_owned());
    out.push(g);
    // 10
    let mut g = SpawnGroup::new("pulsar");
    g.begin = 120;
    g.spacing = 2;
    g.unit_scaling = 3.0;
    g.unit_amount = 5;
    g.effect = Some("overdrive".to_owned());
    out.push(g);
    // 11
    let mut g = SpawnGroup::new("flare");
    g.begin = 16;
    g.unit_scaling = 1.0;
    g.spacing = 2;
    g.shield_scaling = 20.0;
    g.max = 20;
    out.push(g);
    // 12
    let mut g = SpawnGroup::new("quasar");
    g.begin = 82;
    g.spacing = 3;
    g.unit_amount = 4;
    g.unit_scaling = 3.0;
    g.shield_scaling = 30.0;
    g.effect = Some("overdrive".to_owned());
    out.push(g);
    // 13
    let mut g = SpawnGroup::new("pulsar");
    g.begin = 41;
    g.spacing = 5;
    g.unit_amount = 1;
    g.unit_scaling = 3.0;
    g.shields = 640.0;
    g.max = 25;
    out.push(g);
    // 14
    let mut g = SpawnGroup::new("fortress");
    g.begin = 40;
    g.spacing = 5;
    g.unit_amount = 2;
    g.unit_scaling = 2.0;
    g.max = 20;
    g.shield_scaling = 30.0;
    out.push(g);
    // 15
    let mut g = SpawnGroup::new("nova");
    g.begin = 35;
    g.spacing = 3;
    g.unit_amount = 4;
    g.effect = Some("overdrive".to_owned());
    g.items = Some(ItemStack::new("blast-compound", 60));
    g.end = 60;
    out.push(g);
    // 16
    let mut g = SpawnGroup::new("dagger");
    g.begin = 42;
    g.spacing = 3;
    g.unit_amount = 4;
    g.effect = Some("overdrive".to_owned());
    g.items = Some(ItemStack::new("pyratite", 100));
    g.end = 130;
    g.max = 30;
    out.push(g);
    // 17
    let mut g = SpawnGroup::new("horizon");
    g.begin = 40;
    g.unit_amount = 2;
    g.spacing = 2;
    g.unit_scaling = 2.0;
    g.shield_scaling = 20.0;
    out.push(g);
    // 18
    let mut g = SpawnGroup::new("flare");
    g.begin = 50;
    g.unit_amount = 4;
    g.unit_scaling = 3.0;
    g.spacing = 5;
    g.shields = 100.0;
    g.shield_scaling = 10.0;
    g.effect = Some("overdrive".to_owned());
    g.max = 20;
    out.push(g);
    // 19
    let mut g = SpawnGroup::new("zenith");
    g.begin = 50;
    g.unit_amount = 2;
    g.unit_scaling = 3.0;
    g.spacing = 5;
    g.max = 16;
    g.shield_scaling = 30.0;
    out.push(g);
    // 20
    let mut g = SpawnGroup::new("nova");
    g.begin = 53;
    g.unit_amount = 2;
    g.unit_scaling = 3.0;
    g.spacing = 4;
    g.shield_scaling = 30.0;
    out.push(g);
    // 21
    let mut g = SpawnGroup::new("atrax");
    g.begin = 31;
    g.unit_amount = 4;
    g.unit_scaling = 1.0;
    g.spacing = 3;
    g.shield_scaling = 10.0;
    out.push(g);
    // 22
    let mut g = SpawnGroup::new("scepter");
    g.begin = 41;
    g.unit_amount = 1;
    g.unit_scaling = 1.0;
    g.spacing = 30;
    g.shield_scaling = 30.0;
    out.push(g);
    // 23
    let mut g = SpawnGroup::new("reign");
    g.begin = 81;
    g.unit_amount = 1;
    g.unit_scaling = 1.0;
    g.spacing = 40;
    g.shield_scaling = 30.0;
    out.push(g);
    // 24
    let mut g = SpawnGroup::new("antumbra");
    g.begin = 120;
    g.unit_amount = 1;
    g.unit_scaling = 1.0;
    g.spacing = 40;
    g.shield_scaling = 30.0;
    out.push(g);
    // 25
    let mut g = SpawnGroup::new("vela");
    g.begin = 100;
    g.unit_amount = 1;
    g.unit_scaling = 1.0;
    g.spacing = 30;
    g.shield_scaling = 30.0;
    out.push(g);
    // 26
    let mut g = SpawnGroup::new("corvus");
    g.begin = 145;
    g.unit_amount = 1;
    g.unit_scaling = 1.0;
    g.spacing = 35;
    g.shield_scaling = 30.0;
    g.shields = 100.0;
    out.push(g);
    // 27
    let mut g = SpawnGroup::new("horizon");
    g.begin = 90;
    g.unit_amount = 2;
    g.unit_scaling = 3.0;
    g.spacing = 4;
    g.shields = 40.0;
    g.shield_scaling = 30.0;
    out.push(g);
    // 28
    let mut g = SpawnGroup::new("toxopid");
    g.begin = 210;
    g.unit_amount = 1;
    g.unit_scaling = 1.0;
    g.spacing = 35;
    g.shields = 1000.0;
    g.shield_scaling = 35.0;
    out.push(g);

    out
}

/// Wave-difficulty hook (plan 12 owns the campaign implementation).
pub trait WaveDifficulty {
    /// Enemy spawn multiplier for a planet (`difficulty`-adjacent).
    fn enemy_spawn_multiplier(&self, _planet: Option<&str>) -> f32 {
        1.0
    }

    /// Unit health multiplier for a planet.
    fn unit_health_multiplier(&self, _planet: Option<&str>) -> f32 {
        1.0
    }
}

/// Identity difficulty shim used until plan 12 supplies `CampaignRules`.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdentityDifficulty;

impl WaveDifficulty for IdentityDifficulty {}

/// Ensures the content registry has the units named by the vanilla table.
///
/// The table references unit names; plan 12's campaign loads it after content.
pub fn table_units_registered(content: &ContentRegistry, waves: &Waves) -> bool {
    waves
        .spawns
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .all(|group| content.unit_by_name(&group.unit).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    #[test]
    fn builtin_table_has_upstream_shape() {
        let mut waves = Waves::new();
        let table = waves.get();
        assert_eq!(table.len(), 28, "28 upstream groups");
        assert_eq!(WAVE_VERSION, 7);
        // Spot-check a few entries.
        assert_eq!(table[0].unit, "dagger");
        assert_eq!(table[0].end, 10);
        assert_eq!(table[8].unit, "spiroct");
        assert_eq!(table[8].effect.as_deref(), Some("overdrive"));
        assert_eq!(table[27].unit, "toxopid");
        assert_eq!(table[27].shields, 1000.0);
    }

    #[test]
    fn table_units_resolve_in_content() {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        let mut waves = Waves::new();
        let _ = waves.get();
        assert!(table_units_registered(&registry, &waves));
    }

    #[test]
    fn generate_is_deterministic_and_bounded() {
        let a = Waves::generate_seeded(1, 1.0, false);
        let b = Waves::generate_seeded(1, 1.0, false);
        assert_eq!(a, b, "same seed -> same groups");
        assert!(!a.is_empty());
        for group in &a {
            assert!(group.max >= group.unit_amount || group.unit_amount == 0);
            assert!(!group.unit.is_empty());
        }
        // Air-only keeps only the flying family.
        let air = Waves::generate_with(1.0, &mut ArcRand::new(1), false, true, false);
        assert!(air.iter().all(|g| matches!(
            g.unit.as_str(),
            "flare" | "horizon" | "zenith" | "quad" | "antumbra" | "eclipse"
        )));
    }

    #[test]
    fn generate_matches_java_golden() {
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/golden/units/wave_generate.json"))
                .expect("wave golden parses");
        let cases = golden["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for case in cases {
            let seed = case["seed"].as_u64().expect("seed");
            let difficulty = case["difficulty"].as_f64().expect("difficulty") as f32;
            let attack = case["attack"].as_bool().expect("attack");
            let air_only = case["airOnly"].as_bool().expect("airOnly");
            let naval = case["naval"].as_bool().expect("naval");
            let expected = case["groups"].as_array().expect("groups");
            let groups =
                Waves::generate_with(difficulty, &mut ArcRand::new(seed), attack, air_only, naval);
            assert_eq!(
                groups.len(),
                expected.len(),
                "group count seed={seed} difficulty={difficulty} attack={attack} air={air_only} naval={naval}"
            );
            for (got, want) in groups.iter().zip(expected) {
                assert_eq!(got.unit, want["unit"].as_str().unwrap(), "unit");
                assert_eq!(got.begin, want["begin"].as_i64().unwrap() as i32, "begin");
                assert_eq!(got.end, want["end"].as_i64().unwrap() as i32, "end");
                assert_eq!(
                    got.spacing,
                    want["spacing"].as_i64().unwrap() as i32,
                    "spacing"
                );
                assert_eq!(got.max, want["max"].as_i64().unwrap() as i32, "max");
                assert_eq!(
                    got.unit_amount,
                    want["unitAmount"].as_i64().unwrap() as i32,
                    "unitAmount"
                );
                assert_eq!(
                    got.unit_scaling.to_bits(),
                    want["unitScalingBits"]
                        .as_str()
                        .unwrap()
                        .parse::<u32>()
                        .unwrap(),
                    "unitScaling"
                );
                assert_eq!(
                    got.shields.to_bits(),
                    want["shieldsBits"]
                        .as_str()
                        .unwrap()
                        .parse::<u32>()
                        .unwrap(),
                    "shields"
                );
                assert_eq!(
                    got.shield_scaling.to_bits(),
                    want["shieldScalingBits"]
                        .as_str()
                        .unwrap()
                        .parse::<u32>()
                        .unwrap(),
                    "shieldScaling"
                );
            }
        }
    }
}
