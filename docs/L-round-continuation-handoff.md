<!-- SPDX-License-Identifier: GPL-3.0-only -->

> Historical note (2026-10-19): `chains.json` and `parity-ledger.json` were retired and replaced by `TWIN-VERIFICATION.json`. The names below are updated for navigation; the original paths remain in git history.

# Round L — continuation handoff (2026-10-18)

Read this first, then `docs/parity-plans/L-INDEX.md`, `L-environment.md` and the
per-plan summaries. The paused-round coordinator summary is
`docs/L-round-execution-summary.md` (§7 is the first continuation, §8 the late
2026-10-18 L04/L07/PAR-0445 batch).

## 1. Where the tree is

- Branch `main`; the 2026-10-19 continuation merged the four agent branches
  (`d3936804`, `7e2df1ca`, `d694829b`, `3adc976f`), the live PAR-0379 fixes
  (`aa57179d`) and the docs/ledger commit (`5da933aa`). See §7 for the current
  state and the new next work. The late 2026-10-18 batch is the three commits
  before those. The only uncommitted files left on purpose are the
  operator's archival deletions under `docs/` and the
  `docs/parity-plans/archived_plans_do_not_read.zip` change. Do not commit or
  restore those.
- `.opencode/parity-session/**` churn and `.claude/worktrees/` are session
  state, not source. Do not commit them.
- `runs/` is gitignored; evidence/run dirs and the ledger op files live there.
- `TWIN-VERIFICATION.json` was updated in this batch (PAR-0446/0447 verified;
  PAR-0379/0380/0399 fixing; PAR-0407 seeded with T4/T5/T6 notes;
  PAR-0296/0126 notes) and in the late batch (PAR-0363/0365/0366/0367 fixing,
  PAR-0445 fixing, PAR-0407 T5 note).

## 2. What this continuation landed

| Area | Status | Key files |
|---|---|---|
| PAR-0446 solid flags | verified (tests) | `content/registries/blocks/{distribution,units_erekir}.rs` |
| PAR-0447 filter floor/overlay | verified (test) | `maps/filters/mod.rs` |
| L05 T1 hover bars (PAR-0379) | code + unit tests; Java pair open | `mind-gdext/src/sim_host.rs` (`building_bars_json`, `BarRow`/`BarExtras`), `client/ui/fragments/placement_fragment.gd` |
| L05 T2 TargetDummy (PAR-0380) | code + tests + state probe | `determinism/command.rs`, `world/config.rs`, `world/blocks/defense/{shields,behaviors}.rs`, `input/{desktop,mod}.rs`, `block_config_fragment.gd` |
| L05 T2 CoreBlock teams | spec + Sim apply test; live rule mirror open | `sim/mod.rs` (`apply_world_config`), `storage/core_block.rs`, `input/desktop.rs` |
| L04 T5 ExplosionSink (PAR-0399) | host + tests; Java meltdown pair open | `world/blocks/power/{explosion,generator,behavior}.rs`, `sim/runtime.rs`, `mind-gdext/src/fx/explosion_sink.rs`, `sim_host.rs` install |
| L07 T4 logic font/icon tag | code; no Java display pair yet | `mind-gdext/src/display_context.rs`, `assets/atlas.rs`, `render/overlay_top.rs`, `sim_host.rs` |
| L07 T5 canvas | 480 px + PNG I/O + autosave + blending mask; edge sprites/off-screen pre-pass open | `client/ui/dialogs/canvas_edit_dialog.gd`, `mind-core/src/logic/blocks/canvas.rs` |
| L07 T6 sensors/polish | permanent-status seconds, unit `@id` NaN, boss radar; pickers open | `logic/access.rs`, `logic/executor/radar.rs`, `client/ui/dialogs/logic_dialog.gd` |
| L08 T3 `drillMultipliers` | parser only, no fixture | `content/registries/blocks/mod.rs`, `mods/json/mod.rs` |
| PAR-0296 tap hook | tap opens planet launch page | `mind-gdext/src/input/desktop.rs` |

Goldens: `parity/golden_content.json` + `parity/golden_manifest.json` were
re-recorded for the four `solid` fields and `target-dummy configurable = true`;
`content::parity` and `mind-headless parity goldens` pass.

## 3. Gates already green

```
cargo test -p mind-core --lib                 # 2803 passed, 0 failed, 4 ignored
cargo test -p mind-gdext --lib                # 80 passed
cargo test -p mind-headless                   # pass
cargo run -p mind-headless -- parity goldens  # 0 FAIL
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings
cargo fmt --all -- --check
bash tools/godot.sh --headless --editor --quit --path client   # no script errors
```

Late 2026-10-18 batch (after L04/L07/PAR-0445): `mind-core` 2805 pass (4
ignored) + all integration goldens, `mind-gdext` 80, `mind-headless` 133,
`parity goldens` 0 FAIL, clippy/fmt clean, Godot import exit 0,
`ui_layout_check` / `ui_widgets_check` failed=0. The `.so` was rebuilt and the
editor stopped; run `tools/build.sh` once before the first live check only if
the tree changed since commit.

### Landed after this handoff was written (late 2026-10-18)

L04 T1 (`render/unit_parts.rs`), T2 (ForceField/EnergyField/SuppressionField/
ShieldArc draws + state mirrors), T3 partial (weapon-part `layerOffset`),
T4 (`HaloPart` + lustre/scathe/smite/malign trees); L07 T5 canvas edge sprites
+ off-screen image pre-pass; PAR-0445 SaveDialog/LoadDialog inheritance. Per-plan
detail in the L04/L07 summaries and `docs/L-round-execution-summary.md` §8.

## 4. Next work, in order

1. **Live acceptance batch (one build, one editor session)** — stage the Java
   reference where required (L-environment rules 7–8) and capture:
   - PAR-0379 hover-box pairs for duo/cyclone, a power node/diode, a generator
     and reactor, overdrive/force projector, a pump, loader, assembler and
     accelerator (`building_bars_json` rows are unit-tested);
   - PAR-0380 real-click TargetDummy panel and core team toggles. Before the
     core check, verify the live `BuildRules.core_build_and_config` is true
     (`CampaignRuntime` mirror did not reach it in the sandbox probe);
   - PAR-0399 thorium meltdown + blast-compound combustion generator pair;
   - PAR-0126 live Java leg pair (L04 T6);
   - L04 T1 unit parts + T2 abilities + T4 turret trees against Java (the code
     is landed; the frames are the remaining acceptance evidence).
2. **L04 T3 residue** crawl segments (`drawCrawl`), payload composition
   (`drawPayload` for unit/building payloads) and the hand-drawn weapon
   region/cell z (`layerOffset`; weapon parts already apply it).
3. **L08 T2** `ModDataManager` owner/decorators and patch threading
   (`mods/assets`, `maps/mod.rs:463`, `editor/assets.rs`, map assets dialog).
   **L08 T3** remaining parser rows (`UnlockableContent`, plans, weapon parts/
   effects, inline effects, drawer), one fixture per row.
   **L08 T4/T6** evidence debt and the round-M closing walk.
4. **L07 T6 residue** icon pickers/enum columns/drag capture (T5 edge sprites
   and the off-screen image pre-pass are landed).
5. **PAR-0445** is landed (SaveDialog now extends LoadDialog); keep only the
   final Java like-for-like frame on its ledger item.

## 5. Environment and harness notes from this session

- Fresh build: `bash tools/build.sh` (flock); then restart the editor, e.g.
  `bash tools/session-play.sh 1` or
  `OPEN_GODOT_MCP_PORT=6970 DISPLAY=:10 setsid godot --editor --path client`.
  Do not run two editors on the same bridge port.
- Session 1 is display `:10`, bridge `6970`; `tools/parity-session.sh --session 1`
  starts Xvfb + editor. Close the editor/game when idle.
- MCP eval rules that bit this session:
  - `get_state_json`, `tile_blocks()` walks and `for` loops time out; keep evals
    to one expression and return flat dicts.
  - An errored eval trips break-on-error and freezes the frame loop; recover
    with `godot_game stop` + `play`, not `resume`.
  - `Input.config_spec_json(x,y)` is the safe probe for a config panel;
    `building_config_json(x,y)` reads the applied config.
  - `start_map_mode("mini map","sandbox",false,"")` gives a sandbox world;
    `place_block` then succeeds for sandbox-only blocks.
  - `Input.configure_building(x,y,"ints","1,1,50,90,<f32 bits>")` stages an
    `int[]`; `"int"` stages a single int.
- Java captures: use the per-session `parity-java-home` copy and the session
  display; never the operator's default data dir. A Java capture for a
  campaign-only block (accelerator) needs a throwaway campaign profile.

## 6. Suggested opening prompt for the new session

> Continue round L from `docs/L-round-continuation-handoff.md`. The late
> 2026-10-18 batch is committed on `main` (L04 T1/T2/T4, L07 T5 edges/pre-pass,
> PAR-0445, ledger/summary updates). Start with §4: run `bash tools/build.sh`
> once, start session 1, and stage the Java acceptance batch (L-environment
> rules 7–8) for PAR-0379/0380/0399, PAR-0126 and the L04 T1/T2/T4 pairs; then
> finish L04 T3 (crawl/payload/weapon z), L08 T2/T3, L07 T6 and the round-M
> closing walk. Keep the ledger and per-plan summaries updated, and do not touch
> the operator's archived-docs deletions.

---

## 7. Update 2026-10-19 — four branches merged, live hover batch

The four branches from §2's continuation are now merged on `main` (merge commits
`d3936804`, `7e2df1ca`, `d694829b`, `3adc976f`), plus `aa57179d` for the live
PAR-0379 diode/loader fixes. The tree is clean except the operator's archival
deletions under `docs/` and the
`docs/parity-plans/archived_plans_do_not_read.zip` change; do not commit them.

New evidence: `runs/20261018-L-accept/` (Java/Godot hover frames + `notes.md`
recipe). `docs/L-round-execution-summary.md` §9 is the batch record.

What changed since §2:

- L04 T3 crawl/payload/weapon-z code landed (`render/unit_draw.rs`,
  `render/units.rs`, `sim_host.rs`, content assertions). Java frames still open.
- L07 T6 pickers/enum columns/drag-follow landed; effects-preview render and
  sound-picker chrome remain.
- L08 T3 parser rows + fixtures landed (six fixture mods).
- L08 T2 `MindMods` owner, atlas/audio/bundle decorators, map `patches`
  round-trip and MapAssetsDialog body landed; six precise gaps in the L08
  summary.
- L05 T1 live findings: standalone diode and payload-loader bars fixed; unit
  icon markup raw in `MindBar`, accelerator `powerbuffer` row clipped by the
  palette, pump pair deferred (dry mini map).

Gates on the merged tree: `mind-core` 2823 (4 ignored), `mind-gdext` 81,
`mind-headless` 133 + all suites, `parity goldens` 0 FAIL, clippy/fmt clean,
Godot import exit 0, `ui_layout_check`/`ui_widgets_check` failed=0,
`shaders_check` 29/0. The known boot-time `MindWorldRenderer::bind_mut` panic
still appears (no frame-loop impact).

Next work, in order:

1. **L04 T0 Java capture batch** — the remaining blocker for T1/T2/T3/T4/T5/T6
   live pairs. Use the `runs/20261018-L-accept/notes.md` recipe and the Mud
   Flats save; capture merui/cleroi/anthicus/tecta/collaris/elude/avert/
   obviate, quasar/oct/aegires/quell/disrupt, renale/latum crawl and
   mega/quad/oct payloads, scathe/smite/malign/lustre firing, the reactor/
   generator pair, and atrax/spiroct/arkyid/toxopid.
2. **PAR-0379 remaining pairs** (cyclone, generator, reactor, overdrive, force,
   pump on a water map, loader, assembler, accelerator) plus the unit-icon
   markup and hover-panel clipping residuals.
3. **L08 T2 gaps** (content sprites, per-asset warnings, non-embedded cache,
   non-editor map data application, audio/image view chrome) and **L08 T4/T6**.
4. **L07 T6** effects-preview render and sound-picker play/pause state.
5. PAR-0399 meltdown pair, PAR-0126 leg pair, PAR-0445 final Java frame.
