<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L08 — Mod data host, golden hygiene, evidence debt and the closing twin walk

**Goal.** Land the one recorded blocker (the `ModDataManager` client half), get every golden
suite green with attributed regenerations, pay off the evidence debt, and finish the round with
a full twin re-walk on a stable harness.

**Build.** Rust-heavy (`mind-core/src/mods`, `mind-gdext/src/assets`, golden harnesses):
iterate with `cargo check` + targeted tests; `tools/build.sh` only before in-engine checks.
Full `tools/ci.sh` runs at the wave-boundary merges, not per task.

## T1 — Golden suites green with attribution (PAR-0362) — HIGH, run first

- Five suites fail on main:
  - `mind-core` `blocks_golden::family_counts_match_golden`: 6 blocks reclassified
    `other` → `production`;
  - `mind-headless` `audio_golden::audio_events_sim`: `shootDuo` muzzle moved per H01 T1, and a
    tick-4 `explosionArtillery` event delta;
  - `input_golden`;
  - `logistics_golden`;
  - `maps_editor_golden` (preview checksum).
- Protocol: for each failing assertion, find the commit and Java citation that moved it
  (`git log -S` on the golden-producing code is fine; do **not** mine ledger history). Write
  the reason into `parity/golden_manifest.json` and the commit body, then regenerate **only**
  that suite. Use `tools/regen_goldens.sh --only <id>` for manifest oracles. The cargo-test
  goldens (`mind-core/tests/blocks_golden.rs`, `mind-headless/tests/*_golden.rs`) have no bless
  switch, so their committed files (`mind-headless/tests/golden/`) are rewritten from the
  test's dump; adding a small `MIND_BLESS=1` path to each harness is in scope.
- An unexplained delta is a bug, not a regen. Example: the missing tick-4 `explosionArtillery`
  must be explained against Java `ArtilleryBulletType` despawn timing before regenerating.
- L02 T1 (decorations) and L06 change generation and sim checksums. Land T1 first so those plans
  start from a green baseline, then regenerate their moved suites in **one batched pass after
  both plans land** (each regen still carrying its Java citation), not per plan.
- Accept: `tools/ci.sh` (merge-time gate) is green except the documented host bench gate;
  inner-loop checks stay targeted (`cargo check` + the affected test).

## T2 — `ModDataManager` owner + decorator (PAR-0394) — MEDIUM

Seams, from the J09 T4 record (`runs/20261017-s4-J09-T4/notes.md`):
1. An owner node (`MindMods` or a new `MindData` autoload; boot ordering before content) holding
   `mind_core::mods::assets::ModDataManager` (Java `GameState.data`, `core/GameState.java:47`).
2. An image decorator: runtime atlas packing/region registration (`mind-gdext/src/assets`).
3. An audio decorator: `AudioStream` registration (`mind-gdext/src/audio`).
4. A bundle merge seam.
5. Map `patches` load/save: `maps/mod.rs:463` writes `None`; fill `WriteContext.patches`, and
   read patches on load (`Logic.java:305` `state.data`).

Then port the map assets dialog (`editor/data/MapAssetsDialog.java` + `AssetView`,
`MapBundlesView`, `MapContentView`, `MapImagesView`, `MapPatchesView`), replacing the 66-line
stub `client/scenes/editor/data/map_assets_dialog.gd`. Accept: a map with an embedded image,
sound and bundle round-trips through save/load, and the dialog matches Java.

## T3 — Mod content parser TODOs — LOW

Close the `parity/ledgers/mod_fields.md` rows still marked TODO:
- `UnlockableContent` → `UnlockFields`;
- `drillMultipliers` parse;
- assembler plans;
- weapon effect/draw parts;
- inline `MultiEffect`/array effects;
- `draw` (drawer) parsing.

Use one fixture mod per row under `parity/mod_fixtures/`.

## T4 — Evidence debt — MEDIUM

1. PAR-0408: the fresh Java capture of a metal-tiles field (recipe in
   `docs/J02-terrain-rendering_summary.md`); close the item on match.
2. Legged units: covered by L04 T6.
3. The round-K UNVERIFIED rows not covered by round L (campaign Ground Zero HUD, campaign pause
   menu, unit factory config, turret firing frames): capture one Java/Godot pair each.

## T5 — Harness: text input and Java focus — MEDIUM, land in wave 1

1. Godot text entry: covered by L01 T4 (a `unicode` key path in `tools/gmcp.py`).
2. Java first-click swallow after the pointer crosses another window: make `tools/twin.sh`'s
   `JW` hover the target for ~0.4 s before clicking (round L used a `JWC` wrapper for this).
3. Godot hover: have `GC`/`GW` also move the real pointer (`GP`) by default, so hover-driven UI
   state matches what a user sees.
4. Record all three in the learnings files.

Land these with L01 T4 in wave 1 so every later check in the round uses real text input and
reliable hover/focus instead of staged signals.

## T6 — Closing twin walk (round M) — HIGH, last

Re-walk every row of `docs/L-round-twin-verification.md` §1, the J-round rows marked
UNVERIFIED in K, each L01–L07 acceptance frame, and **every exhaustive sweep deferred during
the round** (L01 T3's double opens and orientation switches, per-dialog re-opens). Use real
input throughout (real pointer, real wheel, the unicode text path). Write
`docs/M-round-twin-verification.md` in the same shape.
The coordinator applies the ledger changes listed in every L summary.

## Done criteria

T1 green. T2 landed (or a precise blocker recorded). T3/T4/T5 done. The round-M report exists,
and every L acceptance frame is MATCH or a recorded deviation.
