<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H06 — Logic (world processors, mlog VM, processors, displays, logic editor)

Date: 2026-10-14. Area: everything under `core/logic/**`, the logic blocks
(`core/logic/blocks/**`), `core/ai/types/logic.rs`, the logic seams on
`gdext/sim_host.rs` / `gdext/logic.rs`, and the logic editor / display UI.

**Read `docs/H-environment.md` first** (paths `REPO WT core/ gdext/ ui/ scenes/
J/ ARC/`, the `$G $H $I $C $MC` shorthands, build/test/commit rules, gmcp eval
rules), then `docs/H-INDEX.md` §3 (cross-plan seams), then this file.

Start state: H00 has landed — `git log main --oneline | grep -m1 'H00'` must print
a line (H00 fixes Serpulo `rules.env` so the player spawns, and instant
`player:Some` placements). If it prints nothing: stop, report.

Drift rule: port line numbers below were read on `main` @ `ffbe84dd`
(2026-10-14) and WILL drift; always re-find a symbol with
`grep -n "<function name>" <file>`. Java line numbers (`J/...:N`) were checked
on 2026-10-14 and are stable. If a symbol named here does not exist, or the
behavior is already implemented, stop that task, write what you saw into your
summary, and go on with the next task. Do not improvise new architecture.

Why this area is gameplay-critical: Ground Zero (sector 170) is driven by its
own **world processors**. The tutorial objectives only raise flags
(`flagsAdded: spawnWave / allowWaves / mineOver / spawnOver`); the five world
processors in the map turn those flags into waves, the map-area expansion,
the camera pan and the `duoAmmo`/`scatterAmmo` objective flags. If one
instruction is inert, the tutorial stalls (e.g. the `DestroyUnits` objective
after the `spawnWave` timer never completes because no wave is spawned).

---

## §0 What the shipped campaign maps run (dumped 2026-10-14)

Dump command (script lives outside the repo, inputs are trusted repo assets):

```bash
mkdir -p /tmp/h06 && cat > /tmp/h06/dumpall.py <<'PY'
import sys,zlib,struct
data=zlib.decompress(open(sys.argv[1],'rb').read())
i=data.find(b'\x78\x9c'); n=0; seen=set()
while i!=-1:
  try:
    out=zlib.decompressobj().decompress(data[i:i+60000])
    if len(out)>5 and out[0] in (0,1):
      ln=struct.unpack('>i',out[1:5])[0]
      if 0<ln<len(out):
        code=out[5:5+ln].decode('utf8','replace')
        if code not in seen and ('\n' in code or code.strip().split(' ')[0].isalpha()):
          seen.add(code); n+=1; print('---- block',n,'len',ln); print(code)
  except Exception: pass
  i=data.find(b'\x78\x9c',i+2)
PY
python3 -I /tmp/h06/dumpall.py REPO/assets/maps/serpulo/groundZero.msav
```

Instructions used per shipped map (world processors + map-placed processors):

| Map | Instructions |
|---|---|
| groundZero | `getflag setflag jump spawnwave setrule(waveSending,waveTimer,mapArea) fetch(build,buildCount) op(add,mod) sensor(@ammo of duo/scatter) cutscene(pan,stop) wait stop` |
| frozenForest | `fetch(buildCount,build) set sensor(@efficiency of combustion-generator) jump setflag end setrate getflag setrule(waveTimer <var>)` |
| frontier | `setrule(waves,mapArea) getflag jump stop` |
| fungalPass | `getflag setrule(waveTimer <var>)` |
| extractionOutpost | `control enabled factory1` (link), `wait`, `jump`, `sensor @team`, `setprop @pyratite nucleus1 99999` (item set on a core) |
| overgrowth | `fetch build/buildCount @crux`, `op greaterThanEq @waveNumber`, `control enabled` |
| littoralShipyard, infestedCanyons, mycelialBastion, geothermalStronghold | `getflag control enabled getlink @links sensor @team op` |
| atolls | `cutscene pan/zoom/stop localeprint message announce/toast @wait setrule(waves,unitCap) setblock block @air spawn ubind ucontrol(stop,move,payTake,payDrop) setprop @health wait` |
| cruxscape | `spawn setprop @shield/@speed status ubind ucontrol move sensor @dead wait` |
| planetaryTerminal | `ubind ucontrol ulocate status setprop fetch getlink control sensor` |
| Erekir onset/basin/origin/aegis/... | adds `effect explosion message print getblock spawnwave(positional?) setrule unitBuildSpeed` |

Ground Zero programs (verbatim, all 5; tests in T7 run exactly these):

```
# GZ-1
getflag wave "spawnWave"
jump 0 equal wave false
setflag "spawnWave" false
spawnwave natural 10 true
# GZ-2
getflag waves "allowWaves"
jump 0 equal waves false
setflag "allowWaves" false
setrule waveSending true 0 0 100 100
setrule waveTimer true 0 0 100 100
# GZ-3
fetch build d @sharded i @scatter
fetch buildCount scatters @sharded 0 @scatter
op add i i 1
op mod i i scatters
sensor amount d @ammo
jump 0 equal amount 0
setflag "scatterAmmo" true
stop
# GZ-4
getflag mineOver "mineOver"
jump 0 equal mineOver false
setflag "mineOver" false
setrule mapArea 10 0 0 256 256
getflag end "spawnOver"
jump 4 equal end false
cutscene pan 135 230 0.03 0
wait 11
cutscene stop 100 100 0.06 0
stop
# GZ-5
fetch build d @sharded i @duo
sensor amount d @ammo
op add i i 1
fetch buildCount totalDuos @sharded 0 @duo
op mod i i totalDuos
jump 0 equal amount 0
setflag "duoAmmo" true
stop
```

Note on the live save (added after the 2026-10-14 in-engine verification): the imported profile's GZ
sector save (`saves/sector-serpulo-15.msav` in `mindustry.save.full.tech.tree.zip`, 2024, resumed as
sector 170) carries an OLDER GZ-3/GZ-5: `fetch build d @sharded 0 @scatter` / `fetch build d @sharded 0 @duo`
(first building only, then `sensor amount d @ammo` / `jump 0 equal amount 0` / `setflag` / `stop`).
The verbatim programs above are from `assets/maps/serpulo/groundZero.msav` (used by headless tests and
fresh maps). Processor tiles on the map: GZ-1 (94,42), GZ-2 (95,42), GZ-3 (96,42), GZ-4 (94,43),
GZ-5 (96,43).

Note on GZ-1: `SpawnWaveStatement` fields are `x, y, natural` in that order
(`J/logic/LStatements.java:1598-1599`), so `spawnwave natural 10 true` means
`x = <variable "natural"> (null → 0)`, `y = 10`, `natural = true` →
`logic.skipWave()`. The port's statement table has the same order
(`core/logic/statement.rs` `SpawnWave`): do not "fix" the text.

GZ objective chain (from the map's `objectives` JSON): mine 15 copper → build
drill → conveyors → 100 copper → lead 25 (`flagsAdded:[mineOver]` → GZ-4 expands
the map area) → … → walls (`flagsAdded:[spawnOver]` → GZ-4 pans the camera to
(135,230) for 11 s) → timer (`flagsAdded:[spawnWave]` → GZ-1 runs a wave) →
`DestroyUnits` → … → `Flag duoAmmo` (GZ-5) / `Flag scatterAmmo` (GZ-3) → final
timer (`flagsAdded:[allowWaves]` → GZ-2 turns the wave timer on). No shipped map
uses `completionLogicCode` (grep of every `assets/maps/**/*.msav`: 0 hits).

---

## §1 Inventory

Status: **OK** ported, wired, verified in this audit; **P** partial/wrong;
**INERT** computed but never applied in the live game; **M** missing.

| Java | Port | Status | Evidence (2026-10-14) | Task |
|---|---|---|---|---|
| `LParser`/`LAssembler`/`LStatements` text half | `core/logic/{parser,assembler,statement,textio}.rs` | OK | statement field order = Java (checked `SpawnWave`, `SetRule`, `Cutscene`) | — |
| `GlobalVars.update` (`@tick @time @waveNumber @waveTime @mapw/h @server @client*`) | `core/sim/mod.rs` `update_logic_globals` → `GlobalVars::update_world` | OK | called in `Sim::tick` before the schedule; `@time = tick/60*1000`, `@waveTime = wavetime/60` | — |
| `@clientLocale`/`@clientName` | literals `"en"`/`"player"` | P | settings keys `locale`/`name` exist in `core/io/settings.rs:61,63` | T11 |
| `WaitI` | `executor/mod.rs` `Instruction::Wait` | OK | `<=0` yields without counter decrement; `cur_time += 1/60` | — |
| `OpI strictEqual`, `round` | `executor/mod.rs` (`StrictEqual` special case), `ops.rs` `Round => (av+0.5).floor()` | OK | | — |
| `LookupI` + `logicids.dat` + `@<type>Count` | `globals.rs` `include_bytes!("../../assets/logicids.dat")` | OK | | — |
| `SetRateI` | `pending_ipt` in `logic_block.rs` `update_tile` | OK | | — |
| `LogicBuild.updateTile` accumulator loop | `logic_block.rs` `update_tile` + `executor.run_budget` | OK | same order as `J/world/blocks/logic/LogicBlock.java:556-571` (run while `acc>=1`, then `acc += edelta*ipt`) | — |
| `GetFlagI`/`SetFlagI` | `world.rs` `run_get_flag`/`run_set_flag` on `CampaignRuntime.session.rules.objective_flags` | OK | in-engine G06 T4 (allowWaves) | — |
| `SetRuleI` | `world.rs` `run_set_rule` → `rules.rs` `apply_rule` | P | writes live rules; only `mapArea` sets `rules_world_dirty`; per-team rules never re-synced into `BuildRules`; no change tracking; `World.checkMapArea` building re-enable missing | T5 |
| `SpawnWaveI` | `run_spawn_wave` → `LogicWorldEvent::SpawnWave` | **INERT** | `Sim::drain_logic_events` drops it → GZ-1 never spawns a wave | **T2** |
| `CutsceneI` | `run_cutscene` → event into `Sim.logic_ui_events` | **INERT** | nobody drains `logic_ui_events` (capped at 256, then dropped); `active`/`getHud` never write `p1`; `CameraState.logic_*` fields exist but are never fed | **T3** |
| `FlushMessageI` / `LocalePrintI` | `run_flush_message` (timers OK) / `run_locale_print` (`no_locale()` = key) | **INERT** | events never reach `MindUi`; no bundle/map-locale resolution | **T4** |
| `SetPropI` → `BuildingComp.setProp(Content)` | `access.rs` `set_prop_content` | **P (wrong)** | port ADDS `amount`; Java SETS the item count to `amount` (`J/entities/comp/BuildingComp.java:2210-2227`) | T6 |
| `SetPropI` unit `speed/flag/health<=0/content` | `access.rs` `set_prop_unit_num` | P | `speed` no-op, `flag` missing, unit `setProp(Item/StatusEffect)` missing | T6 |
| `ControlI enabled/config` + link gate | `access.rs` `control` | OK | `validLink` gate + `last_disabler` present | — |
| `SpawnUnitI`, `ApplyEffectI`, `ExplosionI`, `EffectI`, `PlaySoundI`, `PlayMusicI`, `SetBlockI`, `SenseWeatherI/SetWeatherI` | `world.rs` `run_*` | **INERT** | events only; `spawn` result never set; `weather*` uses a private set | T8 |
| `GetBlockI` live floor/ore | `run_get_block` | P | floor/ore `null` when `WorldGrid` is not a resource (live sim) | T8 |
| `FetchI core/coreCount/player/playerCount` | `run_fetch` | M | returns null/0 | T11 |
| `UnitBindI` | `executor/mod.rs` `Instruction::UnitBind` | **BROKEN** | `@unit` is a constant (`assembler.rs:46` `put_obj_const`) and the instruction skips constant vars → `ubind` never binds; unit-object binding missing; privileged/`logicUnitControl` gate missing | T9 |
| `UnitControlI`/`UnitLocateI`/`LogicAI` | `unit_control.rs`, `ai/types/logic.rs` | P/INERT | `ucontrol` gate ignores `exec.privileged`; `AiKind::Logic` has no branch in `unit_update_system` (falls to generic AI) | T9 |
| `MapObjectives.completionLogicCode` | `game/map_objectives.rs:357` `run_logic_script` | P | runs on a throwaway world (PAR-0224); no shipped map uses it | T10 |
| `SenseI` building/unit/content tables | `access.rs` | P | content sense returns `1.0` for every accessor except `@id`; unit `@range`/`@shooting`/`@shootX/Y`/base armor; liquid turret `@ammo` = liquid amount (Java: `totalAmmo` = 0); building `@name` returns the block name (Java: null) | T11 |
| `PrintCharI` content emoji | appends nothing | P | Java `Fonts.getUnicode(name)` = `assets/icons/icons.properties` code | T11 |
| Turret `control shoot/shootp` | `turrets/mod.rs` | M | `logic_control_time` never set/decremented (PAR-0223) | T12 |
| `RadarI` | `executor/radar.rs` `find` | P | `sort_order=true` picks the MIN health (Java max); no range, no cache, attacker/boss false (PAR-0222) | T13 |
| Map/save-loaded processors assemble against `Vars.logicVars` (`J/logic/LAssembler.java:61`) | `logic_block.rs` `update_code` falls back to `GlobalVars::new()` when the resource is absent; `load_sector_world` materializes buildings before `install_sim_seams` installs it | **B** | in engine: GZ-3/GZ-5 and frozenForest FF-1 list `@sharded`/`@duo`/`@combustion-generator` as null variables; `duoAmmo`/`scatterAmmo`/`power` flags never set | **T20** |
| `LogicDialog`/`LCanvas` editor | — | M | no `logic` dialog; `SimHost.logic_set_code/logic_get_state` are placeholders (`gdext/sim_host.rs` ~l.1486-1495) | T1 (seams), T14, T17 |
| Processor tap-to-link / pencil (`onConfigureBuildTapped`, `config(Integer)`) | `logic_block.rs` `configured` `Number` = icon tag; `desktop.rs` text field | P | PAR-0220 | T15 |
| `drawConfigure`/`drawSelect` overlays | — | M | | T16 |
| `LogicDisplay`/`TileableLogicDisplay` | `blocks/display.rs` sim queue | P | not rendered; root fixed to self (PAR-0105) | T18 |
| `CanvasBlock` | `blocks/canvas.rs` | P | not rendered; no read/write (PAR-0225) | T19 |

### Done — do not redo (verified 2026-10-14 by reading the code)

* `update_logic_globals` wiring, `@thisx/@thisy`, `@wave` removed.
* `wait 0`, `strictEqual`, `round`, `format`, print names, `setrate`, message `read`, sensor null defaults.
* `lookup` + `@itemCount` etc. (`logicids.dat` embedded).
* `LogicWorldState`/`MessageState`/`LogicContentIndex` installed by `gdext/sim_host.rs` `install_sim_seams` with `headless = false`.
* Live `with_rules`/`with_markers`/`with_wave`, getflag/setflag, markers, `rules_ref` reading `CampaignRuntime`.
* Turret `@ammo/@ammoCapacity/@rotation/@shootX/@shootY/@progress/@currentAmmoType` for item and power turrets (`world/blocks/defense/turrets/behavior.rs` `logic_sense`) — test `duo_ammo_sensed`.
* `control` link gate (`exec_privileged || valid_link`), `last_disabler`.
* Unit `@x/@y` (/8), velocity (`*60/8`), `@controlled` constants, mining rows.
* `SimHost.logic_rules_probe()` and `SimHost.logic_set_flag(name, on)` seams (both real `#[func]`s).

### Spot-checked "done" claims of round G (G06 summary)

| Claim | Result |
|---|---|
| T1 globals tick every tick | TRUE (`Sim::tick` → `update_logic_globals`) |
| T2 `wait 0` yields once | TRUE |
| T2 `strictEqual` | TRUE |
| T3 lookup / logicids | TRUE |
| T4 setrule/flags live | TRUE for the rule fields; **incomplete**: per-team rules and other rules never reach `BuildRules`/world (only `mapArea` marks dirty) → T5 |
| T7 duoAmmo sensing | TRUE for duo/scatter; **false detail**: liquid turrets return the liquid amount, Java returns `totalAmmo` (0) → T11 |
| T7 "setprop fixes" | **FALSE** for content: `setprop @copper b 100` adds 100 instead of setting the count to 100 → T6 |
| "ubind/ucontrol partially working" (`§1` of G06) | **FALSE**: `ubind` can never bind (constant `@unit`) → T9 |

---

## §2 Tasks

Conventions:
* Java `World.conv(v) = v / 8` (px → tiles), `World.unconv(v) = v * 8`
  (tiles → px). The port's helpers in `core/logic/value.rs` are named the other
  way round; never rename them, write the arithmetic explicitly.
* Live-sim tests go in `core/sim/production_tests.rs` (helpers there:
  `runtime_sim()`, `place(&mut sim, "name", x, y)`, `item()`, `configure()`,
  `install_campaign_session(&mut sim, rules)`, `place_world_processor(&mut sim, x, y, code)`,
  `test_rules()`). `runtime_sim()` does NOT install `LogicWorldState`; add the
  helper below once (T2) and reuse it:
  ```rust
  /// Live-client logic resources (what `gdext` `install_sim_seams` inserts).
  fn install_logic_world(sim: &mut Sim) {
      sim.ecs.0.insert_resource(crate::logic::world::LogicWorldState {
          headless: false,
          ..crate::logic::world::LogicWorldState::new()
      });
      sim.ecs.0.insert_resource(crate::logic::world::MessageState::default());
  }
  ```
  If `logic_events_drain_each_tick` (same file) already defines an equivalent
  helper, reuse that one instead.
* Run the suites: `$L cargo test -p mind-core --lib logic` and
  `$L cargo test -p mind-core --lib production_tests` (`$L` = the flock prefix of env §2).
* Every task ends with clippy + fmt (env §2). GDScript edits: parse gate (env §2).
* Commit subject per task is given; body names the task id (`H06 Tn`) per env §6.

---

### T1 — Live processor seams: read/write code and state of a placed processor

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. On main: `place_block(120,50,"micro-processor")` true, `logic_set_code(120,50,"set t @tick")` -> false, `logic_get_state` -> `{}`, `logic_available()` -> false (it exists and returns false; `logic_place` and `logic_display_commands` are placeholders too; there is no `logic_get_code`). Dry-run: a temporary build of Changes 1-3 compiled and worked (`build_at`, `relative_connections`, `compress`, `format_value_world` are all `pub`): `t` 8630 -> 8690 over `step(60)` (`runs/20261014-s3-verify2/H06-evals.txt`). **Two corrections.** (1) `Executor.var_ids` keeps non-`_`/`@` constants (`true`, `false`, link names; `executor/mod.rs` ~1806) and `@counter`; Java's variables view skips every `s.constant` (`J/logic/LogicDialog.java:188`), so `state_snapshot` must filter `cell.constant` (as written) and `@`-names. (2) A `world-processor` is NOT accessible in campaign (`accessible() = !privileged || editor || playtesting || allowEditWorldProcessors`, port `logic/blocks/mod.rs:214` = Java `LogicBlock.java:120-122`): `logic_set_code` through `SimCommand::Configure` is silently dropped for world processors (verified: code stayed ""). Almost every T2-T13 verify step needs a world processor (`spawn`, `setblock`, `message`, `setprop`, `fetch`, `cutscene` are privileged). So `logic_set_code` must be a debug seam that writes directly, bypassing the gate like the editor: find `e`, `let mut st = world.entity_mut(e).take::<LogicBlockState>()`, `st.read_compressed(world, e, &bytes, true)`, re-insert (verified working on (118,50)); keep the size gate. Document the bypass in the doc comment.

**Symptom / acceptance.** No in-engine way to load code into a placed processor
or read its variables: `SimHost.logic_set_code` / `logic_get_state` are
placeholders that return `false` / `{}` (`gdext/sim_host.rs`, search
`"Placeholder for applying compressed logic code"`). Every later in-engine
verification in this plan needs them. After: placing a `micro-processor` at
(120,50) and `logic_set_code(120,50,"set t @tick")` makes
`logic_get_state(120,50)["vars"]` contain `t` growing every tick.

**Java.** `LogicBuild.showEditDialog` (`J/world/blocks/logic/LogicBlock.java`,
search `showEditDialog`): `byte[] data = compress(code, relativeConnections());
if(data.length > maxByteLen) ui.showErrorMessage(...) else configure(data)`;
`maxByteLen = 16000`. `LogicDialog.show` loads `LAssembler.read(code, privileged)`
and saves `LAssembler.write(statements)` — the editor always normalizes text.
Variables view (`J/logic/LogicDialog.java`, `showVars`): skips `s.constant` vars,
shows value via `PrintI.toString`-style formatting and a type name.

**Port today.** `core/logic/blocks/logic_block.rs`: `LogicBlockState { code, executor, links, ipt, tag, .. }`,
`relative_connections(&self, world, e) -> Vec<LogicLink>`, `compress(code, links) -> Vec<u8>`,
`MAX_COMPRESSED_LEN = 16_000`. `core/logic/assembler.rs` `Assembler::write(statements)`;
reading text into statements: find the reader with `grep -n "pub fn read" core/logic/assembler.rs core/logic/textio.rs`.
Configure path: `SimCommand::Configure { x: i16, y: i16, value: ConfigValue::Bytes(Vec<u8>) }`
(`core/determinism/command.rs:69`) enqueued with `MindSimHost::enqueue_sim_command`
(Rust-only method in `gdext/sim_host.rs`). Variable printing:
`core/logic/executor/mod.rs` `format_value_world(world, &LVar)` (used by `Instruction::Print`).

**Changes.**
1. `core/logic/blocks/logic_block.rs` — add pure helpers (end of the non-test code):
   ```rust
   /// `LogicBuild.showEditDialog` size gate: compressed bytes or the too-long length.
   pub fn encode_for_config(code: &str, links: &[LogicLink]) -> Result<Vec<u8>, usize> {
       let bytes = compress(code, links);
       if bytes.len() > MAX_COMPRESSED_LEN { Err(bytes.len()) } else { Ok(bytes) }
   }
   /// Processor at a tile (multi-tile aware): the building entity whose footprint covers (x,y)
   /// and that carries `LogicBlockState`.
   pub fn processor_at(world: &World, x: i32, y: i32) -> Option<Entity> { /* use crate::logic::access::linked_build(world, x, y) then check world.get::<LogicBlockState>(e).is_some() */ }
   ```
   (`linked_build` is in `core/logic/access.rs`; it resolves the building at a tile.)
2. `core/logic/blocks/logic_block.rs` — add
   `pub fn state_snapshot(world: &World, e: Entity) -> Option<LogicStateSnapshot>` with
   ```rust
   pub struct LogicStateSnapshot {
       pub privileged: bool, pub accessible: bool, pub ipt: i32,
       pub tag: Option<String>, pub code: String,
       pub links: Vec<(String, i32, i32, bool)>,      // name, x, y, valid
       pub vars: Vec<(String, String, &'static str)>, // name, value text, type name
   }
   ```
   `vars`: iterate `state.executor.var_ids` in order, skip cells with `constant == true`
   and names starting with `@`; value = `format_value_world(world, cell)` (make it
   `pub(crate)` if needed); type name = Java `LogicDialog.typeName`:
   `null` (obj None), `number` (!is_obj), `string`, `content`, `building`, `unit`, `team`,
   `unknown` (match on `LogicObject` variants; add `enum` only if a variant exists for it).
3. `gdext/sim_host.rs` — replace the three placeholders (keep names, they are listed in env §4.2):
   * `#[func] logic_get_code(x: i64, y: i64) -> GString`: the processor's `state.code`
     normalized: read with the assembler reader (`privileged` from `privileged_of(world, e)`)
     then `Assembler::write`; empty string when no processor.
   * `#[func] logic_set_code(x: i64, y: i64, code: GString) -> bool`: find `e = processor_at`;
     `links = state.relative_connections(world, e)`; `encode_for_config(&code, &links)`;
     `Err(len)` → store `self.logic_last_error = format!("toolong:{len}")` and return false;
     `Ok(bytes)` → **write directly** (corrected 2026-10-14: a `SimCommand::Configure` is dropped
     for world processors by the Java-correct `accessible()` gate, and every later verify step
     uses world processors): `let mut st = world.entity_mut(e).take::<LogicBlockState>()`,
     `st.read_compressed(world, e, &bytes, true)`, re-insert, return `is_ok()`. Doc comment:
     "debug seam; bypasses `LogicBuild.accessible()` like the editor". (The player-facing path
     stays the configure command, T14/T15.) Add field
     `logic_last_error: String` + `#[func] logic_last_error() -> GString`.
   * `#[func] logic_get_state(x: i64, y: i64) -> Dictionary` from `state_snapshot`:
     keys `privileged, accessible, ipt, tag, code, links (Array of Dictionary{name,x,y,valid}), vars (Array of Dictionary{name,value,type})`; empty Dictionary when no processor.
   * `#[func] logic_available() -> bool` → `true`.
4. Event log probe (used by T2–T8 verification): `core/sim/mod.rs` add
   `pub logic_event_log: std::collections::VecDeque<String>` (cap 64) to `Sim`
   (initialize in the constructor next to `logic_ui_events`); in `drain_logic_events`
   push `event.kind().to_owned()` for every drained event (pop_front when len == 64).
   `gdext/sim_host.rs` `#[func] logic_event_log_json() -> GString` → JSON array of the kinds.

**Tests.**
* `logic_block.rs` tests: `encode_for_config_rejects_too_long` — build 1000 lines
  `print "<30 distinct chars from a fixed LCG>"` (deterministic; no rand crate)
  → `Err(len)` with `len > 16000`; `"set a 1"` → `Ok(_)`.
* `production_tests.rs`: `logic_state_snapshot_lists_vars` — `place(&mut sim, "micro-processor", 10, 10)`,
  `configure(&mut sim, 10, 10, ConfigValue::Bytes(compress("set a 1\nop add b a 2", &[])))`,
  tick 5 → snapshot vars contain `("a","1","number")` and `("b","3","number")`, no `@`-names.

**Verify in-engine.** Ground Zero (env §4.1 Go path). `$G --eval "return $H.place_block(120,50,\"micro-processor\")"` → true;
`$G --eval "return $H.logic_set_code(120,50,\"set t @tick\")"` → true; `$G --eval "$H.step(60); return $H.logic_get_state(120,50)"`
→ `vars` has `t` > 0; repeat after `step(60)` → larger. Save JSON to
`runs/<date>-s<N>-H06-T1/state.json`. Panic count 0.

**Pitfalls.** `logic_get_state` must not hold `self.sim` borrowed while building
Godot values from another node. Do not call `bind_mut()` on another node. The
seam names `logic_set_code/logic_get_state` must keep these names (env §4.2).

**Commit.** `Gameplay H06 T1: live processor seams (logic_get_code/set_code/get_state, event log)`

---

### T2 — `spawnwave` runs waves in the live game (Ground Zero GZ-1) (PAR-0104 slice)

> **Coordinator note (2026-10-14, after in-engine verification).** H00 T2b changes `run_wave_campaign` to return `(PlayEvent, i32)` where the `i32` is the `getSpawned` index (`pre-increment wave - 1`) and makes sessions start at wave 1. Your `run_wave_now` must use that returned index for `pending_wave` — NOT `runtime.session.wave - 1` as the snippet below says (that is the off-by-one H00 T2b fixed).

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. GZ: `logic_set_flag("spawnWave", true)`; `step(5)` -> flags `[]` (GZ-1 consumed the flag), `wave 1`, `wavetime 21600`; `step(120)` -> still wave 1, unit count 1. Symbols checked: `runtime.events.push(run_wave_campaign(...))` at `game/runtime.rs:396`, `UnitRuntime.pending_wave` (`unit_runtime.rs:116`), `groups_from_rules` (`:4508`, private), `Sim::request_wave_spawn`/`refresh_unit_runtime` (`unit_runtime.rs:965/974`), `SpawnGroup.spawn: i32`. Verify-step fix: `center_on_tile(144,230)` is ignored while the player exists — use `$C.set_process(false); $C.center_on_tile(144,230)` and restore `set_process(true)`; after H00 T2b the expected wave is 1 -> 2.

**Symptom / acceptance.** When GZ's timer objective raises `spawnWave`, GZ-1 runs
`spawnwave natural 10 true`; today `LogicWorldEvent::SpawnWave` is dropped in
`Sim::drain_logic_events`, so no enemies come, `state.wave` does not advance and
the following `DestroyUnits` objective can never complete. After: one
`spawnwave natural ...` → `session.wave += 1`, `wavetime = waveSpacing * difficulty mult`,
and the wave-1 groups spawn exactly like a timer wave. Positional
`spawnwave 50 60 false` spawns, for every spawn group whose `spawn` is -1 or
`Point2.pack(50,60)`, `group.getSpawned(state.wave - 1)` units within 16 px of
world (400,480), without advancing the wave.

**Java.** `J/logic/LExecutor.java:2172-2198`:
```java
if(net.client()) return;
if(natural.bool()){ logic.skipWave(); return; }
float spawnX = World.unconv(x.numf()), spawnY = World.unconv(y.numf());
int packed = Point2.pack(x.numi(), y.numi());
for(SpawnGroup group : state.rules.spawns){
    if(group.type == null || (group.spawn != -1 && group.spawn != packed)) continue;
    int spawned = group.getSpawned(state.wave - 1);
    float spread = tilesize * 2;
    for(int i = 0; i < spawned; i++){
        Tmp.v1.rnd(spread);
        spawner.spawnUnit(group, spawnX + Tmp.v1.x, spawnY + Tmp.v1.y);
    }
}
```
`J/core/Logic.java:315-325`: `skipWave() → runWave()`; `runWave(){ spawner.spawnEnemies(); state.wave++;
state.wavetime = state.rules.waveSpacing * (state.isCampaign() ? difficulty.waveTimeMultiplier : 1f); Events.fire(new WaveEvent()); }`.
`WaveSpawner.spawnUnit(group, x, y)` (`J/ai/WaveSpawner.java`): `group.createUnit(team, wave-1)`, set position, `spawnEffect(unit)`.

**Port today.**
* Timer path: `core/game/runtime.rs` `campaign_tick_system` (search
  `runtime.events.push(run_wave_campaign(&mut runtime.session))`): calls
  `run_wave_campaign(&mut runtime.session)` (`core/game/play.rs`, does `wave += 1` and
  `wavetime = wave_spacing * multiplier`), sets `UnitRuntime.pending_wave = Some(wave - 1)`
  and bumps `stats.waves_lasted`.
* Spawn path: `core/sim/unit_runtime.rs` `run_wave_system` consumes `pending_wave`.
  Per-unit spawn = the closure passed to `wave_spawner.spawn_enemies` (search
  `group.create_unit(world, content, *seq, team, x, y, rotation, wave)`), which also
  applies `unmoving`/`invincible` and advances `seq`.
* `Sim::drain_logic_events` (`core/sim/mod.rs`) only forwards Message/Cutscene.

**Changes.**
1. `core/game/runtime.rs`: extract the timer's run-wave block into
   ```rust
   /// `Logic.runWave()` (Logic.java:319-325) on the live session: advances the wave,
   /// resets the countdown, counts `wavesLasted`, and queues `spawnEnemies` for the
   /// pre-increment wave on the unit runtime.
   pub fn run_wave_now(runtime: &mut CampaignRuntime, unit_runtime: Option<&mut crate::sim::unit_runtime::UnitRuntime>) {
       runtime.events.push(run_wave_campaign(&mut runtime.session));
       if let Some(ur) = unit_runtime { ur.pending_wave = Some(runtime.session.wave - 1); }
       if let Some((planet, _)) = runtime.session.sector
           && let Some(stats) = runtime.campaign.stats_for_mut(planet) { stats.waves_lasted += 1; }
   }
   ```
   and call it from `campaign_tick_system` (behavior unchanged: get the unit runtime
   with `world.get_non_send_mut::<UnitRuntime>()` as today and pass `.as_deref_mut()`).
2. `core/sim/unit_runtime.rs`: extract the per-unit closure body of `run_wave_system`
   into `pub(crate) fn spawn_group_unit(world: &mut World, content: &ContentRegistry, seq: &mut u64, group: &SpawnGroup, team: u8, x: f32, y: f32, rotation: f32, wave: i32, unmoving: Option<StatusId>, invincible: Option<StatusId>) -> Option<Entity>`
   (same statements, same order; the closure now calls it). Use the real field types
   (`seq` type: check `UnitRuntime.seq`).
3. `core/sim/mod.rs` `drain_logic_events`: replace the `if let` with a `match`:
   ```rust
   match event {
       LogicWorldEvent::SpawnWave { natural: true, .. } => self.logic_skip_wave(),
       LogicWorldEvent::SpawnWave { natural: false, x, y } => self.logic_spawn_wave_at(x, y),
       e @ (LogicWorldEvent::Message { .. } | LogicWorldEvent::Cutscene { .. }) => { /* existing push (T3/T4 change it) */ }
       _ => {}
   }
   ```
   (`use crate::logic::world::LogicWorldEvent;` at the top of the fn.)
4. `Sim::logic_skip_wave(&mut self)`: take `CampaignRuntime` with
   `self.ecs.0.remove_resource::<CampaignRuntime>()` (return if none), then
   `let mut ur = self.ecs.0.get_non_send_mut::<UnitRuntime>();` → call
   `run_wave_now(&mut rt, ur.as_deref_mut())`, drop `ur`, re-insert the resource.
   Call `self.refresh_unit_runtime()` first (same as `request_wave_spawn`). No
   `CampaignRuntime` (sandbox/scenario): fall back to `self.request_wave_spawn(current wave)`
   only if a `LogicWorldState` exists: increment `LogicWorldState.wave` and request
   `wave - 1` (Java runs waves in custom games too).
5. `Sim::logic_spawn_wave_at(&mut self, x: i32, y: i32)`: read `rules.spawns` and
   `session.wave` and `session.wave_team()` from `CampaignRuntime` (return if none);
   groups = the same conversion `run_wave_system` uses (`groups_from_rules`, make it
   `pub(crate)`); `packed = crate::world::pos::TilePos::new(x as i16, y as i16).pack()`
   (check the `TilePos::new` arg types; Java `Point2.pack(x,y)` = `(x<<16)|(y&0xffff)`);
   for each group with a resolvable unit type and (`group.spawn == -1 || group.spawn == packed`
   — find the field: `grep -n "pub spawn" core/game/spawn_group.rs`), `n = group.get_spawned(wave - 1)`;
   for i in 0..n: `theta = rng.range(RngStream::Sim, 0.0, 360.0).to_radians()`;
   `sx = x*8 + cos(theta)*16; sy = y*8 + sin(theta)*16` (Java `Tmp.v1.rnd(16)` sets a random
   direction of length 16 — same draw as `spawn_enemies`' ground branch);
   rotation = `Angles.angle(sx, sy, width*8/2, height*8/2)` exactly as `spawn_enemies`' `rotation_at`;
   `spawn_group_unit(...)` with `team = group.team.unwrap_or(wave_team)`; push each entity
   into `runtime.units` and add to `units_created` (take the runtime with
   `remove_non_send`, always re-insert). No campaign difficulty multiplier on this path
   (Java does not apply it here).
6. Note in the commit body: the spawn happens one tick later than Java (Java spawns
   inside the processor's update; the port queues `pending_wave` for the next
   `run_wave_system`) — deviation, no gameplay effect.

**Tests** (`production_tests.rs`, use `install_logic_world` + `install_campaign_session`):
* `spawnwave_natural_runs_wave`: rules with one spawn group `dagger` (`begin 1`, `amount 1`
  — build a `SpawnGroup` the way existing wave tests do: `grep -n "SpawnGroup {" core/sim/*tests*.rs | head`)
  and a spawn overlay tile (copy the setup of an existing wave-spawn test, e.g. the one that
  asserts `pending_wave`); `wave` before = w; world processor `spawnwave natural 10 true\nstop`;
  tick 3 → `session.wave == w + 1`, `wavetime == rules.wave_spacing * mult`, and after 2 more
  ticks the dagger count of team crux is +1.
* `gz1_spawn_wave_flag`: insert flag `spawnWave`; run GZ-1 verbatim (§0); tick 5 → flag gone,
  `wave` +1; tick 100 more → wave still +1 (the loop waits for the flag again).
* `spawnwave_positional_no_wave_advance`: same group with `spawn = -1`; `spawnwave 20 20 false\nstop`
  → wave unchanged, one dagger within 16.5 px of (160,160).

**Verify in-engine.** GZ: `$G --eval "return $H.logic_rules_probe()"` (note `wave`),
`$G --eval "return $H.get_group_count(\"unit\")"`;
`$G --eval "return $H.logic_set_flag(\"spawnWave\", true)"`; `$G --eval "$H.step(120); return $H.logic_rules_probe()"`
→ `wave` +1, flags without `spawnWave`; unit count increased; `logic_event_log_json()` contains
`"spawnwave"`. Screenshot of the spawn area (`$C.set_process(false); $C.center_on_tile(144,230)` — the camera
follows the player otherwise; restore with `$C.set_process(true)` — daggers appear near world (1156,1844)).
Evidence `runs/<date>-s<N>-H06-T2/`.

**Pitfalls.** `campaign_tick_system` removes `CampaignRuntime` during the schedule;
`drain_logic_events` runs after the schedule, so the resource is present — still
handle `None`. Never hold `get_non_send_mut` while calling `self.*` methods.

**Commit.** `PAR-0104: world-processor spawnwave runs live waves (Ground Zero spawnWave)`

---

### T3 — Logic cutscenes: camera pan/zoom, input lock, HUD hide, shake (PAR-0104 slice)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. GZ: `logic_set_flag("mineOver")` + `logic_set_flag("spawnOver")`, `step(30)`, then 4 s unpaused: `camera_state_json()` stays at (1036,444), `camerascale 4.0`, `logic_cutscene false`; `is_cutscene()` false (`runs/20261014-s3-verify2/H06-T3-cut-1s.png`, `-4s.png`). The GZ-4 processor at (94,43) ends at `@counter 9` (`stop`): it ran pan/wait/stop with no effect. All names in Changes 3-7 exist as written (`CameraState.logic_cutscene_zoom >= 0.0` guard at `camera_state.rs:216`, `step_logic_cutscene` :391, `clear_logic_cutscene` :409, `gdext/camera.rs:167` step call, `set_cutscene_lock` `gdext/input/mod.rs:748`, `FxEvent::Shake { intensity, duration }` `fx/sink.rs:62`, `CameraState::on_fx_event` :237, `CutsceneAction::{Active,Pan,Zoom,Stop,Shake,GetHud,SetHud}` in `logic/enums.rs`). Java not observed (the GZ `spawnOver` stage is deep into the tutorial; the zoom claim stands on `Renderer.java:156-161`).

**Symptom / acceptance.** GZ-4 after `spawnOver`: `cutscene pan 135 230 0.03 0`
must lerp the camera toward world (1080,1840), zoom out to the cutscene minimum,
lock gameplay input, hold 11 s (`wait 11`), then `cutscene stop` returns control.
Today the event lands in `Sim.logic_ui_events`, which nobody reads.

**Java.**
* `CutsceneI.run` (`J/logic/LExecutor.java:1350-1369`): `if(headless) return;`
  `active → p1.setbool(input.logicCutscene)`; `pan → logicCutscene = true; logicCamPan.set(unconv(p1), unconv(p2)); logicCamSpeed = p3`;
  `zoom → logicCutscene = true; logicCutsceneZoom = clamp(p1)` (0..1);
  `stop → logicCutscene = false`; `shake → renderer.shake(p1, p2 * 60)`;
  `getHud → p1.setbool(!logicHideHud)`; `setHud → logicHideHud = !p1.bool()`.
* `J/input/InputHandler.java:946-950`: `if(logicCutscene && !renderer.isCutscene()) camera.position.lerpDelta(logicCamPan, logicCamSpeed); else logicCutsceneZoom = -1f;`
* `J/core/Renderer.java:156-161`: `float baseTarget = targetscale; if(input.logicCutscene) baseTarget = Mathf.lerp(minZoom, maxZoom, input.logicCutsceneZoom);`
  then `dest = clamp(baseTarget, minScale(), maxScale())`; `minScale()/maxScale()` use
  `minZoom 1.5 / maxZoom 6` while `logicCutscene` (`Renderer.java:535-541`).
  **With `pan` only, `logicCutsceneZoom == -1` → `lerp(1.5, 6, -1) = -3` → clamped to `minScale()`: the camera zooms fully out.**
* `InputHandler.java:86`: `inputLocks` includes `() -> logicCutscene`;
  `DesktopInput.java:276` `if(!locked){ ...camera follow / pan... }` — no follow while locked.
* HUD: `HudFragment.java:1239` `shown() = shown && !logicHideHud`.
* Reset: `InputHandler.java:921-922` (`logicCutscene = false; logicHideHud = false` on world reset), `:1084` on menu.

**Port today.**
* Core: `run_cutscene` (`core/logic/world.rs`) only emits `LogicWorldEvent::Cutscene`.
* Camera: `core/input/camera_state.rs` `CameraState` already has `logic_cutscene`,
  `logic_cutscene_zoom`, `logic_cam_pan`, `logic_cam_speed`, `step_logic_cutscene(delta)`,
  `clear_logic_cutscene()`, and `min_scale/max_scale` switch on `logic_cutscene`. Bug:
  `update()` uses `if self.logic_cutscene && self.logic_cutscene_zoom >= 0.0` (Java has no `>= 0` guard).
* `gdext/camera.rs` `process`: calls `self.camera.step_logic_cutscene(delta_frames)` but
  nothing ever sets `camera.logic_cutscene`; player follow/pan run unconditionally.
* Input locks: `core/input/focus.rs` has `LockId::LogicCutscene` / `locks.logic_cutscene`;
  `gdext/input/mod.rs` `set_cutscene_lock(bool)` handles only the renderer cutscene.

**Changes.**
1. `core/logic/world.rs`: add
   ```rust
   /// `InputHandler.logicCutscene/logicCamPan/logicCamSpeed/logicCutsceneZoom/logicHideHud`.
   #[derive(Clone, Copy, Debug, PartialEq)]
   pub struct LogicCutscene { pub active: bool, pub pan: (f32, f32) /* world px */, pub speed: f32, pub zoom: f32, pub hide_hud: bool, pub shake: Option<(f32, f32)> /* intensity, duration ticks; consumed by the host */ }
   impl Default for LogicCutscene { fn default() -> Self { Self { active: false, pan: (0.0, 0.0), speed: 0.1, zoom: -1.0, hide_hud: false, shake: None } } }
   ```
   and field `pub cutscene: LogicCutscene` on `LogicWorldState` (init in `new()`).
2. Rewrite `run_cutscene`: `let Some(mut state) = world.get_resource_mut::<LogicWorldState>() else { return };`
   `if state.headless { return; }` then exactly Java:
   `Active → exec.set_num(p1.id(), bool as f64)`; `Pan → active = true; pan = (num(p1)*8, num(p2)*8); speed = num(p3)`;
   `Zoom → active = true; zoom = num(p1).clamp(0.0, 1.0)`; `Stop → active = false`;
   `Shake → shake = Some((num(p1), num(p2)*60))`; `GetHud → set p1 = !hide_hud`;
   `SetHud → hide_hud = !bool_of(p1)`. Keep emitting the event (tests count it). Read
   vars before taking the resource borrow (borrowck: compute `num(...)` values first).
   Check the `CutsceneAction` variant names with `grep -n "enum CutsceneAction" -A12 core/logic/enums.rs`.
3. `core/sim/mod.rs`: stop pushing `Cutscene` events into `logic_ui_events` (drop them in
   `drain_logic_events`; the state is authoritative). Add
   `pub fn logic_cutscene(&self) -> crate::logic::world::LogicCutscene` (default when no resource) and
   `pub fn take_logic_shake(&mut self) -> Option<(f32, f32)>` (takes `cutscene.shake`).
   Add `pub fn reset_logic_presentation(&mut self)`: `cutscene = default`, `MessageState` timers to 0,
   `logic_ui_events.clear()` (Java `InputHandler` reset on world load).
4. `gdext/sim_host.rs`: plain Rust `pub fn logic_cutscene_state(&self) -> LogicCutscene` and
   `pub fn take_logic_shake(&mut self) -> Option<(f32,f32)>` (forwarders);
   `#[func] logic_cutscene_json() -> GString` → `{"active","x","y","speed","zoom","hideHud"}`.
   In `load_sector_world` (and any other world-load entry that calls `install_sim_seams`;
   grep `install_sim_seams(` ) call `self.sim.reset_logic_presentation()` after the load.
5. `core/input/camera_state.rs` `update()`: `let base_target = if self.logic_cutscene { lerp(MIN_ZOOM, MAX_ZOOM, self.logic_cutscene_zoom) } else { self.target_scale };`
   (remove the `>= 0.0` guard; cite `Renderer.java:156-161`). Fix its unit test if one
   asserted the old behavior; add `pan_only_cutscene_zooms_to_min_scale`: logic_cutscene = true,
   zoom = -1 → after 200 `update(1.0,..)` calls `camerascale == min_scale()`.
6. `gdext/camera.rs` `process` (minimal edit; H03/H08 share this file):
   * at the top (after the host lookup), `let logic = host.bind().logic_cutscene_state();`
     (use `bind()` only) and copy into the rig **only when not in the landing cutscene**
     (`!self.cutscene.is_cutscene()`, Java `!renderer.isCutscene()`):
     `self.camera.logic_cutscene = logic.active; self.camera.logic_cam_pan = logic.pan; self.camera.logic_cam_speed = logic.speed;`
     `self.camera.logic_cutscene_zoom = if logic.active { logic.zoom } else { -1.0 };`
     When the landing cutscene runs: `self.camera.logic_cutscene_zoom = -1.0` and do not step.
   * wrap the player-follow / axis pan / edge-pan block in `if !self.camera.logic_cutscene { ... }`
     (Java `DesktopInput` `if(!locked)`), keep `clamp_position()` outside.
   * shake: `if let Some((i, d)) = host.bind_mut().take_logic_shake()` → the rig's existing
     shake entry (`self.camera.on_fx_event(&FxEvent::Shake { intensity: i, duration: d })` —
     check the `FxEvent::Shake` field names). Do not hold the bind across other calls.
   * input lock: track `logic_locked: bool` like `cutscene_locked`; on change call
     `input.bind_mut().set_logic_cutscene_lock(active)`.
7. `gdext/input/mod.rs`: add `#[func] pub fn set_logic_cutscene_lock(&mut self, locked: bool)`
   — copy `set_cutscene_lock` but on `self.locks.logic_cutscene` (field in `core/input/focus.rs`).
8. HUD hide: `ui/fragments/hud_fragment.gd` `_process`: every 10 frames read
   `JSON.parse_string($SimHost.logic_cutscene_json()).hideHud` (get the host node the way the
   file already does; grep `SimHost` in it) and set the HUD root `visible = not hide_hud`
   only when the value changes (do not fight other visibility logic: store and restore the
   previous visibility). H03 owns HUD visuals; this is one guarded property toggle.

**Tests** (`production_tests.rs`):
* `cutscene_pan_sets_state`: `install_logic_world`; world processor
  `cutscene pan 10 20 0.05 0\ncutscene active a 0 0 0\nstop` → `sim.logic_cutscene()` =
  `{active: true, pan: (80.0,160.0), speed: 0.05, zoom: -1.0}` and var `a == 1` (read via T1's `state_snapshot`).
* `gz4_cutscene_runs_and_stops`: flags `mineOver` + `spawnOver`; run GZ-4 verbatim; tick 5 →
  `active`, `pan == (1080.0, 1840.0)`, `rules.limit_map_area == true`, `limit_width == min(256, map_w)`;
  tick 11*60+10 more → `active == false`.
* `cutscene_headless_noop`: `LogicWorldState::new()` (headless true) → state stays default.

**Verify in-engine.** GZ: `$H.logic_set_flag("mineOver", true)`, `$H.logic_set_flag("spawnOver", true)`,
`step(30)` → `logic_cutscene_json().active == true`; take 3 screenshots 1 s apart (real time,
unpaused) showing the camera moving toward tile (135,230) and zooming out; `$C.camera_state_json()`
shows decreasing distance to (1080,1840); after 12 s `active == false` and the camera follows the
player again. `$I.get_input_state_json()` shows the logic lock while active (check the field name).
Compare framing with the Java GZ reference frames in `runs/20261009-065416-ref-ground-zero/java/` if one
shows the pan. Evidence `runs/<date>-s<N>-H06-T3/`.

**Pitfalls.** `MindCamera2D` holds `self.input`/`self.host` Gd handles: never keep a
`bind()` of the host while calling `input.bind_mut()`. Do not touch the landing cutscene logic.

**Commit.** `PAR-0104: logic cutscenes drive the camera, input lock and HUD (Ground Zero pan)`

---

### T4 — Messages and `localeprint` reach the player (PAR-0104 slice)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. World processor (118,50) running `print "hello"` / `message announce 3 @wait` -> the event sits in `Sim.logic_ui_events` (`[Message { type_: Announce, text: "hello", duration: 3.0 }]`), no HUD box (`runs/20261014-s3-verify2/H06-T4-announce.png`). `localeprint "atolls.noairunit"` -> event text `"atolls.noairunit"` (bundle has the sentence, `bundle.properties:2436`). The verify step works only with T1's direct-write `logic_set_code` (world processors are not accessible in campaign, see T1).

**Symptom / acceptance.** `print "hello"\nmessage announce 3 @wait` shows a centred
announcement for 3 s; atolls' `localeprint "atolls.noairunit"` + `message announce 6 @wait`
shows the bundle text "This base is too well-fortified to destroy with air units.";
`message notify` shows a HUD toast with the info icon; `message toast 4.5` shows an info
toast; `message mission` sets `rules.mission`. Today the timers are right but the events
never leave `Sim.logic_ui_events`.

**Java.** `FlushMessageI.run` (`J/logic/LExecutor.java:1931-1972`; blocking part is
already ported): `text = UI.formatIcons(textBuffer)`; `if(text.startsWith("@") && bundle.has(text.substring(1))) text = bundle.get(substr)`;
`notify → ui.hudfrag.showToast(Icon.info, text)`; `announce → ui.announce(text, duration)`;
`toast → ui.showInfoToast(text, duration)`; `mission → state.rules.mission = text`.
`LocalePrintI.run` (`LExecutor.java:2402-2420`): `if(textBuffer.length() >= maxTextBuffer) return;`
name must be an object; `strValue = state.mapLocales.getProperty(name)` (desktop) and append.
`MapLocales.getProperty(key)` (`J/type/MapLocales.java:47-54`): current locale → `"en"` →
`bundle.get(key)` if `bundle.has(key)` → `"???" + key + "???"`.

**Port today.** `run_flush_message` (`core/logic/world.rs`) emits `LogicWorldEvent::Message { type_, text, duration }`
with the raw buffer. `run_locale_print` appends the key (`no_locale()`). Core text helpers:
`core/ui/text.rs` `format_icons(s, &Iconc)` (exact `UI.formatIcons` port), `core/assets/icons.rs`
`Iconc`, `core/assets/bundle.rs` `Bundle` (`get`; check for `has`/`contains`). gdext loaders:
`gdext/assets/bundle.rs` `load_bundle`, `load_iconc(assets_dir)` (used by `MindAssets`, see
`gdext/assets/atlas.rs` around `bundle:` / `load_iconc(`). `MindUi` (`gdext/ui/ui_host.rs`) has
`#[func] announce(text, duration)` and `#[func] toast(text, icon)`; `ui/ui_root.gd` `_on_announce`
→ `_announce_box` (faithful `UI.announce`). Map locales: save meta tag `locales` is parsed into
`SaveReadState.locales` (`core/io/save/versions/v1.rs` `read_meta`) — check where it goes after load
(`grep -rn "\.locales" client/rust/mind-core/src client/rust/mind-gdext/src | grep -v test`).

**Changes.**
1. `core/logic/world.rs`: new resource
   ```rust
   /// Text sources for `FlushMessageI`/`LocalePrintI`/`PrintCharI` (host-installed).
   #[derive(Resource, Clone, Default)]
   pub struct LogicText { pub bundle: Option<crate::assets::bundle::Bundle>, pub iconc: Option<crate::assets::icons::Iconc>, pub map_locales: crate::io::json::MapLocales, pub locale: String /* "en" default */ }
   ```
   (`Default` for `locale` must be `"en"`: implement `Default` by hand.) Add `bundle_has(&Bundle, key)`
   if `Bundle` has no `has`: a key is present when the lookup does not fall back to the key itself —
   prefer adding `pub fn has(&self, key:&str) -> bool` to `Bundle` directly.
2. `run_flush_message`: before emitting, `text = format_icons(&text, iconc)` when an `Iconc` is installed;
   then the `@`-bundle substitution exactly as Java. Mission branch uses the same resolved text.
3. `run_locale_print`: Java `getProperty` chain using `LogicText`: `map_locales.get_property(locale, key)`
   only when `map_locales.contains(locale,key) || contains("en",key)` (use the existing
   `MapLocales::contains/get_property`), else bundle (`has` → `get`), else `format!("???{key}???")`.
   Append to the buffer (respect `MAX_TEXT_BUFFER` like today).
4. `gdext/sim_host.rs` `install_sim_seams`: insert `LogicText { bundle: Some(load_bundle(..)), iconc: Some(load_iconc(..)), map_locales: <loaded map locales or default>, locale: <settings "locale" or "en"> }`
   — load exactly the way `MindAssets` does (same assets dir / locale args). On sector load, set
   `map_locales` from the loaded save/map meta (if the load path drops `SaveReadState.locales`,
   thread it through to the host in the smallest way: return it next to the other load results).
5. `core/sim/mod.rs`: `pub fn take_logic_messages(&mut self) -> Vec<(MessageType, String, f32)>`
   draining `logic_ui_events` (only `Message` remains there after T3).
6. `gdext/sim_host.rs`: `#[func] drain_logic_messages() -> Array<Dictionary>` →
   `{type: "announce"|"notify"|"toast", text, duration}` (mission never queued).
7. `ui/ui_root.gd` `_process`: call `drain_logic_messages()` on the SimHost (null-check the node;
   skip when the menu is visible); per entry: `announce` → `get_node("/root/MindUi").announce(text, duration)`;
   `notify` → `MindUi.toast(text, "info")` (check the icon-name convention used by other `toast` callers);
   `toast` → `if MindUi.has_method("show_info_toast"): MindUi.show_info_toast(text, duration) else: MindUi.announce(text, duration)`
   (H03 owns `UI.showInfoToast` visuals; the fallback is temporary — list it in the summary).
   No bundle lookups in GDScript (the core already resolved them).

**Tests.**
* `world.rs` tests: `flush_message_resolves_bundle_key` (insert `LogicText` with a `Bundle`
  holding `atolls.noairunit` → text resolved); `locale_print_falls_back_to_bundle_then_marker`
  (`localeprint "x.y"` with empty map locales and no bundle key → buffer `???x.y???`; map locale `en`
  `x.y=hi` → `hi`).
* `production_tests.rs` `message_announce_drained`: `install_logic_world`, world processor
  `print "hi"\nmessage announce 3 ok\nstop` → `take_logic_messages()` == `[(Announce, "hi", 3.0)]`, `ok == 1`;
  second call returns `[]`.

**Verify in-engine.** GZ: place a `world-processor` with API placement at a free tile
(`$H.place_block(118,50,"world-processor")`), `logic_set_code(118,50,"print \"hello\"\nmessage announce 3 @wait\nstop")`,
`step(5)`, screenshot within 1 s → centred "hello" box. Then code `localeprint "atolls.noairunit"\nmessage announce 4 @wait\nstop`
→ screenshot shows the English bundle sentence. Evidence `runs/<date>-s<N>-H06-T4/`.

**Pitfalls.** Calling `MindUi` from inside `SimHost` while bound is forbidden — the GDScript poller
is the bridge. Do not restyle `_announce_box`.

**Commit.** `PAR-0104: world-processor messages and localeprint reach the HUD (bundle/map locales, formatIcons)`

---

### T5 — `setrule`: change tracking, world/build re-sync, `World.checkMapArea` (PAR-0104 slice)

> **Coordinator override (2026-10-14).** `allow_update`, `check_allow_update`, `Sim::check_all_allow_update`, `BlockDef.auto_reset_enabled` and the `World.checkMapArea` reset are implemented by H09 T4 in `core/world/allow_update.rs` (NOT `core/world/update.rs`). In this task do the change tracking + `rules_world_dirty` + `sync_build_rules` parts only, and for `mapArea` call H09 T4's checkMapArea entry with the PREVIOUS area if it is on `main` (`grep -rn "fn check_all_allow_update" client/rust/mind-core/src`); if it is not, skip the checkMapArea half and list it as `skipped: waiting for H09 T4`. Note: H02's signature is `world::limits::sync_build_rules(world, &rules, is_campaign: bool)` (three arguments).

> **In-engine verification (2026-10-14, session 3):** PARTLY. The live `mapArea` half works today: GZ `logic_set_flag("mineOver")`, `step(30)` -> `limitMapArea false` (GZ is 256x256, so `setrule mapArea 10 0 0 256 256` covers the full map and Java disables the limit, `LExecutor.java:1861-1906`); the former border is lit (`runs/20261014-s3-verify2/H06-T5-area-after.png`); GZ rules have `disableOutsideArea true`. Change tracking (dirty only for `MapArea`, every run), per-team rules and the building re-enable are confirmed by reading `run_set_rule` (`logic/world.rs` ~1124-1185). Verify-step fact: expect `limitMapArea false` after mineOver (not a new rect).

**Symptom / acceptance.** Every `setrule` that changes a value must re-derive the live
state that depends on rules (Java reads `state.rules` live everywhere): per-team rules
(`buildSpeed`, `unitBuildSpeed`, `unitHealth`, `blockHealth`, ...) must reach the
port's `BuildRules`/unit code; `mapArea` must re-enable buildings outside the previous
area. Re-setting an unchanged value (frozenForest/fungalPass run `setrule waveTimer x`
every tick) must cost nothing (no grid clone per tick).

**Java.** `SetRuleI.run` (`J/logic/LExecutor.java:1779-1858`, per-team at 1840-1855),
`checkMapArea` (`:1861-1906`): on a real change calls `world.checkMapArea(prevX, prevY, prevW, prevH)`;
`World.checkMapArea` (`J/core/World.java:469-481`):
```java
for(var team : state.teams.present) for(var build : team.buildings){
    if(!build.enabled && build.block.autoResetEnabled && !Rect.contains(x, y, w, h, build.tile.x, build.tile.y)) build.enabled = true;
    build.checkAllowUpdate();   // enabled = false when !allowUpdate()
}
```
`allowUpdate()` (`J/entities/comp/BuildingComp.java:628-632`): `team != derelict && block.supportsEnv(env) && (block.privileged || !limitMapArea || !rules.disableOutsideArea || Rect.contains(limit rect, tile))`.

**Port today.** `run_set_rule` sets `CampaignRuntime.rules_world_dirty = true` only for
`MapArea`, every time it runs (changed or not). `Sim::apply_dirty_rules_to_world`
(`core/sim/runtime.rs`) → `Sim::apply_rules_to_world(&rules)` (`core/sim/mod.rs`): sets
`grid.limit_area`, `border_darkness`, then `refresh_unit_runtime()` (clones the whole grid).
The comment there says H02's `sync_build_rules` will be called from it. `apply_rule`
returns "applied", not "changed".

**Changes.**
1. `core/logic/rules.rs` `apply_rule`: return `RuleChange { applied: bool, changed: bool, map_area: Option<(i32,i32,i32,i32)> /* previous rect when the area changed */ }`
   (new `pub struct` in the same file). Implement `changed` per arm with a small helper:
   ```rust
   fn set<T: PartialEq>(slot: &mut T, v: T) -> bool { if *slot != v { *slot = v; true } else { false } }
   ```
   (use it in every arm; `ban/unban` → whether the set changed; per-team arms via `team_rule_mut`).
   `check_map_area` returns `Option<(px,py,pw,ph)>` — the previous rect exactly as Java computes it
   (`(-1,-1,0,0)` when the area was not limited before) — when it changed anything, else `None`.
   Keep `apply_to_rules`/`apply_to_state` wrappers compiling (map the struct back to `bool applied`).
   Before coding, diff the `match rule` arms against the Java list (`LExecutor.java:1785-1855`) and add
   any missing rule (`ambientLight`, `solarMultiplier`, `dragMultiplier`, `unitLight`, `ban`, `unban`,
   `rtsMinWeight`, `rtsMinSquad`, ...) with the Java clamp.
2. `run_set_rule`: when `changed` → set `runtime.rules_world_dirty = true` (any rule, not only
   mapArea); when `map_area = Some(prev)` → also push `prev` into a new
   `CampaignRuntime.pending_map_area_checks: Vec<(i32,i32,i32,i32)>` (add the field, `#[serde(skip)]`
   if the struct is serialized; check derives).
3. `core/sim/runtime.rs` `apply_dirty_rules_to_world`: after `apply_rules_to_world`, drain
   `pending_map_area_checks` and run `world_check_map_area(prev)`:
   iterate every building entity in `EntitySeq` order (`(Entity, &Building, &TeamComp, &BuildingComp-or-pos)` —
   use the component that holds the tile pos; `grep -n "pub struct BuildingComp" -A8 core/entities/comp/*.rs`);
   if `!enabled && auto_reset_enabled(block) && !rect_contains(prev, tile)` → `enabled = true`;
   then `if !allow_update(...) { enabled = false }` where `allow_update` = Java formula
   (`team != DERELICT`; env support: `grep -n "fn supports_env" core/content` — if absent, treat as true and note it;
   `privileged` from the block def; `rules.disable_outside_area` — check the field name in
   `core/io/json/rules.rs`). `auto_reset_enabled`: true except `BlockKind::SwitchBlock`
   (`J/world/blocks/logic/SwitchBlock.java:23`). Put `allow_update` in `core/world/update.rs` as
   `pub fn allow_update(world, e, rules) -> bool` so H02/H04 can reuse it (§6).
4. Per-team/BuildRules: `apply_rules_to_world` is the single re-derive point. If
   `world::limits::sync_build_rules` exists on `main` (`grep -rn "pub fn sync_build_rules" client/rust/mind-core/src`),
   call `crate::world::limits::sync_build_rules(&mut self.ecs.0, rules, is_campaign)` inside `apply_rules_to_world` (third argument = whether the live session is a campaign session; read how H02 T2's own call site computes it: `grep -rn "sync_build_rules(" client/rust/mind-core/src`)
   (replace the comment). If it does not exist, leave the comment and list "per-team setrule waits for H02
   sync_build_rules" in the summary — do NOT write your own BuildRules sync (H02 owns it).
5. `refresh_unit_runtime()` in `apply_rules_to_world` stays (the grid area changed); because `changed` now
   gates the dirty flag, it runs only on real changes.

**Tests** (`rules.rs` + `production_tests.rs`):
* `setrule_unchanged_is_not_dirty`: run `setrule waveTimer true` twice on a session that already has
  `wave_timer = true` → `rules_world_dirty == false` after the tick (clear it first).
* `map_area_reenables_outside_buildings`: rules `limit_map_area` 0,0,20,20 + `disable_outside_area = true`;
  a conveyor at (25,25) is disabled by the first check; GZ-4-style `setrule mapArea 10 0 0 50 50` → after
  2 ticks the conveyor is `enabled`.
* `per_team_rule_changes_mark_dirty`: `setrule buildSpeed 2 @sharded 0 0 0` → `team_rule(1).build_speed_multiplier == 2.0` and dirty set.

**Verify in-engine.** GZ: `logic_set_flag("mineOver", true)`, `step(30)`, `logic_rules_probe()` →
`limitMapArea` false (verified 2026-10-14: GZ is 256x256, so the GZ-4 rect is the full map and Java turns
the limit off — this half already works on main); screenshot of the previously dark border region now lit.
The discriminating in-engine part of this task is the building re-enable: place a `conveyor` outside the
original area (a snow tile with y >= 80, e.g. (129,84) — snow in `H06-T5-area-after.png`; check `tile_debug`
first) BEFORE `mineOver`, confirm `building_enabled(129,84) == false` (needs H09 T4's `checkAllowUpdate`),
raise `mineOver`, `step(5)` -> `true`. This needs a fresh GZ (the area is still limited). Evidence `runs/<date>-s<N>-H06-T5/`.

**Pitfalls.** `CampaignRuntime` is shared with H04: add only the one field. `apply_rules_to_world`
is shared with H02/H04: add only the drain call and (if present) the `sync_build_rules` call.

**Commit.** `PAR-0104: setrule tracks changes, re-syncs world rules and runs World.checkMapArea`

---

### T6 — `setprop` / unit property fixes used by campaign maps (PAR-0218 slice)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED with one correction. Core with 50 copper, world processor `getblock building b 129 55` / `setprop @copper b 10` -> core `{"copper":60}` (Java 10). `setprop @flag u 5` then `sensor f u @flag` -> null (no flag). **Not reproduced:** `setprop @health u 0` on a crux dagger (via `fetch unit u @crux 0 @dagger`) removed the unit (the normal death path already kills at health 0) — keep the test, but the `Health <= 0` kill code may already be unnecessary for units; check before adding a second kill path.

**Symptom / acceptance.** extractionOutpost's `setprop @pyratite nucleus1 99999` must SET the core's
pyratite to `min(99999, accepted)`, not add 99999 every tick; `setprop @copper b 10` on a building holding
50 copper must leave 10; cruxscape's `setprop @speed z1 10` must set the status speed multiplier;
atolls' `setprop @health m1 0` must kill the unit; `setprop @flag u 5` sets the unit flag;
`setprop @copper u 20` sets the unit stack; `setprop @wet u 3` sets the status duration to 180 ticks.

**Java.** `BuildingComp.setProp` (`J/entities/comp/BuildingComp.java:2174-2227`) — numeric:
`health → clamp(0,maxHealth); <=0 && !dead → Call.buildDestroyed`; `team → changeTeam`; `totalPower → buffered ? status = clamp(v/capacity)`;
content:
```java
if(content instanceof Item item && items != null){
    int amount = (int)value;
    if(items.get(item) != amount){
        if(items.get(item) < amount) handleStack(item, acceptStack(item, amount - items.get(item), null), null);
        else if(amount >= 0) removeStack(item, items.get(item) - amount);
    }
}else if(content instanceof Liquid liquid && liquids != null){
    float amount = Mathf.clamp((float)value, 0f, block.liquidCapacity);
    if(amount < liquids.get(liquid) || (acceptLiquid(self(), liquid) && (liquids.current() == liquid || liquids.currentAmount() <= 0.1f || block.consumesLiquid(liquid)))) liquids.set(liquid, amount);
}
```
`UnitComp.setProp` (`J/entities/comp/UnitComp.java:355-427`): `health → clamp; <=0 && !dead → kill()`;
`shield → max(v,0)`; `x/y → unconv`; `velocityX/Y → v*8/60`; `rotation`; `team`; `flag → flag = v`;
`speed → statusSpeed(clamp(v,0,1000))`; `armor → statusArmor(max(v,0))`; content: `Item → stack.item = item; stack.amount = clamp((int)v, 0, type.itemCapacity)`;
`StatusEffect → setDuration(effect, v*60)`. (`payloadType` object setter: deferred, see T9 notes.)
`statusSpeed(v)` (`StatusComp`): sets the override multiplier applied on top of statuses — read
`J/entities/comp/StatusComp.java` (search `statusSpeed`) and port exactly.

**Port today.** `core/logic/access.rs`: `set_prop_content` (Building only; adds/removes the delta),
`set_prop_unit_num` (`Speed => true` no-op; no `Flag`), `set_prop_building_num` (`Health` clamps but never
destroys; `Team` writes `TeamComp` only). Building acceptance helpers:
`crate::world::transfer::dispatch_accept_item/handle_item` (used in `duo_ammo_sensed`); find the stack
versions: `grep -n "pub fn dispatch_accept_stack\|pub fn dispatch_handle_stack\|fn accept_stack" core/world/transfer*.rs core/world/**/*.rs`.
Unit stack: `ItemModule` on the unit (see `unit_content_count`). Status: `StatusComp { statuses, speed_multiplier, armor_override, .. }`.

**Changes.**
1. `set_prop_content(world, target, c, amount)`: Building → Java Item/Liquid branches above
   (use the stack accept/handle dispatch so cores/containers apply their own capacity — `CoreBuild.acceptStack`
   uses storage capacity); Unit → Item: set the unit `ItemModule` to exactly one stack (clear, then add
   `clamp(amount as i32, 0, type.item_capacity)`); StatusEffect: set/insert the status entry duration to
   `amount*60` (Java `setDuration`: if the unit has it, set its time; else `apply(effect, duration)`).
2. `set_prop_building_num`: `Health` → when the clamped value `<= 0` set `Health.health = 0` and mark it so
   `resolve_deaths` destroys it this tick (check how damage marks buildings dead: `grep -n "fn resolve_deaths" -A30 core/sim/*.rs`);
   `Team` → route through the port's change-team path if one exists (`grep -rn "fn change_team" core/`), else keep
   `TeamComp` and list "no changeTeam side effects" in the summary.
3. `set_prop_unit_num`: `Health <= 0` → `kill` the unit through the same path combat uses (find `fn kill_unit`/`dead = true`
   in `core/entities/comp/unit/lifecycle.rs`); `Flag` → the unit flag field (`grep -rn "pub flag" core/entities/comp/unit`).
   **`Speed` and `Armor` need Java's dynamic status** — today `Armor` writes `StatusComp.armor_override`,
   which `StatusComp::reset_modifiers` (`core/entities/comp/unit/comp.rs`) resets to -1 on the next status
   update, so `setprop @armor` lasts one tick (false "done" claim). Java (`J/entities/comp/StatusComp.java:136-187, 200-240`):
   `applyDynamicStatus()` adds (once) a `StatusEffects.dynamic` entry with infinite time whose own
   multipliers are folded into the per-tick products; `statusSpeed(v)` → `entry.speedMultiplier = v / (type.speed*60/8)`;
   `statusArmor(v)` → `entry.armorOverride = v`; update: `if(entry.effect.dynamic){ speed *= entry.speedMultiplier; ...; if(entry.armorOverride >= 0) armorOverride = entry.armorOverride; }`.
   Port: add component `DynamicStatus { speed_multiplier, health_multiplier, damage_multiplier, reload_multiplier, build_speed_multiplier, drag_multiplier: f32 (all 1.0), armor_override: f32 (-1.0) }`
   in `core/entities/comp/unit/comp.rs`; `apply_dynamic_status(world, e)` inserts it and applies the `dynamic`
   status (`content.status_id("dynamic")`, duration `f32::INFINITY` — check `apply_status` accepts it; else use
   `f32::MAX`); in `update_unit_status` (`core/combat/damage/status.rs`), when the entry is the dynamic effect,
   multiply by the component's values instead of the effect's (same place the per-effect products are taken).
   `Speed` → `clamp(v,0,1000)` then `speed_multiplier = v / (type.speed*60/8)`; `Armor` → `armor_override = max(v,0)`.
   Saving the dynamic multipliers is out of scope (note it). `combat/damage/status.rs` is shared with H01:
   touch only the dynamic branch.
4. `Instruction::SetProp` dispatch: make sure a `Content(StatusEffect)` type routes to `set_prop_content` for units.

**Tests** (`production_tests.rs`): `setprop_item_sets_count` (container with 50 copper → `setprop @copper b 10` → 10;
`setprop @copper b 99999` on a core → `min(99999, storage capacity)`; run twice → unchanged);
`setprop_unit_health_zero_kills` (dagger → dead after the tick); `setprop_unit_stack_and_status`;
`extraction_outpost_block2` (core of team 2 + world processor running the verbatim extractionOutpost block 2
with the core linked as `nucleus1` — links via `logic_block::compress(code, &[LogicLink{..}])`, see how
`integration_tests` in `core/logic/blocks/mod.rs` builds links) → pyratite count == capacity-limited 99999.

**Verify in-engine.** Headless only is acceptable (no campaign map with these programs is reachable in
the imported profile without long play); say so in the summary.

**Commit.** `PAR-0218: setprop content/unit properties match Java (set, not add; speed, flag, kill, status)`

---

### T7 — Early-sector world-processor conformance suite + Ground Zero end-to-end walk (PAR-0104 evidence)

> **In-engine verification (2026-10-14, session 3):** PARTLY (walk dry-run on main). Step 1 (`mineOver` -> area) and step 5 (`allowWaves` -> `waveTimer true`) already pass; step 2 (cutscene) and step 3 (wave) fail as T3/T2 predict. Step 4 (`duoAmmo`) fails for a reason no task covered: the map's own world processors are assembled before the logic globals exist, so `@sharded`/`@duo`/`@scatter` are plain null variables and GZ-3/GZ-5 loop forever (new **T20**). The drill (130,48) + duo (132,48) recipe gives the duo `@ammo 18` after 1500 ticks (works). **Fact fixed (§0):** the imported profile's GZ save (`saves/sector-serpulo-15.msav` from 2024, resumed as sector 170) runs an OLDER GZ-3/GZ-5: `fetch build d @sharded 0 @scatter|@duo` (first building only, no `buildCount`/round-robin); only `assets/maps/serpulo/groundZero.msav` has the `i` variant. The in-engine walk therefore exercises the old programs; the headless tests use §0. The map processors are at (94,42) GZ-1, (95,42) GZ-2, (96,42) GZ-3, (94,43) GZ-4, (96,43) GZ-5. Run the walk only after T20.

**Symptom / acceptance.** One test per shipped early-Serpulo program proves the
live sim produces the Java effect. This is the regression net for T2–T6. In-engine:
the GZ flag chain produces waves, the map-area expansion, the cutscene, and the
`duoAmmo` objective flag.

**Java.** Each program in §0; semantics per T2–T6 and the instruction table in §1.

**Port today.** Tests exist only for GZ-2 (`flags_roundtrip_live`) and a direct
`sense` call for the duo (`duo_ammo_sensed`).

**Changes.** New test module `core/sim/logic_campaign_tests.rs` (register it in
`core/sim/mod.rs` next to `production_tests` with `#[cfg(test)] mod logic_campaign_tests;`;
make the helpers in `production_tests.rs` it needs `pub(super)`). Store the
programs as `const` strings copied verbatim from §0 / the dump (frozenForest,
frontier, fungalPass, extractionOutpost, overgrowth block 1). Tests:

| Test | Setup | Expect |
|---|---|---|
| `gz1_spawn_wave` | flag `spawnWave`, one spawn group + spawn tile | T2 expectations |
| `gz2_allow_waves` | flag `allowWaves` | `wave_sending && wave_timer`, flag removed |
| `gz3_scatter_ammo` | scatter at (20,20) team 1 fed 5 `scrap` (check the scatter ammo item in content) | after 10 ticks `objective_flags` contains `scatterAmmo` |
| `gz3_no_scatter_waits` | no scatter | after 60 ticks no `scatterAmmo` (null `d` → `amount` null → `jump 0 equal null 0` true) |
| `gz4_map_area_and_cutscene` | flags `mineOver`, `spawnOver`; map ≥ 256 wide | T3/T5 expectations |
| `gz5_duo_ammo` | two duos, only the 2nd has copper | within 20 ticks `duoAmmo` set (round-robin `i`) |
| `ff1_power_flag` | combustion generator burning coal (feed coal; tick until `Building.efficiency > 0`) | `power` flag set; without coal: unset after 120 ticks |
| `ff2_wave_timer_follows_done` | no `done` flag | `wave_timer == false` after 2 ticks; insert `done` → `true` |
| `frontier_reveal` | flag `revealMap` | `waves == true`, `limit_map_area` per `checkMapArea(0,0,1000,1000)` (full → disabled) |
| `fungal_wave_timer` | flag `unitsBuilt` toggled | `wave_timer` follows it |
| `extraction_factory_toggle` | ground-factory linked as `factory1` | enabled=false for 720 ticks then true |
| `overgrowth_factory_gate` | crux air-factory, `@waveNumber` 1 vs 2 | disabled at wave 1, enabled at wave 2 |

Each test asserts `LogicWorldState.events` is empty after the tick (drained).

**Verify in-engine (GZ walk).** Clean re-import (env §4.3), enter GZ (env §4.1 Go path), then:
1. `logic_set_flag("mineOver", true)`; `step(30)`; `logic_rules_probe()` → limit changed (T5).
2. `logic_set_flag("spawnOver", true)`; real-time 2 s → `logic_cutscene_json().active == true`; screenshot; 12 s later `false`.
3. `logic_set_flag("spawnWave", true)`; `step(120)` → `wave` +1, `get_group_count("unit")` up.
4. place a duo next to the drill (env §4.1 facts: drill (130,48), duo (132,48)) with
   `debug_add_building_items` … turret ammo is not in the item module (env §4.2): feed it by
   `$H.place_block` a conveyor chain from the drill or use `$I.transfer_item(...)`; `step(300)` →
   `logic_rules_probe().flags` contains `duoAmmo` (consumed by the objective later).
5. `logic_set_flag("allowWaves", true)`; `step(60)` → `waveTimer: true`.
Save every probe JSON + screenshots under `runs/<date>-s<N>-H06-T7/`; panic count 0.

**Commit.** `PAR-0104: early-sector world-processor conformance suite (Ground Zero, Frozen Forest, Frontier, ...)`

---

### T8 — World side effects applied live: spawn, status, explosion, effect, sound/music, setblock, getblock, weather (Fixes PAR-0104)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. World processor: `spawn @flare 130 60 0 @sharded u` -> unit count unchanged, `u` null; `getblock floor fl 130 60` -> null; `setblock block @air 169 167 @derelict 0` -> the derelict `scrap-wall` at (169,167) stays (derelict walls confirmed at x 169/171/173, y 167). The verify step needs T1's direct-write seam (world processor).

**Symptom / acceptance.** atolls: `spawn @mega 110 400 270 @sharded m1` puts a mega on
the map AND sets `m1` (next line is `ubind m1`); `setblock block @air 210 306 @derelict 0`
clears that tile; cruxscape `status false fast b1 99999` applies `fast`; Erekir basin
`explosion`/`effect`; `playsound`/`playmusic` reach the audio sink; `getblock floor` works
in the live sim; `weathersense/weatherset` read/write the real weather entities.

**Java** (`J/logic/LExecutor.java`):
* `SpawnUnitI` (1615-1650): `if(net.client()) return; t = team.team();`
  `if(t != null && type.obj() instanceof UnitType type && !type.internal && Units.canCreate(t, type))`:
  `unit = type.spawn(t, unconv(x) + Mathf.range(0.01f), unconv(y) + Mathf.range(0.01f), rotation)`;
  `effect ? spawner.spawnEffect(unit) : { unit.unloaded(); UnitSpawnEvent }`; `result.setobj(unit)`.
  `spawnEffect` (`J/ai/WaveSpawner.java:229-247`): apply `unmoving` 30 + `invincible` 60, `unloaded()`,
  `Fx.unitSpawn.at(x, y, rotation, type)` and after 30 ticks `Fx.spawn.at(x, y)`.
* `ApplyEffectI` (1765-1776): `unit.obj() instanceof Unit && effect.obj() instanceof StatusEffect` →
  `clear ? unit.unapply(effect) : unit.apply(effect, duration * 60)`. Not host-event-gated beyond `net.client()`.
* `ExplosionI` (~2004-2040): `if(damage < 0) return;` `Damage.damage(team, unconv(x), unconv(y), unconv(min(radius,100)), damage, pierce, air, ground, true, null)`;
  `effect` → `pierce ? Fx.spawnShockwave.at(x,y, conv(radiusPx)) : Fx.dynamicExplosion.at(x,y, conv(radiusPx)/8)`. Transcribe
  the exact lines before coding (`grep -n "class ExplosionI" -A40 LExecutor.java`).
* `EffectI` (~1975-2002): `rot = type.rotate ? rotation : min(rotation, 1000)`;
  `type.effect.at(unconv(x), unconv(y), rot, Color.fromDouble(color), data.obj())`. Table
  `J/logic/LogicFx.java:14-46` (name → Fx field): warn→unitCapKill, cross→unitEnvKill, blockFall→blockCrash,
  placeBlock→placeBlock, placeBlockSpark→coreLaunchConstruct, breakBlock→breakBlock, spawn→spawn,
  trail→colorTrail, breakProp→breakProp, smokeCloud→missileTrailSmoke, vapor→vapor, hit→hitBulletColor,
  hitSquare→hitSquaresColor, shootSmall→shootSmall, shootBig→shootTitan, smokeSmall→shootSmallSmoke,
  smokeBig→shootBigSmoke, smokeColor→shootSmokeTitan, smokeSquare→shootSmokeSquare,
  smokeSquareBig→shootSmokeSquareBig, spark→hitLaserBlast, sparkBig→circleColorSpark, sparkShoot→colorSpark,
  sparkShootBig→randLifeSpark, drill→mine, drillBig→mineHuge, lightBlock→lightBlock,
  explosion→dynamicExplosion, smokePuff→smokePuff, sparkExplosion→titanExplosion,
  crossExplosion→dynamicSpikes, wave→dynamicWave, bubble→airBubble (verify against the file).
* `PlaySoundI` (2230-2259): invalid id → nothing; positional → `sound.at(unconv(x), unconv(y), pitch, min(volume,2), limit)`;
  else `sound.play(min(volume * sfxVolume, 2), pitch, pan, false, limit)`. `PlayMusicI` (2261-2280).
* `SetBlockI` (1566-1612): `ore`: `(b is OverlayFloor || b == air) && overlay != b → setOverlayNet(b)`;
  `floor`: `b is Floor && floor != b && !isOverlay && !isAir → setFloorNet`; `block`: `(!b.isFloor() || b == air)`,
  `t = team.team() ?: derelict`, `if(block != b || tile.team != t) tile.setNet(b, t, clamp(rot,0,3))`.
* `GetBlockI` (1535-1563): tile at `(round(x), round(y))` (Java `Mathf.round(float)` = `floor(x+0.5)`); null outside.
* `SenseWeatherI`/`SetWeatherI` (1710-1749): sense = an active `WeatherState` of that type; set on →
  create (intensity 1, life `fadeTime`) if inactive else refresh life; off → `life = min(life, fadeTime)` —
  transcribe the exact lines first.

**Port today.** `core/logic/world.rs` `run_spawn_unit`, `run_apply_status`, `run_explosion`,
`run_effect`, `run_play_sound`, `run_play_music`, `run_set_block`, `run_get_block`,
`run_weather_sense/set` — all event-only (weather uses `LogicWorldState.weather_active`).
Helpers: `UnitRuntime::spawn_by_id(world, UnitTypeId, team, x, y, rot) -> Option<Entity>`
(`core/sim/unit_runtime.rs`; pushes to `units`), `entities::comp::unit::queries::can_create_unit(world, team, def)`,
`combat::damage::status::apply_status(world, e, StatusId, duration_ticks)`,
`combat::damage::area::damage_area(world, content, Some(team), x, y, radius, damage, DamageOptions)`,
`content::registries::fx_meta::effect_by_name(name) -> Option<EffectId>`, `UnitRuntime.fx` (`FxSink::effect(&EffectRef, x, y, rot, Rgba)`),
`audio::AudioEvent::{at, play, music_play}` + the `AudioSinkRes` resource (`core/audio/`),
`SimRuntime::set_block_raw(world, grid, x, y, block, rot, team)` / `remove_block_raw(world, grid, x, y)`
(`pub(crate)`, `core/sim/runtime.rs`, callable from `Sim`), weather spawn shape:
`gdext/sim_host.rs` `debug_create_weather` (spawns `WeatherRuntime::new(weather, intensity, duration)` + `WeatherState` + `EntitySeq`).

**Changes.**
1. `run_spawn_unit` (runs inside `update_buildings`, the unit runtime is in the world):
   resolve `UnitTypeId` from `LogicObject::Content(c)` with `c.type_ == ContentType::Unit`; take
   `world.remove_non_send::<UnitRuntime>()` (if absent: keep today's event path and return);
   skip `def.internal`; `can_create_unit(world, team, def)` must be true; offsets
   `rng.range(RngStream::Sim, -0.01, 0.01)` twice (Java `Mathf.range(0.01f)`; deterministic deviation);
   `spawn_by_id(world, id, team, x*8+dx, y*8+dy, rotation)`; `effect` true → `apply_status` unmoving 30 /
   invincible 60, `runtime.fx.effect(&EffectRef::Named(unitSpawn), x, y, rot, white)`; the delayed `Fx.spawn`
   is skipped (note); `exec.set_obj(result.id(), Some(LogicObject::Unit(e)))`; always re-insert the runtime.
   Keep the `SpawnUnit` event push (tests count it) only when `LogicWorldState` exists.
2. `run_apply_status`: apply/clear directly (`StatusId` from `c.id`; clear = remove the entry from
   `StatusComp.statuses` and its `damage_times` slot; if H01 added an `unapply` helper use it).
3. `run_explosion`: Java formula with `damage_area` (content from `UnitRuntime.content`;
   `DamageOptions { air, ground, pierce_armor: pierce, ..Default::default() }` — read the struct fields);
   effects via `effect_by_name("spawnShockwave"/"dynamicExplosion")`.
4. `core/logic/fx.rs`: add `pub fx: &'static str` to each `EffectEntry` from the table; test that every
   `fx` resolves with `effect_by_name`. `run_effect` plays it through `UnitRuntime.fx` (convert the
   `color_from_double` value to `Rgba`).
5. `run_play_sound`/`run_play_music`: emit `AudioEvent::at(SoundId(id as u16), x*8, y*8, pitch, vol.min(2), limit)` /
   `AudioEvent::play(..)` (raw volume — H07's sink applies `sfxvol`) / `AudioEvent::music_play(MusicRef::new(name), interrupt)`
   into the audio sink resource (find its name and `emit` method in `core/audio/mod.rs`). Ignore ids outside the sound table.
6. `run_set_block`: keep the event; in `Sim::drain_logic_events` add the `SetBlock` arm → new
   `Sim::logic_set_block(layer, block, x, y, team, rot)` implementing the Java branches against
   `self.grid` and `self.block_runtime` (`remove_block_raw` then `set_block_raw` for non-air blocks;
   static walls without a building: set `grid` tile block directly via the grid API used by
   `set_block_raw`; floor/overlay: write the tile fields). After any change: `self.refresh_unit_runtime()`
   and set a new `pub logic_world_changed: bool` on `Sim`; `gdext/sim_host.rs` frame pump sets
   `self.world_dirty = true` when `self.sim.logic_world_changed` (then clears it) so the renderer recaches.
   Block kinds: use the block def's floor/overlay classification (`grep -n "fn is_floor\|fn is_overlay\|BlockKind::Floor\|OverlayFloor" core/content/registries/blocks/*.rs | head`).
7. `run_get_block`: when no `WorldGrid` resource, read floor/overlay from
   `world.get_non_send::<UnitRuntime>().grid` (snapshot; refreshed on tile change), rounding with
   `(v + 0.5).floor() as i32`.
8. Weather: move the body of `debug_create_weather` into core
   `pub fn create_weather(world: &mut World, runtime: &mut UnitRuntime, weather: WeatherId, intensity: f32, life: f32) -> Option<Entity>`
   (in the module that owns `WeatherRuntime`; if H07 already added an equivalent helper, use it — grep
   `pub fn create_weather` first); `debug_create_weather` calls it. `run_weather_sense` = any
   `WeatherRuntime` entity with that id; `run_weather_set` per Java with `fadeTime` from
   `J/entities/comp/WeatherStateComp.java` (verify the constant). Drop `LogicWorldState.weather_active`
   uses (keep the field for tests or remove it with its tests).

**Tests** (`logic_campaign_tests.rs`): `spawn_sets_result` (`spawn @dagger 20 20 0 @sharded u\nsensor x u @x\nstop` → +1 dagger,
`x` within 0.01 of 20); `spawn_respects_unit_cap`; `setblock_clears_tile`; `status_applies_fast`;
`explosion_damages` (crux dagger at tile (40,40), `explosion @sharded 40 40 3 50 1 1 0 0` → health dropped by
`50 * falloff(0)` minus armor); `effect_emits_named_fx` (recording `FxSink`: `grep -rn "impl FxSink for" core/ | head`);
`getblock_floor_live`; `weatherset_creates_rain`; `atolls_block1_first_steps` (verbatim atolls block 1 up to the
first `ubind m1` with flag `attack`: 4 setblocks clear tiles, 8 units spawned, `m1` bound after T9 — before T9
assert only the spawn/setblock half).

**Verify in-engine.** GZ: world processor at a free tile (API placement) with `logic_set_code`:
`spawn @flare 130 60 0 @sharded u\neffect warn 130 60 2 %ffaaff\nstop` → `get_group_count("unit")` +1, screenshot
shows the flare + warn effect. `setblock block @air <x> <y> @derelict 0` on a derelict scrap wall near (169,167)
→ `tile_debug` shows air and the render updates. Evidence `runs/<date>-s<N>-H06-T8/`.

**Pitfalls.** Every `remove_non_send` paired with `insert_non_send` on all paths. `update_buildings`
may run with the unit runtime taken out by another system — handle `None`. `combat/` and `audio/` are
H01/H07 files: call their helpers, do not change them.

**Commit.** `Fixes PAR-0104: world-processor spawn/status/explosion/effect/sound/setblock/getblock/weather apply live`

---

### T9 — `ubind` / `ucontrol` / `ulocate` and LogicAI in the live unit update (PAR-0221)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Poly (`content_list("unit").find("poly")` = 21) spawned team 1 at (1080,480); world processor `ubind @poly` / `set u @unit` / `ucontrol move 100 60 0 0 0` -> after 300 ticks `u` null, poly still at (1080,480) with `kind CommandAI`. **Paths fixed:** `unit_control.rs` is `core/logic/executor/unit_control.rs` (`bind_next` :106); `core/ai/controller_registry.rs` has no `default_kind`/`default_ai` — the selector is `select_controller` (:26); `@unit` const at `assembler.rs:46`; `LVar::set_const` `value.rs:257`; `UnitDef.logic_controllable: Option<bool>` (`units/mod.rs:613`).

**Symptom / acceptance.** `ubind` never binds today (constant `@unit`, see §1). After:
`ubind @poly` cycles the team's polys; `ubind u` binds a unit object (atolls `ubind m1`);
`ucontrol move 10 10 0 0 0` drives the unit to world (80,80) through LogicAI; LogicAI
times out to the type's default controller 10 s after the processor stops controlling.

**Java.** `UnitBindI.run` (`J/logic/LExecutor.java:180-209`):
```java
if(!exec.privileged && !state.rules.logicUnitControl) return;
if(exec.binds == null || exec.binds.length != content.units().size) exec.binds = new int[content.units().size];
if(type.obj() instanceof UnitType type && type.logicControllable){
    Seq<Unit> seq = exec.team.data().unitCache(type);
    if(seq != null && seq.any()){ exec.binds[type.id] %= seq.size; if(exec.binds[type.id] < seq.size) exec.unit.setconst(seq.get(exec.binds[type.id])); exec.binds[type.id]++; }
    else exec.unit.setconst(null);
}else if(type.obj() instanceof Unit u && (u.team == exec.team || exec.privileged) && u.type.logicControllable) exec.unit.setconst(u);
else exec.unit.setconst(null);
```
`UnitControlI.checkLogicAI` (`:320-338`) and `run` (`:341-556`) — **read the whole method before
coding**; G06's transcription (`docs/parity-plans/G06-logic.md` T9 "Java" paragraph — you may open
that one section) is accurate for the per-command rules (`move/approach/pathfind/autoPathfind/stop/unbind/within/target/targetp/boost/flag/mine/payDrop/payTake/payEnter/build/deconstruct/getBlock/itemDrop/itemTake`, `logicItemTransferRange`, timeouts 90). `run` starts with
`if(!exec.privileged && !state.rules.logicUnitControl) return;`. `UnitLocateI` (`:212-303`).
`J/ai/types/LogicAI.java:50-123` `updateMovement` + targeting.

**Port today.** `executor/mod.rs` `Instruction::UnitBind` (search `Instruction::UnitBind { type_ }`):
content only, writes `@unit` only `if !exec.is_constant(unit_var)` — `@unit` is created with
`put_obj_const` (`core/logic/assembler.rs:46`) so it never writes. `LVar::set_const` exists
(`core/logic/value.rs`). `Instruction::UnitControl` gates on `logic_unit_control_enabled(rules)` without
`exec.privileged`. `unit_control.rs` `bind_next` sorts by `Entity::index` (Java: `unitCache` insertion
order = spawn order → `EntitySeq`). `core/ai/types/logic.rs` `LogicAi` + `update_logic(ctx, unit, &mut state) -> bool`
exist; `core/sim/unit_runtime.rs` `unit_update_system` dispatch (`slot.kind == AiKind::Player / Command / Missile / Suicide / Builder / Boost / else`)
has no `AiKind::Logic` branch, so logic units run the generic AI.

**Changes.**
1. `Instruction::UnitBind`: Java gate first (`!exec.privileged && !rules_ref(world).logic_unit_control()` → return);
   `Content(Unit)` with `logic_controllable` (`grep -n "logic_controllable" core/content/registries/units/mod.rs`) →
   `bind_next` (sorted by `(EntitySeq, index)`); `LogicObject::Unit(u)` with team match or privileged and
   controllable → `u`; else `None`. Write with `exec.arena.get_mut(unit_var).set_const(value)` (Java `setconst`).
   Resize `exec.binds` to the unit-type count each run like Java.
2. `Instruction::UnitControl`: gate `!exec.privileged && !logic_unit_control` → return. Then the per-command
   rules from Java (`x1 = p1*8`, `y1 = p2*8`, `d1 = p3*8`), writing outputs (`within` → p4; `getBlock` → p3/p4/p5)
   via `exec.set_*`. Keep logic in `unit_control::apply_control` / `LogicAi::apply_control`; extend `LogicAi`
   fields only as needed (`move_x, move_y, move_rad, controller: Option<Entity>, pos_target, main_target, shoot, aim_control, boost`; keep existing names).
3. LogicAI wiring (H06 owns this branch; H01 owns the rest of the dispatch): in `unit_update_system` add
   `else if slot.kind == AiKind::Logic { ... }` right before the generic else, built like the `Boost` branch
   (same `AiCtx` construction), calling `crate::ai::types::logic::update_logic(&mut ctx, entity, &mut state)`
   with the `LogicAi` component taken out and re-inserted; on `false` → reset `slot.kind` to the type's default
   (`grep -n "fn default_kind\|fn select\|fn default_ai" core/ai/controller_registry.rs`) and remove `LogicAi`.
   Timeout: Java `controlTimer` 600 ticks (`LogicAI.logicControlTimeout = 10*60`) decremented while the
   `controller` building is valid; reset otherwise.
4. Payload commands (`payTake/payDrop/payEnter`) for atolls: call the payload helpers the player input uses
   (`grep -rn "fn pickup_payload\|fn drop_payload" core/`); if none take a unit entity, implement only
   `payDrop`/`payTake` of buildings+units through the same core functions `MindInput.pickup_payload` calls, and list the rest.
5. `ulocate`: outputs in tiles (`px/8`), per-instruction cache keyed by instruction index, range check per Java.

**Tests** (`logic_campaign_tests.rs`): `ubind_type_cycles_in_spawn_order`; `ubind_unit_object` (spawn via
world processor, `ubind u` → `@unit == u`); `ubind_gate` (non-privileged micro + `logicUnitControl=false` → null);
`ucontrol_move_reaches_target` (dagger, `ucontrol move 10 10 0 0 0` loop → within 2 tiles of (10,10) after 300 ticks);
`ucontrol_within_and_getblock`; `logic_ai_times_out` (stop the processor → after 601 ticks default AI);
`atolls_mega_bind_and_stop` (spawn + `ubind m1` + `ucontrol stop` → controller is LogicAI, unit not moving).

**Verify in-engine.** GZ: `apply_sim_command_json` spawn op for a `poly` (env §4.2), world processor with
`ubind @poly\nucontrol move 100 60 0 0 0` → `unit_debug_json()` shows the poly `LogicAI` and its position
approaching (800,480) over `step(300)`. Evidence `runs/<date>-s<N>-H06-T9/`.

**Pitfalls.** `unit_update_system` is H01's hot file: add exactly one `else if` branch. `@unit` must stay a
constant for `set`/`op` (only `ubind` writes it with `set_const`).

**Commit.** `Fixes PAR-0221: ubind/ucontrol/ulocate bind and drive live units through LogicAI`

---

### T10 — Objective `completionLogicCode` runs in the live world (PAR-0224)

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in engine (no shipped objective uses it). Code confirmed: `map_objectives.rs:357` calls `run_logic_script(code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false)` (throwaway world); `run_logic_script_in` exists at `logic/script.rs:50`.

**Symptom / acceptance.** An objective with `completionLogicCode = "setflag \"x\" true"` sets flag `x`
on completion. Today `MapObjectivesRuntime` runs it on a throwaway `World`
(`core/game/map_objectives.rs`, search `run_logic_script(code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false)`).
No shipped map uses it (§0), so this is correctness for custom maps.

**Java.** `J/game/MapObjectives.java:213` → `LExecutor.runLogicScript(code)` (`J/logic/LExecutor.java:73-94`):
privileged executor on the live world, ≤ 100000 instructions, no loop.

**Changes.** In `complete`, push the code into a new `pub pending_logic_scripts: Vec<String>` on the objectives
runtime instead of running it. In `Sim::drain_logic_events` (after the event loop) take the pending scripts from
`CampaignRuntime.session.objectives` (drop the resource borrow) and run each with
`crate::logic::script::run_logic_script_in(&code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false, &mut self.ecs.0)`
(check its exact signature in `core/logic/script.rs:50`), then drain events again once (a script may emit).
Update the map_objectives test that asserted the old behavior (`grep -n "completion_logic_code" core/game/map_objectives.rs`).

**Tests.** `objective_completion_script_runs_live` (live sim + campaign session + objective with the code → flag set after one tick).

**Verify in-engine.** Headless only (no shipped objective uses it).

**Commit.** `Fixes PAR-0224: objective completionLogicCode runs against the live world`

---

### T11 — Sensor/fetch/print remainder (Fixes PAR-0218)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. World processor: `fetch core c @sharded 0` -> null; `fetch coreCount` -> 0; `fetch player` -> null, `playerCount` 0; `sensor @dagger @health` -> 1; `sensor @copper @name` -> 1; core building `@name` -> "core-shard" and `@id` -> 19. Java check for row 3: `BuildingComp.sense` (`:2099-2135`) and `senseObject` (`:2136-2144`) have no `name`/`id` case -> null (only `Block.sense` `:1684` and `UnitType.sense` `:1440` have `@id`; `UnitComp` `:320` has `@name`). The verify step needs a world processor (`fetch` is privileged) and T1's direct-write seam.

**Symptom / acceptance.** The `sensor` tables, `fetch core/player`, `printchar` of content and
`@clientLocale/@clientName` match Java.

**Java.** `BuildingComp.sense/senseObject` (`J/entities/comp/BuildingComp.java:2099-2170`), `UnitComp.sense/senseObject`
(`J/entities/comp/UnitComp.java:266-353`), `Block.sense` (`J/world/Block.java:1674-1708`), `UnitType.sense`
(`J/type/UnitType.java:1430-1449`), `Item/Liquid.sense`, family overrides (list in `docs/parity-plans/G06-logic.md`
T7 "Java" paragraph — you may open that section), `FetchI` core/player (`J/logic/LExecutor.java:1404-1406`:
`player → data.players.get(i).unit()` (or its block tile when the unit is a `BlockUnitc`); `core → data.cores.get(i)`;
counts = sizes), `PrintCharI` content (`J/logic/LExecutor.java:1104-1114`): `if(textBuffer.length() >= maxTextBuffer) return;` non-content objects append nothing;
content appends `(char)cont.emojiChar()` = `(char)Fonts.getUnicode(name)` (the code listed for that content name in
`assets/icons/icons.properties`; **0 when absent → Java appends the char U+0000** — port exactly),
`GlobalVars.update` `@clientLocale = player.locale()`, `@clientName = player.name()`.

**Port today / gaps (each is a row to fix):**
1. Content sense (`access.rs` `sense`, `LogicObject::Content` arm) returns `Sensed::Num(1.0)` for everything but `@id`.
   Implement Block/UnitType/Item/Liquid tables (`@name` → `Str(name)`, `@color`, `@health/@maxHealth`, `@armor`, `@size`,
   `@itemCapacity`, `@liquidCapacity`, `@powerCapacity`, `@solid`, `@flying`, `@speed`, `@range`, `@payloadCapacity`);
   values from `BlockTable` / the `LogicContentInfo` resource (extend it with the unit/item/liquid scalars at install).
   `sense_content` of a Block with an Item → `requirement * rules.buildCostMultiplier` (0 with infinite resources).
2. Unit rows: `@range` (`range()/8`), `@shooting`, `@shootX/@shootY` (`aim/8`), `@armor` base type armor when no
   override, `@buildX/@buildY` (`building ? plan.x/y : -1`), `@payloadCount/@totalPayload/@payloadCapacity`,
   `@speed` (`type.speed*60/8*speedMultiplier`), `@flag`; object rows `@name` (player name for the player unit,
   else null), `@controller` (LogicAI → its processor building, else the unit), `@payloadType`, `@building`.
3. Building rows: remove `@name` (Java: not senseable → null) and `@id` (null) unless Java has them (check);
   `@config` (`configSenseable`), `@payloadCount/@payloadType`; family overrides via the existing
   `BuildingBehavior::logic_sense` hook for: GenericCrafter/Drill/Separator/UnitFactory/Reconstructor/ConstructBlock/
   MassDriver/LaunchPad/MendProjector progress, heat families, ForceProjector `@shield/@heat`, Door `@enabled`,
   Conveyor `@progress/@firstItem`, Core `@itemCapacity/@maxUnits` + `sense(UnitType)`, LightBlock `@color`.
   Implement each in its own behavior file; skip (and list) any family whose port state lacks the field.
4. Liquid turrets: `LAccess::Ammo` for `TurretAmmo::Liquid` must return `state.total_ammo` (Java `Turret.sense` → `totalAmmo`,
   never incremented for liquid turrets → 0), not the liquid amount (`world/blocks/defense/turrets/behavior.rs`).
   `LAccess::Heat` only for power turrets (Java `PowerTurret.sense`); item/liquid turrets fall through to the base table (null).
5. `run_fetch` `Core/CoreCount`: the team's cores from `CampaignRuntime.session.teams.get_or_null(team).cores` (registration order),
   fallback to buildings with a core block kind sorted by `EntitySeq` when no campaign; `Player/PlayerCount`: the player unit
   (`Sim::player_unit` equivalent in core: find how `update_logic_globals` gets `self.player_unit()`; store the player entity
   in `LogicWorldState.player_unit` from `update_logic_globals`) for its team, index 0 only.
6. `printchar` content: `LogicText.iconc` (T4) → `iconc.code(name).unwrap_or(0)`; append `char::from_u32(code)` (U+0000 when 0, like Java).
7. `@clientLocale/@clientName`: read settings `locale`/`name` (keys in `core/io/settings.rs:61,63`) — pass them from gdext
   into a `LogicClientInfo { locale, name }` resource at `install_sim_seams` (and on settings change if a signal exists);
   `update_logic_globals` reads it, falling back to the current literals.

**Tests.** One test per row group in `core/logic/access.rs` tests / `logic_campaign_tests.rs`:
`content_sense_table` (`sensor h @dagger @health` == dagger def health; `sensor n @copper @name` == "copper"; `sensor c @copper @color` == Java packed bits);
`unit_range_and_armor`; `liquid_turret_ammo_is_total_ammo`; `fetch_core_and_count`; `printchar_content_emoji`
(`printchar @copper` → the char whose code `icons.properties` lists for `copper`).

**Verify in-engine.** GZ: micro processor (API placement) `fetch core c @sharded 0\nsensor n c @type\nfetch coreCount k @sharded 0 @air\nstop`
→ `logic_get_state` shows `n` = `core-shard`, `k` = 1.

**Commit.** `Fixes PAR-0218: sensor/fetch/printchar tables match Java (content, unit, family overrides, cores)`

---

### T12 — Logic-controlled turrets (`control shoot` / `shootp`) (Fixes PAR-0223)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Duo (132,48) fed by the drill (130,48) (`@ammo 18`); world processor `getblock building d 132 48` / `control shoot d 137 48 1` -> after 65 ticks `@rotation 90`, `@shooting 0`, 0 bullets (Java: turn to 0 deg and fire). **Verify-step simplification:** a privileged world processor needs no link (`control` gate = `exec_privileged || valid_link`), so the in-engine check does not wait for T15: use `getblock building d 132 48` in a world processor (T1 direct-write seam).

Raw material: `docs/parity-plans/G06-logic.md` T8 (you may open that section; Java lines there are
correct). Re-verified 2026-10-14: `TurretState.logic_control_time` (init -1) / `logic_shooting` exist;
`update_turret` sets `state.is_shooting = config.always_shooting || state.target.is_some() || state.logic_shooting`
(wrong per Java), nothing writes `logic_control_time`, `access::control` ignores `Shoot/Shootp`.

**Java.** `J/world/blocks/defense/turrets/Turret.java`: `logicControlCooldown = 120`;
`control(shoot,..)`/`control(shootp,..)` (search `case shoot ->`); `updateTile`: `isShooting = alwaysShooting || (isControlled() ? unit.isShooting() : logicControlled() ? logicShooting : target != null)`;
`if(logicControlTime > 0) logicControlTime -= Time.delta;`; `validateTarget()` true when `logicControlled()`;
in the ammo block `logicControlled() → canShoot = logicShooting` and `targetPos` kept.

**Changes.** Implement the turret `BuildingBehavior::logic_control` hook (`world/behavior/mod.rs` signature
`logic_control(&self, world, e, a, params) -> bool`; `access::control` must route `Shoot/Shootp` to it after the
link gate and pass object params) and the four `update_turret` edits listed in G06 T8 "Changes 1" (touch only
those lines; the turret file is shared with H01/H02). Predict: `grep -rn "fn intercept" client/rust/mind-core/src`.

**Tests.** `control_shoot_aims_duo` (duo with copper, no enemies, linked processor `control shoot duo1 <x+5> <y> 1`
→ after 30 ticks rotation within shootCone of 0° and bullets > 0; 200 ticks after the last command, shots stop).

**Verify in-engine.** (Simplified 2026-10-14: privileged executors need no link.) GZ: drill (130,48) + duo
(132,48) (`step(1500)` -> `@ammo` > 0), world processor at (118,50) via T1's `logic_set_code`:
`getblock building d 132 48\ncontrol shoot d 137 48 1\nsensor r d @rotation\nsensor s d @shooting` ->
after 60 ticks `r` near 0, `s` 1, `get_group_count("bullet") > 0`; replace the code with `stop`, 200 ticks
later no new bullets. (Before the fix: `r` 90, `s` 0, 0 bullets.) The linked micro-processor variant
waits for T15.

**Commit.** `Fixes PAR-0223: logic-controlled turrets (control shoot/shootp)`

---

### T13 — `radar` / `uradar` correctness (Fixes PAR-0222)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Duo disabled (`control enabled d 0`), two crux daggers in range with health 50/150: `radar enemy any any health d 1 hi` -> the 50-health one (Java maximizes -> 150); order 0 -> 150. `distance` with order 1 returned the closest dagger (Java's `-dst2` maximization gives the same), so only the non-distance sorts are inverted in practice.

Raw material: `docs/parity-plans/G06-logic.md` T10 (open that section). Re-verified 2026-10-14:
`core/logic/executor/radar.rs` `find` still uses `key < current` for `sort_order == true` (picks min
health/shield/armor; Java maximizes `sort.func * sortDir` with `distance = -dst2`), no range filter,
`Attacker/Boss → false`, no 30-tick cache, no permission check in `Instruction::Radar`.

**Changes.** As G06 T10 "Changes": `best = argmax(value * dir)`, `distance` value `-(dx²+dy²)`,
range = source range (`Ranged.range()`), team scan per Java, `attacker` = has weapons && `!disarmed`, `boss` =
has the `boss` status, `shield` from `ShieldComp`, `armor` = type armor (or override), 30-tick per-instruction
cache keyed by sim tick, permission gate (`privileged || source.team == exec.team`; building source not
privileged unless exec privileged).

**Tests.** `radar_health_order` (health 50/100, `radar enemy any any health @this 1 r` → the 100 one; `0` → the 50 one);
`radar_range` (unit beyond a micro processor's 10-tile range → null); `radar_cache` (result held for 29 ticks).

**Commit.** `Fixes PAR-0222: radar sort direction, range, filters, permissions and 30-tick cache`

---

### T14 — Logic editor, text mode (PAR-0103 slice; G06 T13a)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. `dialogs_manifest.json` has no `logic` entry; `UiRoot.dialog("logic") == null`. Tapping a placed micro processor opens only the generic config panel (`runs/20261014-s3-verify2/H06-T14-processor-tap.png`).

Raw material: `docs/parity-plans/G06-logic.md` T13 intro + T13a (open those sections; the Java
description and bundle keys are accurate). Re-verified 2026-10-14: no `logic` dialog in `ui/dialogs/`,
`ui/dialogs_manifest.json` has no `logic` entry, `core/ui/manifest.rs:405` already expects `"logic"` with pause = true,
`gdext/logic.rs` `MindLogic` funcs: `statement_count, statement_names, statement_fields, statement_meta,
metadata_handoff, parse, compile, run, default_program`.

**Deltas vs G06 T13a.** The Rust seams (`logic_get_code/logic_set_code/logic_get_state`) are done in T1 —
do not redo them; `logic_set_code` returns `bool` and the too-long error comes from `logic_last_error()`
(`"toolong:<len>"`). Add `#[func] MindLogic.default_statement_text(name) -> GString` (writes
`default_statement(name)` with `Statement::write`, `core/logic/statement.rs:125,620`). The dialog opens with
`MindUi.open_dialog("logic", <context>)`; context carries `x`,`y` (check the manifest arg syntax other dialogs use).
The dialog is reachable for verification before T15 (no pencil yet).

**Verify in-engine.** `MindUi.open_dialog("logic", ...)` for a micro processor placed at (120,50) →
`dialog("logic").visible == true`; set the edit text to `set a 1\nop add b a 2`, `hide_dialog()`, `step(5)` →
`logic_get_state(120,50).vars` has `a=1`, `b=3`. Screenshot `runs/<date>-s<N>-H06-T14/`.

**Commit.** `PAR-0103: text-mode logic editor dialog (load/save, edit menu, add, variables)`

---

### T15 — Processor config panel: pencil button, tap-to-link, link lifecycle, privileged guards (Fixes PAR-0220)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. A real click on the micro processor (120,50) opens `block_config` with `_config = {"text": true, "multiline": false}`: a one-line text field + OK/X, no pencil (`runs/20261014-s3-verify2/H06-T14-processor-tap.png`). Code: `configured` `Number` = icon tag and returns early for non-privileged processors (`logic_block.rs:565-571`). Hiccup: pressing Escape with the panel open opened the pause menu and left the panel up (H08 territory).

**Depends on H02** (generic configure-tap flow). Check:
`grep -rn "fn on_configure_build_tapped\|pub fn config_tap" client/rust/mind-core/src/world`. H02 provides
`world::config_tap::config_tap` and the `BuildingBehavior` method
`on_configure_build_tapped(&self, world, grid, e, other) -> ConfigTapResult` (exact types per `docs/H-INDEX.md` §3).
If absent on `main`: do the **sim half only** (items 1–3 below + tests) and skip item 4; list
"T15 UI half waiting for H02 config_tap" in the summary. Do not invent a second selected-state.

**Java.** `J/world/blocks/logic/LogicBlock.java`: `config(Integer.class, ...)` (line 85: link toggle —
`if(!validLink(build(pos))) return;` remove every link pointing at that building, re-enable it if
`lbuild.block.autoResetEnabled && lbuild.lastDisabler == this`, else add `LogicLink(findLinkName)`),
`config(Character.class, ...)` (line 79: icon tag), `accessible()`, `onConfigureBuildTapped` (`this == other || !accessible()` →
deselect; `validLink(other)` → `configure(other.pos())`, stay open; else super), `removeFromProximity` (re-enable linked
buildings this processor disabled), `damage/collide` immunity for privileged non-destructible, `canPickup() = false`.
`buildConfiguration` → one pencil button (`Icon.pencil`, `Styles.cleari`, size 40) → `showEditDialog`.

**Port today.** `core/logic/blocks/logic_block.rs` `configured`: `ConfigValue::Number(n)` = **icon tag**
(Java: `Integer` = link; `Character` = icon tag). Wire `Int` → `WorldConfigValue::Number` (`core/sim/runtime.rs:220`),
so no link can be toggled through input. `gdext/input/desktop.rs` (search `matches!(def.kind, BlockKind::LogicBlock | BlockKind::MessageBlock)`)
gives processors a one-line text field.

**Changes.**
1. `configured`: `Number(n)` with integral `n` → link toggle at `TilePos::from_pack(n as i32)` (accessible only);
   icon tags become editor-only (no input path; list as deferred).
2. `toggle_link`: Java re-enable on unlink (`auto_reset_enabled` = block kind != `SwitchBlock`, and
   `Building.last_disabler == Some(e)`); `on_removed` of the processor re-enables every linked building it disabled.
3. Guards: privileged + non-destructible processors take no damage (one check in the building damage helper,
   cite `LogicBlock.java` `damage`/`collide`); `can_pickup` false for logic processors; break blocked when `!accessible`.
4. UI (only with H02's seam): implement the `LogicBlock` arm of `on_configure_build_tapped` exactly as Java;
   `config_spec` for `BlockKind::LogicBlock` → `{"logic": true}` (no text field; inaccessible → no panel);
   `ui/fragments/block_config_fragment.gd`: `logic` → one 40x40 pencil button that opens the T14 dialog and hides the panel.
   Add `#[func] SimHost.logic_links_json(x,y)` if T1's `logic_get_state().links` is not enough for the panel.

**Tests.** `link_toggle_via_number_config`, `unlink_reenables_disabled`, `processor_removed_reenables`,
`world_processor_not_accessible`, `privileged_processor_immune`.

**Verify in-engine (UI half).** G06 T11 "Verify in-engine" steps (micro processor (125,50) + switch (126,50), real mouse clicks via
`godot_input`), reading links through `logic_get_state(125,50).links`.

**Commit.** `Fixes PAR-0220: processor pencil button and tap-to-link with Java link lifecycle`
(sim half only: `PAR-0220: processor link config and lifecycle (sim half)`)

---

### T16 — Logic overlays: processor range/links, message hover text (PAR-0103 slice)

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE (depends on H03 `ConfigOverlay`; no overlay is drawn today).

**Depends on H03** `ConfigOverlay`/`SelectOverlay` with a `Logic` variant and `MindInput::config_selected()`
(check: `grep -rn "enum ConfigOverlay\|fn config_selected" client/rust/mind-gdext/src`). If absent: skip, list it.
Raw material: `docs/parity-plans/G06-logic.md` T12 (open that section; Java lines accurate). Implement the
`Logic` variant's draw data via a pure core helper `logic_block::overlay_links(world, e) -> Vec<(String, (f32,f32), f32)>`
(+ harness test: 2x2 memory bank link → half-size 9.0) and `message_block::hover_text(world, e)`.

**Commit.** `Gameplay H06 T16: processor link/range overlay and message hover text`

---

### T17 — Logic editor fidelity: variables/globals dialog, canvas mode, per-statement layouts (Fixes PAR-0103)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED by absence (no logic editor dialog; see T14).

Raw material: `docs/parity-plans/G06-logic.md` T13b, T13c, T13d (open those sections). Unchanged scope; one commit per
sub-slice: `PAR-0103: logic variables and built-in globals dialogs`,
`PAR-0103: canvas-mode logic editor (statement blocks, fields, drag, copy)`,
`Fixes PAR-0103: per-statement editor layouts and jump arrows` (the layout table may be split by category into
three commits; only the last says `Fixes`). Use T1's `logic_get_state().vars` for the variables view.

---

### T18 — Logic displays: sim half + render half (Fixes PAR-0105)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. `logic-display` (124,50) + world processor `draw clear 255 0 0` / `draw rect` / `drawflush disp` (`disp` via `getblock building`) -> `logic_display_commands(124,50)` `[]` and the display draws nothing (`runs/20261014-s3-verify2/H06-T18-display.png`).

Raw material: `docs/parity-plans/G06-logic.md` T14a and T14b (open those sections). Re-verified 2026-10-14:
`core/logic/blocks/display.rs` `root_display: Some(e)` always (no tile fields), `executor/draw.rs` image of a display packs
`DISPLAY_DRAW_TYPE` (30) without `rootDisplay.index << 5`, no display renderer in `gdext/render/`, `SimHost.logic_display_commands`
is a placeholder (replace it with the real queue read: `take_display_commands`). Commits:
`PAR-0105: tileable display linking and display-image root index`, `Fixes PAR-0105: logic displays render drawflush output (incl. tileable displays)`.
Render file `gdext/render/logic_displays.rs` attaches in `render.rs` (H03's composition file: add one child, nothing else).

---

### T19 — Canvas blocks: render + logic read/write (Fixes PAR-0225)

> **In-engine verification (2026-10-14, session 3):** PARTLY. A placed `canvas` (128,46) draws only its base sprite; read/write is code-only (`core/logic/blocks/io.rs` has no Canvas arm).

Raw material: `docs/parity-plans/G06-logic.md` T15 (open that section). `core/logic/blocks/io.rs` has no `CanvasBlock` arm
(re-verified). Render through T18's renderer. Painting dialog deferred.
Commit: `Fixes PAR-0225: canvas blocks render and accept logic read/write`.

---

### T20 — Map/save-loaded processors are assembled before the logic globals exist (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** Every world processor (and player processor) that comes from a map or a
save is compiled while `GlobalVars` is not yet installed, so content/team constants (`@sharded`,
`@duo`, `@scatter`, `@combustion-generator`, `@copper`, ...) are not constants but plain variables
holding null. Ground Zero GZ-3/GZ-5 then run `fetch build d null 0 null` → `d` null → `amount` null →
`jump 0 equal amount 0` loops forever, so `scatterAmmo`/`duoAmmo` are never raised and the tutorial
stalls at the "Flag duoAmmo" objective even with a fed duo; frozenForest FF-1 never raises `power`.
Evidence (2026-10-14, session 3, `runs/20261014-s3-verify2/H06-evals.txt`): GZ processor (96,43) vars
`{"@duo": null, "@sharded": null, "amount": null, "d": null}` with a duo holding 18 ammo; re-assembling
the same code at runtime (globals present) gave `d "duo"`, `amount 26` and the `duoAmmo` flag within 30
ticks. Fresh frozenForest (64) processor (0,0): `"@sharded": null, "@combustion-generator": null`.
Acceptance: after a GZ resume, `logic_get_state(96,43).vars` lists no `@`-names; feeding a duo raises
`duoAmmo` (`logic_rules_probe().flags`); a headless test proves the load order.

**Java.** `LAssembler.var` resolves constants from the global `Vars.logicVars`
(`J/logic/LAssembler.java:59-61`: `LVar constVar = Vars.logicVars.get(symbol, privileged); if(constVar != null) return constVar;`),
created once in `Vars.init` (`J/core/Vars.java:369`) and initialized with the content
(`J/core/ContentLoader.java:107` `logicVars.init()`), i.e. long before any map or save is read
(`LogicBuild.read` → `readCompressed(bytes, false)`, `J/world/blocks/logic/LogicBlock.java:857-864`).

**Port today.**
* `core/logic/blocks/logic_block.rs` `update_code` (~129-147): `world.get_resource::<GlobalVars>().cloned().unwrap_or_default()`
  — the default (`GlobalVars::new()`, static constants only) has no content/team constants.
* `gdext/sim_host.rs` `load_sector_world` (~1940-1985): `install_live_block_runtime()` →
  `materialize_map_buildings(&pending_buildings)` (decodes each `LogicBuild` → `read_compressed` →
  `update_code`) → `apply_decoded_entities` → … → `install_sim_seams()` (~557-559:
  `GlobalVars::install_world(&mut self.sim.ecs.0, &registry)`) LAST. Other world-load entry points
  call `install_sim_seams()` after the world exists too (`grep -n "self.install_sim_seams()" client/rust/mind-gdext/src/sim_host.rs`: ~504, ~1836, ~1983).
* `core/world/generation.rs` `ensure_generation_resources` installs `GlobalVars` for generated maps only.

**Changes.**
1. `gdext/sim_host.rs`: split the globals install out of `install_sim_seams` into
   `fn install_logic_globals(&mut self)` (idempotent: `if get_resource::<GlobalVars>().is_none()`; it
   needs the registry — read how `install_sim_seams` gets it from `self.content_snapshot` and use the same
   source without `take()`-ing it, or clone the registry handle) and call it in `load_sector_world`
   right after the new `Sim` is created and BEFORE `install_live_block_runtime()` /
   `materialize_map_buildings`; same for every other world-load path that decodes buildings (map
   load, save load, `load_scenario` if scenarios can carry processors). `install_sim_seams` keeps
   calling it (no-op the second time).
2. Safety net in core (cheap, keeps headless loaders correct): after any bulk building decode
   (`Sim::materialize_map_buildings` and the save-load equivalent; grep `fn materialize_map_buildings`),
   if a `GlobalVars` resource exists, re-run `update_code(world, e, true)` for every entity with
   `LogicBlockState` whose `executor` was assembled without it — simplest: a `pub fn reassemble_all_processors(world: &mut World)`
   in `logic_block.rs` (iterate entities with `LogicBlockState` in `EntitySeq` order, take/insert the
   component, `update_code(world, e, true)` keeps variable values). Call it once at the end of
   `load_sector_world` (after the globals exist). Do 1 first; keep 2 only if 1 cannot cover a path.
3. Do NOT change `update_code`'s fallback (headless unit tests rely on it).

**Tests.**
* `logic_block.rs` / `production_tests.rs` `processor_loaded_before_globals_resolves_constants`: build a
  world WITHOUT `GlobalVars`, place a world processor with `fetch buildCount n @sharded 0 @duo`
  (compressed config), then install `GlobalVars::install_world` and call the load-order entry you
  changed (or `reassemble_all_processors`) → the executor's arena has no non-constant `@sharded`, and
  with one team-1 duo placed `n == 1` after a tick.
* `unit_runtime.rs` `mod msav_tests` (real GZ map): load `groundZero.msav` through the live loader
  path, then assert no `LogicBlockState` executor has a non-constant cell whose name starts with `@`
  other than `@counter`.

**Verify in-engine.** GZ resume (H-environment §4.1 Go). `return $H.logic_get_state(96,43).vars` (needs
T1) → no `@sharded`/`@duo` keys. Place the drill (130,48) + duo (132,48), `step(1500)`, then
`logic_rules_probe().flags` contains `duoAmmo`. Repeat on a fresh frozenForest (64): processor (0,0)
vars have no `@`-names. Evidence `runs/<date>-s<N>-H06-T20/`. 0 panics.

**Pitfalls.** `install_sim_seams` uses `self.content_snapshot.take()` — calling it twice would lose the
registry; keep a single owner of the registry handle. Never hold a `bind()` across the load. The order
change must not move `install_live_block_runtime` (entity indices must stay stable).

**Commit.** `PAR-0104: map/save-loaded processors resolve logic constants (globals installed before buildings load)`

---

### Deferred (with reason)

* `sync`, `clientdata`, `@server/@client` network semantics — multiplayer out of scope.
* `LogicFilter` and the editor's world-processor dialog — editor scope.
* World-processor icon tag (`Character` config) — editor-only input path.
* Saving `DynamicStatus` multipliers in unit saves (T6) — needs an entity-IO revision; note only.
* Bullets as logic objects (`BulletComp.sense/setProp`, `spawnbullet` result) — no bullet `LogicObject`.
* `query` (`QueryI`) result caching parity — no campaign map uses it.
* Unit `payloadType` object setter (`UnitComp.setProp(payloadType)`) — needs payload creation from content.

---

## §3 Execution order and hot files

Order (gameplay impact; one commit per task unless noted):
1. **T1** seams (needed by every in-engine check).
   **T20** right after T1 (added 2026-10-14; GZ duoAmmo/scatterAmmo blocker, prerequisite of T7's walk).
2. **T2** spawnwave (GZ-1; tutorial blocker).
3. **T3** cutscene (GZ-4 pan).
4. **T5** setrule re-sync / mapArea (GZ-4 area).
5. **T4** messages/localeprint (atolls; every sector with `message`).
6. **T6** setprop/unit props (extractionOutpost, atolls, cruxscape).
7. **T7** early-sector suite + GZ walk (evidence for PAR-0104).
8. **T8** world side effects (Fixes PAR-0104).
9. **T9** ubind/ucontrol/LogicAI.
10. **T11** sensor/fetch/print remainder.
11. **T14** text editor.
12. **T15** tap-to-link (sim half always; UI half after H02).
13. **T10** completionLogicCode.
14. **T12** turret control, **T13** radar.
15. **T18** displays, **T19** canvas.
16. **T16** overlays (after H03), **T17** editor fidelity.

Hot / shared files (keep edits minimal and local):
* `core/sim/mod.rs` `Sim::tick`, `drain_logic_events`, `apply_rules_to_world` (T1–T5, T8, T10) — H01/H02/H04 also edit `Sim`.
* `core/game/runtime.rs` `campaign_tick_system` (T2 extraction), `CampaignRuntime` (T5 one field) — H04.
* `core/sim/unit_runtime.rs` `run_wave_system` closure (T2), `unit_update_system` Logic branch (T9) — H01.
* `core/input/camera_state.rs`, `gdext/camera.rs`, `gdext/input/mod.rs` (T3) — H03/H08.
* `ui/ui_root.gd` (T4 poller), `ui/fragments/hud_fragment.gd` (T3) — H03/H05.
* `core/combat/damage/status.rs`, `core/entities/comp/unit/comp.rs` (T6 dynamic status) — H01.
* `world/blocks/defense/turrets/{mod,behavior}.rs` (T11 rows, T12) — H01/H02.
* `gdext/input/desktop.rs`, `ui/fragments/block_config_fragment.gd` (T15) — H02/H08.
* `ui/dialogs_manifest.json`, `scenes/ui/ui_root.tscn`, `core/ui/manifest.rs` (T14) — H05.
* `gdext/render.rs` (T18) — H03.

## §4 Ledger changes to report (never edit `chains.json`)

* PAR-0104 → `fixed` after T2+T3+T4+T5+T8+T20 with T7's in-engine GZ walk (evidence `runs/<date>-s<N>-H06-T7/`).
* PAR-0106 → `verified` candidate: T1 makes the in-engine `@tick` readback possible (record the T1 evidence).
* PAR-0218 → `fixed` after T6+T11.
* PAR-0220 → `fixed` after T15 (both halves); `partial` with the sim half only.
* PAR-0221 → `fixed` after T9. PAR-0222 → T13. PAR-0223 → T12. PAR-0224 → T10 (headless). PAR-0225 → T19.
* PAR-0103 → `partial` after T14 (+T15), `fixed` after T17. PAR-0105 → `fixed` after T18.
* New items to file (summary only):
  1. "`ubind` never binds: `@unit` is a constant and the instruction skips constants" (T9) — world, S2.
  2. "`setprop <item> <building>` adds instead of setting the count" (T6) — world, S3.
  3. "`setprop @armor/@speed` lasts one tick / no-op (no dynamic status)" (T6) — world, S3.
  4. "World-processor `spawnwave` never spawns a wave (Ground Zero tutorial stalls at DestroyUnits)" (T2) — world, S1.
  5. "Logic cutscene / messages never reach camera or HUD" (T3/T4) — ui, S2 (if not covered by PAR-0104's text, file as notes on PAR-0104 instead).
  7. "Map/save-loaded processors never resolve `@sharded`/`@duo`/content constants (GZ duoAmmo/scatterAmmo,
     frozenForest power flag never set)" (T20) — world, S1 (added 2026-10-14).
  6. "Pan-only logic cutscene keeps gameplay zoom (Java zooms to the cutscene minimum)" (T3) — render, S4.

## §5 Done criteria

* Every task's tests pass; `cargo test -p mind-core --lib --features msav-import` no new failures;
  `mind-headless --lib` and `mind-gdext --lib` failures ⊆ the H00 baseline (`docs/H00-blockers_summary.md`);
  clippy clean; GDScript parse gate clean.
* In-engine, GZ after a clean import: the T7 walk passes (flags → map area, cutscene pan/stop, wave spawn,
  duoAmmo, waveTimer), a world processor shows an announcement, a spawned flare appears, a player micro processor's
  code can be set and its vars read (T1), and (T14) edited in the dialog. 0 panics.
* `REPO/docs/H06-logic_summary.md` written per env §6 (status table, deviations: spawn jitter RNG, one-tick spawnwave
  delay, skipped `Fx.spawn` 30-tick follow-up, info-toast fallback, dynamic status not saved, any skipped family rows).

## §6 Cross-area seams

| Seam | Provider | Consumer | If the provider is late |
|---|---|---|---|
| `world::limits::sync_build_rules(world, &rules)` | H02 | H06 T5 calls it inside `Sim::apply_rules_to_world` | do NOT define it; leave the comment, list per-team rules as waiting |
| `world::config_tap::config_tap` + `BuildingBehavior::on_configure_build_tapped(&self, world, grid, e, other) -> ConfigTapResult` | H02 | H06 T15 adds the `LogicBlock` arm | T15 ships the sim half only |
| `ConfigOverlay`/`SelectOverlay` `Logic` variant + `MindInput::config_selected()` | H03 | H06 T16 | skip T16 |
| `MindUi.show_info_toast(text, duration)` (visual = `UI.showInfoToast`) | H03 | H06 T4 poller | poller falls back to `MindUi.announce` (temporary, listed) |
| `MindUi.announce(text, duration)`, `MindUi.toast(text, icon)` | exists (`gdext/ui/ui_host.rs`) | H06 T4 | — |
| `MindInput.set_logic_cutscene_lock(bool)` | **H06 T3 defines it** | H08 input lock consumers | — |
| `MindSimHost::logic_cutscene_state()` (Rust) / `logic_cutscene_json()` (`#[func]`) | **H06 T3** | H03 HUD, H08 | — |
| `SimHost.logic_get_code/logic_set_code/logic_get_state/logic_last_error/logic_event_log_json` | **H06 T1** | H09 sweep, everyone verifying logic | — |
| `game::runtime::run_wave_now(&mut CampaignRuntime, Option<&mut UnitRuntime>)` | **H06 T2** | H04 (wave timer / skip-wave button may reuse) | — |
| `world::update::allow_update(world, e, rules)` (Java `Building.allowUpdate`) | **H06 T5** | H02/H04 (derelict / env / map-area update gating) | — |
| `create_weather(world, runtime, id, intensity, life)` | H07 if present, else **H06 T8** moves `debug_create_weather`'s body | H07 weather system | — |
| `AudioEvent` sink + `sfxvol` application | H07 | H06 T8 emits raw volume | — |
| `MindUi.play_ui_sound(name)` | H07 (emit-only elsewhere) | not used by H06 | — |
| Map-locales on the loaded session (`SaveReadState.locales` → host) | H04 owns load; **H06 T4** threads it if dropped | H06 T4 | — |
| `rules.mission` display in the HUD wave panel | H03/H05 | H06 T4 writes it | note in summary |
