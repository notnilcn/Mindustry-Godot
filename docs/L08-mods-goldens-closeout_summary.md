<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L08 — Mod data host, goldens, evidence, closing walk — summary

Owner: session B (session 3 worktree, branch `parity/session-3`). Round L, wave 1.
Session A reports L08 T5 (and later T6) in its own wave reports; the coordinator merges them here.

| Task | Status | Wave |
|---|---|---|
| T1 Golden suites green with attribution (PAR-0362) | **DONE** (wave 1) + batched L02/L06 regen **DONE** (wave 3, all suites green) | 1, 3 |
| T2 `ModDataManager` owner + decorator (PAR-0394) | pending | 5 |
| T3 Mod content parser TODOs | pending | 5 |
| T4 Evidence debt (PAR-0408 …) | pending | 5 |
| T5 Harness text input / focus | **DONE** (session A, 766e9cb9) | 1 |
| T6 Closing twin walk (round M) | session A | last |

## T1 — golden suites green with attribution

### Method

- Every failing assertion was traced to the commit that moved it, on golden-producing **code**
  only (no ledger history): `git log -S` / `-L` where the cause was obvious, otherwise a `git
  bisect run` over the code with the suite's own command as the predicate (scripts and logs:
  `runs/20261018-s3-L08-T1/`). Mainline merges were bisected `--first-parent`, then the merge's
  branch commit by commit; every attribution was confirmed by "old value at C^, today's value
  at C".
- Harness work: a `MIND_BLESS=1` path in every cargo-test golden harness touched
  (`blocks_golden`, `combat_golden`, `sim_core_meta`, `sim_core_determinism`,
  `logistics_golden`, `maps_editor_golden`, and the audio/input scenario writers through
  `mind_headless::paths::blessing()`); `tools/regen_goldens.sh --record <id>` re-records one
  manifest sha256 after a harness bless; `parity/golden_manifest.json` entries gained an
  optional `reasons` list (attributed regen history, parsed by `parity::golden::Golden`).
- Only the moved suite was regenerated each time; each commit body carries the reason.

### The plan's five suites

| Suite | Delta | Cause (commit, Java) | Fix | Commit |
|---|---|---|---|---|
| `blocks_golden::family_counts` | other 164→158, production 45→51 | 390b1bc3 (G02 T1): `HeatProducer` → `BlockKindData::Crafter` (`HeatProducer extends GenericCrafter`, HeatProducer.java:13); the six HeatProducer blocks (Blocks.java:1429,1452,1467,1485,1689,6792) | bless | c8d326ab |
| `audio_golden` events_sim | tick-4 `explosionArtillery` missing; `shootDuo` moved | 8e7b68b8 (H01 T1): building Pos = `tile.drawx()` (BuildingComp.java:126), bullet rows `World.toTile` = round (World.java:174, BulletComp.java:187); the scenario spawned at the half-tile centre y=68 → row 9, missing the row-8 walls. **Not** ArtilleryBulletType despawn timing: the `fuse` fixture is a Basic bullet with `hit_sound = explosionArtillery`, 40 of 100 lifetime ticks. Hit pitch 1.0→1.0638 = e414cac9 (H07 T4, BulletType.java:564) | scenario on `tile_origin`; every position = old − (4,4); bless | 0631d671 |
| `input_golden` (4 of 11 scenarios; the loop hid three) | placement table: 10 rows true→false; mobile replay + preview handoff rotation 1→0; rts_move `queue` key + unit target y=1e6 | b1e2f543 (G04 T5, World.getDarkness edge term, Build.java:229 / World.java:483-503); 4ee70402 (H02 T3, same-block canReplace, Block.java:798 / CoreBlock.java:182); 29ebd1b3 (H08 T1, InputHandler.java:1945-1946, 2467-2469); 1eaf4bed (I01 T13, InputHandler.java:340-352); 2b6d7cd1 (PAR-0157 lossless unit-id wire) | bless | 3572e1b2 |
| `logistics_golden` (2 scenarios; the loop hid one) | unloader_drain, payload_driver_throw checksums | 9c4f0e80 (H02 T14): unloader `speed = 60f/11f` (Blocks.java:2164); 4f467a42 (PAR-0139): Arc `Point2.pack` order (x high) | bless | 642dc534 |
| `maps_editor_golden` round-trip preview | 57bfde10… → 52a37ff7… | 49f0268b (PAR-0114): air's default `mapColor` is the no-draw sentinel (MapIO.java:95-103); walls on the flipped row | bless | 3b1014c8 |

### Red suites the plan did not list (all `tools/ci.sh` gates)

| Suite | Cause | Fix | Commit |
|---|---|---|---|
| `sim_core_meta::entitymeta` | b1ea8694 (Audit fix T1): transient `Building.should_consume` field (BuildingComp.java:761) | bless | 95dba852 |
| `combat_golden` (4 tests) | 8e7b68b8 (H01 T1) for basic/turret/volley (half-tile spawns), da2da9aa (G01 T3) for rail, b7d104af (G01 T4, BulletType.java:465-486 `hitTile`) for turret | tests on `tile_origin`: basic + rail reproduce the **old** goldens unchanged; volley/turret blessed (values identical at 8e7b68b8 and HEAD with the fixed test) | a25db842 |
| `sim_core_determinism` (2 tests) | ddeae35d (J08 T3): `ecs::TeamId::SHARDED` 0 → 1 (game/Team.java: `sharded = new Team(1, …)`) | bless sampled checksums + scenario `expect.checksum` | 3dc982bb |
| `run-all --tier T0` (1/4) + `run spine_many_commands` | same ddeae35d | spine_place_break a1a7b961→3e48c583, spine_determinism 57bf3097→2d61ec13, sim_core_boot eaf9472d→87ef92ca, spine_many_commands c8214320→08d1c504; `tools/mcp_smoke.py` GOLDEN_CHECKSUM updated | ee745bb3 |

No unexplained delta remained and no port behaviour was changed for T1. Every move above is
Java-correct; two were harness geometry bugs (audio, combat) fixed in the scenario/test.

### Test commands and results (end of wave 1, branch head)

```
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check                         # clean
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings  # clean
cargo test -p mind-core --no-fail-fast      # 2757 passed / 3 failed (deferred, below) / 4 ignored; all integration suites green
cargo test -p mind-headless --no-fail-fast  # 133 lib + every golden suite green
cargo test -p mind-gdext --lib              # 77 passed
mind-headless parity goldens                # 97 PASS / 0 FAIL
mind-headless run-all --tier T0             # 4/4
mind-headless run spine_place_break|spine_determinism|spine_many_commands --json   # pass
bash tools/godot.sh --headless --editor --quit --path client   # exit 0, no script errors
```

Red **by design** (L-INDEX speed rule 3: regenerate in one batch with L06):
`mind-core maps::planet::{serpulo,erekir,asteroid}::tests::*_generation_checksum`, moved by
L02 T1 (Floor.decoration, see the L02 summary). Not run this wave: the release bench gate and
the STDB checks of `tools/ci.sh` (no server/bench-relevant change).

### Deviations / notes

- Plan text vs findings: the tick-4 `explosionArtillery` was a scenario geometry bug, not
  artillery despawn timing; the "shootDuo muzzle moved per H01 T1" is the same geometry.
- The bisect tooling is kept as evidence only (not committed to `tools/`).

### Wave 3 — batched L02 T1 / L06 regen (L-INDEX speed rule 3)

Attribution: each moved suite was run at the boundary commits in a scratch worktree
(`git worktree add --detach`, shared scratch target; scripts kept out of the repo), so every
value is "old at C^, new at C, unchanged to HEAD".

| Suite | Values | Cause (commit, Java) | Commit |
|---|---|---|---|
| mind-core `maps::planet::{serpulo,erekir,asteroid}::tests::*_generation_checksum` | serpulo 55fcac31 -> b7fbf45c -> e92c6cda; erekir daf8a3e0 -> 7e34db8a -> 56a0d8a4; asteroid 463dbb70 -> b89c2922 -> 49d193e3 | 38dc36f1 (L02 T1): `Floor.decoration` from the prop initialisers (Blocks.java:705-822, Floor.java:70); then 341d9f74 (L06 T6.0): Arc `Simplex.perm` `x & 255` + unsigned shifts (javap arc.util.noise.Simplex) | 467b6081 |
| mind-core `maps::planet::tantros::tests::tantros_generation_checksum` | 7762880f -> e53960b4 | 341d9f74 only | 467b6081 |
| mind-core `maps::generators::simplex::tests::simplex_generation_checksum` | bc1c505d -> 07e2c98b | 341d9f74 only | 28a4f20c |
| `parity/golden_content.json` (`content_ids`) | block[429] `item_capacity` 8000 -> 25000 (only field) | eeedfbb1 (L06 T7.1): `Accelerator.init()` (Accelerator.java:76-85; coreNucleus requirements, Blocks.java:3173) | 7d343932 |
| `mind-core/tests/goldens/ui/campaign.json` (`harness_ui_campaign`) | checksum 26350bc2 -> a5a580c1 -> db3aff19; dump gains `accelerator_selectable` | b73b95a0 (L06 T6.1): `PlanetView.allow_campaign_rules` (Planet.java:139/221) in the hashed views; eeedfbb1 (L06 T7.1): `SectorView.accelerator_selectable` (PlanetDialog.java:414-435) | 643169f6 |
| `mind-headless/tests/golden/render/render_menu_world.json` | tile_hash b7104680 -> 276df838, flyers 5 -> 10, layer counts | 341d9f74 (the menu world is simplex terrain); 341d9f74^ output = committed file, 341d9f74 output = HEAD | 3fef57cd |

Manifest entries gained a `reasons` line each (content_ids, harness_ui_campaign,
harness_render_render_menu_world) and re-recorded sha256 (`tools/regen_goldens.sh --record`);
the inline generation checksums carry the history in a comment. No unexplained delta.
`tools/regen_goldens.sh --only content_ids` fails in a worktree (the source extractor reads a plan
file outside the repo), so the content golden was produced by the manifest's own regen command
(`mind-headless content dump --bundle parity/bundle_keys.json`).

After the batch (branch head 52e6e6c4): `cargo test -p mind-core` 2802 passed / 0 failed,
`mind-headless` 142 / 0, `mind-gdext` 77 / 0, `parity goldens` 97 PASS, `run-all --tier T0` 4/4,
`regen_goldens.sh --check` 0 FAIL, fmt + clippy clean.

## T5 — harness: text input and Java focus (session A, wave 1, commit 766e9cb9)

Record merged from session A (`runs/20261018-L-coordinator/L08-T5-record.md`).

1. Godot text entry = L01 T4 (005e203e): `tools/gmcp.py --text 'sand'`, twin `GT`. The
   runtime autoload's `godot_input text` reused one `InputEventKey` for press and release
   (buffered input delivered two releases); now a fresh press+release per character with
   `unicode`, one frame apart. Proof: typed `sand` into the Custom Game search and read it
   back; 4x `GK Backspace` cleared it (`runs/20261018-s2-L01-T4/`).
2. `tools/twin.sh` `JW x y` hovers with `java_input.py hover`, waits `TWIN_JW_HOVER_S` (0.4 s),
   then clicks (round L's `JWC` as the default; opt out `TWIN_JW_HOVER=0`). Proof
   (`runs/20261018-s2-L01-T2`, :11): pointer parked over the Godot window, `JW 230 289` opened
   Java's editor maps dialog on the first click; `JW 139 500` opened Map Info — no swallowed
   clicks.
3. `GC`/`GW` call `twin_g_pointer`: the real X pointer warps to the target (viewport coords
   scaled via `TWIN_GODOT_VIEW_W/H`, default 1152x648), then the click is injected; a failed warp
   only warns (opt out `TWIN_G_POINTER=0`). Proof: `GC 575 607` -> pointer (639,671) = window
   origin (64,64) + target, `get_mouse_position()` (575,607). Cost ~0.5 s per GC/GW, ~0.4 s per
   JW.
4. Recorded in `java-leg-learnings.md` and `open-godot-mcp-learnings.md` (2026-10-18).

Later (session A wave 2, aace6aca): Java text entry by XSendEvent (`tools/java_xkeys.py`, twin
`JT`) — fixes session B's wave-1 "JK does not reach Java on :12".
Deviation: the installed open-godot-mcp package's `godot_input` description still says "text
is broken" (outside the repo).

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0362 | close (T1): every golden suite green with attribution, including the wave-3 batched L02 T1 / L06 regen | commits c8d326ab..ee745bb3 (wave 1), 467b6081, 28a4f20c, 7d343932, 643169f6, 3fef57cd (wave 3); `runs/20261018-s3-L08-T1/` |
| chains.json recipes pinning `a1a7b96167c9718d` (spine_place_break in-engine checks, e.g. `c420`, `c422`, `k38`, the boot/save recipes) | update the canonical spine checksum to `3e48c583a94fbea4` (J08 T3 TeamId::SHARDED) | ee745bb3 |

## Done criteria (T1)

`tools/ci.sh` gates green except the documented host bench gate and the deferred
generation checksums, which are batched with L06 by plan.


---

## Continuation (later 2026-10-18 session)

| Task | Status | Notes |
|---|---|---|
| T1 goldens | **kept green** | Two content-golden moves from this session were recorded with reasons: the four `solid = true` blocks (PAR-0446) and `target-dummy configurable = true` (PAR-0380/L05 T2). `content::parity` passes. |
| T2 `ModDataManager` | pending | Not started. |
| T3 mod parser TODOs | **partial** | `drillMultipliers` now parses (`BlockSpec.drill_multipliers` → resolved `BlockDef.drill_multipliers`; `Drill.drillMultipliers`). The `UnlockableContent` fields, assembler/factory plans, weapon `parts`/effects, inline `MultiEffect` and `draw`/drawer rows remain. No fixture/test was added for the new row. |
| T4 evidence debt | pending | No Java captures in this session. |
| T6 closing walk | pending | No round-M walk. |

`parity/golden_content.json` + `golden_manifest.json` were re-recorded with
`tools/regen_goldens.sh --record content_ids`; the manifest `reasons` list now
carries both attribution lines.

---

## Continuation (2026-10-19): T2 owner/decorators + T3 parser rows merged

Two branches merged:

```
d3936804 Merge L08 T3 mod content parser rows and fixtures
3adc976f Merge L08 T2 ModDataManager owner, decorators and map assets dialog (PAR-0394)
```

### T3 — mod content parser rows (agent/l08t3)

```
fa16c8fd L08 T3: close the mod content parser rows with fixtures
143b7d5b L08 T3: mark the mod field ledger rows implemented
```

- Parser/content model support for `UnlockableContent` (`UnlockFields`),
  `drillMultipliers`, factory/assembler plans, weapon effect/draw parts,
  inline/array `EffectRef` forms and `Block.drawer` (`DrawerSpec`); `EffectRef`
  propagates through liquid/status effect fields and the view FX snapshot uses
  named ids.
- One fixture mod per row under `parity/mod_fixtures/`: `unlock/`, `drill/`,
  `plans/`, `weapon_fx/`, `effects/`, `drawer/`; boot-path assertions cover each.
- `cargo test -p mind-core --lib` → 2813 pass / 4 ignored in the branch (2823
  after the merge), clippy/fmt clean. No golden moved.
- Remaining ledger TODOs are unrelated: unit `requirements`/reconstructor
  plans, sector rules override, `spawnUnit`, runtime `register_kind!` factories.

### T2 — `ModDataManager` owner + decorators + MapAssetsDialog (agent/l08t2)

```
d65ecc54 L08 T2: ModDataManager map patches save/load and record edits (PAR-0394)
a4a1f7a2 L08 T2: MindMods data owner, image/audio/bundle decorators (PAR-0394)
2a9454b7 L08 T2: port MapAssetsDialog and its five asset views (PAR-0394)
```

- `MindMods` autoload now owns `ModDataManager` (Java `GameState.data`), boots
  before content and publishes records to the runtime atlas (`dp-`/`net-`
  regions), the live bundle, the `MindAssets` sound registry and the `MindAudio`
  stream cache.
- Map `patches` are written into `WriteContext.patches` and read back into
  `MapEditor`; record-edit APIs and `AtlasIndex::remove_region` added; the
  editor applies patches and exposes the dialog mutations (including zip).
- `client/scenes/editor/data/map_assets_dialog.gd` replaces the stub with the
  type strip/search/pane shell and patches/content/bundles/images/audio bodies.
- `cargo test -p mind-core --lib` 2812 / 4 ignored (2823 after merge),
  `mind-gdext` 80 (81 after merge), clippy/fmt clean, Godot import exit 0.
- Live smoke: `/root/MindMods` is present, `data_region_count() == 0` with no
  mods and `data_errors() == []`.

### Remaining gaps (precise)

1. Generated content sprites (`regenerateContentSprites`) only record
   `generated/*`; there is no content-icon renderer/packer, so `dp-<content>` is
   not registered and the content view uses the type glyph.
2. Per-asset warnings (`PatchAsset.warnings`, `ContentAsset.warnings/errored`)
   are a dialog-level list, not per-row warning buttons.
3. External (non-embedded) asset records loaded from a plain map save carry only
   SHA256; the decorators skip them. Exported maps (`embed_assets=true`)
   round-trip.
4. Map data application is editor-only: `MindEditor.begin_edit_map` feeds
   `patches`; `MindSimHost`/`MindCampaign` load paths do not (Java `state.data`
   is global), so playtest/new-game patches are not applied outside the editor.
5. Audio view: no music seek/elapsed label or `countPlaying`; image view:
   text-only info, no `Pixmaps.bleed`, no import-results dialog; unload restore
   has no live bundle snapshot; prompt chrome uses `AcceptDialog`/`show_text`.

### Ledger

PAR-0394: T2 owner/decorators + dialog structure landed; keep `fixing` for the
six gaps above.
