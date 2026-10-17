<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round-K twin re-walk (2026-10-17)

Independent re-run of `docs/J-round-twin-verification.md` §1 on fresh builds, after every
round-J plan (J01–J09) merged into `main`. Verifier session: parity session 5
(`Mindustry-Godot-session5`, branch `parity/session-5` @ `8903fb12` + the J02 T2 commits,
display `:14`, Godot bridge `7010`, Java window on the same display).

Builds: `tools/build.sh` (flock, 6 jobs) for the Godot `libmind_gdext.so`, full editor+game
restart via `tools/session-play.sh 5`; Java launched from
`../Mindustry/desktop/build/libs/Mindustry.jar` with `HOME=parity-java-home` through
`tools/twin.sh twin_launch` (settings.bin backed up and restored, saves snapshotted:
`runs/20261017-s5-K-twin/{settings.bin.bak,saves-before.txt}`).

Evidence: `runs/20261017-s5-K-twin/{java,godot,diff}/`, `run.json`. This report is
verification-only: **no code was changed**.

## 1. Verdicts per surface (round K)

| # | Surface | Verdict | Notes (Java vs Godot) |
|---|---|---|---|
| 00 | Main menu | PARTIAL | Wall drop-shadows and floor edge blending are now drawn in the bake (J02 T4 landed), and a flyer silhouette is visible (`godot/40-menu-check-godot.png`). The background world is random per boot on both sides, so the pixel gate cannot apply; a real residual is the **camera scale** of the menu world (Godot's tiles are ~2× Java's; F-K4). |
| 01 | Play submenu | MATCH | `diff/01-play-side.png`: same items/icons (`Campaign`, `Join Game`, `Custom Game`, `Load Game`), same row order and geometry (±4 px), the accent frame on the opened parent is drawn on both (J06 T1 fixed the missing border). |
| 02-03 | Planet page + sector panel | PARTIAL | `diff/02-planet-side.png`: **orbit arcs** and the **bright purple atmosphere rim** are now present (J07 T1/T2); hex grid, labels, "1 under attack" and the Tech Tree button match. Deviations: Erekir/Serpulo/Difficulty are centred top (Java top-left), the planet is smaller, and the sector panel was not opened this run. |
| 04-05 | Ground Zero HUD | UNVERIFIED | Campaign sector not entered this run (time-box). Blocker: no fresh GZ load; J-round frames remain the last evidence. |
| 06 | Pause menu (campaign) | UNVERIFIED | Java pause dialog could not be opened (window focus/Escape did not reach the game after the sandbox run); Godot's in-game pause menu was captured instead (`godot/35-settings-godot.png` precursor), showing Back/Settings/Save Game/Load Game/Host Multiplayer Game/Quit with the framed `defaultt` cells. |
| 07 | Quit confirm | UNVERIFIED | Needs the pause menu (row 06). |
| 09 | Custom Game list | MATCH (structured) / finding | All **19** custom maps are registered, including the three that used to fail (`Capital Ship Crash Site`, `[Orange]Attack_Secret Island Outpost`, `anthill v5`), each with `preview=True` and mode data (`MindPreview.maps_list()`); the editor's map picker draws their real previews (`godot/29-editor-godot.png`). Sort is newest-first by `lastModified` (matches `MapPriority.recent`). **New finding F-K1**: the grid is clipped at the 5th column and does not scroll — only the first 10 (the `t8-*` set) are reachable; **F-K2**: writing the search field + `text_changed` empties the grid permanently. |
| 10 | Map Play (sandbox) | UNVERIFIED | Java frame captured (`java/10-mapplay-java.png`: mode grid, Customize Rules, preview with the gold scrap dot). Godot's sandbox card is off the first page and the list does not scroll (F-K1), so the dialog could not be opened. Blocker: F-K1. |
| 11-12 | Sandbox world | PARTIAL | `diff/11-sandbox-world-side.png`: Godot now draws the **core-zone stripes** around the core and the **scrap ore** in the minimap (J02 T1/T3); the palette renders. Differences: Java's load scatters **boulders** around the core, Godot's has none (`world_gen_debug`: ore 1201, boulders 0 — profile map drift caveat, F-K3); Java's wave panel/drop-zone ring are not visible in the Godot frame (different entry path, not paused). |
| 13 | Turret palette | UNVERIFIED | Not re-staged (row 14-16 chain not reached). |
| 14-16 | Place duo | UNVERIFIED | Not re-staged this run. |
| 17, 25 | Sandbox waves | UNVERIFIED | Not re-staged (no wave skip driven). |
| 21-23 | Item source config | UNVERIFIED | Not re-staged (block + config click chain not run in the time box). |
| 26 | Pause menu (custom game) | UNVERIFIED | Java pause dialog could not be opened this run; Godot's pause menu shows Save Game/Load Game/Host/Quit (no "Save & Quit" label observed — the J06 T2 spec expects one in custom games). Needs a clean Java capture. |
| 27-28 | Database submenu + Core Database | UNVERIFIED | Not re-staged. |
| 29-32 | Editor (maps → info → open → menu → exit) | PARTIAL | **Maps dialog MATCH** (`godot/29-editor-godot.png` vs `java/09-custom-java.png` style): previews for all maps including the 3 ex-failing user maps, mode icons, Custom/Built-In rows, New Map/Import Map/Back. The editor *world* rows (info panel, in-editor rendering, palette order, core sprite, round checks, J08) were **not** re-staged. |
| 33-34 | Editor menu / exit | UNVERIFIED | Not re-staged. |
| 35 | Settings category menu | MATCH | `java/35-settings-java.png` vs `godot/35-settings-godot-b.png`: same seven rows/order and framed dialog; only fill weight differs. |
| 36-37 | Settings > Graphics | PARTIAL | `diff/36-graphics-side.png`: slider rows are now **framed** with the title/value overlaid and a tall knob (J06 T3 landed). Residuals: rows ~30 px vs Java's 40, knob grey vs tan, and an extra **Bloom Intensity** row that Java does not have (F-K5). |
| 38 | Controls (Rebind Keys) | MATCH | `diff/38-controls-side.png`: centred column, accent key names ("A / D", "Mouse Back"), no raw markup, framed Rebind/Reset cells — the J06 T4 fixes all hold. |
| 39 | Load Game | MATCH | `diff/39-load-side.png`: save **previews**, accent titles, "Map / mode / wave / Autosave / Playtime / date" rows, per-card icons, full-width Back + Import Save, newest-first. Save sets differ per profile (expected). |
| 40 | Tech tree | UNVERIFIED | Neither client opened it this run: the planet page's Tech Tree button did not respond to clicks on either side (focus/first-click swallow suspicion, F-K7). Needs a retest with input flush. |
| 41 | Map Play (Ancient Caldera) | UNVERIFIED | Godot cannot reach the Caldera card (F-K1); Java's frame was not captured. |
| 42-49 | Ancient Caldera survival | UNVERIFIED | Not re-staged (custom-game start chain not completed). Structured partial: the sandbox map shows ore filters run (`world_gen_debug` ore 1201) — the Caldera ore/boulder re-roll needs a survival start. |
| 44 | Game Over (custom) | UNVERIFIED | Not re-staged. |
| 52 | Legged units (Godot only) | UNVERIFIED | Not re-staged (sandbox unit spawn not run). |

Rows 00–52 not listed above (04-05, 06-07, 13-16, 17/25, 21-23, 26, 27-28, 29-34 world rows,
40-52) are UNVERIFIED with the blocker named in their row.

## 2. Findings (ranked)

1. **F-K1 (HIGH, new) — Custom Game map grid is clipped and does not scroll.** With 19 custom
   maps the grid draws 5 columns the right-most of which is cut by the dialog edge, and only the
   first 10 cards (the `t8-*` set) are visible. Wheel events pushed through the viewport, scrollbar
   trough clicks, a thumb drag, and writing `scroll_vertical` on the dialog's `ScrollContainer`
   all left the view unchanged. Consequence: `sandbox`, `Ancient Caldera` and the three J01 maps
   cannot be opened from the dialog (rows 10/41 blocked). Evidence:
   `godot/09-custom-godot.png`, `godot/09-custom-godot-p2b..e.png`.
2. **F-K2 (MEDIUM, new) — Custom Game search empties the grid.** Setting the search `LineEdit`'s
   text and emitting `text_changed` (harness path) leaves the grid empty for an existing map name
   (`sandbox`) and it stays empty after close/reopen. Synthetic-input caveat: needs a real-typing
   retest before filing as user-facing. Evidence: `godot/09-custom-godot-sandbox.png`,
   `09-custom-godot-recovered.png`.
3. **F-K3 (MEDIUM, new) — Sandbox loads without boulders.** Java's sandbox load scatters boulders
   around the core; Godot's `world_gen_debug` for the same map reports ore 1201 / boulders 0 while
   the ore filters did run. Caveat: the two profiles' `sandbox.msav` may differ (the Java client
   autosaves over it), so this is an observation until a like-for-like map is loaded. If it holds,
   the default `ScatterFilter` (boulder) pass in `maps::filters::default_filter_stack` is not
   applied for maps carrying their own `genfilters`.
4. **F-K4 (LOW, new) — Menu background camera scale.** The bake draws the menu world ~2× closer
   than `MenuRenderer` (row 00 side-by-side); shadows/edges/flyers are otherwise present.
5. **F-K5 (LOW) — Graphics slider rows.** Framed rows landed, but they are ~10 px shorter than
   Java's and the knob is grey rather than tan; the port also adds a `Bloom Intensity` row Java
   does not have.
6. **F-K6 (LOW) — Planet page button placement.** Erekir/Serpulo/Difficulty are centred at the
   top; Java puts them top-left. The planet globe is also drawn smaller than Java's.
7. **F-K7 (LOW, observation) — Planet-page Tech Tree button unresponsive to injected clicks** on
   both clients this run (one Java click and two Godot clicks, no dialog). Likely the first-click
   focus swallow seen elsewhere in this walk; retest with an explicit `Input.flush_buffered_events`.

Closed since round J (fresh evidence): the three unimportable user maps + previews/mode icons
(row 09, structured + editor picker), floor edge blending (row 00/11), scrap ore in previews and
minimap (row 11-12), slider framing (row 36-37), keybind markup/colour/column (row 38), Load Game
previews/order/buttons (row 39), planet orbit arcs + atmosphere rim (row 02-03), accent frame on
the opened menu parent (row 01).

## 3. Plan-specific acceptance frames

| Plan | Acceptance | Verdict | Evidence |
|---|---|---|---|
| J01 T1 | Caldera survival load shows ores + boulders, two loads re-roll | UNVERIFIED | Java frame not captured; Godot Caldera not loaded this run (row 42-49). Ore filters proven on the sandbox map (`world_gen_debug` ore 1201, seed 23931065). |
| J01 T1/T2 | 3 user maps import; Custom Game list previews + mode icons | MATCH (structured + editor picker) | `MindPreview.maps_list()` = 19 custom maps, all `preview=True`; `godot/29-editor-godot.png`. Custom Game *cards* blocked by F-K1. |
| J01 T3 | Map list order = `MapPriority` | MATCH (comparator) | `lastModified` descending order in `maps_list()`; per-profile file sets differ so a cross-client frame compare is not meaningful. |
| J01 T4 | Load Game previews, newest-first, Save & Quit label | PARTIAL | Previews/newest-first MATCH (`diff/39-load-side.png`); Save & Quit UNVERIFIED (row 26). |
| J02 T1 | Caldera floor-edge crop | UNVERIFIED | Not re-staged; core-zone stripes on sandbox MATCH (`godot/11-sandbox-world-godot.png`). |
| J02 T2 | metal-tiles autotile | UNVERIFIED this run | Landed in `847f0d4f`; the J02 T2 run's own evidence stands (`runs/20261017-s5-J02-T2/`). |
| J02 T3 | Scrap circle in preview / Map Play / minimap | PARTIAL | Minimap MATCH (`godot/11-sandbox-world-godot.png`, gold dot); Map Play preview UNVERIFIED (F-K1). |
| J02 T4 | Main-menu shadows + flyers | MATCH | `godot/40-menu-check-godot.png` (flyer silhouette + wall shadows + blended floor). Camera scale deviation F-K4. |
| J03 T1-T3 | Zoomed turrets idle/firing | UNVERIFIED | Not staged. |
| J03 T4 | atrax/arkyid legs | UNVERIFIED | Not spawned. |
| J03 T5 | treads, segments, wrecks, payloads | UNVERIFIED | Not staged. |
| J04 | foreshadow vs duo target choice; electrolyzer directions; generator damage | UNVERIFIED | Not staged. |
| J05 | config panels (ItemSelection), factory/reconstructor rows, core hover Capacity, inventory popup | UNVERIFIED | Not staged. |
| J06 T1/T2 | pause menu, quit confirm, editor menu | UNVERIFIED | Java pause dialog not opened; Godot pause menu captured (see row 26). |
| J06 T3 | Graphics sliders | PARTIAL (framed ✓) | `diff/36-graphics-side.png`; F-K5. |
| J06 T4 | Keybind markup/colour/column | MATCH | `diff/38-controls-side.png`. |
| J06 T5 | Load Game cards | MATCH | `diff/39-load-side.png`. |
| J06 T6 | Game Over stats | UNVERIFIED | Not staged. |
| J06 T7 | Map Play portrait maps | UNVERIFIED | F-K1 blocks the Caldera card. |
| J07 T1/T2 | Orbit arcs, atmosphere rim | MATCH | `diff/02-planet-side.png`; layout deviations F-K6. |
| J07 T3/T4 | Tech tree backdrop/icons/header; Accelerator | UNVERIFIED | Tech tree not opened (F-K7); Accelerator not staged. |
| J08 | editor core sprite, palette order/highlight, round checks | PARTIAL | Editor Maps dialog MATCH; the in-editor rows were not re-staged. |
| J09 | Mods dialog unsupported-script reason row | UNVERIFIED | No script-mod fixture installed; Godot dialog captured empty (`godot/45-mods-godot.png`). |

## 4. Gate and limitations

- Gate: parity skill §4 (`size_match`, `changed_fraction <= 0.005`, `mean_abs_rgb <= 12`,
  `coarse_layout.max_channel_mean <= 6`, `palette_distance <= 0.05`, non-blank). Frame diffs ran
  through `SIDE` (`tools/twin.sh`) with the numbers in `diff/*.json`.
- The gate is only meaningful for like-for-like state. Rows whose frames carry different content
  by design (random menu world, per-profile save/map sets, different sim state) are judged
  structurally and their diff numbers are reported for the record:
  `36-graphics` 0.246 / 6.82 / 0.149; `02-planet` 0.452 / 14.82 / 0.145; `39-load` 0.742 / 26.31 /
  0.52; `09-custom` 0.535 / 16.56 / 0.307; `11-sandbox-world` 0.456 / 26.98 / 0.399;
  `38-controls` 0.275 / 7.11 / 0.127.
- The Java client's input stopped landing after the sandbox run (clicks/Escape ignored; process
  alive, no errors in `java-console.log`), which is why the menu-shaped rows after row 39 have no
  fresh Java frame. That is a harness limitation of this walk, not a client verdict.
- Godot `godot_exec` evals pinned the frame loop three times (nested lambdas / immediately after a
  restart); recovered with the gmcp watchdog. Avoided after that.

## 5. Ledger notes (for the coordinator; ledger/chains not edited)

- New finding candidates: **F-K1** (Custom Game grid clipping/non-scrolling — surface `ui`,
  HIGH), **F-K2** (Custom Game search empties the grid — surface `ui`, MEDIUM, needs a real-input
  retest), **F-K3** (sandbox boulders — surface `world`, MEDIUM, needs a like-for-like map),
  **F-K4** (menu background camera scale, LOW), **F-K5** (Graphics row height/knob/Bloom Intensity
  row, LOW), **F-K6** (planet page button placement, LOW), **F-K7** (Tech Tree button retest, LOW).
- Close/annotate from this run: PAR-0114 (three user maps now import with previews — verified),
  PAR-0338 (import fixed), the J02 T3 scrap-preview note (minimap ✓, Map Play still blocked),
  the J06 T3 slider item (framed rows landed; residuals are F-K5), J06 T4 keybind and J06 T5 Load
  Game items (MATCH).
- Rows left UNVERIFIED are listed per surface in §1 with their blockers; none of them contradict
  the J-round verdicts, they simply have no fresh frame.
