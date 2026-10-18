<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round L — plan index (2026-10-17)

**Continuation:** the round was paused after wave 4; the remaining backlog and
the exact next-session start point are in
[`../L-round-continuation-handoff.md`](../L-round-continuation-handoff.md).
Read it before starting new work.

Source: `docs/L-round-twin-verification.md` (an independent twin walk after every round-J merge,
evidence in `runs/20261017-L-twin/`) plus the round-J residuals still open in `chains.json`.
Answer to "are there gaps left besides multiplayer?": **yes**. A handful are confirmed live
(layout engine, world decorations, menu state, item filter, slider/editor/planet skin). A longer
list is behaviour that round J recorded but did not port.

Shared rules: `J-environment.md` still applies in full, with the round-L additions in
`L-environment.md`.

## 1. Plans

| Plan | Title | Priority | Covers |
|---|---|---|---|
| [L01](L01-ui-layout-engine.md) | UI layout engine: min-size propagation + dialog centring | **P0** | G1: Map Play clipping (J06 T7 regressed), Map Info misplacement, empty search grid (PAR-0410), top-aligned fill dialogs; Godot text-entry test path |
| [L02](L02-world-state-fidelity.md) | World generation + game-state fidelity | **P0** | G2: floor `decoration` mapping (boulders on stone/sand/… missing in custom maps **and** generated sectors); G3: state not reset to menu (Core Database all unlocked); G4: `ItemSelection` shows hidden items; PAR-0377; PAR-0378 check |
| [L03](L03-skin-residuals.md) | Skin and presentation residuals | P1 | G5 slider skin; G6 planet orbit orientation + button column (PAR-0414); G7 editor shell skin; HUD skip button/wave wrap/hover box (PAR-0386); Game Over pane; tech-tree clip |
| [L04](L04-unit-fx-draw-residuals.md) | Unit, turret and explosion draw residuals | P1 | PAR-0365–0369 (unit parts, ability draws, layerOffset, crawl segments, payload draws), PAR-0363 turret part trees, PAR-0399 `ExplosionSink` host + generator effects, legs vs Java (PAR-0126) |
| [L05](L05-block-ui-info-residuals.md) | Block UI, hover bars, content info, hints | P1 | PAR-0379 hover bars, PAR-0380 bespoke configs, PAR-0301 stat families, PAR-0302 hints, `selectionSize` |
| [L06](L06-sim-campaign-residuals.md) | Simulation + campaign residuals | P1 | PAR-0402 (assembler gate/hand-off, router ControlBlock, suppression, laser length, tether I/O, liquid output sides), PAR-0391 (prebuildBase, allowCampaignRules, Erekir generator, abandon, land info, clouds), PAR-0296 Accelerator |
| [L07](L07-editor-logic-residuals.md) | Map editor + logic residuals | P2 | editor palette order/`spawn` (PAR-0110), WASD bindings, resetSaved, sub-dialog residue, logic font/printchar/icon tag, display/canvas, logic pickers (`EffectsDialog`) — PAR-0407 |
| [L08](L08-mods-goldens-closeout.md) | Mod data host, goldens, evidence, closing walk | P0 (T1) / last (T6) | PAR-0362 golden suites green with attribution, PAR-0394 `ModDataManager` + map assets dialog, `mod_fields.md` TODOs, PAR-0408 + other evidence debt, harness focus/hover fixes, round-M twin walk |

## 2. Schedule — two parallel sessions

Each session runs in its own worktree with its own X display and Godot editor
(`tools/parity-session.sh`); retire any other parity-session worktrees before starting.

| Wave | Session A — UI + harness (GDScript/Python) | Session B — Rust (world, sim, draw) |
|---|---|---|
| 1 | L01 T4 + L08 T5 (text input + pointer/focus harness), then L01 T1–T2 | L08 T1 (goldens green), then L02 T1–T3 |
| 2 | L01 T3 acceptance frames → L03 T1–T2 | L02 T4–T5 → L06 T1–T3 |
| 3 | L03 T3–T5 → L05 T2–T3 | L06 T4–T7 → L04 T5 |
| 4 | L07 T1–T3 → L05 T4–T5 | L04 T1–T2 → L05 T1 |
| 5 | L07 T4–T6 | L08 T2–T4 |
| last | L08 T6 (round-M closing twin walk) | — |

Each shared file belongs to exactly one session, so the waves need no cross-session merges:
- `client/ui/mind_table.gd` / `mind_dialog.gd` / `mind_scroll.gd` and `theme_builder.gd` —
  session A only (L01 lands in wave 1, before any other UI task touches them);
- `maps/filters/mod.rs` + the environment registry and `mind-gdext/src/sim_host.rs` —
  session B only (L02, L04, L05 T1, L07 T4 are sequential inside B).

The coordinator merges each session branch at its wave boundary and updates the ledger.
Sessions run the Java reference client for their acceptance frames under
`L-environment.md` rules 7–8: own seeded data dir, own display, closed whenever idle.

### Speed rules

1. **Builds follow the tree, not the task list.** Only `client/rust/**` changes need
   `tools/build.sh`. L01 and L03 are pure GDScript: no cargo build, no `.so` reload — verify in
   the running editor. For Rust tasks iterate with `cargo check -p mind-gdext` and targeted
   `cargo test -p mind-core <test>`; run `tools/build.sh` once before the in-engine check that
   needs the new `.so`, then restart the editor (`tools/session-play.sh N`).
2. **Per-task gate = targeted check + test.** `fmt`, clippy, goldens, bench, the Godot import
   check and the STDB typecheck belong to the merge-time `tools/ci.sh`, never the inner loop.
3. **Batch the checksum moves.** L02 T1 and L06 both move generation/sim goldens; regenerate
   both suites in one pass after both plans land, each regen carrying its Java citation. L08 T1
   lands first so the batch starts from green.
4. **Acceptance frames only during the round.** Each task captures the frame(s) its plan names.
   Every exhaustive re-walk — double opens, portrait/landscape switches, the full dialog sweep —
   happens once, in L08 T6.
5. **Stage L04's Java captures in one batch** before implementing any L04 task (L04 T0);
   staging is that plan's bottleneck, not its code.
6. **L01 T4 and L08 T5 land in wave 1** so every later check uses real text input and reliable
   hover/focus instead of staged signals.

## 3. Out of scope (unchanged operator decisions)

- Multiplayer: I08 T7–T13 and PAR-0121, PAR-0122, PAR-0124, PAR-0125, PAR-0151, plus the
  multiplayer-only settings rows and the `net/` classes.
- Platform: Steam, Discord RPC, mobile/iOS packaging, Web (`docs/platform/`).
- Script (JS) mods: recorded deviation.

## 4. Ledger changes proposed by round L (coordinator → `chains.json`)

| Item | Change |
|---|---|
| PAR-0409 (F-K1 grid clip/no scroll) | close: harness artefact (a real wheel scrolls, 5 columns fit), `runs/20261017-L-twin/godot/09-custom-scrolled.png` |
| PAR-0410 (F-K2 search empties grid) | confirmed → L01 T1/T3 |
| PAR-0411 (F-K3 sandbox boulders) | retarget to the decoration mapping (L02 T1); Java's sandbox has none either |
| PAR-0412 (F-K4 menu camera scale) | not reproduced → close or wontfix |
| PAR-0413 (F-K5 sliders) | drop the "extra Bloom Intensity row" clause (Java has it); keep the skin → L03 T1 |
| PAR-0414 (F-K6 planet buttons) | confirmed; add the orbit orientation → L03 T2 |
| PAR-0415 (F-K7 Tech Tree click) | not reproduced → close |
| J06 T7 item "Map Play portrait previews clip" | reopen → L01 |
| new | Floor decorations mapped by name, so stone/sand/ice/… floors get no boulders (custom maps + generated sectors) → L02 T1 |
| new | Returning to the menu leaves the session in the game phase; Core Database shows everything unlocked → L02 T2 |
| new | `ItemSelection` lists hidden items (fissile matter, dormant cyst) → L02 T3 |
| new | Editor Map Info lays out at the bottom on first open, top-aligned after → L01 |
| new | Fill dialogs top-align their content (pause, editor menu, Game Over) → L01 T2 |
| new | Planet orbit line drawn in the wrong plane → L03 T2 |
| new | Editor shell skin (tool buttons, checks, Brush row, map frame) → L03 T3 |
| PAR-0110 | note: `spawn` sorted with floors in the editor palette → L07 T1 |
| PAR-0126 | note: legs drawn, Java pairing pending → L04 T6 |

## 5. Where results go

Per plan: `docs/L<NN>-<slug>_summary.md`. Coordinator: `docs/L-round-execution-summary.md`.
Closing walk: `docs/M-round-twin-verification.md`.
