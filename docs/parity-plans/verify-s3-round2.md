<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Plan verification round 2, session 3 (2026-10-14): H04 saves/campaign loop, H06 logic

Worktree `Mindustry-Godot-session3` @ `ffbe84dd` (main; H00 not landed), bridge 6990, display `:12`.
Godot profile from a clean `--import` (`{"nodes": 375, "unlocked": 282}`), re-imported once more
between H04 and H06. Evidence: `runs/20261014-s3-verify2/` (`H04-evals.txt`, `H06-evals.txt`, PNGs,
`tools/` settings readers/patcher).

Temporary, uncommitted worktree edits (all reverted with `git checkout -- client/` and rebuilt; the
worktree is clean except the ledger symlink):
1. H00 T1 Change 1 (`planet.rs::env_bit` -> Java bits): env 233, the alpha spawns.
2. Probe seams in `gdext/sim_host.rs` shaped like H06 T1: `logic_set_code` (direct `read_compressed`
   write), `logic_set_code_cfg` (the `Configure Bytes` path), `logic_get_state` (vars via
   `format_value_world`), `logic_ui_events_debug`. These made every logic symptom checkable in engine
   and dry-ran T1's design (it compiles as written; two corrections below).

## H04 — saves + campaign loop

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 one campaign | CONFIRMED | settings `turn` 2140 -> two `run_turn()` -> 2140 -> after exit 2140 | readback via settings tool; `remove_sector_items` changes only the facade and never `save_info` (added to Change 5 note) |
| T2 statistics/produced | CONFIRMED | 32 copper delivered over 5400 ticks, record `rawProduction.copper.mean 0.0` | recipe fixed: drill (130,48) does not touch the core, conveyors (130,50..53) added |
| T3 SectorInfo.write | CONFIRMED | record patched to copper 777/lead 55, resume core = save's 40 copper; `lastPresetName null` | verify recipe rewritten (old one could not fail) |
| T4 invasions/defaults | CONFIRMED (a), rest by code | `serpulo-campaign-rules` sectorInvasion/legacyLaunchPads true; profile has no such key | `get_campaign_rules_json()` takes no arg; Erekir `clearSectorOnLose` default note |
| T5 | withdrawn (marker present) | — | — |
| T6 launch pad | CONFIRMED | pad full 2120 ticks, no launch (`H04-T6-pad-full.png`) | recipe coordinates/config verified as written |
| T7 landing pad | CONFIRMED (fake half) | team-2 pad: no water accepted, no items (`H04-T7-fake-pad.png`) | `place` needs numeric block id (428); recipe rewritten |
| T8 wave countdown | SKIP: not reproduced | `allowWaves` flag (GZ-2) -> countdown -300/300 ticks, wave 1->2, 3 units; HUD tracks; frozenForest hasCore true | PAR-0193 provable today; seam optional |
| T9 unit chunk | CONFIRMED | dagger `wet` status lost across exit-save + Go | use `wet`, not `burning` |
| T10 Java import | NOT CHECKABLE | code (`supports_class` 0xF0-0xF2) | — |
| T11 settings hub | PARTLY | flag via MindUi works on fresh 97 (masked by `live_setting_flag`) | verify step made discriminating (grep helpers gone) |
| T12 leftovers | NOT CHECKABLE | code | — |
| V1 | ran pre-T3 | replay restores core + derelicts; `waves` stays true on loss (`H04-V1-*.png`) | note on `lastPresetName` after T3 |

## H06 — logic

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 seams | CONFIRMED | `logic_set_code` false, `logic_get_state` {} | world processors not accessible in campaign -> `logic_set_code` must write directly (Change 3 rewritten); filter constants (`var_ids` keeps `true`/`false`/links) |
| T2 spawnwave | CONFIRMED | flag consumed, wave stays 1, units 1 | camera recipe (`set_process(false)`); symbols verified |
| T3 cutscene | CONFIRMED | camera stays on player, `logic_cutscene false` | all names verified |
| T4 messages | CONFIRMED | events stuck in `logic_ui_events`; localeprint yields the key | needs T1 direct write |
| T5 setrule | PARTLY | mapArea live works (GZ 256x256 -> limit off, border lit) | verify fact fixed; discriminating re-enable step added |
| T6 setprop | CONFIRMED (+1 not reproduced) | core 50 -> setprop 10 -> 60; `@flag` null; health 0 already kills | note |
| T7 GZ walk | PARTLY | steps 1/5 pass, 2/3 fail, 4 blocked by new T20 | §0 note: the profile save runs older GZ-3/GZ-5; processor tiles listed |
| T8 side effects | CONFIRMED | spawn/getblock/setblock inert | — |
| T9 ubind | CONFIRMED | `@unit` null, poly unmoved, CommandAI | file paths fixed (`executor/unit_control.rs`, `select_controller`) |
| T10 completion code | NOT CHECKABLE | code | — |
| T11 sense/fetch | CONFIRMED | fetch core/player null/0, content sense 1, building `@name`/`@id` set | Java line refs for null `@name`/`@id` |
| T12 control shoot | CONFIRMED | duo rotation 90, 0 bullets | verify via world processor + `getblock` (no link needed) |
| T13 radar | CONFIRMED | `health` order 1 returns the 50-hp dagger | — |
| T14 / T17 editor | CONFIRMED (absent) | no `logic` dialog | — |
| T15 config panel | CONFIRMED | one-line text field (`H06-T14-processor-tap.png`) | — |
| T16 overlays | NOT CHECKABLE | H03 dependency | — |
| T18 displays | CONFIRMED | drawflush draws nothing (`H06-T18-display.png`) | — |
| T19 canvas | PARTLY | base sprite only | — |
| **T20 (new)** | CONFIRMED | GZ-3/GZ-5 and frozenForest FF-1 have `@sharded`/`@duo` = null vars; re-assembly at runtime sets `duoAmmo` in 30 ticks | new task added (+ §1 row, §3 order, §4 ledger) |

## Java leg

Not started. Every Java claim checked here is source-unambiguous (cited file:line in the notes:
`CampaignRules.java:7-16`, `Planets.java:84-86,136-137`, `Control.java:451`, `Sector.java:207-231`,
`BuildingComp.java:2099-2144`, `LAssembler.java:59-61`, `ContentLoader.java:107`, `LogicBlock.java:120-122`,
`LogicDialog.java:188`). The GZ processor effects that would need observing (cutscene pan/zoom,
`spawnWave`, `duoAmmo`) only fire deep into the tutorial (>20 min of play from the seeded save, which
opens at "Obtain 0/15 Copper"), and the existing `runs/20261009-065416-ref-ground-zero/java/` frames
stop before them.

## Hiccups

- `get_campaign_rules_json("serpulo")` (wrong arity) wedged the game; `--play` recovered.
- No seam reads the persisted turn/record before H04 T1: wrote settings.bin readers/patcher
  (`runs/20261014-s3-verify2/tools/`).
- The GZ session autosaved within minutes: `mineOver`/area changes survived an editor restart.
- The Campaign Changes Notice appears a few seconds after the first sector entry (close it again).
- Escape with the block config panel open opened the pause menu and left the panel up (H08?).
- The wave fixture spawned flares that attacked the core; killed with `debug_apply_status(id,"melting",3000)`.
All logged in `open-godot-mcp-learnings.md`.

## For the coordinator

1. **New H06 T20 is a tutorial blocker** (S1): processors loaded from maps/saves never resolve
   content/team constants, so GZ `duoAmmo`/`scatterAmmo` and frozenForest `power` never fire. Schedule it
   right after H06 T1 (it may fit better in H04, which owns the load path — decide the owner).
2. **H06 T1 design change:** `logic_set_code` must bypass `accessible()` (direct write); otherwise every
   world-processor verify step in H06 (T2-T13) silently does nothing.
3. **H04 T8** can close PAR-0193 with this run's evidence; keep or drop `debug_set_wave_rules`.
4. Ledger: the profile's GZ save runs older GZ-3/GZ-5 programs than `assets/maps` (first duo/scatter only) —
   expected Java behaviour for that save, not a bug.
5. Wave composition: the GZ-2 fixture's first timer wave spawned dagger + 2 flares (H00 T2b off-by-one, already owned).
6. `remove_sector_items` (launch cost) does not persist the launcher record (H04 T1 Change 5 note);
   `Sector::add_items` also lacks Java's being-played branch (deferred list already mentions it).
