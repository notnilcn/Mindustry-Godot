<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H00 — Cross-cutting blockers: Java `Env` bits, player plans without a builder, new baseline

Date: 2026-10-14. Runs FIRST, alone, in the main checkout (`REPO`, session 1,
bridge 6970, display `:10`), before any H01-H09 session starts. Read
`docs/H-environment.md` first (for H00, `WT` = `REPO`).

Why this plan exists: every round-G in-engine check that needed the player's
own unit failed or used a workaround because the player mech never spawned
on Ground Zero (`spawn_player_unit: no supported core in the loaded sector`).
The root cause is one wrong bit table. A second bug hid it: when there is no
player unit, player placements were applied instantly instead of being
refused, so building "worked" without a builder. Both are fixed here, then
the new test baseline is recorded for every later plan.

Start state: `git log --oneline -1` is `ffbe84dd` or later, and
`git status --short` shows only untracked docs. If `HEAD` has code commits
after `ffbe84dd`, read them (`git log --oneline ffbe84dd..HEAD`) and continue
only if none of them touches `core/game/planet.rs` or `core/sim/mod.rs`
`fn command`; otherwise stop and report.

Drift rule: port line numbers below were read on `ffbe84dd`. Re-locate with
`grep -n`. If the code already does what a step asks, skip the step and say
so in the summary.

---

## 1. Inventory

| Java | Port | Status |
|---|---|---|
| `J/world/meta/Env.java:5-26` bit constants | `core/game/planet.rs:127-139` `env_bit` | **WRONG** (enum order) — T1 |
| same | `core/world/build.rs:40-52` `java_env_bit` (private) | correct duplicate — T1 removes the duplicate |
| same | `core/content/registries/units/mod.rs` inside `UnitTypeDef::supports_env` (local `fn env_bit`, ~line 1466) | correct duplicate — T1 removes the duplicate |
| same | `core/render/env.rs:12-26` `TERRESTRIAL..OXYGEN` (`u32`) | correct (renderer); leave as is |
| `J/Vars.java:65` `defaultEnv` = 233 | `core/io/json/rules.rs:26` `DEFAULT_ENV = 233` | correct |
| `J/type/Planet.java:87` `defaultEnv` (Serpulo 233), `J/content/Planets.java:56` Erekir `scorching|terrestrial` (17), `:117` Tantros `underwater|terrestrial` (5), `:182` sun-adjacent `space` (2) | `Planet::from_def` -> `env_mask(&def.default_env)` | wrong values (31 / 33 / 65 / 128) until T1 |
| `J/type/Planet.java:296-307` `applyRules` sets `rules.env = defaultEnv` | `core/game/planet.rs` `apply_rules` (`rules.env = self.default_env`) | correct shape; wrong input until T1 |
| `J/type/UnitType.java:49-53` `envRequired 0 / envEnabled terrestrial / envDisabled scorching`, `:757` `supportsEnv` | `UnitTypeDef::supports_env` | correct (Java bits) — so alpha is refused on a session with env 31 (bit 16 = scorching) |
| `J/maps/Map.java:106` "serpulo + scorching -> erekir" | `core/maps/map.rs:174` | correct; with env 31 stored in a port-written save it wrongly flips the planet — T1 test covers |
| `InputHandler.flushPlans` -> `player.unit().addBuild(plan)`; a dead player's unit is `Nulls.unit` whose `addBuild` does nothing | `core/sim/mod.rs` `fn command`, `SimCommand::Place`/`Break` with `player: Some(_)` | **WRONG**: falls through to an instant `runtime_place` / P0 break when `player_can_build()` is false — T2 |

Ledger items this plan closes or unblocks: PAR-0174 (player respawn,
`fixing`), PAR-0215 (live `player:Some` placements instant), PAR-0189
(queued-plan ghost staging impossible), PAR-0052 (player mech spawns —
re-verify), plus every round-G task whose in-engine half was "blocked: no
player unit" (listed in the H01-H08 plans).

---

## 2. Tasks

### T1. One Java-faithful `Env` bit table

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. Main: Ground Zero via "Go" -> `env` 31, `has_player_unit()` false, log `spawn_player_unit: no supported core in the loaded sector` (`runs/20261014-s2-verify/H00-T1-before-gz.png`). Experiment with Change 1 only (worktree, reverted after): `env` 233, `planet` "serpulo", `has_player_unit()` true, log `spawn_player_unit: core unit team 1 spawned`, 0 panics, alpha drawn on the core (`H00-T1-after-gz-alpha.png`). `player_unit_pos()` reads **(1036, 444)**, not (1032, 440): the sim building `Pos` is still `(tile+0.5)*8` until H01 T1 — accept (1036,444) here. Change 7: the resume path already re-applies planet rules (the imported Java save stores env 233, yet the resumed session read 31 before and 233 after Change 1) — no code needed, record "already applied on resume". Text fixes made below: `planet` module is `core/game.rs` (no `game/mod.rs`); the registry has no `unit_by_name`/`planet_by_name` — use `registry.unit_id("alpha")` + `registry.unit(id)` and `registry.planet_id("tantros")` (`content/load.rs:712-717`; tantros exists).

**Symptom / acceptance.** On Ground Zero, `JSON.parse_string($MC.get_rules_json()).env`
prints `31`; `$H.has_player_unit()` is `false`; the log has
`spawn_player_unit: no supported core in the loaded sector`. After: `env`
is `233`, `has_player_unit()` is `true` right after entering the sector,
and the log has `spawn_player_unit: core unit team 1 spawned`.

**Java.** `J/world/meta/Env.java:5-26`:
```java
terrestrial = 1,
space = 1 << 1,
underwater = 1 << 2,
spores = 1 << 3,
scorching = 1 << 4,
groundOil = 1 << 5,
groundWater = 1 << 6,
oxygen = 1 << 7,
any = 0xffffffff,
none = 0;
```
`J/type/Planet.java:87`: `public int defaultEnv = Env.terrestrial | Env.spores | Env.groundOil | Env.groundWater | Env.oxygen;` (= 233).
`J/content/Planets.java:56` (erekir): `defaultEnv = Env.scorching | Env.terrestrial;` (= 17).
`J/content/Planets.java:117` (tantros): `defaultEnv = Env.underwater | Env.terrestrial;` (= 5).
`J/content/Planets.java:182`: `defaultEnv = Env.space;` (= 2).
`J/type/UnitType.java:53`: `public int envDisabled = Env.scorching;` and `:757`
`return (envEnabled & env) != 0 && (envDisabled & env) == 0 && (envRequired == 0 || (envRequired & env) == envRequired);`

**Port today.**
* `core/game/planet.rs:127-139`:
  ```rust
  pub const fn env_bit(flag: EnvFlag) -> i32 {
      match flag {
          EnvFlag::Terrestrial => 1 << 0,
          EnvFlag::Spores => 1 << 1,
          EnvFlag::GroundOil => 1 << 2,
          EnvFlag::GroundWater => 1 << 3,
          EnvFlag::Oxygen => 1 << 4,
          EnvFlag::Scorching => 1 << 5,
          EnvFlag::Underwater => 1 << 6,
          EnvFlag::Space => 1 << 7,
      }
  }
  ```
  This is the declaration order of `enum EnvFlag`
  (`core/content/registries/planets.rs:123`), not Java's bits. Serpulo folds
  to `1|2|4|8|16 = 31`, which under Java bits contains `scorching` (16) and
  `space` (2).
* Consumers that already use Java bits and therefore disagree with the
  session env: `UnitTypeDef::supports_env` (`core/content/registries/units/mod.rs`,
  `grep -n "fn supports_env" -A30`), `world::build::supports_env_bits`
  (`core/world/build.rs:70`), `maps::ENV_SCORCHING = 1 << 4`
  (`core/maps/map.rs:25`), `core/sim/unit_runtime.rs:1917`
  (`oxygen: rules.env & (1 << 7) != 0`), the renderer (`core/render/env.rs`).
* Tests pinned to the wrong values: `core/game/planet.rs` `env_mask_bits`
  (`assert_eq!(mask, 1 | 16)` for terrestrial|oxygen and
  `assert_eq!(env_mask(&[EnvFlag::Space]), 128)`).

**Changes.**
1. `core/game/planet.rs`: replace the body of `env_bit` with Java's table and
   cite the source in the doc comment:
   ```rust
   /// `Env` bit for a flag (`mindustry.world.meta.Env`, `Env.java:5-26`).
   /// This is the only `Env` bit table in `mind-core`; every env mask
   /// (`Rules.env`, `Block`/`UnitType` `envEnabled/envDisabled/envRequired`)
   /// is built from it.
   pub const fn env_bit(flag: EnvFlag) -> i32 {
       match flag {
           EnvFlag::Terrestrial => 1,
           EnvFlag::Space => 1 << 1,
           EnvFlag::Underwater => 1 << 2,
           EnvFlag::Spores => 1 << 3,
           EnvFlag::Scorching => 1 << 4,
           EnvFlag::GroundOil => 1 << 5,
           EnvFlag::GroundWater => 1 << 6,
           EnvFlag::Oxygen => 1 << 7,
       }
   }
   ```
2. `core/world/build.rs`: delete the private `const fn java_env_bit` and
   replace its uses (in `fn mask_bits` and `supports_env_bits`) with
   `crate::game::planet::env_bit`. Check the module path compiles
   (`grep -n "pub mod planet" core/game.rs`; there is no `core/game/mod.rs`); if `planet` is not `pub`,
   use the path the compiler suggests — do not make a copy.
3. `core/content/registries/units/mod.rs` `UnitTypeDef::supports_env`:
   delete the nested `fn env_bit` and call `crate::game::planet::env_bit`.
   If this creates a module cycle error (it should not; both are in
   `mind-core`), keep the nested fn but make it a one-line call to
   `crate::game::planet::env_bit`.
4. `core/maps/map.rs:25`: change to
   `pub const ENV_SCORCHING: i32 = crate::game::planet::env_bit(EnvFlag::Scorching);`
   (import `EnvFlag` from where `planets.rs` exports it:
   `grep -rn "pub use.*EnvFlag" core/content`).
5. `core/sim/unit_runtime.rs` (~1917): replace `(1 << 7)` with
   `crate::game::planet::env_bit(crate::content::EnvFlag::Oxygen)` (same
   import check as step 4).
6. Do NOT change `core/render/env.rs` (it is `u32` and already correct);
   add one test there instead (below) that pins it to `env_bit`.
7. Saves written by the port before this fix store `env: 31` in their rules.
   Java re-applies the planet rules on every sector load
   (`J/io/SaveVersion.java:203` `state.rules.sector.planet.applyRules(state.rules)`;
   `J/core/World.java:327`). Check the port does the same on its sector
   load path: `grep -n "apply_rules(" client/rust/mind-gdext/src/campaign.rs core/game/play.rs`
   — there must be a call on the "resume a saved sector" path (expected:
   `gdext/campaign.rs` ~2889, `planet_record.apply_rules(registry, &mut self.session.rules, false, true)`).
   Read that function and confirm it runs for a resume (not only a fresh
   launch). If it does not run on resume, add the call right after the
   save's rules are adopted, with the Java citation above. Record which
   case you found in the commit body.

**Tests.**
* `core/game/planet.rs` `env_mask_bits` -> rewrite:
  ```rust
  #[test]
  fn env_mask_bits_match_java_env() {
      assert_eq!(env_mask(&[EnvFlag::Terrestrial, EnvFlag::Oxygen]), 1 | 128);
      assert_eq!(env_mask(&[EnvFlag::Space]), 2);
      assert_eq!(env_bit(EnvFlag::Underwater), 4);
      assert_eq!(env_bit(EnvFlag::Spores), 8);
      assert_eq!(env_bit(EnvFlag::Scorching), 16);
      assert_eq!(env_bit(EnvFlag::GroundOil), 32);
      assert_eq!(env_bit(EnvFlag::GroundWater), 64);
  }
  ```
* New test in the same module, `planet_default_env_matches_java`:
  build `serpulo`, `erekir` and `tantros` (look them up with `registry.planet_id("<name>")`
  + `registry.planet(id)`, `content/load.rs:717`; `planet_by_name` does not exist) with `Planet::from_def(&registry, def, 0.0)` (copy the
  setup of the existing `apply_rules_sets_env_and_attributes` test) and
  assert `default_env` == `233`, `17`, `5`. Also assert
  `serpulo.default_env == crate::io::json::rules::DEFAULT_ENV`.
* Extend `core/game/play.rs` test `sector_preset_rules_apply` with
  `assert_eq!(session.rules.env, 233);` after the existing asserts.
* New test in `core/content/registries/units/mod.rs` tests (or the nearest
  unit-content test module: `grep -n "mod tests" core/content/registries/units/mod.rs`),
  `alpha_supports_serpulo_not_erekir`:
  ```rust
  let alpha = registry.unit(registry.unit_id("alpha").unwrap()).unwrap(); // no unit_by_name (content/load.rs:712)
  assert!(alpha.supports_env(233));
  assert!(!alpha.supports_env(17)); // erekir: envDisabled = scorching
  ```
  (`registry` = `create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap()`;
  check how neighbouring tests build it.)
* New test in `core/render/env.rs` tests, `render_env_bits_match_planet_env_bit`:
  `assert_eq!(SCORCHING as i32, crate::game::planet::env_bit(EnvFlag::Scorching));`
  and the same for each of the 8 flags.
* Full suites (`H-environment.md` §2). Expected fallout: golden files that
  serialise `rules.env` for Serpulo/Erekir sessions. For each failing golden:
  regenerate it with the command in `H-environment.md` §2, `git diff` it,
  and accept ONLY if every changed line is `env` (31 -> 233, 33 -> 17, 65 -> 5,
  128 -> 2) or a checksum/hash derived from the rules (e.g. `rules_checksum`).
  Any other diff (unit counts, positions, items) means the env change altered
  behaviour: write it down in the summary, and accept it only if you can
  name the Java behaviour (e.g. "alpha now spawns, so the golden has one more
  unit"). Update the sha256 in `parity/golden_manifest.json` for each
  regenerated file.

**Verify in-engine.** (after T2; build once for both)
```bash
CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh
tools/session-play.sh 1 --play
# enter Ground Zero (H-environment §4.1, "Go" path = resume of the imported save)
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'; sleep 5
$G --eval "return JSON.parse_string($MC.get_rules_json()).env"      # 233
$G --eval "return JSON.parse_string($MC.get_rules_json()).planet"   # "serpulo"
$G --eval "return $H.has_player_unit()"                             # true
$G --eval "return $H.player_unit_pos()"                             # (1036, 444) until H01 T1 lands (half-tile building Pos), then (1032, 440)
$G --screenshot runs/20261014-s1-H00-T1/gz-alpha.png                # alpha drawn at the core
grep -c "no supported core" $(ls -t $U/logs/* | head -1)            # 0
grep -c "panic" $(ls -t $U/logs/* | head -1)                         # 0
```
If `get_rules_json` returns a string without `env`, use
`$H.logic_rules_probe()` instead and record which seam showed it.
If `env` is still 31 after the resume: the saved rules were adopted without
re-applying the planet rules (Change 7) — fix that before committing.

**Pitfalls.**
* `env_bit` is `const fn`; keep it `const` (step 4 uses it in a `const`).
* Do not "fix" `EnvFlag` declaration order to match the bits: the enum is
  serialised by name in content specs; order is irrelevant once `env_bit`
  is right.
* `EnvMask::any()` maps to `-1` in the mask helpers; leave that logic alone.
* Erekir sessions change too (33 -> 17). Erekir's `scorching` bit was
  previously 32 (= Java `groundOil`), so Erekir-only blocks/units gated on
  scorching start behaving differently. That is the fix, not a regression;
  note any Erekir test that changes.

**Commit.** `Fixes PAR-0174: Java Env bits for planet rules (Serpulo env 233), so the core spawns the player unit`
(body: the 31-vs-233 explanation, the duplicate tables removed, Change 7
finding, goldens regenerated with reason, tests added).

---

### T2. Player build/break plans need a builder unit (no instant fallback)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. Main (no unit): `apply_sim_command_json` place `{"player":0,"team":1}` copper-wall at (135,58) built instantly and core copper stayed 200; a `player:0` break removed it instantly. Experiment with Changes 1-3 (+ env bug kept, so no unit): place and break both dropped (no building/plan, copper unchanged), 0 panics. Caveat: the JSON `place` op defaults `"team"` to **0** when omitted, and team 0 has no core, so such a command stays instant (derelict wall) — always pass `"team":1` in gmcp lines (live input sends `player_team()`). With Changes 1-3 + T1 (unit present) the click recipe queues correctly but the readback below was racy (the alpha finishes a copper wall in ~33 ticks, before the next eval: plans `[]`, wall present, copper -6). Recipe fixed in place: pause first, click, `step(10)`, then read (`progress` 0.31, ghost + beam in `runs/20261014-s2-verify/H00-T2-plan-queued.png`). `center_on_tile` is ignored while the camera follows the player; `tile_to_screen` is still correct.

**Symptom / acceptance.** With no player unit (dead, or the old env bug),
left-click placement and right-drag removal in a real game act instantly
and for free: `SimCommand::Place { player: Some(_) }` falls through to
`runtime_place`, and `SimCommand::Break { player: Some(_) }` falls through
to the P0 instant break. Java: placement goes through
`player.unit().addBuild(plan)`; with no unit it is a no-op. After: in a
world where the command's team has a core, a `player: Some` place/break
with no build-capable player unit changes nothing (no building, no plan, no
refund) and logs at debug level. A world with no core for that team (the
port's dev scaffold world and the spine/determinism scenarios) keeps the
old instant behaviour, documented as a port-only dev affordance.

**Java.**
* `J/input/InputHandler.java` `flushPlans(Seq<BuildPlan> plans)` (search
  `void flushPlans`): for each valid plan, `player.unit().addBuild(copy)`.
* `J/entities/comp/BuilderComp.java` `addBuild(BuildPlan place, boolean tail)`:
  `if(!canBuild()) return;` (search `void addBuild`; `canBuild()` is
  `type.buildSpeed > 0 && buildSpeedMultiplier > 0`).
* When the player is dead, `player.unit()` is `Nulls.unit`
  (`J/entities/Nulls.java`), whose type has `buildSpeed = 0`, so nothing is
  queued. Read both methods and paste the exact lines into the commit body.

**Port today.** `core/sim/mod.rs`, `fn command` (search
`if player.is_some() && self.player_can_build()`):
```rust
if player.is_some() && self.player_can_build() {
    if !self.queue_player_place(*x, *y, id, rot) { ... }
    self.commands_applied = self.commands_applied.wrapping_add(1);
    return Ok(());
}
if self.runtime_place(*x, *y, id, rot, *team) { ... }   // <- reached with player Some + no builder
```
and below it:
```rust
SimCommand::Break { x, y, player: Some(_) } if self.player_can_build() => { ... }
```
(a `Break { player: Some(_) }` with no builder falls to `_ => {}` and then
`command.to_p0()` -> instant break). `player_can_build()` is in
`core/sim/builder.rs` (~886). The private helper
`fn team_core(world: &World, team: u8) -> Option<Entity>` is in
`core/sim/builder.rs` (~238).

**Changes.**
1. `core/sim/builder.rs`, inside the existing `impl Sim` block that holds
   `player_can_build`, add:
   ```rust
   /// Whether `team` owns a core. Player plans with no build-capable unit
   /// are dropped in any world where the player's team has a core (Java:
   /// `Nulls.unit.addBuild` is a no-op, `BuilderComp.java` `addBuild`
   /// `if(!canBuild()) return;`). Core-less dev worlds keep the port's
   /// instant placement (no Java counterpart; spine/determinism scenarios).
   pub(crate) fn team_has_core(&self, team: u8) -> bool {
       team_core(&self.ecs.0, team).is_some()
   }
   ```
2. `core/sim/mod.rs` `SimCommand::Place` arm: directly after the existing
   `if player.is_some() && self.player_can_build() { ... }` block, add:
   ```rust
   if player.is_some() && self.team_has_core(*team) {
       // No builder: Java's `player.unit()` is `Nulls.unit`, `addBuild`
       // returns at `if(!canBuild()) return;` (BuilderComp.java).
       log::debug!("player plan at ({x}, {y}) dropped: no build-capable player unit");
       self.commands_applied = self.commands_applied.wrapping_add(1);
       return Ok(());
   }
   ```
3. `core/sim/mod.rs` `SimCommand::Break`: add a second arm right after the
   existing `Break { player: Some(_) } if self.player_can_build()` arm:
   ```rust
   SimCommand::Break { x, y, player: Some(_) }
       if self.team_has_core(self.player_command_team()) =>
   {
       log::debug!("player break at ({x}, {y}) dropped: no build-capable player unit");
       self.commands_applied = self.commands_applied.wrapping_add(1);
       return Ok(());
   }
   ```
   and add next to `team_has_core` in `builder.rs`:
   ```rust
   /// Team of player-issued commands that carry no team (`Break`): the
   /// session `rules.defaultTeam` (`Vars.player.team()` in campaign).
   pub(crate) fn player_command_team(&self) -> u8 {
       crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.default_team)
   }
   ```
   Check `rules.default_team` is a `u8` (`grep -n "pub default_team" core/game/rules.rs core/io/json/rules.rs`);
   convert if it is another integer type. Check `rules_of` returns
   `Option<&Rules>` (`grep -n "pub fn rules_of" core/game/runtime.rs`).
4. Do not touch the `player: None` paths (API `place_block`, scenarios,
   STDB replay) — they stay instant.

**Tests.** In `core/sim/builder.rs` tests (next to `sim_with_player`):
* `player_place_without_unit_is_dropped_when_team_has_core`: build the
  `sim_with_player()` world WITHOUT spawning the unit (copy its body, omit
  `spawn_player_unit`); give the core copper (use the existing helper the
  module uses to fill core items — `grep -n "fn .*core.*item" core/sim/builder.rs`);
  command `Place { x: 20, y: 20, block: wall, rotation: 0, team: 1, player: Some(0) }`;
  tick 120 times; assert `sim.grid.entity_at(TilePos::new(20, 20)).is_none()`,
  `sim.player_build_plans().is_empty()` (find the exact accessor:
  `grep -n "pub fn player_build_plans\|pub fn player_plans" core/sim/builder.rs`),
  and the core copper is unchanged.
* `player_break_without_unit_is_dropped_when_team_has_core`: same world,
  place a copper wall with `player: None` at (20,20), then
  `Break { x: 20, y: 20, player: Some(0) }`, tick 120; the wall is still there.
* `player_place_without_core_stays_instant`: `Sim::new(7, 64, 64, AIR, AIR)`
  + runtimes, no core; `Place { ..., team: 1, player: Some(0) }` of a copper
  wall; one tick; the wall exists (dev affordance pinned).
* Re-run the existing `sim_with_player` tests (`cargo test -p mind-core --lib sim::builder`)
  — they must still pass (they have a unit).
* Full suites. If a mind-headless scenario or golden fails, read it: if it
  issues `player: Some` commands in a world with a team core and no player
  unit, it was relying on the bug. Fix the scenario input by spawning the
  player unit first (preferred) or switch that command to `player: None`
  if the scenario tests API placement; name the scenario in the commit body.

**Verify in-engine.** (same build as T1; Ground Zero entered as in T1)
```bash
$G --eval "return $H.has_player_unit()"                                  # true
$G --eval "return $H.debug_add_core_items(\"copper\", 200)"
$G --eval "$I.select_block_by_name(\"copper-wall\"); return true"
# the camera follows the alpha, so center_on_tile is overridden; tile_to_screen is still right
$G --eval "$H.set_paused(true); return $C.tile_to_screen(135, 58)"     # -> X,Y (verified: about 752,244)
python3 tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}'
python3 tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}'
# paused: the click is flushed into the sim on the next tick; without the pause the alpha
# finishes a copper wall (~33 ticks) before the next eval and the plan list reads []
$G --eval "$H.step(10); return $H.player_build_plans_json()"           # contains x 135, y 58 (queued, not instant)
$G --eval "return $H.construct_progress(135, 58)"                        # progress about 0.31 after 10 ticks
$G --screenshot runs/20261014-s1-H00-T2/plan-queued.png                  # ghost + build beam from alpha
$G --eval "$H.step(600); return $H.building_debug(135, 58)"           # block 216 (copper-wall), team 1
# no-unit half (optional): a gmcp `place` MUST carry "team":1 — the JSON op defaults team to 0,
# and team 0 has no core, so a team-0 player:0 place stays instant by design
```
(The exact screen coordinates come from `tile_to_screen`; if the click lands
on a different tile, read `$I.get_input_state_json()` for the cursor tile
and adjust. If no plan appears and the wall appears instantly, T2 is not in
the binary — rebuild.)

**Pitfalls.**
* The `Break` guard must not swallow `player: None` breaks (the arm pattern
  already requires `Some(_)`).
* `commands_applied` must still increment for dropped commands (replay
  counters compare it).
* Do not add a Godot-side gate; the sim is the authority.

**Commit.** `Fixes PAR-0215: player plans with no build-capable unit are dropped instead of applied instantly`
(body also names PAR-0189).

---

### T2b. Wave numbering starts at 1 and spawns `getSpawned(wave - 1)` (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** Resuming Ground Zero (saved `wave` = 1) and calling
`run_wave()` spawns **1 dagger + 2 flares**; Java spawns **1 dagger** for wave 1
(the flare group has `begin: 1`). Cause: Java's match state starts at
`wave = 1` and `runWave` spawns `getSpawned(state.wave - 1)` BEFORE incrementing;
the port's `PlaySession` starts at `wave = 0` and both run-wave paths increment
first, then hand the spawner `wave - 1` (= the pre-increment wave, one too many).
The two errors cancel only for sessions the port created fresh, and break every
resumed Java save. After: sessions start at wave 1 (HUD "Wave 1" before the first
wave, like Java), and wave N spawns `getSpawned(N - 1)` on every path.

**Java.**
* `J/core/GameState.java:17`: `public int wave = 1;` and `J/core/Logic.java:308`
  (`reset()`): `state = new GameState();`.
* `J/core/Logic.java:319-325`:
  ```java
  public void runWave(){
      spawner.spawnEnemies();
      state.wave++;
      state.wavetime = state.rules.waveSpacing * (state.isCampaign() ? state.getPlanet().campaignRules.difficulty.waveTimeMultiplier : 1f);
      Events.fire(new WaveEvent());
  }
  ```
* `J/ai/WaveSpawner.java:74` (inside `spawnEnemies`): `int spawned = group.getSpawned(state.wave - 1);`
* `J/game/SectorInfo.java:64`: `public int wave = 1` (a fresh sector record is wave 1 too).

**Port today.**
* `core/game/play.rs` `PlaySession::new` (`wave: 0,` ~line 161) and
  `reset_world` (`self.wave = 0;` ~line 220).
* `core/game/play.rs` `run_wave_campaign` (`session.wave += 1;` then wavetime).
* Timer path `core/game/runtime.rs` (~402):
  `runtime.events.push(run_wave_campaign(&mut runtime.session));` then
  `unit_runtime.pending_wave = Some(runtime.session.wave - 1);`
* Button/API path `gdext/campaign.rs` `#[func] run_wave` (~1093-1104):
  `run_wave_campaign(&mut runtime.session); let wave = runtime.session.wave; ... request_wave_spawn(wave - 1);`
* Consumer `core/ai/wave_spawner.rs:157` `group.get_spawned(wave)` with the pending
  value (so `pending_wave` must be the `getSpawned` INDEX = pre-increment wave − 1).
  Tests call `sim.request_wave_spawn(0)` for "the first wave" — that is already the
  index convention; keep it.

**Changes.**
1. `core/game/play.rs`: `PlaySession::new` -> `wave: 1,`; `reset_world` ->
   `self.wave = 1;`. Doc comment: `GameState.wave = 1 (GameState.java:17)`.
2. `core/game/play.rs` `run_wave_campaign`: return the spawn index so callers cannot
   get it wrong. Change the signature to
   `pub fn run_wave_campaign(session: &mut PlaySession) -> (PlayEvent, i32)` where
   the `i32` is `session.wave - 1` computed BEFORE `session.wave += 1`
   (`WaveSpawner.java:74` + `Logic.java:319-321`). Update every caller
   (`grep -rn "run_wave_campaign(" client/rust`): the timer path sets
   `pending_wave = Some(index)`; `gdext/campaign.rs::run_wave` calls
   `request_wave_spawn(index)` (the facade-session call above it ignores the index).
   If H06 T2's `run_wave_now` already exists on `main`, change it the same way.
3. Grep for places that compensate for the old 0 start and would now be off by one:
   `grep -rn "wave + 1\|wave - 1\|\.wave == 0\|wave <= 0" client/rust/mind-core/src/game client/rust/mind-core/src/logic client/rust/mind-gdext/src client/ui`.
   For each hit, read the Java counterpart (HUD `J/ui/fragments/HudFragment.java`
   wave text uses `state.wave`; logic `@waveNumber` is `state.wave`
   (`J/logic/GlobalVars.java`); `SectorInfo.write/read` copies `state.wave` as is)
   and make the port use the session value with the same meaning. List each change
   in the commit body.
4. Saves: `SectorInfo.wave` round-trips unchanged; a port-native save written
   before this fix by a fresh port session may hold `wave = 0` — on load, clamp
   `session.wave = record.wave.max(1)` in `core/game/play.rs` (~477) with a comment
   (Java never writes 0).

**Tests.**
* `core/game/play.rs` tests: `fresh_session_starts_at_wave_one`
  (`PlaySession::new(Rules::default()).wave == 1`; after `reset_world()` == 1).
* `run_wave_returns_pre_increment_index`: session.wave = 1 ->
  `run_wave_campaign` returns `(PlayEvent::Wave, 0)` and session.wave == 2;
  with wave = 5 -> index 4, wave 6.
* `core/sim/unit_runtime.rs` `msav_tests`: new `ground_zero_wave_one_is_one_dagger`
  — copy the loader prologue of `ground_zero_wave_spawns_live_units`, install a
  campaign session with `wave = 1` the way existing campaign tests do (grep
  `CampaignRuntime::new` in that file), drive ONE timer wave (set `wavetime = 0`,
  `rules.waves = true`, tick ~120) and assert exactly one team-2 unit whose type is
  `dagger` (read `UnitTypeComp` + `content.unit_id("dagger")`), and `session.wave == 2`.
* Full suites; `units_scenarios`/campaign goldens that record `wave` change by
  exactly +1 at the start — accept only that diff and name `GameState.java:17`.

**Verify in-engine.**
```bash
# Ground Zero via "Go" (H-environment §4.1)
$G --eval "return $MC.get_hud_state()"      # wave field: 1 (the save's value)
$G --eval "$H.set_paused(true); return $MC.run_wave()"
$G --eval "$H.step(5); return $H.unit_debug_json()"   # exactly one team-2 unit: dagger
$G --eval "return $MC.get_hud_state()"      # wave 2
```
Then a fresh preset (sector 98, §4.1) `get_hud_state()` shows wave 1 before any wave.

**Pitfalls.** `request_wave_spawn(0)` in tests is the index of wave 1 — do not
"fix" those. `win_wave` comparisons (`session.wave >= rules.win_wave`) are Java's
(`Logic.java` `checkGameState`) and stay as they are.

**Commit.** `Gameplay H00 T2b: waves start at 1 and spawn getSpawned(wave - 1) on every path`

---

### T3. Ground Zero wave-1 smoke with the real player (acceptance only, no code unless it fails)

> **In-engine verification (2026-10-14, session 2, with T1+T2 temporarily applied):** CONFIRMED FAILING — expect T3 to fail; record it, do not fix here. `run_wave()` at wave 1 spawned **1 dagger + 2 flares**, but that is a port off-by-one: Java `Logic.runWave` (`Logic.java:319-321`) calls `spawner.spawnEnemies()` -> `group.getSpawned(state.wave - 1)` (`WaveSpawner.java:74`) **before** `state.wave++`, i.e. index 0 for wave 1 = **1 dagger only** (the flare group has `begin: 1`); the port passes the pre-increment wave itself (`gdext/campaign.rs` `run_wave` -> `request_wave_spawn(wave - 1)` after the increment, and the timer path `core/game/runtime.rs:402`, both consumed by `run_wave_system` `get_spawned(wave)`). Java client not observed (tutorial save keeps the wave timer off and the skip panel hidden). Fight: the dagger walked into deep water near (1088,1100) (tiles ~136,137-139), slowed to 0.09 px/tick (`wet`) and drowned ~1500 ticks after `run_wave` — the live flowfield (`core/ai/pathfinder/flowfield.rs`) uses uniform step costs, Java `Pathfinder.costGround` (`Pathfinder.java:56-62`) adds +6000 for deep tiles and makes all-deep tiles impassable. The flares circled the core; the duo spent 25 ammo in ~900 ticks for ~4 hits (port turrets aim at the target's current position, `turrets/mod.rs:1829`; Java leads with `Predict.intercept`, `Turret.java:459-471`); the core died ~3400 ticks after `run_wave`; the alpha died after the core (no respawn possible). Owners for the coordinator: wave index (H04/H01), ground path cost (no plan owns it), turret lead (H02). Respawn itself works with H00: alpha killed with `debug_apply_status(<id>, "melting", 2000)` respawned ~60 ticks later. Evidence: `runs/20261014-s2-verify/H00-T3-gameover.png`, `verify-s2-evidence.json`.

**Purpose.** G00's smoke ("the wave dies to a drill-fed duo, the core
survives") failed because alpha could not exist. Re-run it on the fixed
binary. This is the gate that every H plan's in-engine checks assume.

**Steps.**
```bash
# clean profile (H-environment §4.3), then enter Ground Zero with "Go" (T1 recipe)
$G --eval "return $H.has_player_unit()"                                   # true
$G --eval "return $H.place_block(130, 48, \"mechanical-drill\")"          # true
$G --eval "return $H.place_block(132, 48, \"duo\")"                       # true
$G --eval "$H.set_paused(true); $H.step(1800); return true"               # drill fills the duo
$G --eval "return $MC.run_wave()"
# advance in chunks, sampling state
$G --eval "$H.step(600); return [$H.get_group_count(\"unit\"), $H.has_player_unit(), $MC.get_hud_state().hasCore]"
# repeat the line above up to 10 times
$G --screenshot runs/20261014-s1-H00-T3/wave1.png
```
Then run wave 2 the same way (`$MC.run_wave()` once more, sample 10 x `step(600)`):
after T2b wave 1 is a single dagger, so wave 2 (Ground Zero's first flares) is the
air-defence check. Record both waves; the acceptance below applies to wave 1, wave 2 is
recorded evidence (it depends on H02 T16 turret lead).
**Acceptance.** Within 6000 ticks of `run_wave()` the enemy units are gone
(`get_group_count("unit")` back to 1 = the player), `hasCore` stays `true`,
and if alpha died during the fight, `has_player_unit()` is `true` again
within ~5 s of sim time (respawn at the core, PAR-0174). Record the wave
composition you saw (`unit_debug_json()` right after `run_wave()`; G00 and the
2026-10-14 verification saw 1 dagger + 2 flares — that is the port's wave-index
off-by-one, Java wave 1 is 1 dagger; see the verification note above).

**If it fails.** Do not fix gameplay here. Record exactly what happened
(which unit killed what, tick numbers, `unit_debug_json` snapshots) in the
summary under "T3 failure", and name the likely owner: flares killing the
duo/core -> H01 (FlyingAI/targeting) or H02 (duo targeting air); player not
respawning -> H01 (PAR-0174); duo not firing -> H02. The coordinator
forwards it. H01 has a task that re-runs this smoke as its own acceptance.

**Commit.** None (evidence only). If T3 passes, the PAR-0174 ledger note
gets the evidence path.

---

### T4. Record the new baseline

> **In-engine verification (2026-10-14, session 2):** NOT CHECKABLE (needs the final H00 tree; no suites were run in this verification round). Commands are the env §2 ones; nothing to change.

Run every command in `H-environment.md` §2 (full suites, clippy, fmt check,
`parity goldens`, `tools/build.sh`, GDScript parse gate) on the final H00
tree and write the exact numbers into the summary
`REPO/docs/H00-blockers_summary.md` under a heading "Baseline after H00".
Also run `cargo test -p mind-headless --test fx_golden` and record whether
it still fails only on `fx_program_explosion` (owned by H03).

Then create the marker commit:
`git commit --allow-empty -m "H00: blockers fixed (Env bits, player plans); baseline for round H"`
(with your co-author line). Every H plan checks for `H00` in `git log main --oneline`.

---

## 3. Order

T1 -> T2 -> T2b -> (one build) -> T1/T2/T2b in-engine checks -> T3 -> T4. Nothing else
runs on the host during H00.

## 4. Ledger changes to report (coordinator applies to `chains.json`)

| Item | Change |
|---|---|
| PAR-0174 | `verified` if T3 shows respawn; else stays `fixing` with the T1 commit + T3 notes |
| PAR-0215 | `verified` (T2 tests + in-engine queued plan) |
| PAR-0189 | `verified` if the T2 in-engine step shows a queued ghost (`plan-queued.png`) |
| PAR-0052 | re-verified note: alpha spawns on resume (T1 screenshot) |
| new | "Planet `Env` bits used enum order: Serpulo rules.env 31 (Java 233), player never spawned" -> `verified` (T1) |

## 5. Done criteria

* `rules.env` is 233 on Ground Zero in-engine; alpha spawns on resume.
* `player: Some` placements queue a plan in-engine; none applies instantly
  in a world with a core.
* Full suites green (or every new failure explained in the summary and
  fixed), goldens PASS, clippy/fmt clean.
* `docs/H00-blockers_summary.md` written with the baseline; marker commit
  `H00: ...` on `main`.

## 6. Cross-area notes

* After H00, `debug_spawn_player` (`gdext/sim_host.rs`) is only a debug
  seam; plans must use the real spawn (`spawn_player_unit()` or simply
  entering a sector).
* Every round-G check that said "blocked: no player unit" is re-listed in
  the owning H plan (H01 respawn/T8, H03 status icons/near-point banner/build
  beam, H05 hint completion paths, H07 build-beam loop and payload sounds,
  H08 queued-plan readbacks for even-size placement and schematic paste).
