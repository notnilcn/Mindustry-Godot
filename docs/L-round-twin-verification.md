<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round-L twin verification (2026-10-17)

Verifier: Claude Opus 5.5, main checkout @ `39a83ff0`. All 111 round-J commits and the K report
are on main. `libmind_gdext.so` was rebuilt fresh for this walk (`tools/build.sh`, 22:48); the
build on disk before that predated every J merge. This is an independent re-check of
`docs/K-round-twin-verification.md` and of the round-J closures. Multiplayer and features that
need in-depth gameplay are skipped, as instructed.

Setup:
- Both clients on the xrdp display `:10`, driven through `tools/twin.sh` (J10 T1).
- Java reference: `parity-java-home`, 1152x648.
- Godot: session 1, bridge 6970.
- Evidence: `runs/20261017-L-twin/{java,godot,diff}/`. `diff/*-side.png` = Java left, Godot
  right; `diff/*-zoom.png` = 2x crops.

Side effects:
- Java `settings.bin` was backed up and restored byte-identical (`cmp`).
- The 1 Java and 6 Godot custom-game autosaves made during the walk were moved to
  `runs/20261017-L-twin/{java,godot}-session-saves/` by `twin_stop`.
- No code was changed.

Harness note: hover now uses the **real pointer** (`GP`/`JM`), and wheel scrolling uses real
pynput wheel events. Several K-round blockers turned out to be harness artefacts of injected
input, not client bugs (§3).

## 1. Verdicts

| Surface | Verdict | Notes (Java vs Godot) |
|---|---|---|
| Main menu | MATCH (structural) | Shadows, edge blending, flyers present; the menu world is random per boot on both. The K camera-scale finding (F-K4) did not reproduce: tile size is comparable in this pair. |
| Play submenu | MATCH | |
| Custom Game list | MATCH | 5 columns, previews and mode icons for all 27 maps, including the 3 formerly failing ones. **A real wheel scrolls the grid** (`godot/09-custom-scrolled.png`), so F-K1 (PAR-0409) is a harness artefact. The order differs only because the two profiles' map files have different mtimes; Godot follows Java's comparator. |
| Custom Game search | **MISMATCH** | Searching `sand` empties the grid completely, with no `@maps.none` label either (`godot/09-search-pair.png`). The filter logic in `map_list_dialog.gd:_passes_filter` is correct, so the cards are built but never laid out (layout family L01). F-K2 confirmed, with the root cause narrowed. |
| Map Play (sandbox) | MATCH | Floating layout, mode grid, scrap dot in the preview. |
| Map Play (Ancient Caldera) | **MISMATCH** | Content shifted right and clipped at the panel edge on first **and** second open (`diff/41-caldera-side.png`, `godot/41-caldera-2.png`). The J06 T7 fix (`37ef2a8d`) does not hold. |
| Sandbox world | MATCH | Core-zone stripes, scrap in the minimap, palette, HUD. Java's sandbox has no boulders either, so F-K3 as filed (sandbox) is profile drift; the real boulder gap is below. |
| Place duo + palette | MATCH | Affordability tint, `*/35`, 4 columns. |
| Item source config | PARTIAL | The `ItemSelection` grid, pick-closes-panel and centre colour all match. Godot lists **18** items vs Java's 16: the two extras are `fissile-matter` and `dormant-cyst`, which Java hides (`Items.java:128-135 hidden = true`; `ItemSelection.java:68 item.isHidden()`). |
| Turret feeding | MATCH | 30 ammo, copper centre (`diff/23-fed-zoom.png`). |
| Pause menu (custom) | MATCH (minor) | Shows "Save & Quit". Godot's content sits ~20 px higher (vertical centring, L01). |
| Quit confirm | MATCH | |
| Caldera survival world | **MISMATCH** | Ores now load and re-roll (J01 T1 holds). **Boulders are much sparser** in Godot: about 1–2 per screen vs Java's 5–7, and **no sand boulders** at all. Root cause in code (§2 G2). |
| Core hover (real pointer) | MATCH | `Core: Shard` + Health + Capacity 4.0k in both. |
| Game Over (custom) | MATCH (layout residual) | Waves Defeated 2, Time Played and row colours now match. Godot's rows start right under the title; Java has a `black3` pane with margins and the rows lower. |
| Editor maps dialog | MATCH | |
| Editor Map Info | **MISMATCH** | First open draws the content at the bottom, clipped under the buttons (`godot/t4.png`). The second open is top-aligned (`godot/t5.png`); Java centres it. |
| Editor view | PARTIAL | The core sprite now draws (J08 T1 holds). Remaining: (a) the palette still has the red `S` (`spawn`) in row 2, which shifts every later entry (Java sorts `OverlayFloor` after floors, `MapEditorDialog.java:844-870`); (b) the tool column uses grey-filled buttons and square checkboxes (Java black buttons, round toggles, a framed "Brush" button); (c) the map view has a thin red border instead of Java's accent frame. |
| Editor menu | PARTIAL | Same 11 buttons, but the grid is top-aligned (Java vertically centred). Resize shows `#` instead of the resize arrows. |
| Editor exit confirm | MATCH | |
| Core Database (after a game) | **MISMATCH** | After returning to the menu from a game, Godot shows **every block unlocked**; Java shows locks. Godot's `session.phase` stays `game` after quitting (`get_state_json().phase == "paused"`), so `database_json`'s `!is_campaign && phase.is_game()` override fires (`mind-gdext/src/campaign.rs:3226`). Java `DatabaseDialog.unlocked` = `(!isCampaign && !isMenu) || unlocked()` with `state.isMenu()` true after `logic.reset()`. |
| Planet page | PARTIAL | Orbit line and atmosphere are drawn, but **the orbit is oriented wrong**: Java's arc runs top-left → bottom through the planet (near-vertical plane), Godot's is a near-horizontal line. The Erekir/Serpulo/Difficulty column is still top-centre (Java top-left; F-K6 open). |
| Tech tree | MATCH (minor) | Opens on both with a real click, so F-K7 (PAR-0415) did not reproduce. Godot clips the tree above the button row; Java draws under it. |
| Settings > Graphics | **MISMATCH** | Godot's slider is a thin (~10 px) outlined track struck through the title. Java's `sliderBack` box spans the full ~32 px row with a solid knob (`diff/36-graphics-side.png`). Java **does** have a Bloom Intensity row (hidden under the tooltip in the K frame), so that part of F-K5 is wrong. |
| Legged units (Godot only) | PARTIAL (unverified) | Legs, joints and feet now draw for atrax/arkyid (`godot/52-legs.png`). No Java pair was staged (needs in-depth play), so the leg-segment geometry is unconfirmed. |

Not re-walked (unchanged since K or skipped as in-depth gameplay): campaign Ground Zero HUD,
combat/turret firing frames, unit factories, Accelerator, mods dialog, Language, Load Game
(MATCH in K), Controls (MATCH in K).

## 2. Findings (gaps that remain, besides multiplayer)

**Confirmed live in this walk**

1. **G1 (HIGH) Layout family: `MindTable` never propagates its minimum size.**
   - Symptoms:
     - Map Play clipping (Caldera);
     - Map Info first-open content at the bottom, then top-aligned;
     - the filtered Custom Game grid renders empty;
     - fill-dialog contents are top-aligned instead of centred (pause menu, editor menu, Game
       Over pane).
   - `client/ui/mind_table.gd` overrides `_get_minimum_size()` but never calls
     `update_minimum_size()` from `add()`/`clear()`/`clear_children()`/`row()`. The same is
     true of `mind_dialog.gd`/`mind_scroll.gd`. Parents therefore keep the previous content's
     size. Round I/J patched individual dialogs with `sort_now()` + deferred re-sorts, and those
     patches do not hold.
2. **G2 (HIGH) Floor `decoration` mapping is a name heuristic.**
   - `mind-core/src/maps/filters/mod.rs:143-147` looks up `<floor>-boulder`.
   - Java assigns decorations explicitly (`Blocks.java:700-822`): `stone`/`crater-stone`/`char` →
     `boulder`; `sand-floor` → `sand-boulder`; `ice`/`ice-snow`/`salt` → `snow-boulder`;
     `hotrock`/`darksand`/`magmarock` → `basalt-boulder`; `carbon-stone` → `carbon-boulder`;
     `ferric-stone`/`ferric-craters` → `ferric-boulder`; `beryllic-stone` → `beryllic-boulder`;
     `regolith`/`yellow-stone-plates` → `yellow-stone-boulder`; `arkyic-stone` →
     `arkyic-boulder`; `crystalline-stone` → `crystalline-boulder`; `rough-rhyolite` →
     `rhyolite-boulder`; `dense-red-stone` → `red-stone-boulder`; `redmat` → `redweed`;
     `bluemat` → `pur-bush`.
   - Every one of these floors gets **no decoration**. The most common floors (stone, sand) are
     in the list.
   - Consumers: the default `ScatterFilter` stack (custom maps), `BasicGenerator.decoration`
     (`maps/generators/basic.rs:837`), and the Serpulo/Erekir/asteroid generators
     (`maps/planet/serpulo.rs:1774`, `erekir.rs:891`, `asteroid.rs:343`). Campaign sector
     terrain is therefore also wrong.
3. **G3 (MEDIUM) Session phase is not reset to menu on quit.** The Core Database shows
   everything unlocked after any game. Other consumers of `session.phase.is_game()` are
   suspect too (audit them).
4. **G4 (MEDIUM) `ItemSelection` ignores `isHidden()`.**
   `client/ui/widgets/mind_item_selection.gd:_visible_options` filters `unlocked` and
   `on_planet` only, and `config_spec` carries no `hidden` flag. Fissile matter and dormant
   cyst appear in item sources/sorters/unloaders.
5. **G5 (MEDIUM) Slider skin.** J06 T3 framed the track but did not use `sliderBack` at full
   row height. The knob is the wrong size/colour.
6. **G6 (LOW) Planet orbit orientation** (J07 T1 landed with the wrong orbit plane/basis), and
   the planet button column placement (F-K6).
7. **G7 (LOW) Editor shell skin and palette order.**
   - The `spawn` entry is sorted with floors and drawn as a red `S`.
   - Tool buttons are grey-filled; checkboxes are square; the Brush row differs; the map frame
     is wrong.
   - Resize uses a `#` icon.
   - J08 T2 recorded the sort but did not fix it.

**Recorded by round J as residuals and still open (code-confirmed, not re-walked)**

8. **Unit/turret draws:**
   - PAR-0365 (unit `parts`), PAR-0366 (7 ability draws), PAR-0367 (weapon `layerOffset`),
     PAR-0368 (crawl segments), PAR-0369 (payload draws);
   - PAR-0363 residual turret part trees (scathe/smite/malign/lustre);
   - PAR-0399: the explosion view half. `ExplosionSink` has no host in `mind-gdext`, so reactor
     and generator explosions show no effect, sound, shake or scorch.
9. **Block UI:** PAR-0379 (hovered-block bars for most families), PAR-0380 (TargetDummy /
   CoreBlock team / LightBlock picker / MessageBlock cap), PAR-0301 (content-info stat
   families), PAR-0302 (hint predicates).
10. **Sim:**
    - PAR-0402: assembler output gate and hand-off; `router` ControlBlock; bullet suppression +
      `findPierceLength`/`findLaserLength`; tether `spawnerUnitId`; `DrawLiquidOutputs` sides;
      `itemEffMultiplier` row;
    - PAR-0391: `prebuildBase`, `allowCampaignRules`, Erekir generator rules / loadout / waves,
      abandon semantics, land info;
    - PAR-0296: Accelerator launch;
    - PAR-0377: custom-game saves write `mapname: sim`;
    - PAR-0378: the diagonal-proximity claim needs a Java check first. Java proximity is
      edge-only, so it is likely not a gap.
11. **Editor/logic** (PAR-0407):
    - WASD bindings + the `resetSaved` guard;
    - logic font rasteriser / `printchar` / `unicodeToName`;
    - display/canvas residuals (off-screen image pre-pass, edge sprites, PNG import/export +
      auto-save, 480 px surface);
    - logic-editor pickers (`EffectsDialog`, sound/unit/layer/content grids), enum columns,
      drag/jump polish, sensor ids;
    - 5 editor sub-dialog residues.
12. **Mod data:** PAR-0394 (`ModDataManager` owner + decorator, map `patches`), which blocks the
    map assets dialog (J08 T5). The `mod_fields.md` TODO rows (`UnlockFields`,
    `drillMultipliers`, assembler plans, weapon parts/effects, `draw` parsing) are also open.
13. **Goldens:** PAR-0362. Five suites fail on main (`blocks_golden`, `audio_golden`,
    `input_golden`, `logistics_golden`, `maps_editor_golden`). G2 will move the world/campaign
    checksums again.
14. **Evidence debt:** PAR-0408 needs the Java metal-tiles capture. The legged-unit geometry has
    no Java pair.

## 3. Corrections to the K report and the ledger

| K item | Round-L result |
|---|---|
| F-K1 / PAR-0409 grid clipped/non-scrolling | **Not a bug**: a real wheel scrolls; 5 columns fit. Close as a harness artefact (injected wheel/scroll writes do not reach `ScrollContainer` the way real input does). |
| F-K2 / PAR-0410 search empties grid | **Confirmed** (programmatic `text_changed`; typing could not be injected, see §4). Root cause G1. |
| F-K3 / PAR-0411 sandbox boulders | **Retarget**: Java's sandbox has no boulders either. The real gap is G2 (decoration mapping), seen on Caldera. |
| F-K4 / PAR-0412 menu camera scale | **Not reproduced**; close or keep as low-confidence. |
| F-K5 / PAR-0413 slider rows | Bloom Intensity **is** in Java; drop that clause. The slider skin mismatch stands (G5). |
| F-K6 / PAR-0414 planet buttons | Confirmed; add the orbit orientation (G6). |
| F-K7 / PAR-0415 Tech Tree click | **Not reproduced**; close. |
| J06 T7 / Map Play portrait clipping | **Reopen** (G1). |

## 4. Harness limits met

- Injected `godot_input key` events carry no unicode, so a focused `LineEdit` receives no text.
  pynput typing goes to the X-focused window, which stays the Java client: the embedded game
  window never becomes `_NET_ACTIVE_WINDOW`. Text-entry tests need a `godot_input` text path
  that sets `unicode`, or an eval-side `push_input` of `InputEventKey` with `unicode`.
- The first Java click after a pointer jump from another window is sometimes swallowed. Hover
  the target (`JM`) before clicking, as `JWC` does.
- A `godot_exec` with a wrong node path still stalls the frame loop; the gmcp watchdog
  recovered it (restarting the game).
