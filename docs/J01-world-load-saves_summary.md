<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J01 — World load fidelity, map import, saves (wave A: T1 + T2)

Branch `parity/session-2` (worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session2`),
session 2 (display :11, bridge 6980). This wave executed **T1 and T2**; T3–T6 are
wave B and are not started.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — Apply map generation filters on load (`World.FilterContext`) | done (in-engine verified) | `396c9886` |
| T2 — Import the maps Java loads (3 user maps + 7 Serpulo presets) | done (in-engine verified) | `1b844069` |

## Commits

- `396c9886` `J01 T1: apply map generation filters on load (World.FilterContext)`
- `1b844069` `J01 T2: import the maps Java loads (pre-short MSAV + Arc JSON forms)` — body carries `Fixes PAR-0338`
- (this document committed separately)

## T1 — map generation filters

### What changed

- `mind-core`: `io::sim_io::decode_map_bytes` wraps the save decode with the
  `FilterContext` step — `map.filters()` (`Map.java:120-125`: the `genfilters`
  tag, empty for `build < 83` without the tag, else `Maps.readFilters`' default
  stack) is randomized and applied to the tiles before `endMapLoad`
  (`World.java:341-357,661-695`). `world::context::apply_filters` is the shared
  stack runner (`FilterContext::end` uses it too). `Sim::map_gen_seed` records
  the seed; `sim::dump` emits it as `map_gen_seed`.
- `mind-gdext`: `load_map_world` loads through `load_map_grid_file` with a fresh
  seed per load (`Time.get_ticks_usec` ^ world-load counter; Java re-rolls via
  the global `Mathf.rand`, `GenerateFilter.java:91-92`) and records it in the
  fresh sim; `set_next_map_gen_seed` pins a seed for replays; `world_gen_debug`
  reports seed + ore/boulder counts + sample tiles. Save loads and sector loads
  keep `decode_save_bytes` and never re-run filters (`SaveIO.load(file)`).
- `mind-headless`: new `campaign map-mode --map <file> --seed <n>` runs the
  `start_map_mode` world half and reports the counts; a forwarding
  `msav-import` feature reads the shipped `MSAV` maps.

### Tests (exact commands)

```bash
cargo test -p mind-core --lib map_filter_tests          # 4 passed
cargo run -p mind-headless --features msav-import -- campaign map-mode \
  --map assets/maps/default/caldera.msav --seed 7 --json
# {"ore": 3876, "boulders": 95, "pass": true}
# seed 8: ore 4169, boulders 110 (re-roll)
```

`map_filter_tests::map_load_applies_genfilters_ore` fails before the fix (with
the filter step disabled: "the genfilters ore stack adds overlay tiles"), and
covers same-seed stability + new-seed re-roll; `save_load_is_untouched`,
`build_lt_83_map_without_tag_is_untouched` and `dump_records_map_gen_seed` pin
the other rules.

### In-engine evidence (`runs/20261016-s2-J01-T1/`)

Real-input walk (session 2, pid 92820, post-fix build): Play (230,149) →
Custom Game (460,289) → Ancient Caldera (1010,455) → Survival → Play (668,573),
twice (quit → menu → same walk).

- load 1: `world_gen_debug` → seed `41931039`, ore 4388, boulders 97;
  `tile_debug(81,0)` = `ore-lead`, `tile_debug(55,56)` = `basalt-boulder`
- load 2: seed `11400714819198985189`, ore 3839, boulders 111;
  `tile_debug(18,0)` = `ore-copper`, `tile_debug(139,20)` = `basalt-boulder`
  with `ore-titanium`
- state JSON slice: `"map_gen_seed": 41931039` / `11400714819198985189`
- `last_log.txt`: `load_map_file: ... caldera.msav gen seed <seed>` per load
- frames `godot/load1-frame.png`, `godot/load2-frame.png`; diffs
  `diff/load1-vs-java42.json` (vs `java/42-survival.png`, Java thorium load),
  `diff/load2-vs-java46.json` (vs `java/46-conv-sel.png`, Java titanium load),
  `diff/java42-vs-java46.json` (Java's own two rolls: changed 0.0940,
  mean_abs [6.17, 4.99, 4.72], coarse 3.90, palette 0.0640) and
  `diff/load1-vs-load2.json` (Godot re-roll: changed 0.4252).
  The per-pair gate numbers exceed the §4 gate because two rolls intentionally
  place different ores and the known round-J render gaps of this surface (F2
  floor edges, ore palette) are still open; the structural match is the same
  map/camera (130,32, core centred) with ores + boulders present and re-rolled
  per load, exactly like Java frames 42 vs 46.

## T2 — map import

### What changed

- **Arc JSON forms** (`quote_bare_keys` is now a faithful Arc-minimal → strict
  JSON token converter; `JsonReader` unquoted-name/value lexing, keyword/number/
  string classification, `unescape` + strict escaping). Fixes the multi-word
  logic-code strings (hidden/92, 263, infestedCanyons) and the leading-zero
  color tokens (hidden/247, navalFortress).
- Field-level tolerances (never blanket-ignoring):
  - `ObjectSet` wrapper `{"values":[...]}` on Rules bannedBlocks/bannedUnits/
    revealedBlocks/researched/objectiveFlags (`Json.readValue`'s ObjectSet
    branch) — anthill_v5, erekir peaks/siege/split;
  - `weather:null` → empty name, dropped by the runtime's `Logic.java:377`
    retain — hidden/176, 254;
  - numeric legacy spawn effect `8` → `boss` (`SpawnGroup.java:149-154`) —
    default/canyon.msav;
  - Color bean object `{"a":0.12}` in `ColorHex` (`JsonIO.java`'s Color
    serializer `readFields` branch) — CrashSite.
- **Pre-short-chunk `MSAV` v1-3**: ported `LegacySaveVersion.readMap` (no
  packed-check byte; `u16`-framed building chunks with the legacy base layout —
  `u16` health, packed team/rotation, capability-driven module stacks,
  `cons.valid`, per-kind body), `Save1/Save2/Save3.readEntities` (groups
  framing, v3 legacy team plans) and the `PreviewContext` decode — canyon.msav
  (v2), CrashSite.msav (v3), Attack_Secret_Island_Outpost_1.msav (v2).

### Tests (exact commands)

```bash
cargo test -p mind-core --lib --features msav-import import_tests -- --include-ignored
# serpulo_presets_all_import  -> imported 78 serpulo presets
# all_shipped_maps_import     -> imported 114 shipped maps
# user_maps_import           -> imported 9 user maps
# user_maps_generate_previews -> 9 previews (CrashSite teams {1},
#   [Orange]Attack teams {1,2}, anthill v5 teams {1})
```

The user-map tests are `#[ignore]`d by default (third-party maps are not
vendored); they read `MIND_USER_MAPS_DIR` or this checkout's
`parity-java-home/.local/share/Mindustry/maps`.

Pre-fix repro (all fail before the change): the serpulo preset test printed the
7 recorded failures (`invalid type: null, expected a string` ×2, `invalid
number` ×2, `expected , or }` ×3), the user maps failed with `invalid type: map,
expected a set` (anthill) and `Error reading region "map"` (CrashSite,
Attack_Secret), and default/canyon.msav with the declined pre-short message.

New unit tests pin each tolerance: `object_set_wrapper_and_array_both_decode`,
`null_weather_name_decodes`, `legacy_numeric_spawn_effect_maps_to_boss`,
`color_hex_accepts_component_objects`, plus the converter tests
(`arc_json_unquoted_string_values_are_single_tokens`,
`arc_json_leading_zero_color_tokens_stay_strings`,
`arc_json_strict_input_is_preserved`).

### In-engine evidence (`runs/20261017-s2-J01-T2/`)

Session 2, pid 156354 (post-fix build): Play → Custom Game
(`godot/custom-list.png`).

- `MindPreview.texture_for_name` returns a texture for `anthill v5`,
  `Capital Ship Crash Site` and `[Orange]Attack_Secret Island Outpost`.
- `MindPreview.map_modes` returns survival/sandbox/editor `true` for them (the
  card mode icons).
- `last_log.txt`: no new `MindPreview generation ... failed` lines after a
  fresh `refresh()` + `generate_previews()` (the 9 in the shared log are all
  pre-fix).
- `diff/09-custom-side.png` (Java `09-custom.png` left, Godot right): the three
  maps now render real previews and mode icons like Java. Frame gate numbers
  (changed 0.5393, mean_abs [33.65, 31.94, 32.49], coarse 15.11, palette
  0.1545) reflect the still-open card order (J01 T3) and dialog skin (F11), not
  the previews.

## Deviations (with Java locations)

1. **Filter application site.** The port applies the resolved stack right
   before `end_map_load` via `world::context::apply_filters` instead of
   constructing a literal `FilterContext` (the decode uses `EditorContext`
   because `SaveReadState.content` needs the registry borrow). Behaviourally
   identical: `FilterContext.end()` = `applyFilters()` then `super.end()`
   (`World.java:661-695`).
2. **Generation seed.** Java draws each `randomize()` from the global
   `Mathf.rand` (`GenerateFilter.java:91-92`) and never records it; the port
   draws a fresh seed per load, feeds it through the `MapGen` `SimRng` stream and
   records it as `map_gen_seed` in the state JSON, per the plan's
   "deterministic per seed" requirement. The in-engine walk shows two loads
   re-roll like Java.
3. **Bare numeric tokens that are not strict JSON** (e.g. `00000000`) are
   preserved as strings by the converter (Arc keeps the raw text in
   `JsonValue.stringValue`). Upstream's Color serializer would fall through to
   `readFields` and zero the value for a non-string JsonValue (`JsonIO.java:99-113`);
   the port keeps the authored hex.
4. **`Save3` team-plan configs** are read as the legacy `i32` and dropped
   (`TypeValue::Null`); the pre-TypeIO config codec is not ported
   (`Save3.java:readEntities`). Positions/blocks of the plans are preserved.

## Blockers

None for T1/T2.

## Pre-existing failures on main (verified, not caused by this work)

Reverting this branch's files to `fe8fb0e4` reproduces all of them; they come
from behaviour changes merged after the goldens were last regenerated:

- `mind-core --test blocks_golden family_counts_match_golden`: family `other`
  158 vs golden 164.
- `mind-headless` golden tests: `audio_golden` (a `shootDuo` event position),
  `input_golden`, `logistics_golden`, `maps_editor_golden` (preview checksum).
  The last golden regen is G07 T2 (`95d015bb`, 2026-10-13); H01 T1
  (`8e7b68b8`, 2026-10-14) intentionally moved building positions to the Java
  convention, so e.g. the Duo shot event moved from `(33.34946, 65.755646)` to
  `(33.0, 66.25)`. Not regenerated here: out of J01's scope and the coordinator
  should regenerate deliberately (`tools/regen_goldens.sh`) with the owning
  task's Java reason.

## Ledger changes to apply

- **New item (T1)**: "Custom maps load without generation filters (no
  ores/boulders)" — surface `world`, status `verified` (`mode: reference`),
  fix commit `396c9886`, evidence
  `runs/20261016-s2-J01-T1/diff/load1-vs-java42-side.png` +
  `godot/load2-frame.png`; the in-engine walk shows ores/boulders and two
  re-rolled loads against stored Java frames 42/46.
- **PAR-0338 → fix** (`1b844069`): all 78 Serpulo presets import.
- **PAR-0114 note**: the three authored user maps now render previews and mode
  icons (`runs/20261017-s2-J01-T2/godot/custom-list.png`); the scrap-ore
  preview/metaminimap half of PAR-0114 remains open (J01 T3/T4 and the
  `Floor.useColor` path per F12).

## Done criteria (wave A scope)

- T1 landed with tests + a real-input in-engine walk proving ores/boulders and
  per-load re-roll like Java frames 42/46. ✅
- T2 landed with tests (78 presets, 114 shipped maps, 9 user maps incl. the 3
  targets) + the Custom Game list showing previews and mode icons for the 3
  maps. ✅
- Remaining done criteria (T3–T5, full twin re-captures) are wave B.

---

# Wave B (T3–T6) — 2026-10-17, session 2

Branch `parity/session-2` (worktree `Mindustry-Godot-session2`), session 2 (display :11,
bridge 6980). Wave A (T1/T2) is above; this section covers **T3–T6**, each as its own commit.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T3 — Map list order = `MapPriority` | done (in-engine verified) | `a17dda0f` |
| T4 — Save previews (`SaveSlot.savePreview`) | done (in-engine verified) | `44e959f6` |
| T5 — Load list order + Save & Quit label | done (in-engine verified) | `6e07cd0f` |
| T6 — Slot naming parity (deviation) | done (documented) | `b08e17f1` |

## T3 — map list order

**Root cause.** The comparator was already ported, but its key came from GDScript
`FileAccess.get_modified_time` — whole seconds. Java's key is
`-m.file.lastModified()`, i.e. `File.lastModified()` *milliseconds*
(`MapListDialog.java:44-50`); the reference profile's user maps sit in one copy second
with sub-ms differences, so Java forms two mtime buckets and the `Map::name`
tie-break orders them, while the port collapsed everything into one tie.

**Change.** `MindPreview.maps_list` rows carry `lastModified` (ms via
`std::fs::metadata`; 0 for classpath-style built-ins like Java's jar `Fi`s) and the
`mod` flag the dialog already read; `map_list_dialog.gd::_last_modified` uses the row
value (Custom Game and the Editor Maps dialog share the base). Pinned by
`map_row_last_modified_is_ms_for_files_and_zero_for_builtins` (mind-gdext).

**Evidence (`runs/20261017-s2-J01-T3/`).** Session 2 pid 374208, with the session's user
maps normalized to the reference profile's mtimes (`cp -p`):

- Play > Custom Game and Editor > Maps show the Java frame order — 4 corners of death /
  Capital Ship Crash Site / [Orange]Attack_Secret Island Outpost / mini map / pvp //
  sandbox / Tonks / anthill v5 / stainedMountains / Ancient Caldera
  (`godot/custom-recent.png`, `godot/editor-open.png`, structured `_sorted_maps()`
  check).
- A fresh live Java leg reproduced the stored frame order and its builtin tail
  (`java-live-custom.png`, `java-live-custom-scrolled.png`).
- Filter popup real clicks: `name` -> alphabetical, `recent` -> restored, persisted in
  `editormapviewsettings` (`godot/filter-panel.png`, `godot/custom-name.png`,
  `godot/custom-recent-restored.png`).
- Frame diff vs `java/09-custom.png`: changed 0.497, mean_abs [31.67, 29.88, 30.02],
  coarse 13.12, palette 0.104 — the residual is the dialog skin (F11), not order
  (`diff/09-custom-side.png`).

## T4 — save previews

**Change.** `SaveSlot::write_preview_from_file` (mind-core) renders a save's own tiles
into `previews/save_slot_<index>.png` (`Saves.java:343-358`); the live-grid
`savePreview()` half is now called for autosaves, GameOver/capture saves, sector saves
and `run_exit_save` (Save & Quit); `list_save_slots` backfills any slot without a
preview and `import_save` renders the imported slot's; `save_slot` also registers the
slot as current while in game (`Saves.java:265-271` — without it a custom game had no
current slot, so neither the autosave cadence nor the exit save ran). Pinned by
`missing_preview_renders_from_save_file`.

Game Data import parity: `import_data` extracts every archive entry, but Java's
`exportData` omits `mapPreviewDirectory` (`SettingsMenuDialog.java:575-608`) and the
shipped profile zip has no `previews/` entries (checked with `unzip -l`), so previews
are regenerated from the save tiles.

**Evidence (`runs/20261017-s2-J01-T4/`).** Session 2 pid 414445: the Custom Game
savecreate slot wrote its preview; `saveinterval=5` fired a real autosave that
rewrote save+preview in the same second; Save & Quit (exit save) rewrote both; Load
Game shows previews on every card (`save_slot_*.png` 3 -> 38 via the listing
backfill; `godot/load-list.png`, before frame `godot/before-load-list.png`).

## T5 — load list order + Save & Quit

**Change.** `load_dialog.gd` sorts by `timestamp_ms` desc (`LoadDialog.java:93`);
`paused_dialog.gd` binds `@save.quit` to `MindCampaign.current_save_autosave()`
(`PausedDialog.java:116,154-155`) instead of the campaign flag. **Root cause found while
verifying:** `Platform::millis` was "monotonic since host start", but `Time.millis` is
wall-clock epoch milliseconds (`Time.java:122-124`; stored as the save meta `saved`,
`SaveVersion.java:130`) — every new save carried a tiny `saved` and sorted last.
Fixed and pinned by `platform_millis_is_epoch_milliseconds`.

**Evidence (`runs/20261017-s2-J01-T5/`).** Session 2 pid 428628: the pause menu reads
"Save & Quit" in a custom game (`godot/pause-savequit-final.png`); the confirm matches
`java/26-quitconf.png` (changed 0.0997, mean [6.28, 6.34, 6.14], coarse 4.41; the
residual is the F11 confirm-panel skin); Load Game lists the new
`AncientCalderaOct17641` (Oct 17 2026) first, then the 2024 profile saves in descending
timestamp order (`godot/load-newest-final.png`).

## T6 — slot naming

Kept the round-I name-stem design as a **documented deviation** (Java numeric slots:
`Saves.getNextSlotFile` -> `saves/<index>.msav`, name in `save-<index>-name`/meta,
`Saves.java:265-271,283-290`; previews `save_slot_<index>.png`). The port keys the file,
settings and preview by the sanitized name; `Saves::import_save` keeps imported Java
saves on their numeric stems, so a data export/import round-trips by slot identity.
Recorded in `sanitize_slot_name`'s doc (commit `b08e17f1`).

## Deviations (wave B, with Java locations)

1. `Platform::millis` epoch semantics fixed (was a monotonic counter) — no deviation,
   a Java-parity fix (`Time.java:122-124`).
2. `list_save_slots` renders missing previews synchronously on open; Java loads them
   lazily through the asset loader (`Saves.java:359-370`). Same output, simpler seam.
3. Game Data archives keep Java's "no `previews/` in the export" behaviour
   (`SettingsMenuDialog.java:575-608`); previews are regenerated instead.
4. Name-stem slot files (T6 deviation above).

## Residuals observed (wave B)

- Custom-game save meta still writes `mapname: "sim"`, so the Load card's `Map:` line
  reads "Unknown" (Java writes `state.map.name()`, `SaveVersion.java:133`). Candidate
  follow-up.
- The pause/load/map-list dialog skin remains as found (F11 / J06).

## Pre-existing failures on main (unchanged from wave A)

`blocks_golden::family_counts_match_golden` and the mind-headless goldens
(`audio`/`input`/`logistics`/`maps_editor`) still fail at the wave-B base
(`3bd1abe8`); verified pre-existing in wave A by reverting the touched files.

## Ledger changes to apply (wave B)

- **New item (T3)**: "Map list order uses second-precision mtimes (Java
  `MapPriority.recent` reads milliseconds)" — status `verified` (`mode: reference`),
  fix `a17dda0f`, evidence `runs/20261017-s2-J01-T3/diff/09-custom-side.png` +
  `godot/custom-recent.png`; surface `ui`.
- **New item (T4)**: "Save slots have no preview PNG (Load Game cards show the map
  glyph)" — status `verified` (`mode: reference`), fix `44e959f6`, evidence
  `runs/20261017-s2-J01-T4/godot/load-list.png` vs `java/39-load.png`.
- **New item (T5)**: "Load Game lists oldest-first; the pause menu never says Save &
  Quit" — status `verified` (`mode: reference`), fix `6e07cd0f`, evidence
  `runs/20261017-s2-J01-T5/godot/load-newest-final.png`,
  `godot/quit-confirm-final.png` vs `java/26-quitconf.png`.
- **PAR-0114 note**: the three user maps' previews (T2) and every save-slot preview
  (T4) now render.
- **New deviation note (T6)**: name-stem save slots (kept from round I; see T6).
- **Candidate follow-up**: custom-game saves write `mapname: sim` -> "Map: Unknown"
  on the Load card (`SaveVersion.java:133`).

## Done criteria (wave-B scope)

- T3–T6 landed with tests/deviation records; T3/T4/T5 verified in-engine against the
  stored Java frames (§3b) with real input. ✅
- The twin frames re-captured for the three surfaces (`09-custom`, `39-load`,
  `26-quitconf`) match structurally; the remaining frame deltas are the F11 dialog
  skin and are tracked there.
