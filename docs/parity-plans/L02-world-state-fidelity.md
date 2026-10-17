<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L02 — World generation and game-state fidelity

**Goal.** Generated and filtered terrain carries the same decorations as Java. The game state
returns to "menu" when a game ends. Content filters match Java's `isHidden`. Save metadata
matches.

**Build.** Rust-heavy (`mind-core/src/maps`, `mind-gdext/src/campaign.rs`): iterate with
`cargo check` + targeted `mind-core` tests; `tools/build.sh` only before in-engine checks.

**Evidence.** Round-L report G2, G3, G4, plus PAR-0377 and PAR-0378. Frames:
`runs/20261017-L-twin/diff/42-survival-side.png`, `diff/49-hovercore-side.png` (Caldera:
Java 5–7 boulders per screen including sand boulders, Godot 1–2, no sand boulders);
`diff/28-coredb-side.png` (all blocks unlocked after a game); `diff/22-srccfg-zoom.png` (18 vs
16 items).

## T1 — Explicit `Floor.decoration` — HIGH

- Java sets `decoration` per floor inside each prop's initialiser (`content/Blocks.java:700-822`):

  | Decoration | Floors |
  |---|---|
  | `redweed` | `redmat` |
  | `pur-bush` | `bluemat` |
  | `boulder` | `stone`, `crater-stone`, `char` |
  | `snow-boulder` | `snow`, `ice`, `ice-snow`, `salt` |
  | `shale-boulder` | `shale` |
  | `sand-boulder` | `sand-floor` |
  | `dacite-boulder` | `dacite` |
  | `basalt-boulder` | `basalt`, `hotrock`, `darksand`, `magmarock` |
  | `carbon-boulder` | `carbon-stone` |
  | `ferric-boulder` | `ferric-stone`, `ferric-craters` |
  | `beryllic-boulder` | `beryllic-stone` |
  | `yellow-stone-boulder` | `yellow-stone`, `regolith`, `yellow-stone-plates` |
  | `arkyic-boulder` | `arkyic-stone` |
  | `crystalline-boulder` | `crystalline-stone` |
  | `red-ice-boulder` | `red-ice` |
  | `rhyolite-boulder` | `rhyolite`, `rough-rhyolite` |
  | `red-stone-boulder` | `red-stone`, `dense-red-stone` |

  `Floor.decoration` defaults to `Blocks.air`.
- Port: `block_info::decoration` (`mind-core/src/maps/filters/mod.rs:143-147`) guesses
  `<floor>-boulder` by name, so every floor above whose name differs gets **no** decoration.
  Consumers:
  - `default_filter_stack` (`filters/mod.rs:905-927`, the default `ScatterFilter`s for custom
    maps);
  - `BasicGenerator.decoration` (`maps/generators/basic.rs:805-840`);
  - the Serpulo generator (`maps/planet/serpulo.rs:1774`);
  - the Erekir and asteroid generators (`erekir.rs:891`, `asteroid.rs:343`).
- Change:
  1. Add `decoration: BlockId` to the floor def, filled from the table above in the environment
     registry (`content/registries/blocks/environment.rs`).
  2. Make `block_info::decoration` read the field.
  3. Parse `decoration` in the mod JSON parser (Java `ContentParser` reads it as a block name).
- Tests:
  - a `mind-core` unit test pinning all 30 mappings;
  - the default stack on a stone/sand map produces boulders on stone **and** sand at
    ≈ 0.013 density (`ScatterFilter.chance`);
  - the Serpulo sector generator golden changes. Regenerate it in the batched golden pass
    after L06 lands (L-INDEX speed rule 3), with the Java citation in the commit body (see
    L08 T1 for the golden protocol).
- Accept: a Caldera survival load shows Java's boulder density and sand boulders (same frame
  as `java/42-survival.png`). Spot-check one generated Serpulo sector against a Java capture of
  the same sector.

## T2 — Reset state to menu when a game ends — MEDIUM

- Java: `Logic.reset()` clears the groups, sets `world.tiles = new Tiles(0, 0)`, unloads
  `state.data` and replaces `state` with a fresh `GameState` (menu)
  (`core/Logic.java:299-310`). Paths that call it: `PausedDialog.runExitSave` / quit,
  `GameOverDialog`'s `@menu` (`ui/dialogs/GameOverDialog.java:134-138`), the editor exit, and
  `ui.planet` Back from an in-game launch.
- Port: after Save & Quit or Game Over → Menu, `MindCampaign.session.phase` stays `game`
  (`get_state_json().phase == "paused"`). `database_json`'s override
  `!is_campaign && phase.is_game()` (`mind-gdext/src/campaign.rs:3226`) then reports every
  block unlocked on the menu (`diff/28-coredb-side.png`).
- Change: make every return-to-menu path go through one `reset_to_menu()` (facade + sim) that
  sets the menu phase and unloads the world. Then audit every `phase.is_game()` / `is_menu`
  consumer in `mind-gdext` (`grep -rn "is_game()\|is_menu" client/rust/mind-gdext/src`) for
  menu correctness: database, hotkeys, palette, HUD, the music context.
- Test: a gdext/headless test that plays a custom map, quits, and asserts the menu phase and
  `database_json` locks. In-engine, Core Database after a quit matches the Java frame.

## T3 — `ItemSelection` honours `isHidden()` — MEDIUM

- Java `ItemSelection.java:68` skips `!unlockedNow() || !isOnPlanet(planet) || isHidden()`.
  `fissileMatter` and `dormantCyst` are `hidden = true` (`content/Items.java:128-135`). Also
  check the hidden units and blocks that appear in UnitFactory, Constructor and PayloadSource
  lists.
- Port: `config_spec` carries `unlocked`/`on_planet` but no `hidden`
  (`mind-gdext/src/input/desktop.rs:3602-3670`).
  `mind_item_selection.gd:_visible_options` filters only those two.
- Change: emit `"hidden"` per option from `UnlockFields.hidden` (or `def.hidden`) and filter it
  in the widget. Extend the existing `desktop.rs` tests (`:4107`, `:4139`).
- Accept: the item source grid shows Java's 16 items (`diff/22-srccfg-zoom.png`, left).

## T4 — Save metadata `mapname` (PAR-0377) — LOW

Java writes `state.map.name()` into the save meta (`io/SaveVersion.java:133`). Custom-game saves
in the port write `"sim"`, so Load cards read "Map: Unknown". Write the loaded map's name (or the
sector preset name in campaign).

## T5 — PAR-0378 (diagonal proximity) — LOW, verify first

Java `Building.updateProximity` uses the block's edge tiles (`Edges.getEdges(size)`), i.e. only
orthogonally adjacent buildings. Reproduce the J04 T1 staging and compare it with Java before
changing anything. The likely outcome is `wontfix` (Java behaviour), with the evidence recorded.

## Done criteria

T1–T3 landed with tests. The Caldera and Core Database frames match Java. Goldens are
regenerated with reasons. T4 and T5 are resolved. Summary in
`docs/L02-world-state-fidelity_summary.md`. Ledger: new items for G2/G3/G4, PAR-0411 retargeted
to T1, PAR-0377/0378 updated.
