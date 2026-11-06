# CAMPAIGN_GAPS.md — Serpulo campaign gaps vs upstream Mindustry

Code-only survey, 2026-11-06. Godot port `ea57f25d` vs upstream Java
`~/g/code_examples/Mindustry` @ `2cd7aeecf1`.

Scope: the **Serpulo campaign** only — the campaign chooser, planet page, sector
launch/resume/capture/loss, the sector lifecycle and rules, tech tree and launch
loadout, campaign HUD/objectives/hints, Serpulo world generation and authored
sector maps, campaign persistence, and the campaign economy (core, drills,
conveyors, unloaders, launch/landing pads). Generic combat, rendering, editor,
logic, multiplayer and mod gaps are out of scope here and live in
[`GAPS.md`](GAPS.md).

This survey used source only: no ledgers, screenshots or prior run reports were
consulted as evidence. Every finding below is derived from Java source and the
Rust port at HEAD. A later in-engine pass (Godot first, Java reference second)
verified the first-minutes findings; the results, screenshots and exact tool
chains are in [§12](#12-runtime-verification-godot-leg-then-java-reference) and
in [`TWIN-VERIFICATION.json`](TWIN-VERIFICATION.json). A fix pass then closed the
first batch of findings — see [§13](#13-fix-pass--2026-11-06).

Path shorthand: `mc/` = `client/rust/mind-core/src/`, `gx/` =
`client/rust/mind-gdext/src/`, `up/` = `~/g/code_examples/Mindustry/core/src/mindustry/`.

Status legend: **verified in-engine** (Godot + Java frames recorded),
**code-confirmed** (both sides read, no runtime pass), **likely** (one side read
or an ordering-dependent inference).

---

## 0. Priority shortlist

| # | Gap | Severity | Status |
|---|---|---|---|
| 1 | `Text`/`Shape` objective markers are never drawn: the conveyor/turret tutorial prompts and placement highlights are missing after the drill | **P0** | **FIXED** — twin-verified, PAR-0477 |
| 2 | Objective world labels are re-projected 5×/s in screen space → jitter/stutter while walking | **P1** | **FIXED** — twin-verified, PAR-0478 |
| 3 | `Build:` objective text is composed with the wrong arguments (raw id, no icon/name) | **P1** | **FIXED** — twin-verified, PAR-0479 |
| 4 | Campaign autosave fires once per hour instead of once per minute | **P1** | **FIXED** — twin-verified, PAR-0480 |
| 5 | Launching to another sector never saves the sector you leave | **P1** | **FIXED** — verified in-engine, PAR-0481 (Java leg blocked) |
| 6 | Authored sector maps skip `genfilters` (groundZero loses its scatter boulders) | **P1** | code-confirmed |
| 7 | Authored sector weather is overwritten on every fresh landing | **P1** | code-confirmed |
| 8 | Core + adjacent container/vault is not one item pool; unloader layouts dead-end | **P1** | code-confirmed |
| 9 | `SectorInfo.resources` is never computed (sector panel/core info rows empty) | **P2** | code-confirmed |
| 10 | Configure Loadout manual resource editor is inert; launch cost priced from the wrong loadout list | **P1** | code-confirmed |
| 11 | Planet page "Resume" reloads the sector from disk instead of resuming the live world | **P1** | **FIXED** — twin-verified, PAR-0490 |
| 12 | Campaign-complete overlay never appears at capture time | **P1** | code-confirmed |
| 13 | Timer objectives print ticks as `mm:ss` and never count down | **P1** | **FIXED** — in-engine verified, PAR-0492 (Java leg blocked) |
| 14 | Core-items HUD/state read the first core in entity order, not the player's core | **P1** | code-confirmed |
| 15 | A crash/unclean exit resumes with an empty core and wave 1 (sector record written only at autosave/quit) | **P1** | **FIXED** — twin-verified, PAR-0486 |
| 16 | A save loaded from the Load dialog never becomes the current slot: no autosave/exit save/playtime | **P1** | **FIXED** — twin-verified, PAR-0487 |

The rest of this document is grouped by subsystem: [§1 tutorial/first
minutes](#1-tutorial-and-first-minutes), [§2 objectives and
markers](#2-objective-graph-and-markers), [§3 sector lifecycle and
saves](#3-sector-lifecycle-and-saves), [§4 tech tree, research and
loadout](#4-tech-tree-research-and-loadout), [§5 campaign UI and
planet page](#5-campaign-ui-and-planet-page), [§6 world generation and authored
maps](#6-world-generation-and-authored-maps), [§7 campaign
economy](#7-campaign-economy), [§8 Serpulo campaign
data](#8-serpulo-campaign-data), [§9 hint
system](#9-hint-system), [§10 already at parity](#10-already-at-campaign-parity),
[§11 checked and not a gap](#11-checked-and-not-a-gap), [§12 runtime
verification](#12-runtime-verification-godot-leg-then-java-reference).

---

## 1. Tutorial and first minutes

The Ground Zero tutorial is an objective graph stored in
`up/../../../core/assets/maps/serpulo/groundZero.msav` (meta region), with the
narrative prompts carried by objective **markers** (bundle keys `gz.*`); the
top-left HUD shows the objective class text (`Objective.text()`), not `gz.*`.

| # | class | fields | parents | marker |
|---|---|---|---|---|
| 0 | Item | 15 copper | — | **ShapeText** `@gz.mine` at (1096,424) |
| 1 | BuildCount | mechanical-drill ×1 | [0] | **ShapeText** `@gz.research` at (1084,412) |
| 2 | CoreItem | 5 copper | [1] | **Text** `@gz.conveyors` at (1084,480) |
| 3 | CoreItem | 100 copper | [2] | **Text** `@gz.drills` |
| 4/5/8 | Timer | 300 / 180 / 180 ticks | [7]/[4]/[5] | **Text** `@gz.zone1..3` |
| 6 | BuildCount | duo ×2 | [9] | Text `@gz.turrets` + 2× **Shape** |
| 7 | BuildCount | copper-wall ×6 | [15] | Text `@gz.walls` |
| 9 | CoreItem | 25 lead | [3] | Text `@gz.lead` |
| 12/13/14 | Timer / DestroyUnits ×2 / Timer | — | — | Text `@gz.defend` / `@gz.finish` |
| 15/16 | Flag | duoAmmo / scatterAmmo | [6]/[11] | Text `@gz.duoammo` / `@gz.scatterammo` |

### CG-TUT-01 — Tutorial prompt disappears after the drill: only `ShapeText` markers are drawn — **P0**
- **Symptom.** After placing the Mechanical Drill the `gz.conveyors` prompt never
  appears. The top-left panel does advance to `Move into Core: 0/5 Copper`, but
  the in-world tutorial text — the thing the player actually follows — goes
  silent from objective 2 onward. Objective 6's "place turrets here" highlights
  are missing too. (This is the reported "tooltip not progressing to the
  conveyors" symptom; objective 1 *does* complete.)
- **Godot evidence.** `gx/campaign.rs:3493-3497` destructures only
  `ObjectiveMarker::ShapeText` and `continue`s on every other marker class.
  `gx/render/objective_markers.rs` (whole file) implements only the ShapeText
  polyline+label pass. Worse, `objective_markers.rs:90-92` calls
  `queue_redraw()` only when the marker list is non-empty, so when the list
  becomes `[]` the last label stays painted: the drill prompt can freeze on
  screen instead of being replaced.
- **Java evidence.** `up/game/MapObjectives.java:1094-1129` (`TextMarker.draw` →
  `WorldLabel.drawAt`), `:966` (`PointMarker`), `:1020` (`ShapeMarker`), `:1163`
  (`LineMarker`), `:1235` (`TextureMarker`), `:1301` (`QuadMarker`), `:1416`
  (`LightMarker`); `Base ObjectiveMarker.world` defaults to 1
  (`up/game/MapObjectives.java:745`); the renderer draws every marker of every
  running objective each frame (`up/core/Renderer.java:352-360`).
- **Gap.** The marker JSON + renderer is a single-class subset. Ground Zero
  objectives 2–16 lose their world prompts; objective 6 loses its Shape
  highlights.
- **Verification.** GZ: capture objective 0 (`@gz.mine`) and 1 (`@gz.research`);
  place the drill; Java shows the large conveyor WorldLabel, Godot shows
  nothing (or a frozen drill label). Probe
  `MindCampaign.objective_markers_json()` → `[]` while objective 2 is the
  running one.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0477` (similarity 72.0).** All seven marker classes are forwarded behind the `Renderer.java` `world != -1` gate and drawn by `render/objective_markers.rs` (world label, polygon ring, pulse point, gradient line, atlas texture/quad); the stale-label case is closed by the unconditional redraw. See §12 and `runs/PAR-0477/`.

### CG-TUT-02 — World prompt jitters while walking (5 Hz screen-space re-projection) — **P1**
- **Symptom.** The `Move near the :ore-copper: copper ore on the ground…` label
  shakes/stutters whenever the player moves; later promises would too.
- **Godot evidence.** `gx/render/objective_markers.rs:76-92` returns unless
  `accum >= 0.2` and issues `queue_redraw()` **inside** that gate; `draw()` at
  `:103-118` projects `camera.get_canvas_transform() * marker.pos`. The marker
  node lives in a screen-space `CanvasLayer` (`gx/render.rs:479-505`), so
  between redraws the terrain moves under a frozen label. The per-BBCode-run
  glyph layout (`objective_markers.rs:143-193`) re-derives the centring on each
  redraw as well.
- **Java evidence.** `up/core/Renderer.java:352-360` draws markers in the normal
  per-frame pass; `up/game/MapObjectives.java:1118-1130` re-projects from the
  live camera every frame.
- **Gap.** The 0.2 s gate is a JSON-poll cache, but it also gates the redraw;
  projection must run every frame (or on camera change).
- **Verification.** Hold `S`/`W` for 2 s and capture a burst; the label's
  screen position steps every ~12th frame in the port and every frame in Java.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0478`.** `process()` still polls the marker JSON at 0.2 s but redraws unconditionally, and `draw()` re-projects every marker through the live camera transform each frame. The draw-cadence probe (`marker_draw_debug()`: `draws` advances 1:1 with `Engine.get_process_frames()`) carries the claim; the static twin pair has a low similarity (13.8) because the two legs were captured after different camera displacements. See §12 and `runs/PAR-0478/`.

### CG-TUT-03 — `Build:` objective text uses the wrong arguments — **P1**
- **Symptom.** Objective 1 renders as `Build:\nmechanical-drillx\n1` (raw content
  id, missing space, no count/name/icon) instead of `Build: 1x  Mechanical
  Drill`. Research/Produce/DestroyUnits/DestroyBlocks have the same class of
  error.
- **Godot evidence.** `mc/game/map_objectives.rs:619-622` calls
  `locale.format("objective.build", &[block_id, count])`, but the bundle key
  `objective.build` is `[accent]Build: [][lightgray]{0}[]x\n{1}[lightgray]{2}`
  (three args: remaining count, emoji+space, localized name). `Bundle::format`
  drops unknown placeholders (`mc/assets/bundle.rs:303-328`), so `{2}` vanishes
  silently. Same shape at `:582-587` (Research/Produce pass only a raw id),
  `:627-629` (DestroyUnits passes total instead of remaining).
- **Java evidence.** `up/game/MapObjectives.java:442-444` —
  `format("objective.build", count - stats.placedBlockCount.get(block),
  block.emoji() + " ", block.localizedName)`; `:377` (Item), `:410` (CoreItem),
  `:501` (DestroyUnits remaining); keys at
  `up/../../assets/bundles/bundle.properties:859-867`.
- **Verification.** GZ objective 1 active; read the HUD `Status` label. Java:
  `Build: 1x <drill icon> Mechanical Drill`; Godot: raw id.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0479` (similarity 74.0).** Every objective class now passes Java's argument tuples (remaining count, `emoji() + " "`, localized name) and the HUD resolves live stats; Build/Research/Produce/DestroyUnits/DestroyBlocks are covered. See §12 and `runs/PAR-0479/`.

### CG-TUT-04 — Timer objectives print ticks as `mm:ss` and never count down — **P1**
- **Symptom.** Tutorial countdowns are wrong and frozen: objective 12
  (duration 3600 ticks, text `@objective.enemiesapproaching`) reads
  `Enemies approaching in 60:00` and never changes; Java reads `1:00`, `0:59`, ….
- **Godot evidence.** `mc/game/map_objectives.rs:649-676`: `timer_text` treats
  `o.duration` (ticks, 60/tick-second) as seconds (`seconds = duration as i32`)
  and never consults the live `countup` accumulator (which exists at
  `MapObjectivesRuntime::timer_countup`, `:242-244`). The comment at `:651-653`
  admits the count-down is missing.
- **Java evidence.** `up/game/MapObjectives.java:540-548` —
  `i = (int)((duration * objectiveTimerMultiplier - countup) / 60f)` then
  `m = i / 60; s = i % 60`; `:516-520` is the tick accumulator.
- **Gap.** Missing tick→second division and the remaining-time term.
- **Verification.** Reach objective 12 and watch the panel: Godot shows
  `60:00` statically, Java counts down.
- **Status.** **Fixed at HEAD (2026-11-06) — in-engine verified on Godot, `PAR-0492`.** `timer_text` now computes `(duration × objectiveTimerMultiplier − countup) / 60` and formats `m:ss`; a paused countup of 120 ticks renders `Enemies approaching in 58`. No Java frame of objective 12 was obtainable (the full parent chain `16→…→0` is required; the class-mod and save-transfer attempts are documented in `runs/PAR-0492/20261106-140000-run.md`), so the ledger item stays `verified`, not `twin-verified`.

### CG-TUT-05 — Prompt text ignores `.mobile` variants and map locales — **P2**
- `gx/campaign.rs:3526-3541` (`localized_marker_text`) does only
  `bundle.get(key)`. Java `MapObjectives.fetchText` checks map locales first and
  then `key + ".mobile"` (`up/game/MapObjectives.java:806-828`); bundle keys
  `gz.mine` / `gz.mine.mobile` exist at
  `up/../../assets/bundles/bundle.properties:2380-2381`.
- Non-mobile Ground Zero has `locales:{}`, so this only shows on mobile-layout
  runs or authored maps with locale overrides.
- **Status.** Code-confirmed.

### CG-TUT-06 — HUD objective block re-parses full rules JSON 5×/s; hint driver sweeps every frame — **P2**
- Two performance/cadence findings in the tutorial path:
  - `gx/ui/hud.rs:495-546` rebuilds the objective block from
    `get_rules_json()` → `serde_json::from_str::<Rules>` →
    `MapObjectivesRuntime::from_rules` every 0.2 s, then
    `gx/ui/fragments/hud.rs:502-510` assigns `set_text` unconditionally even
    when the text is unchanged. Java reads live `state.rules.objectives`
    through a lazy `Label` (`up/ui/fragments/HudFragment.java:1073-1104`).
  - `gx/ui/fragments/hints.rs:139-140,169-171` calls `hint_state_json`
    (buildings+grid sweep) and `hint_finished` (a settings read) for all 26
    hints **every frame**. Java uses cached team counters and memoises
    `DefaultHint.finished` (`up/ui/fragments/HintsFragment.java:62-68,352-360`).
- Not a text-correctness bug; no screenshot delta. **Status.** Code-confirmed.

See also [§9 hint system](#9-hint-system) for the predicate deltas.

---

## 2. Objective graph and markers

The runtime executor itself is close to Java: `qualified`/`dependency_finished`
(`mc/game/map_objectives.rs:246-264`), flag application and `completionLogicCode`
queueing (`:347-374`), persisted `completed` + Timer `countup` restore
(`:181-198`), and the tick system that copies result data back into
`rules.objectives` (`mc/game/runtime.rs:713-760`) all mirror upstream. The gaps
are in presentation and in the marker class set.

### CG-OBJ-01 — Only `ShapeText` markers reach the renderer
See [CG-TUT-01](#cg-tut-01--tutorial-prompt-disappears-after-the-drill-only-shapetext-markers-are-drawn--p0).
Also affects custom maps, the editor's marker preview and any objective that
uses `Point`, `Shape`, `Text`, `Line`, `Texture`, `Quad` or `Light` markers
(`gx/campaign.rs:3493-3497`, `gx/render/objective_markers.rs`).

### CG-OBJ-02 — Marker label redraw is tied to the 0.2 s JSON poll
See [CG-TUT-02](#cg-tut-02--world-prompt-jitters-while-walking-5-hz-screen-space-re-projection--p1).

### CG-OBJ-03 — Objective text formatting family
Beyond the `Build:` bug above, these objective classes pass raw ids or totals
where Java passes localized, icon-prefixed strings and remaining counts:
- `Research`/`Produce` (`mc/game/map_objectives.rs:582-587`) — raw content id,
  no `emoji()`, no localized name; Java `MapObjectives.java:350-380`.
- `DestroyUnits` (`:627-629`) — passes `count`, Java passes
  `count - stats.enemyUnitsDestroyed` (`MapObjectives.java:501`).
- `DestroyBlocks` (`:636-646`) — passes total positions, not remaining.
- `Timer` — [CG-TUT-04](#cg-tut-04--timer-objectives-print-ticks-as-mmss-and-never-count-down--p1).
- `Flag`/`CommandMode`/`DestroyCore` use the right keys
  (`:641-656`), but no content-icon/format pass runs over them in the HUD path.
- **Verification.** Compare the HUD `Status` text for each objective class
  against the Java reference screenshot at the same save state.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0479`** (the `Build:` case is the recorded pair; Research/Produce/DestroyUnits/DestroyBlocks come from the same argument-tuple pass and unit tests).

---

## 3. Sector lifecycle and saves

Headline: the port **runs** the campaign logic, not just a read model.
`Universe.runTurn` (`mc/game/universe.rs:547-816`), `checkGameState` +
`sector_capture` (`mc/game/play.rs:583-700`), auto turns
(`mc/game/runtime.rs:519-530`), `SectorInfo.prepare/write`
(`mc/game/sector.rs:434-509`), lost-sector `SectorDamage(1f)` replay
(`mc/sim/runtime.rs:2130-2205`, `gx/campaign.rs:1042`), abandon
(`mc/game/runtime.rs:314-325`) and invasion scheduling
(`mc/game/universe.rs:725-816`) are wired to the live sim. The gaps below are
state/ordering deltas, not a missing subsystem.

### CG-SECTOR-01 — Campaign autosave fires once per hour instead of once per minute — **P1**
- **Symptom.** Play a sector for 10–50 minutes, crash or force-quit: no autosave
  was written; the load dialog's sector save is up to an hour stale, and every
  recovery path that uses it (lost-base replay, sector hand-off) starts from
  hour-old data.
- **Godot evidence.** `mc/game/saves.rs:258` accumulates
  `delta_ticks / 60.0` (**seconds**), then `:264` computes
  `interval = save_interval_seconds * 60.0` (seconds²) and compares
  `self.time > interval` (`:265`). Called with the raw `saveinterval` default
  60 (`gx/campaign.rs:534`, `saves.rs:32`), so the threshold is 3600 s. The
  comment at `saves.rs:259-263` wrongly asserts the setting is minutes, and the
  unit test at `saves.rs:664-712` bakes the error in.
- **Java evidence.** `up/game/Saves.java:217-219` — `time += Time.delta` is in
  **ticks** (Arc `Time.toSeconds = 60`), and the test
  `time > saveinterval * 60` is `60*60` ticks = 60 s; the slider is labelled
  seconds (`up/ui/dialogs/SettingsMenuDialog.java:373`).
- **Gap.** 60× too slow (default 1 h vs 1 min).
- **Verification.** Fresh Ground Zero, sit unpaused for 2 min: Godot writes no
  autosave; Java rewrites `sector-serpulo-170.msav` after 60 s.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0480` (similarity 69.9).** `update_policy` now compares the seconds accumulator against `saveinterval` (seconds), so the default is 60 s; the two wrong-unit tests were rewritten. See `runs/PAR-0480/`.

### CG-SECTOR-02 — Launching to another sector never saves the sector you leave — **P1**
- **Symptom.** Build in sector A, launch to B, return to A: A reloads from the
  last autosave. With CG-SECTOR-01 that is up to an hour of building gone
  (SectorInfo item snapshots survive; buildings/units do not).
- **Godot evidence.** `gx/campaign.rs:919-937` (`start_sector_with_loadout`)
  clears `current_map_name`/`playtesting_map`, mirrors the runtime campaign and
  replaces the runtime — no save of the outgoing slot.
  `mc/game/saves.rs:347-371` then repoints `Saves.current` at the destination.
  The only pre-switch save is app exit (`gx/campaign.rs:3934-3954`).
- **Java evidence.** `up/core/Control.java:437-441` — every `playSector` starts
  with `if(saves.getCurrent() != null && state.isGame()){ saves.getCurrent().save();
  saves.resetSave(); }`.
- **Gap.** Missing save-before-switch.
- **Verification.** Place 3 conveyors + a turret, launch to a neighbour, return:
  Java keeps them; Godot reverts to the last autosave.
- **Status.** **Fixed at HEAD (2026-11-06) — in-engine verified on Godot (`PAR-0481`); ledger `verified`, not twin.** `MindCampaign.save_outgoing_sector()` writes the live slot before `MindSimHost.load_sector`, and the accelerator pump does the same before its load (ordering is load-bearing: both callers pre-load the destination). The Java leg is genuinely unreachable — the reference profile's neighbour (Frozen Forest, 63) renders "Locked". See `runs/PAR-0481/`.

### CG-SECTOR-03 — `SectorInfo.resources` is never computed — **P2**
- **Symptom.** The sector panel never lists what a sector contains; the resource
  icon row (`@sectors.resources`) and the core's "Resources on this sector" line
  never appear.
- **Godot evidence.** `mc/io/json/rules.rs:1057` defines `resources:
  Vec<String>`, defaulted empty at `:1109`; no writer exists in
  `mind-core`/`mind-gdext` (readers only: `mc/ui/campaign.rs:1054-1056`,
  `gx/ui/dialogs/batch_campaign2.rs:3517`, `gx/ui/dialogs/batch_c.rs:2215`).
  `SectorInfoState::prepare` (`mc/game/sector.rs:434-479`) does not fill it.
- **Java evidence.** `up/core/World.java:306-329` (`setSectorRules`) scans every
  non-dark tile for floor/overlay/wall drops and liquids, then
  `sector.info.resources = content.toSeq()` sorted; `up/type/Sector.java:105-118`
  does the water/null fixups.
- **Verification.** Land, save, reopen the planet page → stats dialog; Java has
  an icon row, Godot has none.
- **Status.** Code-confirmed. (Same finding as CG-GEN-03/CG-DATA-02.)

### CG-SECTOR-04 — First wave is timed from the outgoing sector's rules — **P2 (likely)**
- **Symptom.** The opening grace timer does not match the destination's
  `waveSpacing`; a fresh launch after a high-threat sector starts from the old
  spacing.
- **Godot evidence.** `mc/game/play.rs:396` `apply_sector_preset_rules` never
  touches `wave_spacing`; `logic_play` computes `wavetime` at `:505-514` from
  the current `rules.wave_spacing`; the destination map rules only arrive later
  in `gx/campaign.rs:1141-1160` (`adopt_map_rules`), which replaces
  `rules.wave_spacing` but not `session.wavetime` (the resumed path restores
  `wavetime` separately at `:1113-1116`).
- **Java evidence.** `up/core/Control.java:573-587` loads the sector (generator
  sets `waveSpacing`/`winWave`) **before** `logic.play()`, whose
  `state.wavetime = 2 * waveSpacing * difficulty.waveTimeMultiplier`
  (`up/core/Logic.java:272`) therefore reads destination rules.
- **Gap.** Ordering inversion; no re-derive after `adopt_map_rules`.
- **Verification.** Fresh launch, read `get_hud_state().wavetime` vs
  `2 * rules.waveSpacing` from `get_rules_json()`.
- **Status.** Likely.

### CG-SECTOR-05 — Off-play capture/loss toasts are absent — **P2**
- `gx/campaign.rs:1864-1868` documents the missing seam;
  `toast_campaign_events` (`:1871-1879`) handles only `SectorInvasion`, and
  `CampaignEvent` (`mc/game/universe.rs:281-300`) has only
  `Turn`/`SectorInvasion`. Java toasts `sector.capture` / `sector.lost` for
  non-played sectors (`up/ui/fragments/HudFragment.java:261-275`).
- **Verification.** Force an off-play loss/invasion turn; watch the toast list.
- **Status.** Code-confirmed.

### CG-SECTOR-06 — Planet/launch views can read a stale facade campaign copy — **P2 (likely)**
- `gx/campaign.rs:3303-3306` (`campaign_views_json`) reads `self.campaign`;
  `pump_runtime` mirrors it only when events fire (`gx/campaign.rs:791-812`).
  Live per-tick deltas are folded into `runtime.campaign`
  (`mc/game/runtime.rs:600-612`), so item/production/`beingPlayed` rows can lag
  between events. The record-level getter does prefer the runtime copy
  (`gx/campaign.rs:2056-2068`). Java has one live `Sector.info`
  (`up/game/Universe.java:193-261`).
- **Verification.** With a base live, compare
  `campaign_views_json("serpulo")` vs `sector_record_json("serpulo", id)` for
  the same sector; a turn should resynchronise.
- **Status.** Likely.

### CG-SECTOR-07 — Abandon loses the sector's live world without the pre-abandon save
- **Symptom.** `Abandon` from the pause/planet flow ends the sector as a loss;
  the live world (and anything built since the last autosave) is not written
  before the reset, so returning to the sector replays the pre-abandon
  `SectorDamage` world rather than the abandoned one. Java's abandon dialog
  runs `state.getSector().saveInfo()`/`Saves` through the normal exit path
  (`up/ui/dialogs/PlanetDialog.java` / `PausedDialog` abandon branch).
- **Godot evidence.** `mc/game/runtime.rs:314-325` abandons the session;
  `gx/campaign.rs`'s abandon handler does not call `run_exit_save` first (the
  only exit save is `:3934-3954`).
- **Verification.** Build, abandon, re-land: count the blocks.
- **Status.** Code-confirmed from the lifecycle read; not separately frame-tested.

### 3.1 Persistence (saves)

The save codec and campaign slot plumbing are largely ported; the gaps are
records that are written too late (or never), and slot identity being derived
from file names instead of save meta.

#### CG-SAVE-01 — A crash/unclean exit resumes the sector with an empty core and wave 1 — **P1**
- **Symptom.** Launch a sector, mine/build/advance waves, then kill the process.
  Next boot the planet map still offers the base and Resume loads the right
  terrain/buildings, but the core is empty and the wave counter is back at 1:
  the launch record (`items`, `wave`, `attempts`, `spawnPosition`,
  `storageCapacity`, `bestCoreType`) was never written. Clean quits work;
  crashes expose the whole first session.
- **Godot evidence.** `gx/campaign.rs:1415` finishes `start_sector` with
  `persist()`, which writes planet rules/stats + universe but no sector record.
  `persist_sector_record_in` (`:4041-4056`) is the only `SectorInfo::prepare`
  caller and is reached only from autosave (`:559`), capture (`:609`),
  game-over (`:638`) and `reset_to_menu` (`:4009`). On resume,
  `record.info.write_to_state(...)` (`:1267`) and
  `write_record_items_to_core` (`:1316`) read that still-default record;
  `gx/sim_host.rs:7998` clears the core module first. Defaults are
  `has_core: true`, empty `items`, `wave: 1` (`mc/io/json/rules.rs:1102-1114`).
- **Java evidence.** `up/core/Control.java:587` calls
  `saves.saveSector(sector)` on every `playNewSector`; `Saves.java:253-264,
  330-345`; `up/io/SaveVersion.java:117-118` prepares/saves the sector info on
  every save's `writeMeta`; `up/core/Logic.java:79-85` writes it on
  `SaveLoadEvent`.
- **Verification.** Ignore the unrelated autosave unit bug: kill the game after
  a few minutes (not via the menu), relaunch, Resume; the core's `ItemModule`
  is empty and `wave == 1` while the tile save is intact.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0486` (similarity 68.4).** `start_sector`'s fresh-launch branch now calls `persist_sector_record()` before queuing the launch save, mirroring `SaveVersion.writeMeta`'s `info.prepare + saveInfo`; the record carries spawnPosition/lastPresetName/bestCoreType/storageCapacity/attempts. A `kill -9` + Resume now keeps the record items and wave. See `runs/PAR-0486/`.

#### CG-SAVE-02 — Loading a save never makes it the current slot — **P1**
- **Symptom.** Load any save from the Load dialog, play, quit: nothing is
  written (no autosave, no exit save), playtime does not advance, and the pause
  label reads `@quit` instead of `@save.quit`.
- **Godot evidence.** `gx/campaign.rs:3878-3911` `load_slot` applies meta and
  sets `total_playtime` but never `Saves::set_current_by_file`;
  `mc/game/saves.rs:235-262` gates playtime and autosave on
  `self.current.is_some()`; `gx/campaign.rs:3934-3953` `run_exit_save` requires
  `get_current()`.
- **Java evidence.** `up/game/Saves.java:319-324` (`SaveSlot.load` sets
  `current = this`); `:204-231` (playtime/autosave keyed on `current`);
  `up/core/Control.java:599-610` (exit saves `getCurrent()`).
- **Note.** Campaign `start_sector` avoids this because it calls
  `set_current_by_file` (`gx/campaign.rs:1392-1395`); the bug bites
  custom-game/imported saves and every Load-dialog load.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0487` (similarity 70.9).** `load_slot` now calls `adopt_loaded_slot` (`Saves::set_current_by_file` + meta playtime), mirroring `SaveSlot.load`; the pause menu reads `Save & Quit` and the exit save writes the loaded slot. Unit test `loaded_slot_becomes_current_and_autosaves`. See `runs/PAR-0487/`.

#### CG-SAVE-03 — Resuming a base wipes the stored launch loadout/resources — **P2**
- `gx/ui/dialogs/batch_c.rs:2930-2945` calls `clear_loadout_info` for every
  plain `start_sector` branch, including a resume; Java only clears in the
  free-launch branch (`up/ui/dialogs/PlanetDialog.java:1396-1403`, resume at
  `:1449`). Clears `launch-resources-seq` + `lastloadout-<core>`
  (`mc/game/universe.rs:134-141`). **Status.** Code-confirmed.

#### CG-SAVE-04 — Resume forgets camera position, controlled unit and control groups — **P2**
- `mc/io/sim_io.rs:1222-1274` writes only wave/tick/wavetime/stats/rules/
  playerteam/nocores over the placeholder meta; `mc/io/save/versions/v1.rs:1242-1268`
  keeps `viewpos "(0,0)"`, `controlledType null`, `controlGroups null`, `mods
  "[]"`, `sectorPreset ""`. Java writes/reads all of them
  (`up/io/SaveVersion.java:139-146,170-183`). **Status.** Code-confirmed.

#### CG-SAVE-05 — Sector saves are attached by canonical filename only — **P2**
- `gx/campaign.rs:296-303` derives `sector.save` from
  `NativeFs.exists(paths.sector_save(planet, id))`; `assign_sector_saves`
  (`mc/game/saves.rs:149-223`) only plans remaps and never sets `sector.save`.
  Java associates from save meta (`up/game/Saves.java:120-186`), so a copy under
  another name still resumes. **Status.** Code-confirmed.

#### CG-SAVE-06 — A base is inferred from file existence, not a readable save — **P1 (likely)**
- With a sector file present but undecodable, the planet page offers Resume;
  `gx/sim_host.rs:3475-3545` falls through to `generate_sector` silently and the
  stale record items are stocked into the new core. Java catches
  `SaveException`, deletes the slot and shows `@save.corrupted`
  (`up/core/Control.java:555-561`). The same asymmetry hides such files from
  `Saves.slots`/clear paths (`mc/io/save/slot.rs:305-314`). **Status.** Likely.

#### CG-SAVE-07 — Beta megabase cleanup deletes the save but leaves the record — **P2**
- `mc/game/saves.rs:129-147` deletes the file only; Java also calls
  `getSector().clearInfo()` (`up/game/Saves.java:50-62`). Old profiles show
  captured ghost sectors with no resumable save. **Status.** Code-confirmed.

---

## 4. Tech tree, research and loadout

Serpulo node data is fully wired: 223/223 nodes resolve with Java-identical
depths (`mc/content/tech/mod.rs:886-888`), and `research_tree_json` feeds the
graph (`gx/campaign.rs:2500-2660`). The gaps are launch-loadout and
dependency-objective behaviour.

### CG-TECH-01 — Configure Loadout manual resource editor is inert — **P1**
- **Symptom.** In `LaunchLoadoutDialog`, turn `@resources.max` off, press
  `@resources`, edit amounts, confirm: the amounts are not carried into the
  launch and the dialog rows never refresh.
- **Godot evidence.** `gx/ui/dialogs/batch_campaign2.rs:2879` calls
  `campaign.has_method("set_launch_resources")`, but `MindCampaign` has no such
  method (only `update_launch_resources_json`, `gx/campaign.rs:5068`);
  `loadout_seed` (`:2541`) reads `campaign_json("get_launch_resources")`, which
  returns a `PackedInt32Array` (`gx/campaign.rs:5020`) while `campaign_json`
  casts to `GString` (`gx/ui/dialog.rs:405`); and `loadout_changed` (`:2886`,
  declared `gx/ui/dialog.rs:825`) has no listener, so the parent launch dialog
  is never rebuilt.
- **Java evidence.** `up/ui/dialogs/LaunchLoadoutDialog.java:138-141` round-trips
  the edited `ItemSeq` through `Universe.updateLaunchResources` and calls
  `update.run()` + `rebuildItems.run()` on hide;
  `up/ui/dialogs/LoadoutDialog.java:76-83`.
- **Verification.** Edit copper +10 in the resource editor, close; read
  `MindCampaign.get_launch_resources()` — unchanged in Godot.
- **Status.** Code-confirmed.

### CG-TECH-02 — Launch cost is priced from the planet's default-core loadout list — **P1**
- **Symptom.** Once the launcher's best core is foundation/nucleus, the picker
  offers those schematics but only the staged resources are charged; the
  schematic's block requirements are never removed from the source sector.
- **Godot evidence.** `gx/ui/dialogs/batch_c.rs:2693` searches
  `campaign_section("loadouts")`, built by `mc/ui/campaign.rs:1719-1722,1746`
  from `planet.default_core` only; the picker at `gx/campaign.rs:5304-5320`
  offers every schematic whose core fits the launcher. So
  `launch_cost = planet_loadout_launch_stacks(...)` (`batch_c.rs:2858`) sees an
  empty row for larger-core schematics.
- **Java evidence.** `up/ui/dialogs/PlanetDialog.java:1405-1412` selects the
  launcher core (or planet default when schematics are disallowed) and charges
  `loadout.requirements()` + `universe.getLaunchResources()` for the actual
  stored loadout.
- **Verification.** Core-nucleus launcher; note `Sector.items()`; pick a
  nucleus schematic; launch. Godot keeps the schematic's copper/titanium; Java
  removes them.
- **Status.** Code-confirmed.

### CG-TECH-03 — Implicit dependency `Research` objectives are missing — **P2**
- Java `TechTree.java:129-136` adds a `Research` objective for every
  `content.getDependencies()` entry (build-cost items, non-optional item
  inputs, producing reconstructor). Rust materialises only
  `researchRequirements` + explicit `NodeObjective`s
  (`mc/content/tech/mod.rs:295-350,485`); `Block::get_dependencies` has no
  production caller (`mc/content/registries/blocks/mod.rs:2900`).
- Concrete case: `silicon-smelter` consumes coal but does not cost it; Java
  gates research on coal being unlocked, Rust does not.
- **Verification.** Hover `silicon-smelter` / `plastanium-conveyor` in Research:
  Java lists the extra dependency rows.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0497` (similarity 77.8).** `tech/mod.rs` now builds `content_dependencies()` (`BlockDef::get_dependencies` = cost items + non-optional `ConsumeItems`; `UnitType::get_dependencies` = reconstructor + `researchRequirements`), deduped and prepended as `Research` objectives before the explicit ones (roots too). `BlockDef::get_dependencies` finally has a production caller. `parity/golden_content.json` was regenerated (tech `objectives` only). See `runs/PAR-0497/`.

### CG-TECH-04 — Last-used loadout is not restored — **P2**
- `gx/campaign.rs:5173` seeds the dialog from the core default
  (`loadout_index.filter(valid).or(default_index)`); `get_last_loadout`
  (`:5358`) has no caller. Java seeds from `universe.getLoadout(core)` /
  `LaunchLoadoutDialog.java:162`, persisted as `lastloadout-<core>`.
- **Status.** Code-confirmed.

### CG-TECH-05 — `allowLaunchSchematics` is ignored by the launch picker — **P2**
- Only definitions exist (`mc/content/registries/planets.rs:287`,
  `sectors.rs:136`, `mc/game/sector.rs:698`); the `mind-gdext` launch path
  (`gx/campaign.rs:5120,5320-5336`) always uses the launcher's best core and the
  full schematic grid. Java `PlanetDialog.java:1405` +
  `LaunchLoadoutDialog.java:167-196` gate on the destination.
- **Status.** Code-confirmed.

### CG-TECH-06 — Tech-tree root-selector button is always shown and no-ops with one tree — **P2**
- `gx/ui/dialogs/batch_campaign2.rs:1542` uses
  `visible = !tree.root.is_empty()`; `research_open_root_selector` returns early
  when `roots.len() <= 1` (`:983-990`). Java's visibility is the count of
  selectable roots (`up/ui/dialogs/ResearchDialog.java:108`). Fresh Serpulo
  profile shows a dead "Serpulo" button.
- **Status.** **Refuted at HEAD (2026-11-06) — in-engine verified on both clients, `PAR-0496` (similarity 78.6).** Erekir's root content `core-bastion` is `alwaysUnlocked` (`Blocks.java:3196`, ported `storage.rs:120`), so Java's `roots.count(!(requiresUnlock && !unlockedHost))` is **2** and the Java dialog shows the same title button; the port's `research_tree_json("serpulo").roots` is `[serpulo, erekir]` (2 rows), so its picker opens too. The dead-button case needs a genuinely single/locked root (a mod/MP state). Java's exact predicate (`root_select_visible(rows, selected_root_locked) = rows − selected_locked > 1`) and a unit test are landed anyway, but there is no vanilla user-visible gap. See `runs/PAR-0496/`.

### CG-TECH-07 — `selectable` uses the SP unlock flag while `locked` uses `Rules.researched` — **P2 (MP)**
- `gx/campaign.rs:2561-2566` (`selectable` = `content_unlocked`, SP) vs
  `:2770` (`locked` = `unlocked_with_rules`). Java uses `unlockedHost()` for
  both (`up/ui/dialogs/ResearchDialog.java:403-406`,
  `up/ctype/UnlockableContent.java:274-286`). In a host-authoritative campaign a
  node can render unlocked yet reject the click.
- **Status.** Code-confirmed.

---

## 5. Campaign UI and planet page

### CG-UI-01 — "Resume" reloads the sector from disk instead of just resuming — **P1**
- **Symptom.** While playing, open the planet page, click your sector, press
  Resume: the port re-enters from the last save, discarding everything since
  (and resetting the wave timer). Java hides the page and continues the live
  world.
- **Godot evidence.** `gx/ui/dialogs/batch_c.rs:2581` routes `"resume" | "go"`
  to `planet_launch(..., resume=true)`; `:2815` unconditionally calls
  `prime_sector_load` (`:2835`) and `host.load_sector` (`:2881`) before the
  `resume && play_sector` branch (`:2894`). `gx/sim_host.rs:3459`
  (`load_sector_world`) drops the live runtime and reads
  `user://saves/sector-<planet>-<id>.msav`.
- **Java evidence.** `up/ui/dialogs/PlanetDialog.java:1373` —
  `if(sector.isBeingPlayed()){ hide(); return; }`.
- **Verification.** Place one block (before an autosave), press N, click the
  highlighted sector, Resume: Godot loses the block and the tick resets; Java
  keeps it.
- **Status.** **Fixed at HEAD (2026-11-06) — twin-verified, `PAR-0490` (similarity 38.5; the pair differs in globe zoom/progress, not the panel).** `planet_sector_action_pressed` short-circuits Resume on the sector being played (`planet_sector_is_being_played` via `get_sector_state`) and only closes the page, mirroring `PlanetDialog.playSelected`; no `load_sector`/`start_sector` reload. Probe: tick 13118→13246 with playtime advancing = continuity. See `runs/PAR-0490/`.

### CG-UI-02 — Planet globe draws no vulnerability/shield/import-export arcs and no "you are here" fill — **P1**
- **Symptom.** On Serpulo with 2+ bases the player cannot see which base is
  exposed to an enemy base, which sectors are shielded, where exports go, or
  which sector they are currently playing (besides the selection ring).
- **Godot evidence.** `gx/ui/planet.rs:1125` `rebuild_markers` only builds
  rings/fills/icons (`:1202` `status_color`, `:1288` `add_icon`); no arc
  primitive exists. `mc/ui/campaign.rs:169` `SectorView` exposes only
  `shielded: bool`; `shield_target` stops at `mc/game/sector.rs:556`.
- **Java evidence.** `up/ui/dialogs/PlanetDialog.java:503-545` —
  `renderOverProjections` (shield arc, vulnerable-sector attack arc under
  `sectorInvasion`, import/export arcs); `:481-486` fills the current sector.
- **Verification.** Planet page with a base and a neighbouring enemy base: Java
  draws the red arc + gold current-sector fill; Godot draws none
  (`/root/Spine/Planet/PlanetView/SectorMarkers` has no arc child).
- **Status.** Code-confirmed.

### CG-UI-03 — Hovering an unlandable sector shows no reason — **P1**
- **Symptom.** Hovering a locked numbered sector shows nothing; Java shows
  `⊗ Core Foundation required` or the defense icon + "Shielded" under the
  cursor. New players read the map as broken.
- **Godot evidence.** `gx/ui/dialogs/batch_c.rs:3385` `planet_update_hover` sets
  the label only when `sector_tile == tile && !row_bool(&sector, "locked")`
  (`:3403`); `gx/ui/planet.rs:1646` hides empty labels. Neither
  `sector.foundationrequired` nor `sector.shielded` is referenced anywhere in
  `client/rust/**` (both keys exist in `parity/bundle_keys.json`).
- **Java evidence.** `up/ui/dialogs/PlanetDialog.java:962-976`
  (`getLockedText`) and
  `up/maps/planet/SerpuloPlanetGenerator.java:114-123` (the two strings).
- **Verification.** Hover a numbered sector adjacent to your base; screenshot
  the cursor label.
- **Status.** Code-confirmed.

### CG-UI-04 — Campaign-complete overlay never appears at capture time — **P1**
- **Symptom.** Capturing Serpulo's last sector does nothing in-game in the port;
  Java shows the campaign-complete overlay ~2 s later, with the planet name,
  playtime and `@menu`/`@continue`.
- **Godot evidence.** `gx/campaign.rs:612` emits `campaign_complete`, but no node
  connects it; the only presentation path is
  `gx/ui/dialogs/batch_c.rs:1602` (`planet_maybe_show_campaign_complete`),
  queued from opening the planet page (`:1560`).
- **Java evidence.** `up/core/Control.java:175-181` schedules
  `ui.campaignComplete.show(e.sector.planet)` 2 s after an initial final-sector
  capture; `up/ui/dialogs/CampaignCompleteDialog.java:30-44` (title + playtime
  + slide/fade-in).
- **Verification.** Capture the final sector; Godot shows nothing until the
  planet page is opened.
- **Status.** Code-confirmed.

### CG-UI-05 — Loss on Normal never offers the difficulty guide — **P2**
- `gx/ui/dialogs/batch_b.rs:1308` (`continue_pressed`) only closes the game-over
  dialog and opens the planet page; no `difficultyguide` setting/string exists
  in `client/rust/**`. Java `GameOverDialog.java:107-125` shows the one-time
  `@difficulty.guide.*` prompt after 2 attempts on Normal and links into the
  difficulty dialog.
- **Status.** Code-confirmed.

### CG-UI-06 — Accelerator destination picker lacks its mode chrome — **P2**
- Java `PlanetDialog.java:721-728` shows the `@sectors.launchselect` banner in
  `planetLaunch` mode, hides `@techtree` (`:316`) and fades the sector panel
  (`:369-370`). Godot `gx/ui/dialogs/batch_c.rs:2014,2058` always builds
  Back + panel + Tech Tree; only the placeholder `@sectors.select` exists
  (`:2100`).
- **Status.** Code-confirmed.

### CG-UI-07 — Escape closes the planet page instead of clearing the sector selection — **P2**
- `gx/ui/dialogs/batch_c.rs:3335` `planet_handle_reveal_escape` returns unless
  `new_presets.len() > 1` and never clears `selected_sector`; `:561`
  `blocks_escape` only covers `chooser_active`, so `gx/ui/root.rs:1675-1683`
  closes the dialog. Java `PlanetDialog.java:91-102` deselects first, then
  hides.
- **Status.** Code-confirmed.

### CG-UI-08 — No "available" toast when a capture makes tech nodes available — **P2 (likely)**
- Captures toast only unlocked content (`gx/campaign.rs:836-853`); Java also
  toasts every node whose `SectorComplete` objectives just became satisfied but
  still needs items (`up/core/Control.java:161-174`, bundle key `available`).
  No `"available"` lookup exists in `client/rust/**`.
- **Status.** Likely.

---

## 6. World generation and authored maps

Generated-sector geometry and authored `.msav` parity are separated in each
finding.

### CG-GEN-01 — Authored sector maps skip every `genfilters` boulder/prop pass — **P1**
- **Symptom.** Ground Zero in the first minutes is missing its authored scatter
  (shale/snow/sand/basalt boulders); `fungalPass`/`taintedWoods`/`coastline`
  lose ~30 scatters each. Terrain is walkable/buildable where Java has solid
  boulders.
- **Godot evidence.** `gx/sim_host.rs:10675` `load_preset_map_grid` →
  `load_grid_file` (`:10703`) → `decode_save_bytes` (`:10718`), which passes
  `filter_seed = None` (`mc/io/sim_io.rs:766`); `apply_filters` is only called
  inside `filter_seed.inspect(...)` in `decode_bytes_inner`
  (`mc/io/sim_io.rs:850`).
- **Java evidence.** `up/maps/generators/FileMapGenerator.java:71-82` loads
  through `World.new FilterContext(map)`, whose `end()` calls `applyFilters()`;
  `up/core/World.java:681-694` applies every `map.filters()` entry;
  `up/maps/Map.java:120-125` reads the `genfilters` tag. groundZero carries 5
  scatter filters, fungalPass 30, taintedWoods 32.
- **Verification.** Launch groundZero; count solid `*-boulder` tiles in the
  loaded grid at the stone basin east of spawn. Authored boulders remain;
  filter-placed ones are 0.
- **Status.** Code-confirmed.

### CG-GEN-02 — Preset sectors bypass candidate resolution and the core invariant — **P2**
- `gx/sim_host.rs:10695-10705` builds exactly one path
  (`{assets}/maps/{owner_name}/{map_name}.msav`), logs `preset map missing` and
  falls back to `generate_sector` (`:3506-3524`). The ported helpers
  `resolve_candidate` (`mc/maps/generators/file_map.rs:26`) and `require_core`
  (`:44`) are unused by the live path. Java tries four candidates
  (`FileMapGenerator.java:26-49`) and throws
  `"All maps must have a core."` (`:139-141`) instead of silently regenerating.
- **Status.** Code-confirmed.

### CG-GEN-03 — `lightCoverage` is never computed, so captured sectors never glow — **P2**
- `mc/game/sector.rs:64` `core_snapshot_of` never fills `light_coverage`;
  `SectorInfoState::prepare` copies the 0.0 default (`:457`). Readers are
  starved: `mc/maps/planet/serpulo.rs:630` (`light_scl = sector.light_coverage`)
  and `gx/campaign.rs:5801` (`SectorGlow`). Java
  `SectorInfo.java:230-237` sums `lightRadius * efficiency` over default-team
  `emitLight` buildings + units; `SerpuloPlanetGenerator.java:152-176` paints
  the glow.
- **Status.** Code-confirmed.

### CG-GEN-04 — Generated sectors get no dark border: `trimDark` is a no-op — **P2**
- `mc/maps/planet/serpulo.rs:1647` calls
  `trim_dark(tiles, content, |_, _| 0.0)`; `trim_dark`
  (`mc/maps/basic.rs:933-958`) therefore never sets a wall. The sector polygon
  is attached only after generation (`gx/sim_host.rs:3551-3563`), so the
  `World.getDarkness` edge+polygon terms Java uses
  (`up/maps/generators/BasicGenerator.java:451-462`,
  `up/core/World.java:484-536`) cannot be evaluated at trim time.
- **Verification.** Open a numbered Serpulo cell and walk the edge: Java has a
  wall ring, Godot open floor to the last tile.
- **Status.** Code-confirmed.

### CG-GEN-05 — Authored sector weather is thrown away on every fresh landing — **P1**
- **Symptom.** `saltFlats`/`tarFields`/`atolls` sandstorm, `taintedWoods`
  sporestorm + always-fog, `sunkenPier` fog, etc. never play as authored; the
  sector gets terrain-derived weather instead.
- **Godot evidence.** `gx/campaign.rs:1160-1176` overwrites `rules.weather`
  with `add_weather` after loading the map, so the authored `weather:[…]`
  (with `minFrequency`/`always`/cooldown) is lost on a fresh launch.
- **Java evidence.** `up/core/World.java:266,303` runs `setSectorRules`
  (`addWeather`) **before** `SaveIO.load`; `SaveVersion.readRules`
  (`up/io/SaveVersion.java:196`) then replaces `state.rules` with the map's
  rules, authored weather included.
- **Verification.** Fresh-launch one of the authored weather sectors; compare
  `get_rules_json().weather` with the map metadata / Java.
- **Status.** Code-confirmed.

### CG-GEN-06 — `assets/planets/serpulo.json` is never loaded — **P2**
- The file ships but no loader reads it; `preset_remap`/`attack_sectors` are
  baked literals in `mc/content/registries/planets.rs:713-768` and
  `load_planet_data` is a write-only flag. Java `Planet.getData` reads
  `planets/<name>.json` (`up/type/Planet.java:477-490`), which is the mod/data
  override path.
- **Status.** Code-confirmed.

### CG-GEN-07 — Hidden submission sector labels leak internal ids — **P2**
- `mc/ui/campaign.rs:890-905` + `gx/ui/dialogs/batch_c.rs:820-832` render
  `"sector-serpulo-76"` instead of `"76"` for hidden submission cells; Java
  `Sector.name()` (`up/type/Sector.java:153-161`) gates on
  `requireUnlock || showHidden`.
- **Status.** Code-confirmed.

### CG-GEN-08 — Preset content order and submission credits differ — **P2**
- Survival preset 202 is registered after the six megabases
  (`mc/content/registries/sectors.rs:390-425` vs
  `up/.../SectorSubmissions.java:70-77`), permuting content ids; submission
  `credit` is never set (`sectors.rs:290-319` vs `SectorSubmissions.java:134`).
- **Status.** Code-confirmed.

---

## 7. Campaign economy

### CG-ECON-01 — A container/vault next to the core does not join the core item pool — **P1**
- **Symptom.** Core → container → unloader → belt is a dead end: deposits into
  the container forward to the core, but the unloader only sees the container's
  own (always-empty) module, so it never moves an item.
- **Godot evidence.** `mc/world/blocks/storage/core_block.rs:215`
  (`on_proximity_update` records `linked_core` and adds capacity but never
  aliases the module — the module doc at `:11-16` calls this deferred);
  `mc/world/blocks/storage/storage_block.rs:70-92` (inbound forwarding only),
  `:107-109` (`remove_stack` is plain, no core path);
  `mc/world/blocks/storage/unloader.rs:290-311` (`is_possible_item` requires
  `item_count(other, item) > 0`), `:79-81`.
- **Java evidence.** `up/world/blocks/storage/CoreBlock.java:764-770` — owned
  storages get `t.items = items` (one module); `Unloader.java:136-160` +
  `:113-131` see the shared pool; `StorageBlock.java:54-75,84-92`.
- **Verification.** GZ: core + touching container + adjacent unloader + conveyor
  into a turret; mine/belt 50 copper, step 600. Godot's lane stays empty and
  `core_items_json` stays 50; Java's lane fills.
- **Status.** Code-confirmed.

### CG-ECON-02 — Core-items HUD/state read the first core in entity order, not the player's core — **P1**
- `gx/sim_host.rs:10242-10269` (`core_items_json`) returns the first
  `CoreBlock` module with no team filter; the team-filtered sibling
  `player_core_items` is at `:6860-6887`. Java `CoreItemsDisplay.java:33` uses
  `player.team().core()` (`Team.java:83-85`). On attack sectors or maps with a
  derelict/wave core the HUD can show the wrong inventory.
- **Verification.** Attack sector or a map with an enemy core: compare
  `core_items_json` vs `player_core_items_json` in the same frame.
- **Status.** Code-confirmed.

### CG-ECON-03 — Multi-core teams: first-core-only HUD/sector record vs all-core objectives — **P2**
- `mc/game/sector.rs:60-131` `core_snapshot_of` keeps only the first core;
  `ObjectiveEnv::team_has_item` sums every core module
  (`mc/game/runtime.rs:800-816`). Java aliases all cores to one module
  (`CoreBlock.java:764-770`), so HUD, objectives and the saved sector record
  agree. Build a second core and stock it: the HUD/relaunch lose it while the
  objective counts it.
- **Status.** Code-confirmed.

### CG-ECON-04 — Destroying/deconstructing a core discards its whole inventory — **P2**
- `CoreBehavior` implements no `on_removed`/`placed`/`after_destroyed`
  (`mc/world/blocks/storage/core_block.rs:182-490`; default no-op hooks at
  `mc/world/behavior/mod.rs:277-296`), so `destroy_building`
  (`mc/sim/runtime.rs:885-910`) drops the entity + module and all items. Java
  `CoreBlock.java:852-866` redistributes a capacity-proportional share into
  owned storages, unregisters the core and re-proximities the rest.
- **Verification.** Kill a core with 200 copper + adjacent container: Java leaves
  ~13 copper in the container, Godot 0.
- **Status.** Code-confirmed.

### CG-ECON-05 — Player mining affordance/order ignores `mineRange` and stack acceptance — **P2**
- `mc/ai/types/player.rs:278-295` `can_mine_tile` only checks
  `get_mine_result(...).is_some()` — no range, no `accepts_item`, no
  `playerUnmineable`. The cursor path (`mc/input/cursor.rs:56-57,135`) shows a
  drill over far tiles; the order is dropped next tick by
  `mc/entities/comp/unit/miner.rs:262-270`. Java gates at the input layer with
  `validMine` + `acceptsItem` + `playerUnmineable`
  (`up/input/InputHandler.java:2083-2090`, `MinerComp.java:50-60`).
- **Status.** **Fixed at HEAD (2026-11-06) — in-engine verified on Godot (`PAR-0498`); no AVIF pair.** `can_mine_tile` = `valid_mine` (`mineRange`) + `acceptsItem` (shared `miner::accepts_item`) + `playerUnmineable` floor/overlay gates. Real `DesktopInput.tryMineAt` path: ore `(137,53)` yields `player_mine_tile=[137,53]` at 8 tiles and `[]` at 27 tiles. No pair because X11 captures exclude the OS cursor and the port renders no `CursorContext` drill cursor (a separate, unfiled gap); the Godot world frame was also degraded after a fresh-XDG editor restart. See `runs/PAR-0498/`.

### CG-ECON-06 — Core incineration has no belt/drill visual feedback — **P2**
- `mc/world/blocks/storage/storage_block.rs:30` `incinerate_effect` is an empty
  stub; `core_block.rs:320-360` calls it for belt/drill arrivals. Only
  `handle_stack` (player offload) emits the real burn (`core_block.rs:390-420`).
  Java `StorageBlock.java:40-49` plays `Fx.coreBurn` with 30% chance and a
  `wasVisible` gate whenever the core is full.
- **Status.** Code-confirmed.

### CG-ECON-07 — Fresh Ground Zero core stock is empty at HEAD (no gap) — **refuted**
- `gx/campaign.rs:1060-1070` clears `rules.loadout` for an empty payload;
  `mc/game/play.rs:330-380` clears then re-adds the loadout; the map's authored
  `loadout:[]` and Java `Logic.play` produce the same empty core. The earlier
  PAR-0056 "100 copper" divergence does not reproduce on the player-facing
  launch path. Residual risk only for harnesses that call `play_new_sector`
  directly with `Rules::default()` (`mc/io/json/rules.rs:869-875`).
- **Status.** Refuted at HEAD; verify with `core_items_json == {}` on a fresh
  `start_sector("serpulo", 15)`.

---

## 8. Serpulo campaign data

The 29 Serpulo presets' fields, the submission table, `serpulo.json`'s remap
and attack cell sets, `SectorDifficulty`, `CampaignRules`/`Difficulty`, the
weather definitions and the 223-node tech tree all match upstream (the
data-parity audit explicitly checked them). Remaining data gaps are the
resource/weather/locale surfaces above plus:

- **`Sector.loadInfo` legacy migration missing** — old saves with `water` in
  `resources` are not converted to `Liquids.water` and nulls are not dropped
  (`mc/game/sector.rs:608-610` vs `up/type/Sector.java:105-118`). P2.
- **Preset order/credit** — see [CG-GEN-08](#cg-gen-08--preset-content-order-and-submission-credits-differ--p2).
- **Hidden sector labels** — see [CG-GEN-07](#cg-gen-07--hidden-submission-sector-labels-leak-internal-ids--p2).
- **Planet JSON not loaded** — see [CG-GEN-06](#cg-gen-06--assetsplanetsserpulojson-is-never-loaded--p2).

---

## 9. Hint system

The hint catalogue (26 desktop hints), skip/persistence (`<name>-hint-done`) and
the overall panel layout match Java. **CG-HINT-01** is fixed (`PAR-0499`); two
predicate/cadence deltas remain (**CG-HINT-02**, **CG-HINT-03**).

### CG-HINT-01 — `desktopPause` completes on any pause, not on the pause key — **P2**
- `gx/ui/fragments/hints.rs:452` uses `self.is_paused()` for the
  `desktopPause` **complete** predicate; Java completes on
  `Core.input.keyTap(Binding.pause)` (`up/ui/fragments/HintsFragment.java:189-192`).
  Any auto-pause (window focus loss) or pause-opening dialog permanently burns
  the hint and persists `<name>-hint-done`.
- **Verification.** Fresh profile, GZ wave ≥ 2, click away to auto-pause,
  return: Java still shows the pause hint, Godot never does.
- **Status.** **Fixed at HEAD (2026-11-06) — in-engine verified on Godot (`PAR-0499`); no AVIF pair.** `hints.rs` completes `desktopPause` on the `pause` hotkey press edge (`MindInput.hotkey` → `_on_hotkey` → `pause_key_tap`), and the old `is_paused()` arm is removed; probe `hint_finished("desktopPause")` is false after a Research-dialog pause and true after the pause key. The Java leg was time-boxed (GZ stayed wave 1, so the hint's `wave >= 2` shown predicate was not reachable). See `runs/PAR-0499/`.

### CG-HINT-02 — `desktopShoot` and `zoom` complete on state, not the input edge — **P2**
- `hints.rs:431-433` (`desktopShoot` → `consume_hint_key("shoot")`) and
  `:434-441` (`zoom` → camera-zoom delta > 0.01). Java completes on
  `player.shooting` (`HintsFragment.java:179-182`) and
  `Core.input.axis(KeyCode.scroll) != 0` (`:170-172`). A scripted camera-zoom
  change (loadout launch, cutscene) can complete `zoom` without the player
  touching the wheel.
- **Status.** Code-confirmed.

### CG-HINT-03 — Hint driver does a building/grid sweep + 26 settings reads every frame — **P2**
- See [CG-TUT-06](#cg-tut-06--hud-objective-block-re-parses-full-rules-json-5s-hint-driver-sweeps-every-frame--p2).
- **Status.** Code-confirmed.

---

## 10. Already at campaign parity

These are large areas where the port is already 1:1 (or within measured visual
tolerance). They are recorded so future gap hunts do not re-audit them.

- **Content/data:** all 447 blocks, 69 units, items, liquids, status effects,
  29 Serpulo presets (ids/order/values), 223 Serpulo tech nodes with identical
  depths and explicit objectives, 46 submission presets' values, weather
  definitions, generator constants, `SectorDifficulty`, `CampaignRules` and
  `Difficulty` percentages.
- **Objective runtime:** all 13 objective classes parse and execute;
  `qualified`/`dependency_finished`, `completed`/`countup` persistence, flags,
  `completionLogicCode` against the live world, and the tick system that copies
  results back into `Rules.objectives`.
- **Sector lifecycle:** `Universe.runTurn` (production/import/invasion),
  capture/loss checks, auto turns, `SectorInfo.prepare/write`, lost-sector
  replay, abandon-as-loss, invasion scheduling, sector save slots and
  `Saves`-driven autosave policy plumbing (the unit bug is CG-SECTOR-01).
- **Rules:** preset rules + planet `ruleSetter`, `winWave`/`captureWave`,
  `waveTimer`/`waveSending` tutorial rules, difficulty multipliers, sector
  item storage, sector stats accumulation, `limitMapArea`, `coreDestroyClear`.
- **World:** authored `.msav` tile data, groundZero's core/ore overlays and
  empty starter core, generated-sector geometry, baseparts/enemy-base
  generation, capture tile replacements, `placeLaunchLoadout`, sector polygon
  derivation (render only; CG-GEN-04 is the trim pass).
- **UI:** planet page layout/planet switching/hex selection/sector panel
  (launch/Go/Abandon/Stats/Rename), difficulty dialog, research dialog graph and
  node data, in-game HUD (core items, wave pane, status, minimap, pause
  objective text, game-over title/sector name), first-run chooser and its
  persistence, loadout capacity math (`0.5 × itemCapacity`), max recomputation,
  item step sizes and `clearLoadoutInfo` on a free launch.
- **Save:** the save codec/version readers, sector-slot policy, autosave
  plumbing, sector records, research/hint persistence, import/export and the
  Load/Save dialogs are ported. The gaps are write timing and slot identity
  (CG-SAVE-01…07), plus the autosave unit bug (CG-SECTOR-01).

---

## 11. Checked and not a gap

- Fresh Ground Zero core is **empty** at HEAD; PAR-0056's 100-copper stock no
  longer reproduces through the player-facing launch (CG-ECON-07).
- Objective 1 (`BuildCount mechanical-drill`) **does complete**: placement emits
  `BlockPlacedEvent` (`mc/sim/builder.rs:1420-1440`), the tick-end flush
  increments `stats.placedBlockCount` (`mc/sim/mod.rs:2766-2790`), the
  `BuildCount` predicate fires (`mc/game/runtime.rs:713-760`) and the data is
  copied back into `Rules.objectives` (`:757`). The missing part is the world
  marker (CG-TUT-01), not the completion.
- HUD `status_text` is **not** recomposed per frame (0.2 s) and objective
  qualification has no player/camera term; the movement jitter is the marker
  pass (CG-TUT-02).
- Hint `Skip` and `<name>-hint-done` persistence match Java
  (`gx/ui/fragments/hints.rs:220-240`, `gx/ui/ui_host.rs:785-800`).
- `waveTimer:false` / `waveSending:false` tutorial rules and the skip-button
  gate match (`gx/campaign.rs:6335-6365`, `gx/ui/fragments/hud.rs:563-580`).
- `limitMapArea` is a pathing/render bound in Java too; not a placement rule.
- Minimap/light marker channels are unimplemented, but Ground Zero's markers
  all default `minimap = -1`, so they are not tutorial-visible.
- `allowLaunchSchematics`, capacity math, `max`/`valid`/`total`, step sizes,
  `clearLoadoutInfo`, `checkAutoUnlocks` (PAR-0080 fix), `clearResearch` and
  node visibility propagation were checked and match; CG-TECH-05 is the one
  gate that is not consulted.
- Authored weather is loaded from the map but overwritten afterwards
  (CG-GEN-05); `addWeather` predicates themselves match Java.
- 223/223 Serpulo tech nodes resolve and render; the old PAR-0145 "root null"
  grouping no longer reproduces.
- The tech-tree root-selector "dead Serpulo button" (CG-TECH-06) is **refuted**:
  Erekir's root content `core-bastion` is `alwaysUnlocked`, so Java also has two
  selectable roots and shows the title button; the port matches (PAR-0496
  verified both clients). The Java predicate was still aligned for genuinely
  single-root mod/MP states.
- The first-run `@campaign.select` chooser does **not** reappear: the flag is
  written and flushed through the shared settings store
  (`gx/ui/dialogs/batch_c.rs:649-658,1373`, `gx/ui/ui_host.rs:819-825`), so
  quitting from the chooser and relaunching skips it (CG-SAVE-08 refuted).
- Preset `winWave`/`captureWave`, baseparts/base catalogs, enemy-base
  generation, capture replacements and authored core/items match.

---

## 12. Runtime verification (Godot leg, then Java reference)

A first-minutes pass was run after this survey was written. It targets the
tutorial marker cluster plus the `Build:` objective text, because those are the
gaps the player hits in the first five minutes. The exact Godot/Java tool-call
chains, timestamps, captures and statuses live in
[`TWIN-VERIFICATION.json`](TWIN-VERIFICATION.json) (`PAR-0477`, `PAR-0478`,
`PAR-0479`) and under `runs/PAR-0477/`…`runs/PAR-0479/`.

| Finding | Godot observation | Java reference | Twin item |
|---|---|---|---|
| CG-TUT-01 (Text/Shape markers, stale label) | Objective 1 completes and the HUD advances to `Move into Core: 0/5 Copper`, but `objective_markers_json()` returns `[]` while objective 2 runs; the last `Research the Mechanical Drill…` label stays frozen instead of being replaced by `Research and place conveyors…`. | The conveyor WorldLabel appears immediately after the drill is placed; objective 2's HUD text is `Move into Core: 0/5 Copper` in both clients. | **PAR-0477 (twin-verified, fixed)** |
| CG-TUT-02 (5 Hz marker redraw) | With the camera panning, the world label's offset from the core/ore changes by roughly 35 px between captured frames; the label is re-projected only on the 0.2 s poll and snaps back. | Java's label stays rigidly anchored to the ore marker; archived `r02-00-before` / `r02-04-after-w` frames show the same anchor before/after movement. | **PAR-0478 (twin-verified, fixed)** |
| CG-TUT-03 (`Build:` text) | Live HUD: `Build: mechanical-drillx 1` (raw id, no icon/name/count order). | Live HUD: `Build: 1x [drill icon] Mechanical Drill`. After the drill both show `Move into Core: 0/5 Copper`. | **PAR-0479 (twin-verified, fixed)** |

Extra runtime checks from the same pass:

- **Fresh Ground Zero core is empty** — `start_sector("serpulo", 170)` +
  `core_items_json()` returned `{}`; the earlier 100-copper divergence
  (PAR-0056) does not reproduce at HEAD. The map is sector **170**, not 15:
  sector 15 is a generated cell with no objectives (`sector: "serpulo-15"`,
  `winWave: 25`), so the older PAR-0045/0046 chains that used `15` were
  launching the wrong cell.
- Objective 0 completed after mining (`player_mined_count: 95`,
  `core_items_json: {"copper":190}`), confirming the tutorial's mine/offload
  loop; objective 1 then completed on real mouse placement of the drill
  (`get_rules_json().objectives[1].completed == true`), confirming that the
  reported drill stall is the **prompt renderer**, not the objective counter.
- The "jitter" is not the HUD text: the status panel is composed from a 0.2 s
  refresh and stayed identical while the world label visibly moved in the pan
  captures; the movement-dependent surface is the world-label pass (CG-TUT-02).

Screenshots: `runs/PAR-0477/godot/20261106-122826-03-after-drill-no-conveyor-prompt.png`
vs `runs/PAR-0477/java/20261106-123616-02-after-drill-conveyor-prompt.png`;
`runs/PAR-0478/godot/20261106-122519-pan2-{02,06,10}.png` vs the Java
`runs/archive/20261009-065416-ref-ground-zero/java/r02-*.png`;
`runs/PAR-0479/godot/20261106-122733-02-build-text-active.png` vs
`runs/PAR-0479/java/20261106-123616-01-build-text-active.png`.

Frame-diff metrics (the pairs also differ in camera framing/label scale; the
metrics are recorded as evidence, not as a pass threshold):

| Pair | changed_fraction | mean_abs_rgb | coarse max channel | palette |
|---|---|---|---|---|
| PAR-0477 after-drill | 0.465 | 28.75 / 29.54 / 30.18 | 21.02 | 0.1548 |
| PAR-0479 Build text | 0.419 | 26.60 / 27.04 / 27.55 | 18.90 | 0.1428 |

Both exceed the parity-eval §4 gate; the delta is recorded as the gap itself,
not as a tolerance failure. The remaining doc findings are code-confirmed only;
a full campaign run would need one Java+Godot pass per finding.

### Enqueued twin items

All in-engine-verified findings and the highest-value code-confirmed ones were
enqueued through `tools/twin_tx.py` (the only sanctioned writer of
`TWIN-VERIFICATION.json`). `mismatch` items carry the fresh Godot+Java call
chains; `seeded` items carry the code evidence and a `source` anchor back here.

| CG | PAR | Status | CG | PAR | Status |
|---|---|---|---|---|---|
| CG-TUT-01 | PAR-0477 | twin-verified | CG-SAVE-01 | PAR-0486 | twin-verified |
| CG-TUT-02 | PAR-0478 | twin-verified | CG-SAVE-02 | PAR-0487 | twin-verified |
| CG-TUT-03 | PAR-0479 | twin-verified | CG-TECH-01 | PAR-0488 | seeded |
| CG-SECTOR-01 | PAR-0480 | twin-verified | CG-TECH-02 | PAR-0489 | seeded |
| CG-SECTOR-02 | PAR-0481 | verified | CG-UI-01 | PAR-0490 | twin-verified |
| CG-GEN-05 | PAR-0482 | seeded | CG-UI-04 | PAR-0491 | seeded |
| CG-GEN-01 | PAR-0483 | seeded | CG-TUT-04 | PAR-0492 | verified |
| CG-ECON-01 | PAR-0484 | seeded | CG-UI-02 | PAR-0493 | seeded |
| CG-ECON-02 | PAR-0485 | seeded | CG-UI-03 | PAR-0494 | seeded |

The other findings in §3–§9 remain doc-only (they need a Java leg or a deeper
save state to verify) and are the natural next batch for the twin queue.



---

## 13. Fix pass — 2026-11-06

A fix pass ran after the survey and the first runtime verification. Every fixed
item below has fresh Godot + Java frames and a canonical AVIF pair under
`parity/twin/<PAR>/`; the ledger status lives in `TWIN-VERIFICATION.json` and the
full recipes in `runs/<PAR>/`.

| CG | Twin item | Status | Fix | Evidence |
|---|---|---|---|---|
| CG-TUT-01 (P0) | PAR-0477 | twin-verified (72.0) | `objective_markers_json` forwards all seven marker classes behind the `Renderer.java` `world != -1` gate; `render/objective_markers.rs` draws world-label/polygon/pulse/line/texture/quad; the unconditional redraw clears the stale label | `runs/PAR-0477/`, `parity/twin/PAR-0477/` |
| CG-TUT-02 (P1) | PAR-0478 | twin-verified (probe; static pair 13.8) | marker JSON still polls at 0.2 s, but projection and `queue_redraw()` run every frame; `marker_draw_debug()` shows draws advance 1:1 with process frames | `runs/PAR-0478/`, `parity/twin/PAR-0478/` |
| CG-TUT-03 / CG-OBJ-03 (P1) | PAR-0479 | twin-verified (74.0) | Java argument tuples for Build/Research/Produce/DestroyUnits/DestroyBlocks (remaining count, `emoji() + " "`, localized name) plus live HUD stats | `runs/PAR-0479/`, `parity/twin/PAR-0479/` |
| CG-TUT-04 (P1) | PAR-0492 | verified (Godot-only; the Java objective-12 state is unreachable — see the run doc) | `timer_text` = `(duration × objectiveTimerMultiplier − countup) / 60`, formatted `m:ss`, with the live executor countup | `runs/PAR-0492/` |
| CG-SECTOR-01 (P1) | PAR-0480 | twin-verified (69.9) | `update_policy` compares the seconds accumulator against `saveinterval` seconds, so the default is 60 s; wrong-unit tests rewritten | `runs/PAR-0480/`, `parity/twin/PAR-0480/` |
| CG-SECTOR-02 (P1) | PAR-0481 | verified (Godot-only; the Java launch-to-neighbour flow is unreachable — neighbour 63 is Locked) | `MindCampaign.save_outgoing_sector()` writes the live slot before `MindSimHost.load_sector`; the accelerator pump does the same before its load | `runs/PAR-0481/` |
| CG-SAVE-01 (P1) | PAR-0486 | twin-verified (68.4) | `start_sector`'s fresh-launch branch calls `persist_sector_record()` before the launch save (mirrors `SaveVersion.writeMeta`'s `prepare + saveInfo`); `kill -9` + Resume keeps record items/wave | `runs/PAR-0486/`, `parity/twin/PAR-0486/` |
| CG-SAVE-02 (P1) | PAR-0487 | twin-verified (70.9) | `load_slot` calls `adopt_loaded_slot` (`set_current_by_file` + meta playtime); pause reads `Save & Quit`, exit save writes the loaded slot; new unit test | `runs/PAR-0487/`, `parity/twin/PAR-0487/` |
| CG-UI-01 (P1) | PAR-0490 | twin-verified (38.5; pair differs in globe zoom/progress, not the panel) | `planet_sector_action_pressed` short-circuits Resume on the sector being played and only closes the page (no `load_sector`/`start_sector` reload) | `runs/PAR-0490/`, `parity/twin/PAR-0490/` |
| CG-TECH-03 (P2) | PAR-0497 | twin-verified (77.8) | `content_dependencies()` from `BlockDef::get_dependencies` + `UnitType::get_dependencies`, deduped and prepended as `Research` objectives; golden regenerated | `runs/PAR-0497/`, `parity/twin/PAR-0497/` |
| CG-TECH-06 (P2) | PAR-0496 | **refuted** (both clients show the 2-root picker; aligned Java's predicate anyway) | `root_select_visible(rows, selected_root_locked) = rows − selected_locked > 1` + test; no vanilla user-visible change | `runs/PAR-0496/`, `parity/twin/PAR-0496/` |
| CG-ECON-05 (P2) | PAR-0498 | verified (Godot-only; captures exclude the OS cursor / no drill cursor is drawn) | `can_mine_tile` gates on `valid_mine` + `acceptsItem` + `playerUnmineable`; click path rejects a far tile | `runs/PAR-0498/` |
| CG-HINT-01 (P2) | PAR-0499 | verified (Godot-only; the Java `desktopPause` state was unreachable at wave 1) | `desktopPause` completes on the `pause` hotkey press edge (`_on_hotkey` → `pause_key_tap`), not `is_paused()` | `runs/PAR-0499/` |

New bugs recorded while fixing (all in `TWIN-VERIFICATION.json`):

- **PAR-0495** (P2, mismatch) — a mid-session sector reload leaves the world
  renderer blank until the process restarts (observed while staging PAR-0492;
  not investigated).
- **PAR-0500** (P2, mismatch) — `campaign.rs::research_parent_unlocked` masks
  multi-node content by the *first* tech node's parent; Java uses
  `content.techNode` (the last created), so Java prints `Research Silicon` where
  the port prints `Research ???` (visible in the PAR-0497 pair). Fix: use
  `UnlockFields.tech_node`.
- Not filed, pre-existing, noted in PAR-0498: `get_mine_result` never falls back
  to `tile.floor().itemDrop`, so the `playerUnmineable` floor gate is currently
  unobservable in vanilla.

The remaining doc-only findings in §3–§9 are still open and are the next twin
queue batch.
