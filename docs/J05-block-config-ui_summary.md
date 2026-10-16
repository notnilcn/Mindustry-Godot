<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J05 — Block configuration panels and hovered-block info (wave C) — summary

Session: parity session 4 (`parity/session-4`, worktree `Mindustry-Godot-session4`),
display `:13`, bridge `7000`. Date: 2026-10-17. Wave C covers the whole plan (T1–T5).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — `ItemSelection.buildTable` widget | **done** (in-engine: item source/sorter/unloader/constructor) | `0a7f40cc` |
| T2 — `UnitFactory`/`Reconstructor` command rows | **done** (in-engine vs Java `step-277`) | `3069c447` |
| T3 — remaining bespoke panels | **part landed** (`LaunchPad` button, `MessageBlock` limit); core/dummy/accelerator/LightBlock picker recorded | `2ed429de` |
| T4 — hovered-block bars (`Building.display`) | **done for the accept families** (core/drill/battery/conduit/heat); other families recorded | `584a0e30` |
| T5 — inventory popup + item-source centre | **done** (size fix; centre already correct, verified) | `fbddeab2` |

Evidence per task: `runs/20261017-s4-J05-T{1..5}/` (frames + notes with Java citations).

## T1 — `ItemSelection.buildTable`

What landed:

- `desktop.rs::config_spec` `selection` route: per-option `uiIcon` chain, `selectionSize`
  (24/28/30), `unlockedNow()`/`isOnPlanet` flags, `pick_kind`, `selection_rows/columns`
  (`PayloadSource` 8x8), `select_scroll` (`DesktopBridge.config_scroll` + `set_config_scroll`).
- `item_selection_shape` (kind-keyed: sorter/inverted, item/liquid sources, unloaders, duct router,
  landing pad, unit cargo unload point, constructor pair, payload pair, every `UnitFactory`).
- `sim_host`: `in_campaign()`, `config_planet()` (`GameState.java:60-62,85-87`); campaign unlocks
  ask `MindCampaign.content_unlocked` (the sim registry carries no campaign tech state).
- `mind_item_selection.gd`: `black6` table, 40 px `clearNoneTogglei` cells, `uiIcon` at
  `clamp(selectionSize, 0, 40)` in a centered cell, min-check-0 `ButtonGroup`, the
  `@players.search` row above `rows * 1.5` rows, pane capped at `40 * rows` with the
  `smallPane` scrollbar, `selectScroll` restore/writeback.
- `block_config_fragment.gd`: the route draws no title/pane and closes on pick; re-tap submits
  `none` (Java's `null`).

In-engine (sandbox, real taps): item source/sorter/unloader grids = Java's `22-srccfg.png` cell for
cell; picking copper sets `{"type":"item","id":0}` and closes; re-tapping clears; the constructor
panel shows 106 blocks with the search row and `tita` filters to the three titanium blocks
(`option-tungsten`/`oxide` absent through `isOnPlanet`).

## T2 — command rows

- `UnitFactoryBuild.command` (`UnitFactory.java:227`) with Java's config semantics, the Java
  readback (`config()` = the plan's unit, `:375-378`), the plan-change command drop and the
  spawn's `command ?? defaultCommand` (`:437`); plan/command serialized like Java.
- Spec: `command_options_json` + the `commands` object (factory 40 px / separator /
  `clamp(units, 2, selectionColumns)` / no close; reconstructor 44 px / 5 columns / close);
  `MindItemCommandRow` (new) renders it; `MindIcons` gains the `resize` codepoint.

In-engine: the factory panel matches `step-277-factory-config.png`; `enterPayload` → `[0,5]` and
the panel stays open; the reconstructor's ten commands pick `repair` (`unitCommand id 1`) and close.

## T3 — see `runs/20261017-s4-J05-T3/notes.md`

Landed: `LaunchPad`'s `Icon.upOpen` button (campaign-only; opens the sector picker and calls
`set_launch_destination`), `MessageBlock` `max_length = 400`. Recorded with anchors: `CoreBlock`
team toggles (no core team config in the sim), `TargetDummy` (`int[5]` config), `Accelerator`
(J07 T4), the `LightBlock` `ColorPicker` dialog.

## T4 — hovered bars

`BarExtras` + family rows: `CoreBlock` capacity, `Drill` drillspeed, `HeatProducer`/`HeatConductor`/
`HeatCrafter` heat, `UnitFactory` progress, and the item/liquid `removeBar` families. The
player-team guard on `building_bars_json` is dropped (Java shows any building's `display`).

In-engine (real pointer): core (Health + Capacity: 13k), mechanical drill (Health/Water/Drill
Speed), battery (Power), conduit (Liquid), heat source (Heat). Remaining families are recorded in
the T4 notes with anchors.

## T5 — inventory popup + item-source centre

The inventory fragment now sizes to its content (`92x48` for two items instead of the 260x200
stub); the `ItemSource` centre (item colour / `cross-full`) was already correct and is verified.

## Gates

- `cargo fmt --all -- --check`, `cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings`
  clean.
- `cargo test -p mind-core` 2730 passed / 0 failed / 4 ignored; `-p mind-gdext` 72 passed.
- `tools/build.sh` + the headless Godot import check clean.

## Ledger (for the coordinator)

- **PAR-0092 reopens**: item sources/sorters/unloaders/constructors/factories/reconstructors now
  build the Java panels; evidence `runs/20261017-s4-J05-T{1,2}/`. Residual: the reconstructor has no
  payload-unit branch (no payload model).
- **PAR-0093?** (hovered-block info): the core's Capacity bar + the family bars landed (T4).
- New residual candidates: hover bars for the remaining families (turrets, power nodes/diodes,
  generators, reactors, overdrive/force, pumps, payload loaders, assemblers, campaign pads), the
  factory's 3-arg `units` bar, `HeatCrafter`'s 2-arg `heatpercent` label, `GenericCrafter`'s
  output-liquid bars, `TargetDummy`, `Accelerator`, the `LightBlock` picker dialog, `CoreBlock`
  team toggles, `MessageBlock` newline cap.
