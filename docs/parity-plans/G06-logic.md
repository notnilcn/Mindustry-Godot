<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G06 — Logic (processors, mlog VM, world processors, displays, logic editor)

Date: 2026-10-13. This plan is self-contained: read `00-environment.md` first
(paths `core/`, `gdext/`, `ui/`, `scenes/`, `J/`, `ARC/`, the `$G $H $I $C $MC`
shorthands, build/test/commit rules), then work the tasks of §2 in the order of
§3. If what you see in the code differs from what a task says (a symbol is
missing, a line moved by more than a search can fix, a behavior is already
implemented), stop that task, write down what you saw, and go to the next task.

Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line).

Why this area matters for the campaign: almost every campaign map ships
**world processors**. Decoding the Java maps (`Mindustry/core/assets/maps`) shows
world-processor code in groundZero, frozenForest, frontier, fungalPass, atolls,
overgrowth, planetaryTerminal, onset, aegis, basin, marsh, origin, ... For
example Ground Zero (sector 170) runs:

```
getflag waves "allowWaves"          # objectives raise the flag
jump 0 equal waves false
setflag "allowWaves" false
setrule waveSending true 0 0 100 100
setrule waveTimer true 0 0 100 100
---
getflag mineOver "mineOver"
jump 0 equal mineOver false
setflag "mineOver" false
setrule mapArea 10 0 0 256 256      # lifts the tutorial map-area limit
getflag end "spawnOver"
jump 4 equal end false
cutscene pan 135 230 0.03 0
wait 11
cutscene stop 100 100 0.06 0
stop
---
fetch build d @sharded i @duo        # objective "duoAmmo" waits for this flag
sensor amount d @ammo
op add i i 1
fetch buildCount totalDuos @sharded 0 @duo
op mod i i totalDuos
jump 0 equal amount 0
setflag "duoAmmo" true
stop
```

Erekir onset uses `op div speed @tick 100000` + `setrule unitBuildSpeed speed @crux`;
atolls uses `spawn ... m1` + `ubind m1` + `ucontrol move` + `message announce 6 @wait`
+ `cutscene pan/zoom` + `setblock`. Today the live client runs these processors
but every side effect is lost (no `LogicWorldState` resource, rules are a dead
copy, `@tick` is frozen, `sensor @ammo` returns 0, spawned units are not bound).
The decoder used is `python3 -I REPO/docs/parity-plans/tools/g06_dumpcode.py <map>.msav`
(zlib-inflate the msav, find every inner zlib stream whose first byte is
0/1, print `code[5:5+len]`).

---

## §1 Inventory

Java root `J/` = `Mindustry/core/src/mindustry/`. Status: **W** ported & wired,
**NW** ported but not wired into the live client, **P** partial/wrong, **M** missing.

| Java | Port | Status | Evidence (audit tree) | Task |
|---|---|---|---|---|
| `logic/LParser.java` | `core/logic/parser.rs` | W | tests in `core/logic/tests.rs`, `textio` round-trip | — |
| `logic/LAssembler.java` | `core/logic/assembler.rs` | W | `Assembler::assemble_with`, `read`, `write` | — |
| `logic/LStatements.java` (53 statements, text half) | `core/logic/statement.rs`, `textio.rs` | W | `define_statements!` table, field order = Java field order | — |
| `LStatements.*.build(LAssembler)` | `core/logic/executor/mod.rs` `build_statement` (~l.1981) | P | `Statement::SetRate` falls into `_ => Instruction::Noop` (no `SetRate` instruction) | T2 |
| `LStatements.*.build(Table)` (editor UI) | — | M | no logic dialog in `ui/dialogs/` | T13a-d |
| `logic/LExecutor.java` core (`runOnce`, `load`, Set/Op/Select/Jump/End/Stop/Noop) | `executor/mod.rs` `Instruction::run` (~l.970) | P | `Op` does not special-case `strictEqual` (always 0); `ops.rs` `Round` uses Rust `round` (half away from zero) not Java `Math.round` (floor(x+0.5)) | T2 |
| `WaitI` | `executor/mod.rs` `Instruction::Wait` | P | `wait_for <= 0` branch decrements `@counter` (Java does not) → `wait 0` hangs forever | T2 |
| `PrintI`/`PrintCharI`/`FormatI` | `executor/mod.rs` `append_print*`, `format_value`, `substitute_format` | P | `format` substitutes buffer into the value (inverted); `print` of content gives `content:<id>` (Java: name), building/unit give `[object]` (Java: block/unit type name), team gives `team:<id>` (Java: name) | T2 |
| `LookupI` + `GlobalVars.lookupContent/lookupLogicId` + `logicids.dat` | `Instruction::Lookup` (~l.1066), `globals.rs` `lookup_content` | M | always null; `@itemCount` etc. absent; file `Mindustry/core/assets/logicids.dat` exists | T3 (PAR-0107) |
| `GlobalVars.update` | `globals.rs` `GlobalVars::update_world` | NW | no caller in `core/` or `gdext/` (only tests/headless scenario) | T1 (PAR-0106) |
| `GlobalVars.init` | `globals.rs` `init_static`/`init_content`; installed by `gdext/sim_host.rs` `install_sim_seams` | P | registers `@wave` (not in Java); blocks: LegacyBlock not skipped, units: `internal` not skipped (verify); OK otherwise | T1 |
| `ReadI`/`WriteI` | `core/logic/blocks/io.rs` `read_target`/`write_target` | P | no `MessageBlock` read (Java returns char), no `CanvasBlock` read/write | T2, T15 |
| `SenseI` + `Senseable` impls (Building/Unit/Block/UnitType/Item/Liquid/Bullet + 40 block overrides) | `core/logic/access.rs` `sense`, `sense_building`, `sense_unit` | P | ~20 of 77 accessors; unknown → `0` (Java `NaN` → null); `null @dead` → 1 missing; privileged accessor gate missing; turret `ammo` etc. missing; building `x/y` wrong for size 3/4 | T7 |
| `ControlI` + `Controllable` | `access.rs` `control` | P | no `validLink` gate, `lastDisabler` cleared instead of set, `config`/`shoot`/`shootp`/`color` no-op | T7, T8 |
| `SetPropI` + `Settable` | `access.rs` `set_prop_*` | P | unit velocity/speed/armor/shield/flag/payloadType, building totalPower semantics, bullets missing | T7 |
| `RadarI` + `RadarTarget/RadarSort` | `executor/radar.rs` | P | no range limit, no permission check, sort direction inverted for health/shield/armor/maxHealth, attacker/boss false, no 30-tick cache | T10 |
| `UnitBindI`/`UnitControlI`/`UnitLocateI` | `executor/unit_control.rs` | P | ubind: no `logicUnitControl` gate, no bind-to-unit-object; ucontrol: tile coords not `World.unconv`, within/getBlock outputs never written, item/pay/mine/build/target no-ops | T9 |
| `ai/types/LogicAI.java` | `core/ai/types/logic.rs` `update_logic` | NW | never called; `unit_update_system` (`core/sim/unit_runtime.rs` ~l.1120) sends `AiKind::Logic` to the generic else-branch | T9 |
| DrawI / DrawFlushI / PrintFlushI | `executor/draw.rs` `pack_draw`, `io.rs` `flush_draw`/`flush_print` | P | image of a display packs `DISPLAY_DRAW_TYPE` without `rootDisplay.index << 5` | T14a (PAR-0105) |
| `world/blocks/logic/LogicBlock.java` | `core/logic/blocks/logic_block.rs` | P | link toggle via `Number` config impossible (wire `Int`→`Number` hits icon-tag branch); unlink/`removeFromProximity` re-enable missing; `@ipt` ok; privileged damage/pickup guards missing | T11 |
| LogicBlock UI (`buildConfiguration`, `onConfigureBuildTapped`, `drawConfigure`, `drawSelect`) | `gdext/input/desktop.rs` `config_spec` (~l.1604), `tile_tapped` (~l.1106) | P/M | processor tap opens a one-line text field that only sets a world-processor tag; no link tapping; no range/link overlay | T11, T12, T13a |
| `MemoryBlock.java` | `core/logic/blocks/memory.rs` | W | read/write/codec tests in `blocks/mod.rs` | — |
| `MessageBlock.java` | `core/logic/blocks/message.rs` | P | print/config ok; `drawSelect` text never rendered; logic read missing | T2, T12 |
| `SwitchBlock.java` | `core/logic/blocks/switch.rs` + tap in `desktop.rs` | W | PAR-0108 verified by previous round | — |
| `LogicDisplay.java` | `core/logic/blocks/display.rs` | P | sim queue ok, nothing renders | T14a, T14b (PAR-0105) |
| `TileableLogicDisplay.java` | same file | M | `root_display` fixed to self, no `linkDisplays`, size sensors | T14a |
| `CanvasBlock.java` | `core/logic/blocks/canvas.rs` | P | pixel storage ok; not rendered; no logic read/write; painting dialog missing | T15 |
| `LExecutor` world/privileged: SetBlock, SpawnUnit, SpawnBullet, ApplyEffect, WeatherSense/Set, SpawnWave, SetRule, FlushMessage, Cutscene, Effect, Explosion, GetFlag/SetFlag, SetMarker/MakeMarker, PlaySound/PlayMusic, LocalePrint, Fetch, Query, GetBlock | `core/logic/world.rs` `run_*` | NW/P | all need `LogicWorldState`, never installed in the live client; rules/markers are a private copy; spawn result never set; events never drained (unbounded growth if installed) | T4, T5, T6 (PAR-0104) |
| Sync / ClientData | `world.rs` `run_sync`/`run_client_data` | NW | multiplayer only | deferred |
| `LogicFx.java` | `core/logic/fx.rs` | P | table names ok; missing the Fx mapping (`warn`→`unitCapKill`, ...) | T5 |
| `MapObjectives.completionLogicCode` → `runLogicScript` | `core/game/map_objectives.rs` `complete` (~l.320) | P | runs on a throwaway `World` → side effects lost | T6 |
| `LogicDialog.java` + `LCanvas.java` + `LStatement` UI helpers + `GlobalVarsDialog.java` | — | M | `expected_pause("logic") == true` already in `core/ui/manifest.rs:405`; `MindLogic` node (`gdext/logic.rs`) exposes statement metadata | T13a-d (PAR-0103) |
| `LogicFilter` (map generation), editor `WorldProcessorDialog` | `logic/script.rs` `run_logic_filter` | P | editor/mapgen only | deferred |
| `LMarkerControl`/marker instructions | `world.rs` `run_set_marker`/`run_make_marker` | NW | markers are a private copy | T4 |

Already done / verified (do not redo):
* Parser, assembler, text IO, statement table, `LAccess` ordinals, enums (tests green per previous rounds).
* `LogicBlock` save codec revision 5, link fixpoint, `compress`/`read_compressed`, `find_link_name` (tests `logic_build_revision5_roundtrip`, `link_names_increment`).
* Memory cell/bank codec and read/write.
* Switch tap toggle (PAR-0108), message config panel (PAR-0093 sim half).
* Content values: ipt 2/8/25/8, ranges 80/176/336/MAX, world-processor `max_ipt` 1000,
  display sizes 80/176/32, memory 64/512/512, canvas 12/24 (`core/world/block_kind_data.rs:913-937`).
* `fetch build/buildCount/unit/unitCount` work on the live `World` (`world.rs` `run_fetch`).

---

## §2 Tasks

Conventions used below:
* "Java `conv`/`unconv`": Java `World.conv(v) = v / 8` (world px → tiles) and
  `World.unconv(v) = v * 8` (tiles → world px). **The port names them the other
  way round**: `core/logic/value.rs` `conv(v) = v * 8` (tiles → world) and
  `unconv(v) = v / 8`; `LVar::numf_world()` = `v * 8` = Java `unconv(x.numf())`
  (correct). Wherever this plan writes "tiles = px / 8" or "px = tiles * 8",
  write the arithmetic explicitly; do not rename the helpers (other areas call them).
* Logic tests: `core/logic/tests.rs` has a minimal fixture; sim-level logic tests
  use `crate::world::harness::BuildHarness` exactly like the tests in
  `core/logic/blocks/mod.rs` `integration_tests` (`processor_rig()`, `configure`
  with `ConfigValue::Bytes(logic_block::compress(code, &[]).into())`,
  `harness.tick()`). Read that module once; every "harness test" below follows it.
* Live-sim tests (needing `CampaignRuntime`, units, turrets) use
  `core/sim/production_tests.rs` `runtime_sim()` / `place()` / `configure()` and
  `core/sim/combat_tests.rs` helpers (see env §2).
* Run the logic suite with `$L cargo test -p mind-core --lib logic`.

### T1 — Live logic globals (`@tick @time @second @minute @waveNumber @waveTime @mapw @maph @server @client @client*`) (PAR-0106)

**Symptom / acceptance.** Today a processor running `set t @tick` keeps `t = 0`
forever (and onset's `op div speed @tick 100000` never ramps). After: `@tick`
equals the sim tick (+1, see Java), `@time = @tick/60*1000`, `@waveNumber`
follows the wave counter, `@mapw/@maph` the map size, `@server = 1`,
`@client = 0`, `@clientUnit` = the player unit, `@clientTeam` = player team.
Acceptance: harness/live test values below, and in-engine a micro-processor with
`set t @tick` linked to nothing shows a growing `t` through
`$H.logic_get_state(x,y)` (added in T13a; until then through the test).

**Java.** `J/core/Logic.java:526-546` (inside `if(!state.isPaused())`):
`state.tick += delta*60; ... Time.update(); logicVars.update();` — this runs
**before** `Groups` entity updates of the same frame, so processors see the
already-incremented tick. `J/logic/GlobalVars.java:192-226` `update()`:
```
varTime.numval   = state.tick / 60.0 * 1000.0;
varTick.numval   = state.tick;
varSecond.numval = state.tick / 60f;
varMinute.numval = state.tick / 60f / 60f;
varWave.numval   = state.wave;
varWaveTime.numval = state.wavetime / 60f;
varMapW.numval = world.width();  varMapH.numval = world.height();
varServer.numval = (net.server() || !net.active()) ? 1 : 0;   // offline = 1
varClient.numval = net.client() ? 1 : 0;                     // offline = 0
if(player != null){
  varClientLocale.objval = player.locale();   // "en" etc (settings "locale", default system)
  varClientUnit.objval   = player.unit();
  varClientName.objval   = player.name();
  varClientTeam.numval   = player.team().id;
  varClientMobile.numval = mobile ? 1 : 0;     // desktop = 0
  varClientMusicPlaying.numval = control.sound.isPlaying() ? 1 : 0;
  varClientCurrentMusic.objval = music file stem or null;
}
```
Init (`GlobalVars.java:41-189`): there is **no** `@wave` constant (only
`@waveNumber`); blocks register `@<name>` only if `content.item(name) == null`
and the block is not a `LegacyBlock`; units only if `!type.internal`.

**Port today.** `core/logic/globals.rs`: `GlobalUpdate` (l.61), `GlobalVars::update`
(l.355), `update_world` (l.344) — never called by the live sim. `install_world`
is called by `gdext/sim_host.rs` `install_sim_seams` (l.367). The executor reads
live values through `refresh_globals` (`core/logic/executor/mod.rs` ~l.1816)
from the `GlobalVars` **resource**, so updating the resource is sufficient.
`Sim::tick` is in `core/sim/mod.rs` (~l.586). `Sim.state.tick` is the completed
tick count (`GameState::advance`, `core/game.rs:119`, runs after the schedule).
Wave state lives in the `CampaignRuntime` resource (`core/game/runtime.rs:178`,
`session.wave: i32`, `session.wavetime: f32` ticks). Player unit:
`Sim::player_unit()` (`core/sim/unit_runtime.rs:895`).

**Changes.**
1. `core/logic/globals.rs` `GlobalUpdate`: add `pub client_unit: Option<bevy_ecs::entity::Entity>`
   and `pub client_name: Option<String>`. In `update`, add
   `self.set_obj_raw("@clientUnit", update.client_unit.map(LogicObject::Unit));`
   and `self.set_obj_raw("@clientName", update.client_name.clone().map(LogicObject::Str));`.
2. Remove the `@wave` registration from `init_static` (l.196) and from `update`
   (l.361); fix the test `update_math_matches_upstream` (drop the `@wave` assert;
   add `assert!(g.get("@wave", false).is_none())`).
3. `init_content`: skip blocks whose `BlockKind` is a legacy kind (the variants used
   in `core/content/registries/blocks/legacy.rs`: grep `BlockKind::Legacy` there and
   skip exactly those) and skip units with `internal == true`
   (`core/content/registries/units/mod.rs:1193`). Get the def through
   `content.block_by_name(name)` / `content.unit_by_name(name)`.
4. `core/sim/mod.rs`: add
   ```rust
   /// `Logic.update` -> `logicVars.update()` (Logic.java:546), before entity updates.
   fn update_logic_globals(&mut self) {
       let world = &mut self.ecs.0;
       if world.get_resource::<crate::logic::globals::GlobalVars>().is_none() { return; }
       let (wave, wavetime) = world
           .get_resource::<crate::game::runtime::CampaignRuntime>()
           .map(|rt| (rt.session.wave, rt.session.wavetime))
           .unwrap_or((1, 0.0));
       let update = crate::logic::globals::GlobalUpdate {
           tick: self.state.tick + 1, // Java increments state.tick before logicVars.update()
           wave, wavetime,
           map_width: self.grid.width(), map_height: self.grid.height(),
           server: true, client: false,
           client_locale: Some("en".to_owned()),
           client_team: <player team, see below>,
           client_mobile: false, client_music_playing: false, client_current_music: None,
           client_unit: self.player_unit(),
           client_name: Some("player".to_owned()),
       };
       crate::logic::globals::GlobalVars::update_world(world, &update);
   }
   ```
   Player team = `CampaignRuntime.session.rules.default_team` if present, else
   `world.get_resource::<crate::world::limits::BuildRules>().map(|r| r.default_team)`,
   else `crate::ecs::TeamId::SHARDED.0` (same chain as `gdext/sim_host.rs` `player_team`).
   `self.grid.width()` returns `i32` (check: `grep -n "pub fn width" core/world/grid*.rs`; cast if needed).
   Call it in `Sim::tick` right after `self.triggers.fire(Trigger::Update, ...)`
   and before `self.schedule.run(...)`.
   Locale/name: Java uses the player's locale/name; the port has no player name
   setting in core, so use the literal values above and note it in the commit body.
5. `@thisx/@thisy` bug found here (same file family, tiny): `core/logic/blocks/logic_block.rs:196-201`
   puts `conv(tile)` = tile*8. Java: `World.conv(x)` with `x` = building world
   centre = `tile.x*8 + offset`, `offset = ((size + 1) % 2) * 4`. Replace with
   `tile.x as f64 + offset as f64 / 8.0` (so a 1x1 at tile 5 → 5.0; a 2x2 logic
   processor whose centre tile is 5 → 5.5; a 3x3 hyper processor → 5.0).
   Block size via `block_size_of(world, e)`.

**Tests** (`core/logic/globals.rs` tests + a new test in `core/sim/production_tests.rs`):
* `live_tick_global_advances`: `let mut sim = runtime_sim();` (check it installs
  `GlobalVars`; if not, call `GlobalVars::install_world(&mut sim.ecs.0, &content)`
  with the sim's registry — `sim.content()` or the registry used by `runtime_sim`),
  place `micro-processor` at (10,10), configure it with code `set t @tick` (wire
  `ConfigValue::Bytes(compress(code, &[]))` through `configure()`), `sim.tick()` 30 times,
  read `LogicBlockState.executor.optional_var("t")`: expected value `> 25.0` and
  `== state.tick at the tick the instruction last ran`. Concretely: after N ticks the
  last `set` ran during tick N, when `@tick = N` (tick count before advance = N-1, +1).
  With ipt 2, `set t @tick` + implicit end loop: assert `t == 30.0`.
* `thisx_matches_java_conv`: harness, place `logic-processor` (2x2) at centre tile
  (4,4); `@thisx` var (`executor.arena.get_id("@thisx")`) num = 4.5; `micro-processor`
  at (8,8) → 8.0.
* Update `update_math_matches_upstream` as in change 2.

**Verify in-engine.** After T13a lands, re-run: enter Ground Zero (env §4.1),
`$H.place_block(120,50,"micro-processor")`, then
`$I.configure_building(120,50,"string", ...)` is not usable for code — use
`$H.logic_set_code(120,50,"set t @tick")` (T13a) and after 2 s
`$G --eval "return $H.logic_get_state(120,50)"` shows `t` > 100. Until T13a, headless only.

**Pitfalls.** `GlobalVars` must exist as a resource (installed in
`install_sim_seams` and `world/generation.rs:43`); `update_world` is a no-op
without it, which is fine for scenario sims. Do not iterate HashMaps. The
`checksum` of `Sim` must not include `GlobalVars` (it does not today; keep it so).

**Commit.** `Fixes PAR-0106: logic globals (@tick/@time/@wave...) update every tick`

### T2 — Executor correctness batch (wait, strictEqual, format/print, round, setrate, read message, sensor null)

**Symptom / acceptance.** Each bullet is an mlog program whose Java result is given;
all must hold after the task (unit tests in `core/logic/executor/mod.rs` tests module).

**Java and port, per item.**
1. `wait 0` / `wait -1` (`J/logic/LExecutor.java:1243-1258` `WaitI.run`):
   `if(value.num() <= 0){ exec.yield = true; curTime = 0f; }` — **no** counter
   decrement: the wait yields once and the next tick continues with the next
   instruction. Port `Instruction::Wait` (executor/mod.rs ~l.1030) decrements the
   counter → infinite loop. Fix: remove the two counter lines in that branch.
   Test `wait_zero_yields_once`: program `wait 0\nset x 1\nstop`, `run_budget` with
   ipt 8 for 2 ticks (`acc` starts 0; first call only accumulates — mirror the
   existing `budget_clamping_limits_instructions` test style): after tick 2 `x == 1`.
2. `op strictEqual` (`LExecutor.java:863-865` `OpI.run`):
   `dest.setnum(a.isobj == b.isobj && ((a.isobj && Structs.eq(a.objval,b.objval)) || (!a.isobj && a.numval == b.numval)) ? 1 : 0)`.
   Port `Instruction::Op` uses `op.eval` which returns 0 for `StrictEqual`. Fix: in
   `Instruction::Op`, `if *op == LogicOp::StrictEqual { value = if ConditionOp::StrictEqual.test(&a,&b) {1.0} else {0.0} }`.
   Test: `op strictEqual r 1 1` → 1; `op strictEqual r null 0` → 0; `op equal r null 0` → 1.
3. `op round` (`LogicOp.java` `round("round", Math::round)`): Java `Math.round(double)`
   = `floor(x + 0.5)`. Port `ops.rs` `Round => av.round()`. Fix: `(av + 0.5).floor()`.
   Test: `round -2.5` → -2; `round 2.5` → 3.
4. `format` (`LExecutor.java:1117-1165` `FormatI`): scan the **text buffer** for the
   placeholder `{N}` (`N` a digit 0-9) with the **lowest N** (first occurrence of
   that lowest digit), replace those 3 chars with the value string (same rules
   as print: integer if `|v - round(v)| < 1e-5`, objects via `PrintI.toString`),
   then truncate to 400. No placeholder → no-op. Port `append_format`/`substitute_format`
   (executor/mod.rs ~l.1915, ~l.1968) is inverted. Rewrite:
   ```rust
   fn append_format(&mut self, value: &LVar) {
       let buf: Vec<char> = self.text_buffer.chars().collect();
       let (mut idx, mut best) = (None, 10u32);
       for i in 0..buf.len() { if buf[i] == '{' && buf.len() - i > 2 {
           if let Some(d) = buf[i+1].to_digit(10) { if buf[i+2] == '}' && d < best { best = d; idx = Some(i); } } } }
       let Some(i) = idx else { return };
       let s = format_value(value);
       let mut out: String = buf[..i].iter().collect(); out.push_str(&s);
       out.extend(buf[i+3..].iter()); self.text_buffer = out; truncate_text(&mut self.text_buffer);
   }
   ```
   Test: `print "a {1} b {0}"\nformat 7\nformat "x"` → buffer `a x b 7`.
5. `print` object strings (`PrintI.toString`, `LExecutor.java:1080-1091`):
   null→`null`; String→itself; MappableContent→`content.name` (items, liquids,
   blocks, units, status effects, weathers); Building→`build.block.name`;
   Unit→`unit.type.name`; Enum→`name()`; Team→`team.name`; else `[object]`.
   Port `format_value` (executor/mod.rs ~l.1934) has no world access. Change
   `format_value(value: &LVar, names: &dyn Fn(&LogicObject) -> Option<String>)`… simpler:
   store the names at assemble time is impossible for buildings, so add a
   `name_of(world: &World, obj: &LogicObject) -> String` in `core/logic/access.rs`
   (content name via `world.get_resource::<BlockTable>()` for blocks and a
   `ContentRegistry`-backed lookup; the executor has no registry, so add an
   optional `LogicContentInfo` resource: `Vec<String>` per `ContentType` built in
   `GlobalVars::install_world` from the registry and inserted next to `GlobalVars`).
   Team name: `crate::game::team::Team::get(id).name`. Building: block name of its
   `Building.block`; Unit: `UnitTypeComp.type_id` name. `Instruction::Print`/
   `Format` pass `world` to the formatter. `printchar` of content: Java appends
   `(char)cont.emojiChar()` — no emoji table in the port: append nothing and note
   it as a deviation in the commit body.
   Test (harness): `sensor b @this @type\nprint b` in a micro-processor → buffer
   `micro-processor`; `print @copper` → `copper`; `print @sharded` → `sharded`.
6. `setrate` (`LExecutor.java:1211-1228` `SetRateI`): `if(build == null) return;
   build.ipt = clamp(amount.numi(), 1, block.privileged ? maxInstructionsPerTick : instructionsPerTick);
   ipt.numval = build.ipt;`. Port: no instruction. Add `Instruction::SetRate { amount: VarRef }`
   (append at the END of the enum; update `for_each_var_ref_mut` (~l.583-960) to
   visit `amount`), build it in `build_statement` from `Statement::SetRate { amount }`
   (check the field name in `statement.rs:435`). Running it needs to change
   `LogicBlockState.ipt` while the state is taken out of the entity (it is:
   `update_tile` takes it). So: add `pub pending_ipt: Option<i32>` to `Executor`;
   `SetRate` computes the clamp with `exec.build_ipt_max` (new field `pub ipt_max: i32`,
   set in `LogicBlockState::update_code` to `def.max_ipt` if privileged else `def.ipt`),
   writes `exec.set_num(exec.ipt, clamped)` (bypass constant: write `arena.get_mut(exec.ipt).num`
   directly — `@ipt` is a constant), and `exec.pending_ipt = Some(clamped)`.
   `LogicBlockBehavior::update_tile` after `run_budget`: `if let Some(v) = state.executor.pending_ipt.take() { state.ipt = v; }`.
   Test: world processor `setrate 1` → `state.ipt == 1`; micro `setrate 50` → 2.
7. `read` on a message (`MessageBlock.java:172-181`): readable = `isValid()` only;
   `output.setnum(address < 0 || address >= message.length() ? NaN : message.charAt(address))`.
   Port `io.rs` `read_building`: add `Some(BlockKind::MessageBlock) =>` using
   `MessageBlockState::read_char` (`message.rs:29`), and make the readability
   check for messages `is_valid_building` only.
   Also `ReadI` on a non-readable object (`LExecutor.java:629-643`): a Java `Seq`
   (the port's `LogicObject::Query`) is already handled; strings handled.
8. `sensor` head (`LExecutor.java:683-720` `SenseI`):
   * `target == null && sense == @dead` → `to = 1`.
   * LAccess with `privileged` (`cameraX/Y/Width/Height`) only when `exec.privileged`.
   * string target with `@size`/`@bufferSize` → its char length.
   * anything unhandled → `to.setobj(null)`.
   Port `Instruction::Sensor` (~l.1101): add the three branches; and in
   `access.rs` `sense_building`/`sense_unit` change the final `_ => Sensed::Num(0.0)`
   to `_ => Sensed::Obj(None)` (Java default `NaN` → null). The full accessor table
   is T7; this item only fixes the defaults.
   Tests: `sensor d null @dead` → 1; `sensor s "abc" @size` → 3; `sensor h @this @ammo` on a processor → null.

**Port today.** As cited per item; all in `core/logic/executor/mod.rs`,
`core/logic/ops.rs`, `core/logic/blocks/io.rs`, `core/logic/access.rs`.

**Pitfalls.** `Instruction` derives `PartialEq`/`Clone`; keep the new variant at the
end. `for_each_var_ref_mut` must visit every `VarRef` of the new variant or global
lowering silently breaks. `format_value` is also used by `gdext/logic.rs`? (grep;
adapt callers). Keep `MAX_TEXT_BUFFER` truncation char-based.

**Commit.** `Gameplay G06 T2: mlog executor fixes (wait 0, strictEqual, format, print names, round, setrate, message read, sensor null)`

### T3 — `lookup` and logic ids from `logicids.dat` (PAR-0107)

**Symptom / acceptance.** `lookup item r 0` gives null today; after: `@copper`.
`lookup block r 0` → `@graphite-press`; `lookup unit r 0` → `@dagger`;
`lookup liquid r 0` → `@water`; `lookup team r 1` → `@sharded`; `@itemCount` = 20,
`@blockCount` = 262, `@unitCount` = 56, `@liquidCount` = 11; `sensor i @copper @id` → 0
(logic id, not content id).

**Java.** `J/logic/GlobalVars.java:157-188` reads `logicids.dat`: for each type in
`writableLookableContent = {block, unit, item, liquid}` (this order): `short amount`
(big-endian), then `amount` × `readUTF` (u16 big-endian byte length + bytes);
`logicIdToContent[type][i] = content.getByName(type, name)` (null when unknown),
`contentIdToLogicId[type][content.id] = i`; `putEntry("@" + ctype.name() + "Count", amount)`
(names: `@blockCount`, `@unitCount`, `@itemCount`, `@liquidCount`).
`lookupContent(type, id)` (l.237-245): team → `id in 0..256 ? Team.all[id] : null`;
else `arr != null && 0 <= id < arr.length ? arr[id] : null`.
`LookupI.run` (`LExecutor.java:1286-1289`): `dest.setobj(lookupContent(type, from.numi()))`.
`Block/Item/Liquid/UnitType.sense(@id)` → `getLogicId()` = `lookupLogicId(this)`
(`-1` if absent) (`J/world/Block.java:1684`, `J/type/Item.java:150`, `Liquid.java:173`, `UnitType.java:1439`).
The file verified on disk: block 262 entries (first `graphite-press`, last `target-dummy`),
unit 56 (`dagger` … `emanate`), item 20 (`copper` … `carbide`), liquid 11 (`water` … `arkycite`), no trailing bytes.

**Port today.** `core/logic/globals.rs` header says the file is intentionally absent
(HLP §9) — that decision is superseded by parity (the file is a shipped Java asset).
`lookup_content` (l.447) handles only teams; `lookup_logic_id` (l.458) returns -1;
`Instruction::Lookup` (executor/mod.rs ~l.1066) is `{ result, .. }` and sets null —
check that the variant carries the content type (`Statement::Lookup { result, id, .. }`
has a `type_` field: `grep -n "Lookup, \"lookup\"" -A5 core/logic/statement.rs`).

**Changes.**
1. Copy `Mindustry/core/assets/logicids.dat` to `REPO/assets/logicids.dat`
   (the repo `assets/` mirrors Java assets; add a line to `assets/ASSET_PROVENANCE.md`
   if that file lists copied assets). Embed it in `mind-core` with
   `include_bytes!` via a path relative to the crate: check how other Java assets
   are embedded (`grep -rn "include_bytes!" client/rust/mind-core/src | head`); if
   none reach `REPO/assets`, place the copy at `client/rust/mind-core/data/logicids.dat`
   and `include_bytes!("../../data/logicids.dat")` from `globals.rs`.
2. `GlobalVars`: add `pub logic_to_content: [Vec<Option<u16>>; 4]` and
   `pub content_to_logic: [Vec<i32>; 4]` indexed by `0 block, 1 unit, 2 item, 3 liquid`.
   In `init_content(content)`: parse the bytes (pure fn
   `parse_logic_ids(bytes) -> Option<[Vec<String>;4]>`), resolve names with
   `content.get_by_name(type, name)` (as used at l.277), fill both tables
   (`content_to_logic` sized `content.entries(type).len()`, default -1), and
   `put_num(&format!("@{}Count", type_name), amount as f64, false)` with
   `type_name` = `block|unit|item|liquid`. Call `reindex()` after (already at the end).
3. `lookup_content(type, id)`: for the four types return
   `Some(LogicObject::Content(ContentRef::new(type, content_id)))` when mapped.
   `lookup_logic_id(c)`: table lookup, -1 otherwise.
4. `Instruction::Lookup { result, id, type_ }`: `let v = exec.globals.lookup_content(*type_, exec.arena.get(id.id()).numi()); exec.set_obj(result.id(), v)` — prefer the
   live resource (`world.get_resource::<GlobalVars>()`) and fall back to `exec.globals`.
5. `access.rs` `sense` for `LogicObject::Content(c)` (l.~258): replace
   `LAccess::Id => c.id` with the logic id (`GlobalVars` resource `lookup_logic_id`).
   The full content-sense table (`@color`, `@name`, health/size/... for blocks and
   unit types) is T7.
6. Update the module doc comment and the test `lookup_is_absent_file_behavior`
   → `lookup_maps_logic_ids` with the values in the acceptance line.

**Tests.** In `globals.rs` tests (use `BuildHarness::load_content()` as
`content_constants_register` does): `@itemCount == 20.0`, `@blockCount == 262.0`;
`lookup_content(Item, 0)` = content `copper`; `lookup_content(Block, 261)` =
`target-dummy` if the port has that block (else `None` — assert by name lookup
through `content.get_by_name`); `lookup_logic_id(copper) == 0`. Executor test:
`lookup item r 0\nsensor n r @name` → after T7 `n == "copper"`; before T7 assert
`r` is `Content(Item, id_of_copper)`.

**Verify in-engine.** Headless only (pure VM); covered by tests.

**Pitfalls.** `GlobalVars::new()` (no content) must keep working (empty tables).
`Clone` of `GlobalVars` happens per `update_code` — tables are small (≈350 entries), fine.

**Commit.** `Fixes PAR-0107: lookup and @<type>Count from logicids.dat`

### T4 — World-processor bridge, part 1: live rules, flags, markers, wave state; event log drained (PAR-0104)

**Symptom / acceptance.** In the live client every privileged sink returns early
(`world.get_resource::<LogicWorldState>()` is `None`): `getflag`/`setflag`,
`setrule`, `makemarker`/`setmarker`, `message`, `effect`… do nothing. Ground
Zero's `setrule mapArea 10 0 0 256 256` and `setrule waveSending/waveTimer true`
are lost. After: those instructions read and write the **live**
`CampaignRuntime.session` (`rules`, `rules.objective_flags`, `markers`, `wave`,
`wavetime`); `LogicWorldState` is installed in the live sim and its event log is
drained every tick (never grows). Acceptance test values below; in-engine:
Ground Zero `rules.waveSending` becomes true right after the objective sets
`allowWaves`.

**Java.** `J/logic/LExecutor.java:1779-1865` `SetRuleI.run` (transcribed: `waveTimer`→`rules.waveTimer=bool`;
`wave`→`state.wave=max(numi,1)`; `currentWaveTime`→`state.wavetime=max(numf*60,0)`;
`waves`, `waveSending`, `attackMode`, `canGameOver`, `pauseDisabled`, `lighting`,
`unitLight` → bools; `waveSpacing`→`numf*60`; `enemyCoreBuildRadius`/`dropZoneRadius`→`numf*8`;
`unitCap`→`max(numi,0)`; `musicVolume`→`clamp(numf,0,1)`; `ambientLight`→`fromDouble`;
`solarMultiplier`/`dragMultiplier`→`max(numf,0)`; `ban`/`unban`: `Block`→`bannedBlocks.add/remove`,
`UnitType`→`bannedUnits`; per-team (`p1.team()`, skip if null): `buildSpeed`→`clamp(v,0.001,50)`,
`unitHealth`→`max(v,0.001)`, `unitBuildSpeed`→`clamp(v,0,50)`, `unitMineSpeed`→`max(v,0)`,
`unitCost`→`max(v,0)`, `unitDamage`→`max(v,0)`, `blockHealth`→`max(v,0.001)`,
`blockDamage`→`max(v,0)`, `rtsMinWeight`→`v`, `rtsMinSquad`→`(int)v`;
`mapArea`: `checkMapArea(p1..p4)` (l.1866-1910): clamp `x,y >= 0`, `w = min(world.width, w)`,
`h = min(world.height, h)`; `full = x==0&&y==0&&w==W&&h==H`; if `limitMapArea` and same
bounds → nothing; if `limitMapArea` and full → disable; if not limited and full → nothing;
else enable with the bounds). `GetFlagI`/`SetFlagI` (l.2109-2157): `getflag` sets
`result = objectiveFlags.contains(name)` only when `flag.obj()` is a `String`, else
`result.setobj(null)`; `setflag` adds/removes only when the String flag changes.
Markers: `MakeMarkerI`/`SetMarkerI` (l.2282-2390) act on `state.markers`.

**Port today.** `core/logic/world.rs`: `LogicWorldState` (l.355) carries its own
`rules: Rules`, `markers: MapMarkers`, `wave`, `wavetime`; `run_set_rule` (l.1086) →
`core/logic/rules.rs` `apply_to_state` (writes the copy); `run_get_flag`/`run_set_flag`
(l.1267/1280) and `run_set_marker`/`run_make_marker` (l.1304/1354) use the copy.
`core/logic/blocks/mod.rs` `rules_ref` (l.339) reads `LogicRulesRes` (never
installed) → `DefaultLogicRules`. `LogicBlockBehavior::update_tile`
(`logic_block.rs` ~l.606) reads `LogicRulesRes` for `disable_world_processors`.
Live rules: resource `crate::game::runtime::CampaignRuntime` → `session.rules`
(`crate::io::json::rules::Rules`, fields `objective_flags: IndexSet<String>`,
`mission: Option<String>`, `banned_blocks/banned_units: IndexSet<String>` (names),
`limit_map_area`, `limit_x/limit_y/limit_width/limit_height`, `wave_timer`, ...),
`session.markers: MapMarkers`, `session.wave: i32`, `session.wavetime: f32`.
Per-team rules: `rules.team_rule(team)` exists read-only (used in
`entities/comp/unit/lifecycle.rs`); find the mutable accessor with
`grep -n "fn team_rule" core/io/json/rules.rs` (add `team_rule_mut` next to it if absent,
same body with `&mut`).

**Changes.**
1. `core/logic/blocks/mod.rs`: `impl LogicRulesApi for crate::io::json::rules::Rules`
   mapping each method to the field of the same name (`editor`, `allow_edit_world_processors`,
   `disable_world_processors`, `logic_unit_control`, `logic_unit_build`,
   `logic_unit_deconstruct`, `allow_logic_data`, `world_processor_player_link`,
   `default_team` (`TeamId(self.default_team)`; check the type of `TeamId`), `playtesting_map` → `false`).
   `rules_ref`: first `if let Some(rt) = world.get_resource::<CampaignRuntime>() { return &rt.session.rules; }`,
   then `LogicRulesRes`, then default. `logic_block.rs` `update_tile`: use
   `super::rules_ref(world).disable_world_processors()`. `valid_link`
   (`logic_block.rs:455`): implement Java's
   `!(privileged && !rules.worldProcessorPlayerLink && other.team == rules.defaultTeam)`.
2. `core/logic/world.rs`: add live accessors (all return `None` when neither source exists):
   ```rust
   /// Live `state.rules` (session) or the standalone copy.
   pub fn with_rules<R>(world: &mut World, f: impl FnOnce(&mut Rules) -> R) -> Option<R> {
       if let Some(mut rt) = world.get_resource_mut::<crate::game::runtime::CampaignRuntime>() {
           return Some(f(&mut rt.session.rules));
       }
       world.get_resource_mut::<LogicWorldState>().map(|mut s| f(&mut s.rules))
   }
   pub fn with_markers<R>(world: &mut World, f: impl FnOnce(&mut MapMarkers) -> R) -> Option<R> { /* same, session.markers / s.markers */ }
   pub fn with_wave<R>(world: &mut World, f: impl FnOnce(&mut i32, &mut f32) -> R) -> Option<R> { /* session.wave/wavetime or s.wave/s.wavetime */ }
   ```
   Rewrite `run_get_flag`, `run_set_flag`, `run_set_marker`, `run_make_marker`,
   `run_client_data` (`allow_logic_data`), `run_set_block` (default team) and the
   mission branch of `run_flush_message` on these accessors (events still go to
   `LogicWorldState` when present).
3. `core/logic/rules.rs`: change `apply_to_state` to
   `pub fn apply_rule(rules: &mut Rules, wave: &mut i32, wavetime: &mut f32, map_w: i32, map_h: i32, content: Option<&ContentRegistry>, rule, value, p1..p4) -> bool`
   covering **every** Java case above (add the missing ones; `ban/unban` resolve the
   content name: `value.value_obj()` is `LogicObject::Content(c)`; name via
   `content.entries(c.type_)`/`get_by_id` — pass the registry from the
   `LogicContentInfo` resource added in T2 item 5 (store names there, then you
   need no registry here); per-team rules through `team_rule_mut`).
   `run_set_rule`: read map size from `TileBuilds` (`width/height` fields, see
   `world.rs` l.~830) and call `apply_rule` inside `with_rules` + `with_wave`
   (nest: take `wave`/`wavetime` copies via `with_wave(|w,t| (*w,*t))`, apply,
   write back with a second `with_wave`). Keep `apply_to_state` as a thin wrapper for
   the existing tests.
   After a rule change, re-derive the grid/build state: when `MapArea` changed the
   limit fields, set G04 T5's `CampaignRuntime.rules_world_dirty = true` so `Sim::tick` runs
   `Sim::apply_rules_to_world(&rules)` (rules → `WorldGrid.limit_area`, border darkness); for any rule
   change call G02 T8's `sync_build_rules` (rules → `BuildRules` resource) — record a
   `rules_dirty` flag on `LogicWorldState` in `run_set_rule` and call it from
   `Sim::drain_logic_events`. If either function is not on `main` yet, leave the flag set
   and note the dependency in the summary.
4. Install in the live client: `gdext/sim_host.rs` `install_sim_seams` (l.367): after
   `GlobalVars::install_world`, insert
   `LogicWorldState { headless: false, ..LogicWorldState::new() }`,
   `MessageState::default()`, and `LogicContentIndex::from_content(&registry)`
   (all three from `mind_core::logic::world`).
5. Drain: add to `Sim` (`core/sim/mod.rs`) a field
   `pub logic_ui_events: Vec<crate::logic::world::LogicWorldEvent>` (default empty) and
   ```rust
   /// Drains the world-processor event log once per tick (T5/T6 extend the match).
   fn drain_logic_events(&mut self) {
       let events = match self.ecs.0.get_resource_mut::<crate::logic::world::LogicWorldState>() {
           Some(mut s) => std::mem::take(&mut s.events), None => return };
       for event in events {
           match event {
               // T5 adds SetBlock / SpawnWave handling here.
               e @ (LogicWorldEvent::Message { .. } | LogicWorldEvent::Cutscene { .. }) => {
                   if self.logic_ui_events.len() < 256 { self.logic_ui_events.push(e); }
               }
               _ => {}
           }
       }
   }
   ```
   Call it in `Sim::tick` after `self.update_builders()` and before `resolve_deaths()`.
   Also keep `LogicWorldState.tick/map_width/map_height` current in
   `update_logic_globals` (T1) when the resource exists (`run_sync` uses `tick`).
6. `MessageState` timers (needed for `message ... @wait` blocking, Java
   `ui.hasAnnouncement()` / `hudfrag.hasToast()`, `J/core/UI.java:626,636-653,405-419`,
   `J/ui/fragments/HudFragment.java:741-743`): add to `MessageState`
   `pub announce_until: u64, pub toast_until: u64`. In `run_flush_message`, when an
   announce/toast is emitted set `announce_until = tick + ceil(duration*60)`;
   notify sets `toast_until = tick + 210` (3.5 s). Blocking test uses
   `tick < announce_until` / `tick < toast_until` (tick from `LogicWorldState.tick`).
   Keep `has_announcement/has_toast` fields but derive them in that function.
   Java keeps the `@`-prefixed text and only replaces it when the bundle has the key:
   remove the `strip_prefix('@')` in `run_flush_message` (the host resolves keys, T6).

**Tests** (`core/logic/world.rs` tests or `core/sim/production_tests.rs`):
* `setrule_writes_live_rules`: `runtime_sim()` with a `CampaignRuntime` (check how
  campaign tests build one: `grep -rn "CampaignRuntime::new\|fn campaign_runtime_for_test" core/game`),
  place `world-processor`, code `setrule waveSending true 0 0 100 100\nsetrule unitCap 4 0 0 100 100\nstop`,
  tick 3 → `session.rules.wave_sending == true`, `unit_cap == 4`.
* `flags_roundtrip_live`: insert `"allowWaves"` into `session.rules.objective_flags`;
  ground-zero block 2 code (header of this plan) → after 3 ticks the flag is gone and
  `wave_timer == true`.
* `map_area_rule`: map 300x300, `setrule mapArea 10 0 0 256 256` → `limit_map_area`,
  `limit_width == 256`; then `setrule mapArea 0 0 0 300 300` → `limit_map_area == false`.
* `wave_rule`: `setrule wave 0 0 0 100 100` → `session.wave == 1` (max 1);
  `setrule currentWaveTime 2.5 ...` → `session.wavetime == 150.0`.
* `events_drained_each_tick`: processor `fetch buildCount c @sharded 0 @duo` in a loop;
  after 100 ticks `LogicWorldState.events.len() == 0`.
* `message_wait_blocks`: privileged `print "a"\nmessage announce 2 @wait\nset x 1`: with
  `headless=false`, x becomes 1 after the first announce; a second program that
  announces twice keeps `x` unset until `tick >= announce_until` (120 ticks).

**Verify in-engine.** Ground Zero (env §4.1 Go path). Read flags/rules:
`$G --eval "return $MC.get_rules_json()"` — careful, it reads MindCampaign's own
session; prefer a new `#[func] logic_rules_probe() -> GString` on SimHost returning
`{"waveSending":..,"waveTimer":..,"flags":[...],"limitMapArea":..,"limitWidth":..}`
from `CampaignRuntime.session.rules` (add it in this task, 15 lines). Complete the
tutorial objectives that raise `allowWaves` (or insert the flag through a test seam:
add `#[func] logic_set_flag(name, on)` that writes `session.rules.objective_flags`,
for verification only), step 60 ticks, probe: `waveSending: true`, flag removed.
Screenshot `runs/<date>-s<N>-G06-T4/gz-flags.png`. Log must have 0 panics.

**Pitfalls.** `MindCampaign` keeps a separate `session` (`gdext/campaign.rs` `run_wave`
uses both); the authoritative one during play is the `CampaignRuntime` resource in
the sim world — never write MindCampaign's. `take_campaign_runtime` temporarily
removes the resource (accessors must tolerate `None`). Bevy `get_resource_mut`
inside `with_*` conflicts with holding `&Executor` from the world — executors are
taken out of the entity during `update_tile`, so this is fine; do not hold a
`Mut` across the closure.

**Commit.** `PAR-0104: world processors read/write live rules, flags, markers and wave state`

### T5 — World-processor bridge, part 2: spawn, status, explosion, effect, sound, music, spawnwave, setblock, weather, getblock (PAR-0104)

**Symptom / acceptance.** atolls' `spawn @mega 110 400 270 @sharded m1` must put a
mega on the map **and** set `m1` immediately (the next line is `ubind m1`);
`setblock block @air 210 306 @derelict 0` must clear that tile; Ground Zero's
`spawnwave natural 10 true` must run the next wave; `effect`/`playsound` must show/play;
`explosion` must damage. Today all are events nobody applies (and `spawn`'s result
is never set).

**Java** (all `J/logic/LExecutor.java`):
* `SpawnUnitI` (l.1615-1650): `if(net.client()) return; t = team.team();`
  `if(t != null && type.obj() instanceof UnitType type && !type.internal && Units.canCreate(t, type))`:
  `unit = type.spawn(t, unconv(x)+range(0.01), unconv(y)+range(0.01), rotation)`
  (`Mathf.range(0.01)` = random in [-0.01,0.01]; use the sim RNG or 0 — use 0 and
  note it), `if(effect) spawner.spawnEffect(unit)` else `unit.unloaded(); fire UnitSpawnEvent`;
  `result.setobj(unit)`.
* `ApplyEffectI` (l.1751-1777): unit + `StatusEffect`: `clear ? unit.unapply(e) : unit.apply(e, duration*60)`.
* `ExplosionI` + `logicExplosion` (l.2004-2040): `if(damage < 0) return;`
  `Damage.damage(team, x, y, radius, damage, pierce, air, ground, true, null)` with
  `x,y = unconv(...)`, `radius = unconv(min(radius, 100))`; `if(effect)`
  `pierce ? Fx.spawnShockwave.at(x,y, conv(radius)) : Fx.dynamicExplosion.at(x,y, conv(radius)/8)`.
* `EffectI` (l.1975-2002): `rot = type.rotate ? rotation : min(rotation, 1000)`;
  `type.effect.at(unconv(x), unconv(y), rot, Color.fromDouble(color), data.obj())`.
  `LogicFx` table: `J/logic/LogicFx.java:14-46` maps names to Fx fields:
  warn→unitCapKill, cross→unitEnvKill, blockFall→blockCrash, placeBlock→placeBlock,
  placeBlockSpark→coreLaunchConstruct, breakBlock→breakBlock, spawn→spawn,
  trail→colorTrail, breakProp→breakProp, smokeCloud→missileTrailSmoke, vapor→vapor,
  hit→hitBulletColor, hitSquare→hitSquaresColor, shootSmall→shootSmall,
  shootBig→shootTitan, smokeSmall→shootSmallSmoke, smokeBig→shootBigSmoke,
  smokeColor→shootSmokeTitan, smokeSquare→shootSmokeSquare, smokeSquareBig→shootSmokeSquareBig,
  spark→hitLaserBlast, sparkBig→circleColorSpark, sparkShoot→colorSpark,
  sparkShootBig→randLifeSpark, drill→mine, drillBig→mineHuge, lightBlock→lightBlock,
  explosion→dynamicExplosion, smokePuff→smokePuff, sparkExplosion→titanExplosion,
  crossExplosion→dynamicSpikes, wave→dynamicWave, bubble→airBubble.
* `PlaySoundI` (l.2230-2259): `sound = Sounds.getSound(id.numi())` (none if invalid);
  positional → `sound.at(unconv(x), unconv(y), pitch, min(volume,2), limit)`;
  else `sound.play(min(volume * sfxVolume, 2), pitch, pan, false, limit)`.
* `PlayMusicI` (l.2261-2280): plays the named music track (`interrupt` flag).
* `SpawnWaveI` (l.2159-2199): `natural` → `logic.skipWave()` = `runWave()`
  (`J/core/Logic.java:315-325`: spawn enemies, `wave++`, `wavetime = waveSpacing * difficulty mult`);
  else for each `SpawnGroup` with `type != null` and (`spawn == -1 || spawn == Point2.pack(x,y)`):
  `spawned = group.getSpawned(state.wave - 1)` units, each at `unconv(x,y) + rnd(16)`
  (`spawner.spawnUnit(group, ...)`).
* `SetBlockI` (l.1566-1612): tile at `(x.numi(), y.numi())`, block must be a `Block`:
  `ore`: `(b is OverlayFloor || b == air) && overlay != b` → `setOverlayNet(b)`;
  `floor`: `b is Floor && floor != b && !f.isOverlay && !f.isAir` → `setFloorNet(f)`;
  `block`: `(!b.isFloor() || b == air)`; `t = team.team() ?: derelict`;
  `if(block != b || tile.team != t) tile.setNet(b, t, clamp(rotation.numi(),0,3))`.
* `GetBlockI` (l.1535-1563): tile at `(round(x), round(y))`; floor/ore(overlay)/block/building; null outside.
* `SenseWeatherI`/`SetWeatherI` (l.1710-1749): sense = `weather.isActive()`; set on →
  create weather (intensity 1, life `WeatherState.fadeTime`) if inactive else refresh life;
  off → shorten life to `fadeTime`.

**Port today.** `core/logic/world.rs` `run_spawn_unit` (l.919), `run_apply_status` (l.997),
`run_explosion` (l.1223), `run_effect` (l.1189), `run_play_sound` (l.1388),
`run_play_music` (l.1416), `run_spawn_wave` (l.1064), `run_set_block` (l.848),
`run_get_block` (l.801), `run_weather_sense/set` (l.1023/1038) — event-only.
Helpers available: `UnitRuntime::spawn_by_id(world, UnitTypeId, team, x, y, rot)`
(`core/sim/unit_runtime.rs:219`; take the runtime out with
`world.remove_non_send::<UnitRuntime>()` and put it back — `update_buildings`
runs while the runtime is in the world); `combat::damage::status::apply_status(world, e, StatusId, duration)`
(`core/combat/damage/status.rs:190`); `combat::damage::area::damage_area(world, content, team, x, y, radius, damage, DamageOptions)`
(`core/combat/damage/area.rs:132`; read `DamageOptions` fields for air/ground/pierce);
`UnitRuntime.fx: FxHandle` with `FxSink::effect(&EffectRef, x, y, rot, Rgba)` and
`shake` (`core/combat/view.rs:21`); `content::registries::fx_meta::effect_by_name`;
`audio::AudioSinkRes` resource `.emit(AudioEvent::at(..)/AudioEvent::play(..)/AudioEvent::music_play(MusicRef::new(name), interrupt))`
(`core/audio/mod.rs`, `core/audio/events.rs:105-173`), `SoundId(u16)`;
`game::play::run_wave_campaign(&mut PlaySession)` (`core/game/play.rs:507`) +
`Sim::request_wave_spawn(wave)` (`core/sim/unit_runtime.rs:732`), as
`gdext/campaign.rs:1025` `run_wave` does; `SimRuntime::set_block_raw/remove_block_raw`
(`core/sim/runtime.rs:418/623`, `pub(crate)`, callable from `Sim`).
Unit cap: `entities/comp/unit/queries.rs:185` `can_create`.

**Changes.**
1. `core/logic/fx.rs`: add `pub fx: &'static str` to `EffectEntry` (constructor
   takes it) and fill it from the table above (Java field names; map each to the
   port's `effect_by_name` name — they use the same camelCase Fx field names; assert
   in a test that every `fx` resolves).
2. `run_effect`: resolve `effect_by_name(entry.fx)`; if `Some(id)`, take the
   `UnitRuntime`, call `runtime.fx.effect(&EffectRef::Named(id), x*8, y*8, rot, color_from_double(color))`
   (convert `ColorHex` → `Rgba`: `ColorHex(Rgba)` wraps it), put it back. Keep the event push
   only if `LogicWorldState` exists (tests count them).
3. `run_spawn_unit`: if `UnitRuntime` present and the host check passes: content
   `UnitTypeId` from `LogicObject::Content(c)` with `c.type_ == ContentType::Unit`;
   skip internal types (`content.unit(id).internal`); unit-cap check via the same
   predicate the wave spawner uses (`grep -rn "can_create(" core/ | head`); spawn at
   `(x*8, y*8)`; `exec.set_obj(result, Some(LogicObject::Unit(entity)))`.
   `effect=true` → `runtime.fx.effect(spawn effect)`: Java `spawner.spawnEffect(unit)`
   plays `Fx.unitSpawn` and sets the unit invisible-rising; port: emit
   `effect_by_name("unitSpawn")` at the unit only (deviation noted).
4. `run_apply_status`: `StatusId::new(c.id)` (check constructor) → `apply_status(world, unit, id, duration*60)`;
   `clear` → remove the status entry from `StatusComp` (grep `pub struct StatusComp`
   and its remove/unapply method; if none, `retain(|s| s.id != id)` on its list).
5. `run_explosion`: `damage_area(world, &content, Some(team), x*8, y*8, min(r,100)*8, damage, opts{air, ground, pierce_armor: pierce, ..})`
   with content from `UnitRuntime.content`; effect as Java with `effect_by_name("spawnShockwave")`
   (rotation = radius_px/8) or `dynamicExplosion` (rotation = radius_px/8/8).
6. `run_play_sound`: positional → `AudioEvent::at(SoundId(id as u16), x*8, y*8, pitch, min(vol,2), limit)`;
   else `AudioEvent::play(SoundId(id), min(vol,2), pitch, pan, false, limit)` (volume
   is emitted raw; the sink applies `sfxvol` — owned by G07 T1/T2 (audio pump/sink);
   do not apply settings volume in core). Ignore ids outside `0..SOUND_COUNT`.
   `run_play_music`: `AudioEvent::music_play(MusicRef::new(name), interrupt)`.
7. `run_spawn_wave`: keep the event; in `Sim::drain_logic_events` (T4) handle
   `SpawnWave { natural: true, .. }` exactly like `gdext/campaign.rs:1025` but
   through the sim's own `CampaignRuntime` resource (`run_wave_campaign(&mut rt.session)`
   then `self.request_wave_spawn(rt.session.wave - 1)`); `natural: false` →
   for each spawn group of `rt.session.rules.spawns` with matching `spawn`
   (`-1` or `TilePos::new(x,y).pack()`), spawn `group.get_spawned(wave-1)`
   (find the port's `getSpawned`: `grep -rn "fn get_spawned" core/`) units of the
   group type at `(x*8, y*8)` + offset from the sim RNG within radius 16
   (`runtime.rng.range(RngStream::Sim, -16.0, 16.0)` per axis — document the
   deviation from `Tmp.v1.rnd(spread)`), team `rules.wave_team`.
8. `run_set_block` → event (already) and handle `SetBlock` in `drain_logic_events`:
   `Block`: if block is not a floor (`content.block(id).kind` not floor/overlay kinds:
   use `BlockDef::is_floor()` if it exists, `grep -n "fn is_floor" core/content`) or is air:
   when the tile's block or team differ → `rt.remove_block_raw` then (if not air)
   `rt.set_block_raw(&mut self.ecs.0, &mut self.grid, x, y, id, rot, team)`;
   `Floor` (floor kind, not overlay, not air) → `self.grid` tile `floor = id`;
   `Ore` (overlay kind or air) → tile `overlay = id`. After any change:
   `self.refresh_unit_runtime()` and bump the world revision the renderer watches
   (grep `world_revision\|revision +=` in `core/sim/mod.rs`; use the same call `place_block` uses).
9. `run_get_block` live path: when `WorldGrid` is not a resource, read floor/overlay from
   the `UnitRuntime.grid` snapshot (`world.get_non_send::<UnitRuntime>()`), rounding
   `x,y` with `f64::round` → Java `Mathf.round(float)` = `floor(x+0.5)`; use that.
10. Weather: `run_weather_sense` → active iff a `WeatherState` entity of that type
    exists (`core/entities/comp` `WeatherState`; see `weather_system`); `run_weather_set`:
    on → if inactive spawn the weather the way `weather_system` creates it
    (`grep -n "fn create_weather\|WeatherState {" core/sim core/game`) with intensity 1
    and life = `WeatherState.fadeTime` (Java `fadeTime = 60*4` — verify in
    `J/entities/comp/WeatherStateComp.java`); else set life to fadeTime; off →
    if life > fadeTime set life = fadeTime. If the port has no creation helper,
    implement only `weathersense` and leave `weatherset` as event (document; G07 owns weather).

**Tests** (`core/sim/production_tests.rs` or a new `core/logic/live_tests.rs` included
from `core/logic/mod.rs` under `#[cfg(test)]`, using `runtime_sim()` which installs the
unit runtime — confirm with `sim.has_unit_runtime()`):
* `spawn_sets_result`: world processor `spawn @dagger 20 20 0 @sharded u\nsensor x u @x\nstop`
  → unit count +1, `x == 20.0` (after T7's sensor `@x` fix; before T7 assert the unit
  `Pos.x == 160.0`).
* `setblock_clears_tile`: copper-wall at (30,30) team derelict; `setblock block @air 30 30 @derelict 0`
  → tile (30,30) has no building after the drain.
* `spawnwave_natural_runs_wave`: wave before = w → after = w + 1.
* `explosion_damages`: a dagger of team crux at (40,40) tiles; `explosion @sharded 40 40 3 50 1 1 0 0`
  → health decreased by > 0 (exact value via `damage_area` falloff: distance 0 → full 50 minus armor).
* `effect_emits_named_fx`: install a recording `FxSink` (see how combat tests record
  FX: `grep -rn "RecordingFx\|impl FxSink for" core/`) and assert one `Named(id of "spawn")`.

**Verify in-engine.** Load sector 98 (saltFlats) or any sector, place a world
processor with `$H.place_block(x,y,"world-processor")` and code (T13a's
`logic_set_code`) `spawn @flare 130 60 0 @sharded u\nstop`; `get_group_count("unit")`
+1; screenshot shows the flare. `effect warn 130 60` shows the warn effect.

**Pitfalls.** Never call these from a non-host (`LogicWorldState.is_host`); keep that
gate. `remove_non_send` must always be paired with `insert_non_send`, including early
returns. Spawned units must be pushed to `runtime.units` (spawn_by_id does).

**Commit.** `PAR-0104: world-processor spawn/status/explosion/effect/sound/spawnwave/setblock apply to the live sim`

### T6 — World-processor bridge, part 3: messages, cutscene camera/HUD, objective completion scripts (PAR-0104)

**Symptom / acceptance.** atolls' `localeprint "atolls.noairunit"` + `message announce 6 @wait`
shows a centred announcement for 6 s and blocks until it ends; `cutscene pan 263 135 0.06 0`
pans the camera; `cutscene stop` returns control; objective `completionLogicCode` runs
against the live world. Ground Zero's `cutscene pan 135 230 0.03 0` after `mineOver`.

**Java.** `FlushMessageI` (`LExecutor.java:1917-1973`, transcribed in T4): text
`@key` → `Core.bundle.get(key)` if the bundle has it; `notify` → `hudfrag.showToast(Icon.info, text)`;
`announce` → `ui.announce(text, duration)`; `toast` → `ui.showInfoToast(text, duration)`;
`mission` → `state.rules.mission = text`. `LocalePrintI` (l.2391-2410): appends
`state.mapLocales.getProperty(name)` (map-bundled locale strings!) to the text buffer.
`CutsceneI` (l.1334-1370): `active`→`p1 = logicCutscene`; `pan`→`logicCutscene=true; logicCamPan=(unconv(p1),unconv(p2)); logicCamSpeed=p3`;
`zoom`→`logicCutscene=true; logicCutsceneZoom=clamp(p1,0,1)`; `stop`→`logicCutscene=false`;
`shake`→`renderer.shake(p1, p2*60)`; `getHud`→`p1 = !logicHideHud`; `setHud`→`logicHideHud = !p1`.
Camera (`J/input/InputHandler.java:946-950`): `if(logicCutscene && !renderer.isCutscene()) camera.position.lerpDelta(logicCamPan, logicCamSpeed) else logicCutsceneZoom = -1`;
zoom (`J/core/Renderer.java:159`): `if(logicCutsceneZoom > 0) baseTarget = lerp(minZoom, maxZoom, logicCutsceneZoom)`;
`logicCutscene` is an input lock (`InputHandler.java:86`: `inputLocks` includes it);
`logicHideHud` hides the HUD (`HudFragment.java:446,1239`). `reset()` clears both (l.921).
`MapObjectives.completionLogicCode` → `LExecutor.runLogicScript(code)` (`J/game/MapObjectives.java:213`,
`LExecutor.java:73-94`: privileged executor, ≤100000 instructions, no loop).

**Port today.** `run_cutscene` (world.rs l.1166) only emits; `active`/`getHud` never
write `p1`. `run_locale_print` uses `no_locale()` (returns the key). Map locales:
check `grep -rn "map_locales\|mapLocales" core/` — if the session stores map locale
bundles, read them; else keep the key (deviation). `core/game/map_objectives.rs:320-341`
`complete` runs `run_logic_script` on a fresh `World`.

**Changes.**
1. `LogicWorldState`: add `pub cutscene: LogicCutscene` with fields
   `active: bool, pan: (f32, f32) /*world px*/, speed: f32, zoom: f32 /* -1 = none */, hide_hud: bool`
   (default `false, (0,0), 0.1, -1.0, false`). `run_cutscene` implements Java exactly on
   this struct (`active`/`getHud` write `p1` through `exec.set_num`), `shake` → `runtime.fx.shake(p1)`
   (duration ignored by the port's `shake` signature; note it).
2. `gdext/sim_host.rs`: `#[func] logic_cutscene_json() -> GString` →
   `{"active":..,"x":..,"y":..,"speed":..,"zoom":..,"hideHud":..}`; and
   `#[func] drain_logic_messages() -> Array<Dictionary>` popping `Sim.logic_ui_events`
   `Message` entries as `{type:"announce|notify|toast", text, duration}`.
3. `gdext/camera.rs` `process` (~l.97): read the cutscene state from the host each frame
   (`host.bind().logic_cutscene_state()` — a plain Rust accessor, not a `#[func]`, to avoid
   Variant churn); when `active` and the landing cutscene is not running
   (`in_cutscene` flag already computed there), set the camera target position by
   `lerp_delta(pos, pan, speed)` (= `pos + (pan - pos) * min(speed * delta60, 1)` with
   `delta60 = delta * 60`), suppress follow, and lock input exactly like the landing
   cutscene does (`input.bind_mut().set_cutscene_lock(true)`); when `zoom > 0` set the
   camera scale target to `lerp(min_zoom, max_zoom, zoom)` (find the camera's min/max zoom
   constants in `camera.rs`). Release the lock when inactive.
4. HUD hide: `ui/fragments/hud_fragment.gd` `_process` — if `$H.logic_cutscene_json().hideHud`
   hide the HUD root like Java `shown == false` (poll every 10 frames to limit evals).
5. Messages: `ui/ui_root.gd` `_process` polls `drain_logic_messages()` (every frame;
   cheap when empty) and for each: resolve `text` if it starts with `@` and
   `MindAssets.bundle_get(key)` returns a value different from the key; then
   `announce` → `MindUi.announce(text, duration)`; `notify` → `MindUi.toast(text, "info")`;
   `toast` → new `MindUi.show_info_toast(text, duration)` (add `#[signal] show_info_toast(text, duration)`
   + `#[func]` in `gdext/ui/ui_host.rs` next to `announce` (l.351), and a handler in
   `ui_root.gd` that shows a top-centred `Styles.black3` label for `duration*0.9` then
   fades over `duration*0.1`). Use only these entry points (`MindUi.announce/toast/show_info_toast`);
   G03 owns the faithful `UI.announce`/`showInfoToast` visuals — if G03 already added
   `show_info_toast`, reuse it instead of adding one; do not restyle `ui_root.gd` labels.
6. Objective scripts: change `MapObjectivesRuntime::complete` to push the code into a
   new `pub pending_logic_scripts: Vec<String>` instead of running it; in the sim
   tick, after the campaign/objective systems ran (inside `drain_logic_events`, T4),
   take `CampaignRuntime.session.objectives.pending_logic_scripts` and run each with
   `crate::logic::script::run_logic_script_in(&code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false, &mut self.ecs.0)`
   (needs `CampaignRuntime` re-inserted first: take the Vec out, drop the borrow, then run).
   Update the map_objectives test that asserted the old behavior (grep `completion_logic_code`
   in tests).

**Tests.**
* `cutscene_pan_sets_state`: privileged `cutscene pan 10 20 0.05 0\ncutscene active a 0 0 0\nstop`
  → `cutscene.active`, `pan == (80.0, 160.0)`, `a == 1`.
* `message_announce_queued`: `print "hi"\nmessage announce 3 ok` with `headless=false`
  → `sim.logic_ui_events` has one `Message{Announce, "hi", 3.0}` and `ok == 1`.
* `objective_completion_script_runs_live`: objective with `completion_logic_code = "setflag \"x\" true"`
  completed → after one tick `session.rules.objective_flags` contains `x`.

**Verify in-engine.** Ground Zero: set `mineOver` and `spawnOver` through T4's
`logic_set_flag` seam; within 2 s `logic_cutscene_json` shows `active:true`, screenshots
1 s apart show the camera moving toward tile (135,230); after 11 s `active:false`.
Announce: world processor `print "hello"\nmessage announce 3 @wait\nstop` → screenshot shows "hello".

**Pitfalls.** Polling from GDScript must not `get_state_json()`. Do not call MindUi
from inside `SimHost` while it is bound (re-entrancy). The camera reads the host with
`bind()` only (no `bind_mut`).

**Commit.** `Fixes PAR-0104: world-processor messages, cutscenes and objective scripts reach the player`

### T7 — Sensor / control / setprop coverage (Building, Unit, content, per-block overrides)

**Symptom / acceptance.** Ground Zero's and onset's world processors wait on
`sensor amount d @ammo` of a duo/breach (always 0 today → the `duoAmmo`/`breachAmmo`
objectives never complete). Player programs reading `@x`, `@totalPower`, `@progress`,
`@heat`, `@firstItem` of drills, `@shootX`, `@controlled`… get wrong values; unknown
accessors give `0` instead of `null`; `control enabled` from a processor out of range
works (it must not). After: the tables below hold; tests per row group.

**Java** (verbatim semantics; `NaN` results become `null` through `setnum`).
`BuildingComp.sense(LAccess)` (`J/entities/comp/BuildingComp.java:2099-2135`):
`x,y → conv(x/y)` (world px / 8 of the building centre); `color → toDoubleBits(team.color.r,g,b,1)`;
`dead → !isValid()`; `solid → block.solid || checkSolid()`; `team → team.id`; `health`;
`maxHealth`; `efficiency`; `timescale → timeScale`; `range → this instanceof Ranged ? range()/8 : 0`;
`rotation` (0-3); `totalItems → items==null?0:items.total()`; `totalLiquids → liquids==null?0:liquids.currentAmount()`;
`totalPower → power==null || consPower==null ? 0 : power.status * (consPower.buffered ? consPower.capacity : 1)`;
`itemCapacity → hasItems ? itemCapacity : 0`; `liquidCapacity → hasLiquids ? liquidCapacity : 0`;
`powerCapacity → consPower != null ? consPower.capacity : 0`;
`powerNetIn → graph.getLastScaledPowerIn()*60`; `powerNetOut → graph.getLastScaledPowerOut()*60`;
`powerNetStored → graph.getLastPowerStored()`; `powerNetCapacity → graph.getLastCapacity()` (0 without power);
`armor → block.armor`; `enabled`; `controlled → ControlBlock && isControlled() ? 2 : 0`;
`payloadCount → payloads.total() + (payload != null ? 1 : 0)`; `size`;
`cameraX/Y/Width/Height → ControlBlock ? unit.sense : 0`; default `NaN`.
`senseObject`: `type → block`; `firstItem → items.first()`; `config → block.configSenseable() ? config() : null`
(configSenseable = block has an Item/Liquid/UnlockableContent/Block/UnitType config, `J/world/Block.java:822-825`);
`payloadType → UnitPayload ? unit.type : BuildPayload ? block : null`.
`sense(Content)`: item → `items.get(i)`; liquid → `liquids.get(l)`; with payloads: unit type/block counts; else `NaN`.
Overrides (`super` = the above): Turret `ammo → totalAmmo`, `ammoCapacity → maxAmmo`,
`rotation → rotation (degrees)`, `shootX/Y → conv(targetPos)`, `shooting → isShooting`,
`progress → clamp(reloadCounter/reload)`; PowerTurret `ammo → power.status`, `ammoCapacity → 1`,
`heat → heatRequirement>0 ? heatReq : NaN`; ItemTurret `currentAmmoType → last ammo entry item`;
LiquidTurret/ContinuousLiquidTurret `currentAmmoType → liquids.current()`;
GenericCrafter `progress`, `totalLiquids → outputLiquid ? liquids.get(output) : super`;
Drill `progress` (only if dominantItem != null), `firstItem → dominantItem`;
Separator, BlockProducer, PayloadDeconstructor, UnitAssembler, Duct `progress`;
UnitFactory/Reconstructor `progress → clamp(fraction())`, `itemCapacity → round(itemCapacity * rules.unitCost(team))`,
`config → current plan unit / reconstructor output unit`;
ConstructBlock `progress → clamp(progress)`, `breaking`, `building → current`;
MassDriver `progress → clamp(1 - reloadCounter)`; PayloadMassDriver `clamp(1 - reloadCounter/reload)`;
LaunchPad `clamp(launchCounter/launchTime)`; MendProjector `clamp(charge/reload)`;
ShockwaveTower `progress = reloadCounter/reload`, `heat`; HeatCrafter/HeatConductor/HeatProducer/
NuclearReactor/VariableReactor/HeaterGenerator `heat`; ImpactReactor `heat → warmup`;
ForceProjector `heat → buildup`, `shield → broken ? 0 : max(shieldHealth + phaseShieldBoost*phaseHeat - buildup, 0)`;
Pump `efficiency → shouldConsume ? efficiency : 0`, `totalLiquids → liquids.get(liquidDrop)`;
SolidPump `efficiency → (validTiles + boost) * efficiency`; Door/AutoDoor `enabled → open`;
Conveyor `progress → len==0 ? 0 : ys[len-1]`, `firstItem → ids[len-1]`;
CoreBlock `itemCapacity → storageCapacity`, `maxUnits → Units.getCap(team)`, `sense(UnitType) → team unit count`;
StorageBlock `itemCapacity → linkedCore ? core's : super`; LightBlock `color → its color`;
MemoryBlock `memoryCapacity`; MessageBlock `bufferSize → message.length()`;
LogicDisplay `displayWidth/Height → displaySize`, `bufferSize → root.commands.size`, `operations → root.operations`;
TileableLogicDisplay `displayWidth = tilesWidth*32 - 12`, `displayHeight = tilesHeight*32 - 12`;
CanvasBlock `displayWidth/Height → canvasSize`; BuildTurret `buildX/Y`, `building/breaking` from its unit.
Unit (`J/entities/comp/UnitComp.java:266-353`): `totalItems → stack.amount`; `itemCapacity → type.itemCapacity`;
`rotation`; `health`; `shield`; `maxHealth`; `flying`; `x,y → conv`; `velocityX/Y → vel*60/8`;
`dead → dead || !isAdded()`; `team`; `shooting`; `boosting → type.canBoost && isFlying()`;
`range → range()/8`; `shootX/Y → conv(aimX/aimY)`; `camera*` (player only, else 0);
`mining`; `mineX/Y → mining ? mineTile.x/y : -1`; `buildX/Y → building ? plan.x/y : -1`;
`armor → armorOverride>=0 ? armorOverride : armor`; `flag`; `speed → type.speed*60/8*speedMultiplier`;
`controlled → !valid ? 0 : LogicAI ? 1 : Player ? 2 : CommandAI with command ? 3 : 0`
(**note: `ctrlProcessor = 1`, `ctrlPlayer = 2`, `ctrlCommand = 3`; the port maps Logic→2, Player→1: wrong**);
`payloadCount`, `totalPayload → payloadUsed/64`, `payloadCapacity → type.payloadCapacity/64`;
`size → hitSize/8`; `color → team color`; `selectedRotation`; `pingX/Y`; default NaN.
Unit `senseObject`: `type`; `name → player name or null`; `firstItem → stack.amount==0 ? null : item`;
`controller → !valid ? null : LogicAI ? ai.controller (processor building) : this`;
`payloadType`; `building`/`breaking` (plan tile build); `selectedBlock`; `pingText`.
Unit `sense(Content)`: `== stack.item → amount`; unit type / block → payload counts;
StatusEffect → `hasEffect ? duration/60 : 0`; else NaN.
Content objects: Block (`J/world/Block.java:1674-1708`): `color → mapColor`, `health,maxHealth → health`,
`armor`, `solid`, `size`, `itemCapacity`, `liquidCapacity`, `powerCapacity → buffered ? capacity : 0`,
`id → logic id`, `name`; `sense(Item)` → requirement amount * `rules.buildCostMultiplier` (0 if infiniteResources);
UnitType (`J/type/UnitType.java:1430-1449`): `health,maxHealth → health`, `armor`, `range → maxRange/8`,
`size → hitSize/8`, `flying`, `itemCapacity`, `speed → speed*60/8`, `payloadCapacity → payloadCapacity/64` (if Payloadc), `id`, `name`;
Item/Liquid: `color → color.toDoubleBits()`, `id`, `name`; Team objects are not Senseable (null).
Bullets (`J/entities/comp/BulletComp.java:354-410`) — the port has no bullet `LogicObject`; skip (deferred).
`ControlI` (`LExecutor.java:558-594`): only if target is a Building and
`exec.privileged || (exec.build != null && exec.build.validLink(b))`; `enabled`:
`p1.bool() ? b.noSleep() : b.lastDisabler = exec.build`; then `isObj && p1.isobj ?
control(type, obj, ...) : control(type, num, ...)`. `BuildingComp.control`: `enabled → enabled = !zero(p1)`;
object `config` → `if(block.logicConfigurable && !(p1 instanceof LogicBuild)) configured(null, p1)`
(`logicConfigurable` = the block has any UnlockableContent-typed config, `Block.java:1482-1488`, or TargetDummy).
Overrides: Turret `shoot`/`shootp` (T8), LightBlock `color → color = fromDouble(p1).rgba8888`,
Door `enabled` → `if(open == shouldOpen || (unit inside && !shouldOpen) || !timer(80)) return; configureAny(shouldOpen)`,
PayloadRouter `config → rotation = mod((int)p1, 4)`, `controlTime = 360`.
`SetPropI` (`LExecutor.java:2201-2228`) → Building `setProp`: `health → clamp(0,max)` (≤0 → destroy),
`team → changeTeam`, `totalPower → buffered ? status = clamp(v/capacity) : ignore`; obj `team`;
content: item → set amount via accept/remove stack; liquid → `clamp(v, 0, liquidCapacity)`, set if
decreasing or accepted. Unit `setProp`: `health` (≤0 → kill), `shield → max(v,0)`, `x/y → unconv`,
`velocityX/Y → v*8/60`, `rotation`, `team`, `flag`, `speed → statusSpeed(clamp(v,0,1000))`,
`armor → statusArmor(max(v,0))`; obj `team`, `payloadType` (add/pop payload); content: item stack
`(item, clamp(v,0,itemCapacity))`, status → `setDuration(effect, v*60)`. ForceProjector `shield`
→ `buildup = max(shieldHealth + phaseShieldBoost*phaseHeat - v, 0)`.

**Port today.** `core/logic/access.rs` `sense_building` (l.~287), `sense_unit` (l.~420),
`set_prop_*` (l.~470-640), `control` (l.650). Bugs beyond missing rows: building `x/y`
use `tile + (size-1)/2` then `conv` (= ×8): wrong both in scale and for odd sizes ≥3 —
use the `Pos` component (world px) / 8; unit `x/y` return raw px (must be /8);
unit velocity raw; `controlled` constants swapped; `TotalPower` returns `stored`.

**Changes.**
1. Add to `crate::world::behavior::BuildingBehavior` (`core/world/behavior/mod.rs:133`):
   ```rust
   /// `Building.sense(LAccess)` override hook; `None` = fall back to the base table.
   fn logic_sense(&self, _world: &World, _e: Entity, _a: crate::logic::LAccess) -> Option<crate::logic::access::Sensed> { None }
   /// `Building.control(LAccess, ...)` override hook; returns true when handled.
   fn logic_control(&self, _world: &mut World, _e: Entity, _a: crate::logic::LAccess, _p: &[crate::logic::LVar; 4]) -> bool { false }
   ```
   `access::sense_building` first asks the behavior (via `BlockTable` instance of the
   building's block), then the base table. Implement overrides in the behavior files of
   the families listed above (find each with
   `grep -rln "impl BuildingBehavior for <Name>Behavior" core/world/blocks`): turrets
   (`core/world/blocks/defense/turrets/mod.rs`, `TurretState`: `total_ammo`,
   `config.max_ammo`, `rotation`, `target_pos`, `is_shooting`, `reload_counter`,
   `config.reload`, `heat_req`, `ammo.last().item`), crafters/drills/separators/units/
   payload/distribution/power/heat/defense per the list (each reads its own state
   component's `progress`/`heat`/... field; when a family has no such field in the
   port, skip that row and list it in the summary).
2. Rewrite `sense_building` base table exactly as transcribed; values from: `Pos`
   (x/y px), `Health`, `Building` (`rotation`, `enabled`, `efficiency`, `time_scale`),
   `BlockDef` (`armor`, `size`, `item_capacity`, `liquid_capacity`, `has_items`,
   `has_liquids`, `solid`, consumes power capacity/buffered — find the power-consumer
   fields with `grep -n "buffered\|capacity" core/content/registries/blocks/mod.rs | head`),
   `ItemModule`, `LiquidModule`, `PowerModule { status, graph }` + `PowerGrids` graph
   (`core/world/blocks/power/graph.rs:105-140`: `last_scaled_power_in/out`,
   `last_capacity`, `last_power_stored`), team colour from `crate::game::team::Team::get(id).color`
   packed with `rgba_u8_to_double_bits`/`pack_color` (alpha 1). `range`: logic
   blocks `LogicBlockDef.range/8`; turrets `config.range/8`; others 0 (add to the
   override hook for other `Ranged` blocks only if they exist).
3. `sense_unit`: full table; `x/y/shootX/shootY = px/8`; velocity `*60/8`; `speed`
   from the unit type def `speed*60/8` times the status speed multiplier (find in
   `StatusComp`/`UnitState`); `controlled` constants fixed (Logic→1, Player→2,
   Command (only when it has a command; `ControllerSlot.target.is_some()` is the closest
   port signal — document) →3); `mining/mineX/mineY` from the miner component
   (grep `mine_tile` in `core/entities/comp/unit`); `buildX/Y` from the builder plans.
4. Content sense: in `sense()`'s `LogicObject::Content(c)` branch, implement the
   Block/UnitType/Item/Liquid tables (needs registry data: use `BlockTable` for
   blocks; for units/items/liquids add the needed scalars to the `LogicContentInfo`
   resource from T2 item 5 and extend it to store per-content
   `name`, `color_bits`, and for units `health, armor, max_range, hit_size, flying,
   item_capacity, speed, payload_capacity`; fill from the registry at install).
   `@name` returns `LogicObject::Str(name)`.
5. `sense_content(target, Content)` per Java (unit stacks, status durations, core
   unit counts, block requirements).
6. `control`: add the `validLink` gate (`exec.privileged || valid_link(world, exec.build, privileged=false, Some(b))`
   — `Instruction::Control` must pass `exec.build`/`exec.privileged` into `access::control`);
   `enabled=false` sets `last_disabler = exec.build`, `enabled=true` clears sleep
   (port has no sleep: just `enabled = true`); object `config` → if the block is
   logic-configurable (its behavior's `config_kinds()` contains `Item|Liquid|Block|Unit|Content`)
   and the object is not a logic processor building → call the behavior's `configured(world, e, None, value)`
   with `ConfigValue::Item/Liquid/Block/Unit` built from the content object; LightBlock
   `color` (behavior override in `core/world/behavior/light.rs`); Door `enabled` (override,
   respect the existing 80-tick… the port's door throttle is 60? check `DoorBehavior::configured`
   and use 80 as Java `timerToggle` here); PayloadRouter `config`.
7. `set_prop_*`: complete per Java; unit `x/y` = `v*8`; building `health <= 0` →
   mark dead the way damage does (set `Health.health = 0`; `resolve_deaths` handles it).

**Tests** (`core/logic/access.rs` tests + live tests):
* `duo_ammo_sensed`: live sim, duo at (40,40) given 10 copper via
  `debug_add_building_items` equivalent (`add_item` test helper), run Ground Zero
  block 3 code (top of plan; replace `@scatter` with `@duo`) → flag `duoAmmo` set
  (needs T4) — or directly `sensor a d @ammo` → `a > 0` and `== TurretState.total_ammo`.
* `building_xy_tiles`: 3x3 hyper processor centred at (10,10) → `@x == 10.0`;
  2x2 at centre (10,10) → 10.5; 1x1 at (3,4) → 3.0.
* `unit_xy_tiles`: dagger at px (80,160) → `@x 10`, `@y 20`.
* `controlled_constants`: logic-bound unit → 1.
* `unknown_is_null`: `sensor r @this @ammo` (processor) → null.
* `content_sense`: `sensor h @dagger @health` → 130 (dagger health in Java `UnitTypes.java`;
  read the port value from content); `sensor n @copper @name` → "copper".
* `control_out_of_range_ignored`: micro processor at (0,0), switch at (30,30): `control enabled s 0` with `s` obtained via `getblock building` (privileged not) — use a fetch-free path: write the switch entity into a variable through a test seam (set `exec.arena` directly) → switch stays enabled.

**Verify in-engine.** Ground Zero: place a duo with ammo (`debug_add_building_items(x,y,"copper",10)`),
and a micro processor linked to it (T11) running `sensor a duo1 @ammo`; T13b's
variables view shows `a`. Until T13: headless only.

**Pitfalls.** Behavior files are shared with G01/G02 — add only the two hook
methods, never touch their update logic. `Sensed::Num(NaN)` must become null:
route numeric results through `LVar::set_num` (it already converts).

**Commit.** `Gameplay G06 T7: logic sensor/control/setprop tables match Java (turret ammo, progress, heat, units, content)`

### T8 — Logic-controlled turrets (`control shoot` / `control shootp`)

**Symptom / acceptance.** `control shoot duo1 100 50 1` makes the duo turn to tile
(100,50) and fire for 2 s even with no enemy; `control shootp duo1 u 1` aims at unit `u`.
Today the call is a no-op (`access::control` ignores `Shoot/Shootp`) and the turret
update ignores `logic_control_time`.

**Java.** `J/world/blocks/defense/turrets/Turret.java`: `logicControlCooldown = 120` (l.38).
`control(shoot, p1,p2,p3)` (l.386-395): if `!unit.isPlayer()`: `targetPos = (unconv(p1), unconv(p2))`,
`logicControlTime = 120`, `logicShooting = !zero(p3)`. `control(shootp, obj, p2)` (l.397-409):
if not player-controlled: `logicControlTime = 120; logicShooting = !zero(p2); if(obj is Posc) targetPosition(pos)`
(`targetPosition` l.~455-480: no-op when `!hasAmmo()`; with `predictTarget && bullet.speed >= 0.01`
`targetPos = Predict.intercept(this, pos, bullet.speed)` else `targetPos = pos`).
`updateTile` (l.497-600): `isShooting = alwaysShooting || (player ? unit.isShooting : logicControlled() ? logicShooting : target != null)`;
`if(logicControlTime > 0) logicControlTime -= delta`; `validateTarget()` returns true when
`logicControlled()` (l.633-635), so a null target does not abort; in the `hasAmmo` block:
`logicControlled() → canShoot = logicShooting` (no `targetPosition(target)`, keeps `targetPos`);
then `targetRot = angleTo(targetPos)`, turn, `if(!alwaysShooting && angleDist(rotation, targetRot) < shootCone && canShoot) { wasShooting = true; updateShooting(); }`.

**Port today.** `core/world/blocks/defense/turrets/mod.rs` `update_turret` (~l.1273):
`is_shooting` (l.1293) ORs `logic_shooting` in (wrong); after `find_target` it
returns when `state.target.is_none()` (l.~1395) — logic aim impossible;
`logic_control_time` never decremented; `TurretState.logic_control_time/logic_shooting/target_pos`
exist (l.314-328). Predict helper: `grep -rn "fn intercept" core/` (use it if present, else `target_pos = pos`).

**Changes.**
1. `turrets/mod.rs` (shared with G01/G02; touch only these lines):
   `let logic = state.logic_control_time > 0.0;`
   `state.is_shooting = config.always_shooting || if logic { state.logic_shooting } else { state.target.is_some() };`
   right after the recoil/heat block: `if state.logic_control_time > 0.0 { state.logic_control_time -= 1.0; }`;
   in the target section: `if state.target.is_none() && !logic { return; }`; when `logic`,
   skip the re-aim that sets `target_pos` from the target (find where `find_target`/
   `validate_target` write `target_pos` and guard it with `!logic`); fire condition
   `if !config.always_shooting && in_cone && (if logic { state.logic_shooting } else { true })`
   (keep the existing `always_shooting` branch). `validate_target`: return true when `logic`.
2. Turret behavior `logic_control` override (T7 hook): `Shoot` → if the turret is not
   player-controlled (port: no turret control yet → always true) set
   `target_pos = (p1*8, p2*8)`, `logic_control_time = 120.0`, `logic_shooting = p3 != 0`;
   `Shootp` → `logic_control_time = 120`, `logic_shooting = p2 != 0`, and if `p1` is a
   unit/building: when the turret has ammo, `target_pos` = its `Pos` (or intercept).
   `access::control` must pass object params: `p1` as `LVar` (object) — the hook takes `&[LVar;4]`.

**Tests** (`core/sim/combat_tests.rs` style): duo with copper ammo, no enemies; a
processor linked to it runs `control shoot duo1 <tx> <ty> 1` (tile 5 east of the duo);
after 30 ticks the duo `rotation` is within `shootCone` of 0° and `total_shots > 0`
(bullet group count > 0). After 200 ticks without re-issuing, `logic_control_time <= 0`
and shots stop.

**Verify in-engine.** Ground Zero: duo at (132,48) with copper, micro-processor at
(132,46) linked (T11) running `control shoot duo1 140 48 1`; screenshot shows bullets
flying east.

**Commit.** `Gameplay G06 T8: logic-controlled turrets (control shoot/shootp)`

### T9 — `ubind` / `ucontrol` / `ulocate` and LogicAI in the live unit update

**Symptom / acceptance.** atolls: `spawn ... m1` → `ubind m1` → `ucontrol stop` / `ucontrol move 210 306 0 0 0`
must move the mega to tile (210,306). Player programs: `ubind @poly` + `ucontrol move @thisx @thisy`
+ `ucontrol itemTake core1 @copper 30` must move/transfer. Today `ubind` cannot bind a unit
object, `ucontrol move` stores raw tile numbers as world px, and LogicAI never runs.

**Java.** `UnitBindI` (`LExecutor.java:169-209`): `if(!exec.privileged && !rules.logicUnitControl) return;`
`binds` sized to unit type count; `type.obj() instanceof UnitType t && t.logicControllable` →
`seq = team.data().unitCache(t)`; if any: `binds[t.id] %= size; unit = seq[binds[t.id]]; binds[t.id]++` else unit = null;
`type.obj() instanceof Unit u && (u.team == exec.team || privileged) && u.type.logicControllable` → `unit = u`; else null.
`UnitControlI.checkLogicAI` + `run` (l.306-556) — transcribed fully in the source;
key rules: `control = type != unbind && type != within`; units must be `isValid`, bound
(`exec.unit.obj() == unit`), same team unless privileged, controller `isLogicControllable`;
installs `LogicAI` (clearing `mineTile` and build plans) when `control`; every call sets
`ai.controlTimer = 600`; `x1,y1,d1 = unconv(p1,p2,p3)`.
Per command: `idle, autoPathfind → ai.control = type`; `move, stop, approach, pathfind →
control = type, moveX/Y = x1/y1, approach: moveRad = d1; stop: mineTile=null, clearBuilding`;
`unbind → resetController`; `within → p4 = within(x1,y1,d1)`; `target → posTarget=(x1,y1), aimControl=target, mainTarget=null, shoot=p3.bool`;
`targetp → aimControl=targetp, mainTarget = p1 (Teamc) or null, shoot = p2.bool`;
`boost → ai.boost = p1.bool`; `flag → unit.flag = p1.num`;
`mine → tile = tileWorld(x1,y1); if(canMine) mineTile = validMine(tile) ? tile : null`;
`payDrop`/`payTake`/`payEnter` (timeout `transferDelay = 90` ticks via `unitTimeouts`);
`build` (rules.logicUnitBuild || privileged, `p3` Block, `canBeBuilt`, unlocked or AI team,
plan at `toTile(x1 - offset/8, ...)`, rot `mod(p4,4)`, config `p5`);
`deconstruct` (rules.logicUnitDeconstruct || privileged; `validBreak`);
`getBlock → if !within(x1,y1,max(range, buildRange)) p3=p4=p5=null else p3=tile.block, p4=build, p5=overlay!=air?overlay:floor`;
`itemDrop` (`p1 == @air` → clear stack; else building of same team, valid, allowDeposit,
`within(build, logicItemTransferRange + size*4)`, amount `min(stack, p2)`, accepted → transfer; timeout 90);
`itemTake` (same team, `p2` Item, `within(...)`, `min(build.items.get(item), min(p3, maxAccepted))`; timeout 90).
`logicItemTransferRange = 45` (`J/Vars.java`, verify: `grep -n logicItemTransferRange Vars.java`).
`UnitLocateI` (l.212-303): `ai.controlTimer = 600`; per-instruction cache refreshed when
`ai.checkTargetTimer(this)`; ore → `indexer.findClosestOre(unit, item)`; building →
closest flagged (enemy or own) building; spawn → closest spawn; damaged → `findDamagedTile`;
outputs `outX/outY = conv(build ? build.x : tile.worldx())`, `found`, `outBuild` (only if within
`max(range, buildingRange=220)` or own team).
`LogicAI.updateMovement` (`J/ai/types/LogicAI.java:50-123`): radar timer 40; control
timeout: `if(controlTimer > 0 && controller != null && controller.isValid()) controlTimer -= delta else resetController`;
move → `moveTo((moveX,moveY), 1, 30)`; approach → `moveTo(.., moveRad - 7, 7, true, null)`;
pathfind → flying ? moveTo : `controlPath.getPathPosition` + moveTo; autoPathfind → enemy core
or spawner logic (l.83-106); stop → `clearBuilding`; boost elevation; look direction.
Targeting: `shouldShoot = shoot && !(canBoost && boost)`; `target()` → `posTarget` (target)
or `mainTarget` (targetp), else null; `retarget()` always true.

**Port today.** `core/logic/executor/unit_control.rs` (`bind_next` l.106, `apply_control` l.140,
`run_ulocate` l.274), `Instruction::UnitBind/UnitControl` in `executor/mod.rs` (~l.1210-1260),
`core/ai/types/logic.rs` (`LogicAi`, `apply_control`, `update_logic` — never called),
`core/sim/unit_runtime.rs` `unit_update_system` (l.1120) AI dispatch chain (Player/Command/
Missile/Suicide/Builder/Boost/else generic).

**Changes.**
1. `UnitBind`: gate on `rules_ref(world).logic_unit_control() || exec.privileged`; accept
   `LogicObject::Unit(u)` when team matches (or privileged) and the type is logic-controllable
   (`UnitTypeDef` field: `grep -n "logic_controllable" core/content/registries/units/mod.rs`;
   if absent default true and note it); `bind_next` must order by the unit's `EntitySeq`
   (spawn order = Java `unitCache` insertion order), not `Entity::index`.
2. `UnitControl` (`executor/mod.rs`): implement `control = !matches!(type_, Unbind | Within)`;
   convert `x1 = p1*8, y1 = p2*8, d1 = p3*8`; write `within` (p4), `getBlock` (p3,p4,p5)
   outputs through `exec.set_*`; pass object params. Move all per-command logic into
   `unit_control::apply_control(world, exec_ctx, unit, type, params)` writing `LogicAi`
   (`ai/types/logic.rs`): extend `LogicAi` with `move_x, move_y, move_rad, controller: Option<Entity>`
   (the processor building), `pos_target`, `main_target`, `shoot`, `aim_control`, `boost`
   (some exist; keep names that exist). Timeouts (`LogicTimeouts`, `blocks/mod.rs:354`) keyed
   by unit `EntitySeq`, delay 90, time source `LogicWorldState.tick` or the building clock.
   Item transfer: reuse the player-transfer helpers G08 uses (`grep -rn "fn transfer_item\|fn take_items" core/`);
   if none exist, move items between `ItemModule`s directly (`remove`/`add` with capacity)
   and note "no transfer FX".
   `mine`: set the unit's mine tile (same component the player mine path uses,
   `Sim::set_player_mine_tile` shows it); `build`/`deconstruct`: push a `BuildPlan` into the
   unit's builder plan queue (grep `BuilderComp\|build_plans` in `core/entities/comp/unit`).
3. LogicAI wiring (G06 owns it; G01 dropped its LogicAI task): in `unit_update_system`, add a branch
   `else if slot.kind == AiKind::Logic { ... }` before the generic else that takes the
   `LogicAi` component, builds an `AiCtx` exactly like the Boost branch does, and calls
   `crate::ai::types::logic::update_logic(&mut ctx, entity, &mut state)`; if it returns
   false → reset the controller (`slot.kind = <the type's default AiKind>`: use the same
   function the controller registry uses to pick defaults, `grep -n "fn default_ai\|fn select" core/ai/controller_registry.rs`)
   and remove `LogicAi`. Extend `update_logic` per the Java above: timeout counts
   `controller` validity (`Health > 0`), `move` → `ctx.move_to(unit, x, y, 1.0, 30.0)`
   (use the closest existing `AiCtx` movement helper: `move_direct`/`move_to`; read
   `core/ai/ai_controller.rs` for names), approach with radius `move_rad - 7`, pathfind
   via `ctx.pathfind`, stop → clear builder plans. Weapons: when `shoot`, aim at
   `pos_target`/`main_target` by setting the unit's shooting target the way other AIs do
   (`ctx.stop_shooting` exists; find its counterpart `ctx.aim_at`/`shoot_at`).
4. `ulocate`: outputs in tiles (`px/8`), `building` output range check per Java; cache
   per instruction index keyed in `LogicAi` (`exec_cache: IndexMap<usize, (f32,f32,bool,Option<Entity>)>`;
   the instruction index is the current `@counter - 1`).

**Tests.** `ubind_unit_object` (spawn dagger u; `ubind u` → `@unit == u`); `ucontrol_move_converts_tiles`
(`ucontrol move 10 10 0 0 0` → `LogicAi.move_x == 80`); live: after 120 ticks the dagger is
within 2 tiles of (10,10); `ucontrol_within` → p4 = 1 when at the spot; `item_take` from a core
with 100 copper: `ucontrol itemTake core 30` (core var via `fetch core c @sharded 0`) → unit stack 30
(capped by `itemCapacity`); `logic_ai_times_out`: stop running the processor → after 600 ticks
the unit's `ControllerSlot.kind` is back to its default.

**Verify in-engine.** Sector 98: spawn a `poly` via `apply_sim_command_json` spawn op,
micro-processor (T13a code) `ubind @poly\nucontrol move 100 60 0 0 0`; `unit_debug_json()`
shows the poly `kind: "LogicAI"` and moving toward (800,480) px.

**Commit.** `Gameplay G06 T9: ubind/ucontrol/ulocate and LogicAI drive live units`

### T10 — `radar` / `uradar` correctness

**Java.** `RadarI` (`LExecutor.java:727-831`) transcribed in the audit: base must be
`Ranged` with `(privileged || r.team() == exec.team)` and (a building not privileged
unless exec privileged, or a unit passing `checkLogicAI(..., true)`); `range = r.range()`
(processors: link range; turrets: turret range; units: `unit.range()`); refresh only every
30 ticks per instruction (or when the source changed) for buildings, per LogicAI target
timer for units, else return the cached unit; team scan: any target `enemy` → every present
team except own; else if none `ally` → every present team; else own team; candidates
`Units.nearby(team, x, y, range)` with `u.within(b, range) && u.targetable(team) && b != u`
and all three filters; maximise `sort.func(b, u) * sortDir` with `sortDir = sortOrder.bool() ? 1 : -1`;
`distance` func = `-dst2`, `health`, `shield`, `armor`, `maxHealth`. Filters
(`RadarTarget.java`): enemy = `team != other.team && other.team != derelict`; ally;
player = `isPlayer`; attacker = `canShoot()`; flying = `isFlying`; boss = `isBoss` (has the
boss status); ground = `isGrounded`.

**Port today.** `core/logic/executor/radar.rs` `find` (l.122) uses `key < current` when
`sort_order` is true → picks the *lowest* health/shield/armor for `sortOrder 1` (Java picks the
highest); no range; `attacker/boss` false; `shield/armor` 0; no cache;
`Instruction::Radar` (executor/mod.rs ~l.1180) has no permission checks.

**Changes.** Rewrite `find` as `best = argmax(sort_value * dir)` with
`sort_value(distance) = -(dx²+dy²)`; add `range` filter `dist <= range` using the hit
radius as Java's `u.within(b, range)` (centre distance only); candidates from
`UnitRuntime.units` order (stable); fill `shield` (`UnitCore`/shield component), `armor`
(type armor), `boss` (status list contains `boss`), `attacker` (unit type has weapons and
`!disarmed`). Permission + range source in `Instruction::Radar`. Cache: add
`last: Option<Entity>, last_source: Option<LogicObject>, timer: f32` fields to
`Instruction::Radar` (update `for_each_var_ref_mut` unaffected) and refresh when
`timer` reached 30 ticks (`timer += 1` per run; Java `Interval.get(30)` uses `Time.time`
— approximate by sim ticks: store the sim tick of the last refresh) or source changed.

**Tests.** `radar_health_order` (two enemy units health 50/100, `radar enemy any any health @this 1 r` → the 100 one;
`0` → the 50 one); `radar_range` (enemy 200 tiles away from a micro processor (range 10 tiles) → null);
`radar_cache` (move the nearest unit away; result unchanged for 29 ticks, changes at 30).

**Commit.** `Gameplay G06 T10: radar sort direction, range, permissions and 30-tick cache match Java`

### T11 — Processor config panel + tap-to-link + link lifecycle (part of PAR-0103)

**Symptom / acceptance.** Tapping a processor shows a dead one-line text field.
After: tapping a (non-privileged, or editor/`allowEditWorldProcessors`) processor opens
the config panel with a single pencil button (opens the `logic` dialog, T13a); while that
panel is open, tapping another building within range toggles the link (cell1, switch1, duo1 …
names) and keeps the panel; tapping the processor again closes the panel; world processors
in campaign open nothing. Unlinking a building that the processor disabled re-enables it.
Acceptance: in-engine sequence below (links visible through `building_config_json` / T12 overlay).

**Java.** `LogicBlock.java`: `buildConfiguration` → `table.button(Icon.pencil, Styles.cleari, this::showEditDialog).size(40)`
(l.~700); `shouldShowConfigure → accessible()`; `accessible() = !privileged || rules.editor || playtesting || rules.allowEditWorldProcessors`
(l.120); `onConfigureBuildTapped(other)` (l.~720): `if(this == other || !accessible()) { deselect(); return false; }`
`if(validLink(other)) { configure(other.pos()); return false; }` `return super.onConfigureBuildTapped(other)`
(= `self != other`, i.e. switch selection). `config(Integer pos)` (l.86-109): if `!validLink(build(pos))` return;
remove every link pointing at that building (`trySet(null)`); if any removed and
`lbuild.block.autoResetEnabled && lbuild.lastDisabler == this` → `lbuild.enabled = true`;
else add `LogicLink(x, y, findLinkName(block), valid=true)` + `trySet`; `updateLinks()`.
`autoResetEnabled` default true, false for `SwitchBlock` (`SwitchBlock.java:23`).
`removeFromProximity` (l.453-461): every linked building with `!enabled && lastDisabler == this` → `enabled = true`.
`damage`/`collide`: privileged and not `destructible` → immune (l.429-449); `canPickup() = false`;
`canBreak(tile) = accessible()`. `InputHandler.tileTapped` (`J/input/InputHandler.java:1973-2030`)
transcribed in the audit: config shown + `getSelected().onConfigureBuildTapped(build)` decides.

**Port today.** `gdext/input/desktop.rs` `config_spec` (l.~1450-1660): `text = kind == LogicBlock || MessageBlock`
→ text field → `configure_building(x,y,"string",…)` → `ConfigValue::String` → world-processor
tag only (`logic_block.rs:539`). No configure-selected state; `tile_tapped` (l.1106) always
opens the tapped building's panel. Link config path: `ConfigValue::Point2` is lowered by
`input/command_emit.rs:256` `to_sim_config` to wire `Int(packed)` and back by
`sim/runtime.rs:200` `resolve_config` to `ConfigValue::Number(packed)`, which
`LogicBlockBehavior::configured` treats as an **icon tag** (l.549) → a link can never be
toggled through the input path. `access::control` clears `last_disabler`.

**Changes.**
1. `logic_block.rs` `configured`: `ConfigValue::Number(n)` → if `n` is an integral value,
   treat it as a packed `Point2` link toggle (`TilePos::from_pack(n as i32)`; Java `Integer`
   config). Drop the icon-tag-from-Number branch (icon tags only come from the editor; add
   a `ConfigValue::String` with prefix `"\u{1}icon:"`? — no: icon tags are editor-only,
   leave them unset and list as deferred).
   `toggle_link`: implement the re-enable on unlink (needs `SwitchDef`-style
   `auto_reset_enabled`: true for all blocks except `BlockKind::SwitchBlock`), checking
   `Building.last_disabler == Some(e)`.
2. `LogicBlockBehavior::on_removed` (or `on_proximity_removed` — use `on_removed`):
   for each `state.executor.links` building with `!enabled && last_disabler == Some(e)` → `enabled = true`.
3. Privileged guards: in the damage path, skip damage for buildings whose block is
   privileged and not `destructible` (find the building damage helper:
   `grep -rn "fn apply_health\|fn damage_building" core/combat`; add the check there,
   one line, cite `LogicBlock.java:429-449`); `canPickup` false for LogicBlock/MemoryBlock
   (grep the pickup gate `fn can_pickup` in `core/sim/runtime.rs` ~l.678); `canBreak`
   false when `!accessible` (break plan validation; grep `fn valid_break`).
4. `gdext/input/desktop.rs`:
   * `config_spec`: for `BlockKind::LogicBlock`: if `!host.logic_accessible(x,y)` return
     `None` (no panel); else spec `{"title":..., "block":..., "x","y", "logic": true, "options": []}`
     and no `text` flag. Add `#[func] logic_accessible(x,y) -> bool` on SimHost
     (`accessible(privileged_of, rules_ref)`).
   * **Depends on G02 T10** (generic configure-tap flow: `core/world/config_tap.rs`
     `on_configure_build_tapped(world, grid, selected, other) -> TapResult`, the
     `DesktopBridge.config_selected` field and `MindInput.close_block_config`). Do not add
     a second selected-state field. Implement only the `BlockKind::LogicBlock` arm of that
     helper (if G02 renamed it, use the name in `G02-blocks.md` T10, "the G02 T10 hook"):
     `selected == other || !accessible(selected)` → `ConfigureAndDeselect(vec![])` (close);
     `valid_link(world, selected, privileged_of(selected), Some(other))` →
     `Configure(vec![(selected, WorldConfigValue::Number(TilePos::new(ox, oy).pack() as f64))])`
     with `(ox, oy)` the other building's centre tile (panel stays open);
     otherwise `Switch`. Add `#[func] logic_valid_link(px,py,ox,oy)` only if the T12
     overlay or tests need it.
   * `ui/fragments/block_config_fragment.gd`: `_rebuild`: `elif bool(_config.get("logic", false)): _build_logic_button()`
     → one 40x40 button with the pencil glyph (`MindWidgets.glyph("pencil", 20)`; check the
     glyph name list used elsewhere) that calls
     `get_node("/root/MindUi").open_dialog("logic", JSON.stringify({"x": x, "y": y}))` and hides the panel.
5. `access::control` `Enabled=false` must set `last_disabler = Some(exec.build)` (T7 item 6 —
   if T7 is done, skip).

**Tests.** `link_toggle_via_number_config` (harness: configure processor with
`ConfigValue::Number(TilePos::new(5,4).pack() as f64)` → one link `cell1`; again → zero links);
`unlink_reenables_disabled`: processor runs `control enabled conveyor1 0` once
(`stop` after), then unlink → conveyor enabled; `world_processor_not_accessible`.

**Verify in-engine.** Ground Zero (`--import` profile has logic unlocked? use API placement):
`$H.place_block(125,50,"micro-processor")`, `$H.place_block(126,50,"switch")`;
click (real mouse via `godot_input`, coords from `$C.tile_to_screen(125,50)`) on the processor
→ `block_config.visible == true`, `_config.logic == true`; click the switch → `building_config_json(125,50)`
decodes (add `#[func] logic_links_json(x,y)` → `[{"name":"switch1","x":126,"y":50,"valid":true}]`)
and the panel is still visible; click the switch again → `[]`. Screenshot.

**Pitfalls.** Start only after G02 T10 is on `main` (`grep -n on_configure_build_tapped core/world/config_tap.rs`); if it is not, stop and report. The tap runs inside `MindInput` while it is bound — never call back into `MindInput`. `Point2` packing is
`(x<<16)|(y&0xffff)` (`core/world/pos.rs:62`).

**Commit.** `PAR-0103: processor config pencil button and tap-to-link with Java link lifecycle`

### T12 — Logic overlays: processor range/links (`drawConfigure`), message text (`drawSelect`), world-processor tag

**Java.** `LogicBuild.drawConfigure` (`LogicBlock.java:~663-683`): `super.drawConfigure()`
(selection square `Drawf.square`/`Pal.accent` — G03's generic config outline); if
`!privileged`: `Drawf.circles(x, y, range)` (dashed double circle, `Pal.accent`); for every
link whose building is a valid link: `Drawf.square(build.x, build.y, build.block.size*8/2 + 1, Pal.place)`
and `build.block.drawPlaceText(l.name, build.tileX(), build.tileY(), true)` (name above the
building). `drawPlace` (l.~185): `Drawf.circles(x*8+offset, y*8+offset, range)` while placing.
`MessageBuild.drawSelect` (`MessageBlock.java:76-100`): when hovered: text (or `[lightgray]` + `@empty`)
in `Fonts.outline` at scale 1/4, width 90, wrapped, drawn below the block with a
`0,0,0,0.2` background rect. `LogicBuild.drawSelect` (world processors only, l.~686-720): tag text
above, icon tag; units controlled by this processor get `Drawf.square(unit, hitSize, rotation+45)`.

**Port today.** No `drawConfigure`/`drawSelect` hooks exist in `gdext/render/`
(`overlays.rs` only draws core edges). G03 owns the generic selection/config outline (§6).

**Changes.** If G03 T14 has landed a generic `drawConfigure`/`drawSelect` hook
(check `G03-rendering-hud.md` T14 and `grep -rn "draw_configure\|draw_select" client/rust/mind-gdext/src/render`),
implement the processor and message draws as entries of that hook instead of a new node;
(G03 T14 names the hooks `ConfigOverlay`/`SelectOverlay` with a `Logic` variant reserved for this task, plus the `MindInput::config_selected()` accessor: use them when G03 T14 is on `main`); otherwise add `gdext/render/logic_overlay.rs` (`LogicOverlay`, Node2D, same pattern as
`shields.rs`: host lookup in `bootstrap`, `queue_redraw` every frame while anything is
shown, `draw()` in world px) attached in `render.rs` next to the shield renderer, on the
`Layer::Overlay`-ish band used by `overlays.rs` (use `BandKey::base(Layer::Overlay)` if
it exists, else the plans band). Data: `MindSimHost::logic_overlay(x, y) -> Option<(f32 cx, f32 cy, f32 range, Vec<(String, f32 bx, f32 by, f32 half)>)>`
(plain Rust method) for the processor in `MindInput`'s `config_selected` (expose
`MindInput::config_selected()` as a Rust accessor); `MindSimHost::message_text(x,y) -> Option<String>`
for the building under the cursor (`hover_info` already computes the hovered tile; reuse
`$I.get_input_state_json` field for the cursor tile). Draw: dashed circles = Arc
`Drawf.circles` (two circles r and r-? — transcribe from `J/graphics/Drawf.java` `circles`
before coding); squares with `Pal.place` (`#6ecdec`); link names with the default font
at 0.25 scale centred above the building; message text below the block with the 0.2-alpha
black rect, max width 90 px, wrapped.

**Tests.** `logic_overlay` data unit test in `gdext`? (no gdext tests) → add a
`core/logic/blocks/logic_block.rs` pure helper `overlay_links(world, e) -> Vec<(String, (f32,f32), f32)>`
with a harness test (link to a 2x2 memory bank → half = 8*2/2+1 = 9.0).

**Verify in-engine.** T11 setup; with the processor panel open, screenshot shows the
range circle (micro: radius 80 px = 10 tiles) and `switch1` above the switch. Hover a
message with text "hi" → screenshot shows "hi" under it.

**Commit.** `Gameplay G06 T12: processor link/range overlay and message hover text`

### T13 — Logic editor dialog (PAR-0103), four slices

**What Java does** (`J/logic/LogicDialog.java`, `LCanvas.java`, `LStatement.java`,
`GlobalVarsDialog.java`): `LogicDialog` is a full-screen `BaseDialog("logic")`, `shouldPause = true`.
`show(code, executor, privileged, modified)` (l.349-370) loads `code` into the canvas
(`LAssembler.read(code, privileged)`, truncated to 1000 statements); on hide the canvas is
saved (`LAssembler.write(statements)`) and, if it changed (or Restart was pressed),
`modified(result)` → `LogicBuild.showEditDialog` (`LogicBlock.java:~700-715`):
`bytes = compress(code, relativeConnections())`; if `bytes.length > 16000` →
`ui.showErrorMessage(bundle.format("logic.error.toolong", 16000, bytes.length))` else `configure(bytes)`.
Bottom buttons (`setup`, l.102-250), each 160x64: `@back` (Icon.left) hides;
`@edit` (Icon.edit) opens a small dialog titled `@editor.export` with (280x60 each, `Styles.flatt`):
[world processors only, not in editor] `@editor.worldprocessors.editname` (text input max 32 → `configure(tag)`),
`@clear` (confirm `@logic.clear.confirm` → clear all statements), `@copy.clipboard` (canvas text →
clipboard), `@load.clipboard` (load clipboard text into the canvas; disabled when the clipboard is
empty), `@logic.restart` (force re-apply even if unchanged, then hide both dialogs);
`@variables` (Icon.menu; text becomes `@logic.globals` when there is no executor/vars, then it opens
`GlobalVarsDialog`): a dialog listing every non-constant executor var: name (accent),
value (refreshed every 15 ticks, flashes accent on change; numbers formatted like `print`),
type name (`number/null/string/content/building/team/unit/enum/unknown`) on a colour
(`typeColor`: number `Pal.place #6ecdec`, null darkGray `#404040`, string `Pal.ammo #ff8947`,
content `logicOperations #877bad`, building `logicBlocks #d4816b`, unit/team `logicUnits #c7b59d`,
enum `logicIo #a08a8a`), plus a `@logic.globals` button; while open the game runs (`state.set(playing)`)
and pauses again on close; `@add` (Icon.add, disabled at 1000 statements) → add dialog:
search field (`@players.search` hint; Enter adds the first match), statements grouped by
`LCategory` in category order (io, block, operation, control, unit, world) with a category
header (icon + `lcategory.<name>` in `Pal.darkishGray`, tooltip `lcategory.<name>.description`),
buttons 130x50 in rows of 3 coloured with the category colour, tooltip `lst.<key>`; hidden
statements and privileged ones (when not privileged) are skipped; unit category skipped when
`!privileged && !rules.logicUnitControl`. Shift+Enter opens the add dialog.
`LCanvas`: a vertical drag layout of `StatementElem`s: header row (category colour) with the
localized name (`instruction.<key>`), the index, buttons add-below (Icon.add → add dialog at
index+1), copy (Icon.copy; middle-click also copies; a copied `jump` whose target is after it
gets `destIndex+1`), delete (Icon.cancel); dragging the header reorders; body = the statement's
own `build(Table)` UI (text fields 180x40 `Styles.nodeField`, enum selector buttons opening a
grid of options, colour pickers for `%rrggbb` literals); `jump` has a `JumpButton` whose drag
draws a curve to the target statement (`JumpCurve`, coloured by state); target width =
`isCompact ? 410 : clamp(screenW*0.95 - 80, 400, 1200)`; auto-scroll near edges while dragging.
Category colours (`J/graphics/Pal.java:124-129`): blocks `#d4816b`, control `#6bb2b2`,
operations `#877bad`, io `#a08a8a`, units `#c7b59d`, world `#6b84d4`, unknown darkishGray `#4d4d4d`.
Bundle keys (all exist in `assets/bundles/bundle.properties`): `back`, `edit`, `variables`, `add`,
`logic.globals`, `logic.restart`, `logic.clear.confirm`, `logic.error.toolong`, `copy.clipboard`,
`load.clipboard`, `clear`, `editor.export`, `editor.worldprocessors.editname`, `editor.name`,
`players.search`, `instruction.*`, `lst.*`, `lcategory.*`, `lenum.*`, `lglobal.*`.

**Port today.** No dialog. `gdext/logic.rs` `MindLogic` (node `/root/Spine/MindLogic`,
`scenes/game.tscn:44`) exposes `statement_names`, `statement_fields`, `statement_meta`
(category, privileged, hidden), `parse(source, privileged)` → `{ok, normalized, statements}`,
`compile`, `default_program`. `SimHost.logic_set_code/logic_get_state/logic_place` are
placeholders (`gdext/sim_host.rs:1100-1125`). `core/ui/manifest.rs:405` already expects
`"logic"` with pause = true.

#### T13a — Text-mode logic dialog, end to end

**Delivers.** Tapping a processor (T11 pencil) opens `logic` with the program as text
(Java clipboard format). The player edits text, Back applies it to the processor (with the
16000-byte check); Edit menu: Clear / Copy / Load from clipboard / Restart; Add inserts a
default statement line chosen from the categorized add dialog. Variables button lists live vars.

**Changes.**
1. Rust seams on `gdext/sim_host.rs` (replace the placeholders):
   * `#[func] logic_get_code(x, y) -> GString` — `LogicBlockState.code` of the building at
     (x,y) normalized through `Assembler::read(code, privileged)` + `Assembler::write`
     (Java's canvas always rewrites the text); empty when not a processor.
   * `#[func] logic_set_code(x, y, code: GString) -> GString` — compress with the
     **relative** links (`state.relative_connections(world, e)` + `logic_block::compress`);
     if `bytes.len() > MAX_COMPRESSED_LEN` return the bundle-formatted error key
     `"toolong:<len>"`; else enqueue `SimCommand::Configure { x, y, value: wire::Bytes(bytes) }`
     (same `enqueue_sim_command` as `configure_building`) and return `""`.
   * `#[func] logic_get_state(x, y) -> Dictionary` — `{privileged, accessible, tag, ipt,
     vars: [{name, value(string, print-formatted), type}]}` from `executor.var_ids`
     skipping constants (Java skips `s.constant`); `type` per `typeName` above.
   * `#[func] logic_privileged(x, y) -> bool`.
2. `ui/dialogs/logic_dialog.gd` (`extends MindDialog`, `should_pause = true`, `full_dialog = true`)
   + `scenes/ui/dialogs/logic_dialog.tscn` (instance of `mind_dialog_base.tscn` like
   `full_text_dialog.tscn`) + entry
   `{"name": "logic", "scene": "res://scenes/ui/dialogs/logic_dialog.tscn", "pause": true, "menu_openable": false, "args": ["x:Int", "y:Int"]}`
   in `ui/dialogs_manifest.json` + instance node `logic` under `DialogLayer` in
   `scenes/ui/ui_root.tscn` (copy the `full_text` lines) + add `"logic"` to the
   appropriate `EXPECTED_*_DIALOGS` list in `core/ui/manifest.rs` (M3 list). Check the
   arg type syntax used by other entries (`"open:Bool"` exists; use `"x:Int"` only if an
   `Int` type is accepted by `DialogsManifest` validation — else pass a JSON context string
   like `content`).
   `shown()`: read `x,y` from `_context`; `_original = $H.logic_get_code(x,y)`;
   `cont` holds a `CodeEdit` (code-instantiated: content is runtime data), monospace font,
   min size 80% of the viewport, with syntax colouring by first token: category colour of
   that statement (`MindLogic.statement_meta(token).category` → colour table above).
   Buttons via `add_button(_t("@back"), …, "left", 160)`, `@edit`, `@variables`, `@add`.
   `hidden()`: `text = $MindLogic.parse(edit.text, privileged).normalized`; if
   `_force_restart or text != _original` → `err = $H.logic_set_code(x,y,text)`; if `err`
   starts with `toolong:` → `MindUi` error prompt with
   `_t("@logic.error.toolong")` formatted `{0}=16000 {1}=<len>`. Parse failures: Java
   `canvas.load` turns bad lines into `InvalidStatement` (noop) — `parse` already does; never block closing.
   Edit dialog: small popup (`_build_prompt`-style Panel or a second MindDialog `logic_edit` — keep it
   inside `logic_dialog.gd` as a code-instantiated `PanelContainer` with the 4–5 buttons).
   Clipboard: `DisplayServer.clipboard_get/set`. Load from clipboard replaces the text after
   `parse(...).normalized`. Restart sets `_force_restart = true` and hides.
   Add: popup with a `LineEdit` search + `ScrollContainer` grid per category; statement list
   from `MindLogic.statement_names()` + `statement_meta(name)` (skip `hidden`, skip
   `privileged` when not privileged, skip `noop`); button text `_t("@instruction." + key)`
   where `key` = Java `statementKey()` = class simple name minus `Statement`, lowercased
   (= `meta.rust_variant.to_lowercase()`; verified: every port variant name lowercased
   matches an `instruction.*` key, e.g. `operation`, `unitbind`, `spawnunit`, `flushmessage`); insert
   `MindLogic.default_statement_text(name)` (new `#[func]` in `gdext/logic.rs`: writes
   `default_statement(name)` with `Statement::write`) as a new line at the caret.
   Variables: a second code-instantiated panel listing `logic_get_state(x,y).vars`
   refreshed every 15 frames, colours per `typeColor`.
3. Escape/back closes (MindDialog default).

**Tests.** Rust: `logic_set_code` size check helper as a pure fn in `core/logic/blocks/logic_block.rs`
(`fn encode_for_config(code, links) -> Result<Vec<u8>, usize>`) with a test that 1000 lines of
`print "<random 30 chars>"` exceed 16000 compressed bytes → `Err(len)`. Manifest test
`repo_dialogs_manifest_complete` passes. GDScript parse gate (env §2).

**Verify in-engine.** T11 setup: click processor → pencil →
`$G --eval 'return get_node("/root/Spine/Ui/UiRoot").dialog("logic").visible'` → true; set the
text through `dialog("logic")._edit.text = "set a 1\nop add b a 2"` and `hide_dialog()`;
after 5 ticks `$H.logic_get_state(x,y).vars` contains `a=1`, `b=3`. Screenshot the dialog.

**Commit.** `PAR-0103: text-mode logic editor dialog (load/save, edit menu, add, variables)`

#### T13b — Variables view + built-in globals list (`GlobalVarsDialog`)

**Delivers.** Faithful variables dialog (rows: name stack on gray, value pane width 140 with
flash, type stack min width 120, colour per `typeColor`, 45 px rows) and the `logic.globals`
dialog: `J/logic/GlobalVarsDialog.java` (61 lines: table of `logicVars.getEntries()`,
section headers `lglobal.section<Name>` for `sectionX` entries, each var row: name in
`Pal.accent`-ish with description `lglobal.<name>` (strip `@`), privileged rows only in
privileged context). Add `#[func] MindLogic.global_entries() -> Array<Dictionary>`
(name, privileged, is_section) from `GlobalVars::new().entries` (ensure `init_static`
pushes the `section*` entries Java pushes: `sectionProcessor` (+ `@this @thisx @thisy @links @ipt`),
`sectionGeneral`, `sectionMap`, `sectionNetwork`, `sectionLookup` — `putEntryOnly` order in
`GlobalVars.java:42-160`). While the variables dialog is open the game runs (Java sets
`state.playing`), so call `$H.set_paused(false)` on open and restore on close only when the
logic dialog paused it.

**Commit.** `PAR-0103: logic variables and built-in globals dialogs`

#### T13c — Canvas mode: statement blocks with editable fields

**Delivers.** The Java canvas look: one panel per statement (header with name, index,
add/copy/delete; body with fields), drag-to-reorder, copy with jump fix-up, 1000-statement
cap, text round-trip identical to T13a (canvas and text are two views of the same
`Array` model; keep a "Text" toggle button so T13a stays available).

**Changes.**
1. Rust (`core/logic/statement.rs` macro): add `pub fn field_values(&self) -> Vec<String>`
   (each field through `MlogField::write` without the leading space) and
   `pub fn from_fields(name: &str, values: &[String]) -> Option<Statement>` (= `read_statement`
   with `[name, values...]`). `MlogField` enum fields: add
   `fn options() -> &'static [&'static str]` (empty for String/int/bool) implemented by
   `logic_enum!` from its `ALL` names. `gdext/logic.rs`: `#[func] read_model(source, privileged) -> Array`
   (`[{name, category, fields:[{name, kind, value, options}]}]`) and
   `#[func] write_model(model: Array) -> GString`.
2. GDScript `ui/logic/logic_canvas.gd` (+ `.tscn` with a ScrollContainer/VBox): build one
   `PanelContainer` per statement: header `HBoxContainer` (category colour background via
   `StyleBoxFlat`, label `instruction.<key>`, spacer, index label, buttons `+`, copy, `x`),
   body `HFlowContainer` (Java `WrapTable` when `useWrapping`): for each field a `LineEdit`
   (180x40) or an enum `MenuButton` listing `options` (labels `lenum.<name>` when the bundle has
   them). Jump fields: `destIndex` shown as a number field in this slice. Drag: header
   `gui_input` press/motion/release moves the panel in the VBox (`move_child`) and updates
   `destIndex` of every jump the same way Java's index-based model implies (Java stores jump
   targets as `StatementElem` references and re-resolves indices on save; the port model must
   remap every jump `destIndex` when statements move/insert/delete: keep a stable id per
   statement and resolve to index in `write_model`).
3. Copy: duplicate below, `jump` with `destIndex > index` gets `+1` (Java `StatementElem.copy`).
4. Sanitize field input like `LStatement.sanitize` (`J/logic/LStatement.java:91-153`):
   transcribe that function into GDScript (strip newlines, keep quoted strings, etc.) before
   building the model.

**Tests.** Rust round-trip: `write_model(read_model(src)) == Assembler::write(read(src))` for
the Ground Zero programs at the top of this plan and `tests.rs` fixtures.

**Commit.** `PAR-0103: canvas-mode logic editor (statement blocks, fields, drag, copy)`

#### T13d — Per-statement layouts and jump arrows

**Delivers.** Each statement body matches its Java `build(Table)` (literal words between
fields, e.g. `op`: `result = a <op> b`; `jump`: `if a <cond> b` with a jump button;
`sensor`: `result = <sensor> in <target>`; `draw`: fields depend on the graphics type;
`ucontrol`: params depend on the command; `setrule`/`fetch`/`effect` similar) and jump
arrows (curves from the jump button to the target header, draggable).

**Changes.** Add a layout table in Rust `core/logic/layout.rs`:
`pub fn layout(statement: &Statement) -> Vec<LayoutItem>` with
`enum LayoutItem { Text(&'static str /* literal or bundle key */), Field { name: &'static str, width: f32 }, Select { field: &'static str }, Color { field }, JumpTarget, Row }`
transcribed **one statement at a time** from `J/logic/LStatements.java` (`build(Table)` of
each class; e.g. `OperationStatement` l.675-737, `JumpStatement` l.929-1017,
`SensorStatement` l.523-648, `DrawStatement` l.122-260, `UnitControlStatement` l.1062-1101).
Commit in three sub-commits by category (io+block; operation+control; unit+world).
`gdext/logic.rs` `#[func] layout(name, values) -> Array`. Canvas renders layouts instead of the
generic field list. Jump curves: a `Control` overlay drawing a cubic from the jump button's
right edge to the target header's right edge (`JumpCurve.drawCurve`, `LCanvas.java:695-723`:
transcribe the control-point offsets), recomputed on layout change; dragging the jump button
and releasing over a statement sets the target.

**Commit.** `PAR-0103: per-statement editor layouts and jump arrows (<categories>)`

### T14a — Displays, sim half: image root index, tileable linking, sizes (PAR-0105)

**Java.** `DrawI` image (`LExecutor.java:1002-1012`): content → `packed = (id << 5) | (ctype.ordinal() & 31)`;
display building → `packed = (d.rootDisplay.index << 5) | 30`; `num1 = packed & 0x3FF`, `num4 = packed >> 10`.
`TileableLogicDisplay.linkDisplays(start)` (`J/world/blocks/logic/TileableLogicDisplay.java:39-99`):
flood-fill over `proximity` members that are `TileableLogicDisplayBuild` (same block, same team via proximity),
root = the member with smallest x, or smaller y (`next.tile.x < root.tile.x || next.tile.y < root.tile.y`),
bounds `botX..topX, botY..topY`; every member: `rootDisplay = root`, `tilesWidth = topX-botX+1`,
`tilesHeight = topY-botY+1`, `originX = botX`, `originY = botY`, `rectangular = w*h == count`,
`buffer = null` (render half merges old buffers). Triggered: `onProximityAdded` → `needsUpdate = true`
(+ neighbours' `onProximityUpdate` for the autotile bits); `onProximityRemoved` → every
proximity tileable display `needsUpdate = true`; `updateTile` → if `needsUpdate` → `linkDisplays(this)`.
`draw(graphicsBuffer)` forwards to the root. Sensors: `displayWidth = tilesWidth*32 - 12`,
`displayHeight = tilesHeight*32 - 12` (frameSize 6), `bufferSize/operations` from the root.
Autotile `bits`: 8 neighbours `Geometry.d8(i)` with the same block and team → `bits |= 1<<i`.
`maxDisplayDimensions = 16` (no buffer beyond).

**Port today.** `core/logic/blocks/display.rs` (`root_display = Some(self)` always, no tile fields);
`draw.rs` `pack_draw` image of a building packs only `DISPLAY_DRAW_TYPE` (l.~205);
`io.rs` `flush_draw` appends to the target, not its root; `access.rs` `DisplayWidth` uses
`display_def_of` only.

**Changes.** `LogicDisplayState`: add `tiles_width: i32 (1), tiles_height: i32 (1), origin_x: i32, origin_y: i32,
rectangular: bool (true), needs_update: bool, bits: u8, revision: u64` (`revision` += 1 whenever
the link set changes, for the renderer). `DisplayBehavior::on_proximity_added/updated/removed` +
`update_tile` implement the Java triggers for `BlockKind::TileableLogicDisplay` (flood fill through
`Building.proximity`, deterministic: process neighbours in proximity order).
`pack_draw` image: `Building(e)` → `index = root.index` (read `LogicDisplayState` of `e`'s root through a
world lookup: `pack_draw` gets `&World`; pass it from `Instruction::Draw`). `flush_draw`: append to the
root's queue. Sensors per Java (T7 hook or here). `LogicDisplays::remove`: Java swap-removes
(last display takes the freed index, `LogicDisplay.java:296-306`) — switch to that so indices
match Java.
Add `pub fn take_display_commands(world: &mut World, root: Entity, max: usize) -> Vec<u64>` (pops from
the front; Java processes the whole queue when drawn).

**Tests.** `tileable_links_rectangle` (3x2 tile displays → all share root = bottom-left, tiles 3x2,
`displayWidth` = 84, `displayHeight` = 52); `image_of_display_packs_root_index`;
`flush_to_member_goes_to_root`.

**Commit.** `PAR-0105: tileable display linking and display-image root index`

### T14b — Displays, render half (PAR-0105)

**Java.** Transcribed in the audit: `LogicDisplay.LogicDisplayBuild.draw/processCommands`
(`LogicDisplay.java:97-251`): per display a `FrameBuffer(displaySize²)` cleared to
`backgroundColor = Pal.darkerMetal (#525252)`; commands are consumed in order when the display is
drawn: `clear(r,g,b)` → whole buffer `(r/255,g/255,b/255,1)` ignoring transform; `color` →
`Color.toFloatBits(x,y,p1,p2)` (0-255 each); `stroke` → `Lines.stroke(x)`; `line` →
`Lines.line(x,y,p1,p2)` with square caps extended by stroke/2 (`ARC/arc-core/src/arc/graphics/g2d/Lines.java:89-112`);
`rect` → `Fill.crect(x,y,p1,p2)`; `lineRect` → `Lines.rect` = four `Fill.crect` bands of width stroke
inside the rect (`Lines.java:511-522`); `poly` → `Fill.poly(x,y,min(p1,25),p2,p3)` (vertices at
`rot + i*360/sides`); `linePoly` → `Lines.poly` (quads between radius ± stroke/2/cos(space/2),
`Lines.java:398-412`); `triangle` → `Fill.tri`; `image` → content `fullIcon` or another display's
buffer at `(x,y)` size `p2 × p2/ratio` rotation `p3`; `print` → one glyph of `Fonts.logic`
(`fonts/logic.ttf`, size 16) at `(x + w/2 + xoffset, y + h/2 + yoffset + capHeight + ascent)`;
`translate/scale (×0.05 step)/rotate/reset` modify the persistent `transform` matrix (saved in the
building, revision 1). The buffer is drawn at the display centre with world size `bufferPx * scaleFactor(1) * Draw.scl`, where `Draw.scl = 1 / scale_marker.width = 0.25` (`J/core/Control.java:330`; the marker sprite is 4 px): logic-display 80 px → 20 world px (block face 24 px minus the frame), large 176 → 44 px, tileable `(32w - 12) * 0.25 = 8w - 3` px. The y size is negative in Java (buffer flipped). Blending disabled when blitting. Tileable: root buffer `32*w - 12` × `32*h - 12`, drawn either full
(rectangular) or per tile (`TileableLogicDisplay.java:241-259`), frame regions `tile-logic-display-#` (47 autotile
variants by `TileBitmask.values[bits]`) on top.

**Changes.** `gdext/render/logic_displays.rs`: `LogicDisplayRenderer` owning, per visible root display,
a `SubViewport` (size = buffer size, `transparent_bg = false`, `render_target_clear_mode = NEVER`,
`render_target_update_mode = DISABLED` until there are commands) with a child Node2D
`DisplayCanvas` whose `draw()` replays the batch of commands popped this frame
(`take_display_commands` via a `MindSimHost` Rust method; only for displays inside the camera view, like
Java), then sets `UPDATE_ONCE`. First creation: draw a full `#525252` rect. Coordinates: draw logic
coords directly in viewport pixels (viewport y-down); the world camera flips y (`gdext/camera.rs:197-203`),
so the texture drawn at the display in world space shows logic (0,0) at the bottom-left like Java.
Because of that flip, glyphs and images inside the viewport must be drawn mirrored vertically
(`draw_set_transform(pos, 0, Vector2(1,-1))` around each glyph/image). Persistent transform: keep a
`Transform2D` per display mirroring Java's `Mat` ops (translate, scale by `x*0.05`, rotate by `p1` degrees
counter-clockwise in logic space = positive angle in the y-down viewport after the flip; verify visually).
Font: load `res://` path of `assets/fonts/logic.ttf` (find how fonts are imported:
`grep -rn "logic.ttf\|\.ttf" client/ --include=*.tscn --include=*.gd --include=*.rs | head`), size 16;
UNVERIFIED glyph metrics: executor compares a `print "AB"` render against the Java reference capture
(take one with the Java client per env §5 only if no `runs/*ref*` frame shows a display) — tolerance 1 px.
World blit: a `Sprite2D`/`draw_texture_rect` of the viewport texture centred on the display at the block's
face size, under the block-overlay band (z just above `Layer::Block`). `DisplayCmd` decode helpers exist in
`core/logic/executor/draw.rs` (`cmd_type/x/y/p1..p4`, `unpack_sign`).
Also feed the "image of content" command from the atlas resolver (`render/atlas_bind.rs` `RegionResolver`).

**Tests.** Pure decode test in core: `decode_commands(&[u64]) -> Vec<DisplayOp>` (new `core/render/logic_display.rs`)
with the Java arithmetic (line cap extension, lineRect bands, poly vertices); e.g. `line 0 0 10 0` stroke 2 →
quad corners (-1,-1),(-1,1),(11,1),(11,-1).

**Verify in-engine.** Ground Zero: logic-display at (125,52) + micro processor linked:
`draw clear 255 0 0\ndraw color 255 255 255 255\ndraw rect 10 10 20 20\ndrawflush display1`;
screenshot: red face with a white square in the lower-left quadrant. Compare to a Java frame.

**Commit.** `Fixes PAR-0105: logic displays render drawflush output (incl. tileable displays)`

### T15 — Canvas blocks: render, logic read/write (painting dialog deferred)

**Java.** `J/world/blocks/logic/CanvasBlock.java`: `palette` default
(`CanvasBlock.java:~22` — transcribe the default int array before coding) / large-canvas woodspark
palette (`Blocks.java:6963-6990`), `bitsPerPixel = log2(nextPow2(palette.length))`; pixels packed
little-endian bits (`getByte`/`setByte`); `read(position)` (l.237-240) → palette index of pixel
`position` (or NaN out of range), `write` (l.247-~255) sets a pixel index; `readable/writable` =
same-team rules; `sense(displayWidth/Height) = canvasSize`; draw: texture of the pixmap at
`size*8 - padding` (padding 3.5) with side/corner frame regions when not blended with neighbours.

**Port today.** `core/logic/blocks/canvas.rs` (`set_pixel`, `get_pixel`, `bits_per_pixel`), no read/write
dispatch in `io.rs`, no render.

**Changes.** `io.rs` `read_building`/`write_target`: `BlockKind::CanvasBlock` → `get_pixel/set_pixel`
with `x = pos % size, y = pos / size`. Render: in the T14b renderer add canvases as an `ImageTexture`
rebuilt when the state revision changes (`palette[index]` RGBA), drawn at
`size*8 - 3.5` px centred. Painting dialog (`CanvasBlock.buildConfiguration` → canvas editor) is
deferred (§2 deferred list).

**Commit.** `Gameplay G06 T15: canvas blocks render and accept logic read/write`

### Deferred (with reason)

* `sync`, `clientdata` and `@client*` network semantics — multiplayer is out of scope.
* `LogicFilter` (map-generation scripts) and the editor's world-processor dialog — editor scope.
* World-processor icon tag (`Character` config) — editor-only input path.
* Canvas painting dialog — Erekir cosmetic block editor; render + logic access are in T15.
* `printchar` of content (needs Java emoji char table) and exact `Double.toString` formatting of
  non-integers ≥ 1e7 (`1.0E7` in Java) — rare; noted in T2 commit bodies.
* `query` instruction result caching parity (`QueryI`, privileged, not used by campaign maps per the scan).
* Bullets as logic objects (`BulletComp.sense/setProp`, `bullet` instruction result) — no bullet
  `LogicObject` in the port; `bullet` spawns stay event-only (no campaign map uses `bullet` per the scan).

---

## §3 Execution order, dependencies, shared files

Order (player impact first; each line = one commit unless noted):
1. **T1** (globals) — no deps.
2. **T2** (executor fixes) — no deps; introduces `LogicContentInfo` used by T3/T7.
3. **T4** (live rules/flags/markers, drain) — needs nothing; unlocks Ground Zero's processors.
4. **T7** (sensors/control) — after T2; Ground Zero/onset `@ammo` objectives need it.
5. **T5** (spawn/setblock/spawnwave/effects) — after T4 (drain).
6. **T6** (messages, cutscene, objective scripts) — after T4.
7. **T11** (processor panel + link tapping) — after G02 T10 and T7 item 6 (`last_disabler`).
8. **T13a** (text editor) — after T11 (pencil button); provides `logic_set_code/logic_get_state` used by later verifications.
9. **T3** (lookup) — any time after T2.
10. **T9** (ubind/ucontrol/LogicAI) — after T4/T5 (atolls flow: spawn → ubind).
11. **T8** (turret logic control) — after T7.
12. **T10** (radar).
13. **T14a**, **T14b** (displays).
14. **T12** (overlays) — after T11.
15. **T13b**, **T13c**, **T13d** (editor fidelity; T13d is 3 commits).
16. **T15** (canvas).

Files shared with other areas (rebase-sensitive; keep edits minimal and local):
* `core/sim/mod.rs` `Sim::tick` (T1, T4) — G01/G04 also add tick steps; insert only the two calls named.
* `core/sim/unit_runtime.rs` `unit_update_system` (T9 Logic branch) — G01 owns AI dispatch (§6).
* `core/world/blocks/defense/turrets/mod.rs` `update_turret` (T8) — G01/G02.
* `core/world/behavior/mod.rs` trait (T7 adds two default methods) and per-family behavior files — G02.
* `gdext/input/desktop.rs` `config_spec`, `core/world/config_tap.rs` (T11, after G02 T10) — G02, G08.
* `ui/fragments/block_config_fragment.gd` (T11) — G02.
* `gdext/camera.rs` (T6 logic cutscene) — G03/G08.
* `ui/ui_root.gd`, `ui/fragments/hud_fragment.gd` (T6) — G03.
* `core/game/map_objectives.rs` `complete` (T6 item 6) — G04.
* `ui/dialogs_manifest.json`, `scenes/ui/ui_root.tscn`, `core/ui/manifest.rs` (T13a) — G05 adds dialogs too.
* `gdext/render.rs` composition (T12, T14b) — G03.

## §4 Ledger changes to report (do not edit `chains.json`)

* PAR-0106 → `fixed` after T1 (headless + in-engine `@tick` readback via T13a seam).
* PAR-0107 → `fixed` after T3 (headless; note `logicids.dat` now shipped).
* PAR-0104 → `fixed` after T4+T5+T6 (in-engine: Ground Zero flags → waveSending; atolls/onset flow).
  Note: Ground Zero map-area expansion additionally needs G04's `limitMapArea` enforcement.
* PAR-0103 → `partial` after T11+T13a (text editor), `fixed` after T13d.
* PAR-0105 → `fixed` after T14a+T14b.
* New items to file:
  1. "mlog `wait 0`/negative waits hang the processor" (T2) — world, S2.
  2. "mlog `op strictEqual` always 0; `format` inverted; `print` of blocks/units/content prints ids" (T2) — world, S3.
  3. "`sensor` returns 0 for most properties (turret @ammo, @progress, @heat...) instead of Java values/null; building/unit @x/@y in pixels" (T7) — world, S2 (blocks Ground Zero `duoAmmo`, onset `breachAmmo`).
  4. "`@thisx/@thisy` are 8x the tile coordinate" (T1 item 5) — world, S3.
  5. "Processor links cannot be created by tapping; `control enabled` ignores link range" (T11/T7) — ui/world, S2.
  6. "`ucontrol`/`ubind` mostly inert; LogicAI never runs" (T9) — world, S2.
  7. "`radar` sorts health/shield/armor backwards and ignores range" (T10) — world, S3.
  8. "`control shoot` cannot aim turrets" (T8) — world, S3.
  9. "Objective `completionLogicCode` runs in a throwaway world" (T6) — world, S2.
  10. "Canvas blocks render blank" (T15) — world, S4.

## §5 Done criteria

* Every task's tests pass; `cargo test -p mind-core --lib --features msav-import` has no new failures;
  `mind-headless` failures ⊆ the G00 list; clippy clean; GDScript parse gate clean.
* In-engine (session N, Ground Zero after `--import`): with the objectives' flags forced through the
  T4 seam, `waveSending/waveTimer` become true, the `mineOver` cutscene pans, `duoAmmo` is set once a duo
  has ammo; a player-placed micro processor can be opened, edited (text mode), linked by tapping, and its
  variables viewed; a logic display shows a drawn rectangle. Evidence PNG/JSON under
  `runs/<date>-s<N>-G06-<task>/`, 0 panics in the log.
* `docs/parity-plans/G06-logic_summary.md` written (env §6 shape) listing per-task status, deviations
  (locale/name literals, emoji, spawn jitter, weatherset if skipped, glyph metrics) and the ledger changes above.

## §6 Cross-area findings

1. **G01** — resolved: G06 T9 owns the single `AiKind::Logic` branch of `unit_update_system`; G01 removed its T18b.
2. **G02** — resolved: G02 T10 builds the generic configure-tap flow; G06 T11 depends on it and adds only the LogicBlock arm.
3. **G03** — Announcement visuals: `ui/ui_root.gd:393` `_on_announce` shows a top-left label for
   `max(1, duration)`; Java `UI.announce` is a centred `Styles.black3` box fading with `pow4In` over
   `duration` (`J/core/UI.java:636-653`); `showInfoToast` (top-centred) has no port entry point (T6 adds a
   minimal one). `state.rules.mission` (set by `message mission`) is shown in Java's HUD wave panel; check
   the port HUD shows it.
4. **G03** — Generic `drawConfigure`/`drawSelect` hover/config overlays are absent (`gdext/render/overlays.rs`
   draws core edges only); T12 adds logic-specific ones.
5. **G04** — `rules.limitMapArea` is set by world processors (Ground Zero `setrule mapArea`, frontier,
   onset) but the port only uses it in rendering (`core/render/rules.rs`, `render/shadow.rs`); Java
   `World.checkMapArea` also blocks building/units outside the area. Ground Zero's tutorial expansion depends on it.
6. **G04** — Save meta `tick` is a Java float (`J/io/SaveVersion.java:135,158` `map.getFloat("tick")`);
   `core/io/sim_io.rs:712` parses it as `u64` (`"123.5"` fails → tick lost). `@tick` after loading a Java save
   restarts from 0.
7. **G07** — `AudioEvent::Play` volume is documented as "already final (sfxvol applied)"; logic
   `playsound` non-positional needs `sfxVolume` applied (T5 note).
8. **G08** — Logic cutscenes must lock input (`InputHandler.inputLocks` includes `logicCutscene`); T6 reuses
   `set_cutscene_lock`.
