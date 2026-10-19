<!-- SPDX-License-Identifier: GPL-3.0-only -->
# M01 — Twin-walk residuals (round M, 2026-10-19)

**Goal.** Close the small differences that the round-M twin walk and its twelve fix agents found but
left open. None of these blocks a screen. Each one is a visible pixel-level or
behaviour-level mismatch against the Java reference, or a code-confirmed bug with a visible symptom.

**Source.** The round-M walk (`runs/20261019-M-twin/`), the per-fix runs
(`runs/20261019-M-fix-<slug>/`) and the closing walk (`runs/20261019-M-final/`). Java is on the left
and Godot on the right in every `diff/*-side.png` and `*-zoom.png`.

**Rules.** `L-environment.md` applies unchanged. Follow the twin recipe in
`runs/20261019-M-twin/BRIEF.md`: the `/tmp/tw.sh` preamble, `JX` hover-then-click, real wheel `S`,
and moving the Godot window to (1350,760). Work one task at a time, with one editor and at most one
Java client. A task is done only when a fresh twin pair from real input shows the gap closed, and
the gates for the touched tree pass.

**Profile normalisation first.** These differences come from the two profiles, not the code. Align
them or ignore them before you file anything:
- The universe clock `utimei` (Godot 304280, Java 262181) drives the planet's sun direction and
  orbit. Patch Godot's `config/settings.bin` to Java's value while the game is stopped, then
  restore it.
- `blockstatus` is on in Godot and off in Java. The red status diamonds and the no-ammo marker on
  freshly placed Godot blocks come from this setting.
- Sector state ("N under attack", enemy sectors), map-list order and the random menu world.

---

## T1 — Arc text line box and glyph spacing — MEDIUM

Arc's label line box is about 2 px taller than `MindLabel`'s, and the extra space sits above the cap
height. `Fonts.outline` also uses `spaceX -= borderWidth`. Round M worked around both locally,
and the remaining offsets are:
- Core Database category rules are 2 px high (Items y=166 vs Java 168; Blocks 246 vs 250).
- Map-play dialog content and pause-menu rows are about 2 px high. Custom Game cards are about 3 px
  high (`runs/20261019-M-final/diff/01-search-side.png`).
- The placement header with a selected block is 3 px short (frame top 316 vs 313). The two-line
  rich label is the short element (`runs/20261019-M-fix-palette/diff/41-pick-zoom.png`).
- Text inside the confirm-dialog and editor-menu buttons is 1–2 px off vertically.
- In the editor left column, the Brush label and the Show Blocks/Terrain/Floor text sit about 2 px
  high and 1 px right.
- Bar labels are about 2 px narrower than Java's over "Health". Outline glyphs sit about 1 px left
  of and above Java's.
- Godot wraps a line when the word plus its trailing space overflows; Arc does not. Map Info works
  around this with a 6 px right margin, but other wrapped labels may break one word early.

Fix:
- Give the label/theme font metrics Arc's line height, i.e. `Font.lineHeight` and the cap
  offset (Arc `Label.layout`, `GlyphLayout`). Do it once in the theme or `MindLabel`, not per
  dialog.
- Remove the per-dialog compensations that round M added: the Map Info line pitch and cap lead
  (`editor_map_dialog.gd`), the confirm-prompt +2/+1 px (`ui_root.gd` `_apply_base_dialog_spacing`)
  and the settings pane inset (`settings_menu_dialog.gd` `PANE_INSET_TOP`).
- Port the `Fonts.outline` advance (`spaceX -= borderWidth`) into the outline label variation.
- Match Arc's wrap rule (trailing whitespace does not count toward the wrap width).

Accept:
- Re-twin Core Database, Custom Game, map play, pause, the confirm dialog, the editor menu, Map
  Info and the hover bar header. Every text row lands within 1 px of Java.
- `ui_layout_check` gains a line-height assertion.

## T2 — Hand-made label outlines — LOW

`logic_dialog.gd:605-606` and `effects_dialog.gd:120-121` draw a 1 px black outline
(`outline_size 4`, `Color.BLACK`). Java's `Fonts.outline` is a 2 px `Color.darkGray` (#3f3f3f)
border (`Fonts.java:247-249, 264-271`). Switch both to the `outlineLabel` variation, which is fixed
in round M.

Accept: 8× crop pairs of a logic-editor statement label and an effects-dialog label.

## T3 — Effect random seed per instance — MEDIUM

About 75 effect bodies outside `fx/custom/wave_f56.rs` still seed with `e.id.raw()`, which is the
effect *type* id. Every instance of those effects therefore draws the same "random" pattern. Java
calls `rand.setSeed(e.id)` with a unique id per spawned effect (`Effect.java`, `EffectState.id`).
Round M added `EffectContainer.seed`, a unique value per spawn assigned in `fx/pool.rs:326`, but
only `wave_f56::seeded()` uses it.

Fix:
- Switch every `e.id.raw()` seed to `e.seed`, across `mind-core/src/fx/custom/*`.
- Expect the fx goldens to move. Record the Java citation (`Effect.java` `rand.setSeed(e.id)`) with
  the regen.

Accept:
- A twin pair of several simultaneous instances of one effect, e.g. place three conveyors quickly
  (`Fx.place`) or break blocks (`Fx.breakBlock`). The instances differ from each other, as in Java.
- `parity goldens` 0 FAIL after the regen.

## T4 — Liquid floor shaders other than water — MEDIUM

Round M wired `CacheLayer.water` through a `CanvasGroup` + `water_group.gdshader` (see
`mind-gdext/src/render.rs` `sync_water`). The other shader cache layers still draw plain, because
nothing binds them:
- `CacheLayer.mud`, `tar`, `slag`, `arkycite`, `cryofluid` and `space`
  (`graphics/CacheLayer.java`, `Shaders.java`).
- Their Java shaders also need `u_noise` (`sprites/noise.png`) and the `SurfaceShader` uniforms.

Fix:
- Generalise `sync_water` into a per-layer group: one `CanvasGroup` per shader layer, with a
  `<name>_group.gdshader` port that reads the screen texture and the noise texture.
- Feed `u_campos`, `u_resolution`, `u_time` and `u_noise`, all gated on `animatedwater`.
- Remove or repurpose the unused `client/shaders/water.gdshader`; the registry still loads it.

Accept:
- Twin pairs on maps that contain each floor: slag/tar on a Serpulo sector or a custom map; space and
  arkycite on Erekir maps.
- Compare the pattern scale, colour and direction (these shaders animate, so not exact pixels).

## T5 — Hovered-building info rows — MEDIUM

Round M ported the `ItemTurret` ammo row (`building_consumption_json`, `sim_host.rs:8968`). These
are still open:
1. **Other consumer rows.** `ConsumeItems`/`ConsumeLiquid`/`ConsumeItemFilter`/`ConsumePower` rows
   on crafters, generators and so on (`BuildingComp.displayConsumption`,
   `BuildingComp.java:1579-1580, 1659-1665`; each `Consume.build`). A good test block is a Graphite
   Press fed and unfed.
2. **Power bar when nothing is requested.** Java shows a full bar when `requestedPower == 0` and the
   graph has production or storage (`Block.java:720`). `building_bars_json` only reads `status`.
3. **Power node and source bars.** `PowerNode.setBars` (balance, batteries, connections). Godot
   shows only Health for power sources and nodes. Also check the link/config overlay when a power
   source is selected.
4. **Team gate.** `building_bars_json` returns bars for any team. Java shows the bars only for
   `team == player.team()`. Hover an enemy core in a survival map to see it.
5. **Turret header placement.** The ammo icon and turret name sit about 1–2 px high (folds into T1).

Accept: one hover twin pair per item. Use the sandbox map; Ancient Caldera survival for the enemy
team gate.

## T6 — Power-link residuals — LOW

- `getNodeLinks` in Rust omits Java's consumer-to-consumer adjacency exclusion
  (`PowerNode.java` ~line 300, `getNodeLinks`). Check that a consumer placed next to another
  consumer does not double-link.
- Laser end caps use the `lod_l2` alpha. Java uses `Lod.alpha1` for caps narrower than 10
  (`Drawf.laser`, `Drawf.java:520-535`).
- Re-check ordinary power-node lasers after round M's `laserScale = 0.25` cap fix
  (`render/links.rs:23,153`). That fix shrank every node's caps.

Accept: a twin pair of two power nodes linked to a battery and a consumer, plus a zoom on the caps.

## T7 — Planet page polish — LOW

Align `utimei` first (see the normalisation section).
- **Bloom.** Java's planet bloom (threshold 0.8, intensity 2.5) brightens the lit limb and the
  nearby rings. The glow environment in `planet_view.tscn` has no visible effect (PAR-0142).
- **Ring tint.** Sector rings render in Godot's transparent pass, so the atmosphere's screen
  texture misses them. Rings near the limb get less tint than in Java.
- **Rim weight.** On the lit side, Godot's rim is slightly heavier at the lower left. On the night
  side, the lit-limb glow is a little dimmer. Locked-sector cells read a touch lighter.
- **Clouds.** `clouds.gdshader` uses `cull_disabled`; Java culls back faces. Visible when zoomed out.
- **Unindexed meshes.** `_godot_winding` (`planet_view.gd` ~line 688) only flips indexed meshes.
  Make it flip unindexed ones too, so modded planets render right.
- **Harness.** Add a `twin_align_universe` helper to `tools/twin.sh`. It backs up Godot's
  `settings.bin`, patches `utimei` to the Java profile's value, and restores the file in
  `twin_stop`.

Accept: planet twin pairs at the Java clock (night side) and at the Godot clock (lit side), each
with a 2× zoom.

## T8 — Dialog shell and dialog-specific residuals — LOW

- The shell's 8 px side margin folds the rule's `pad(4)` into the panel, so the Body is 1 px
  narrower on each side than Java's `cont`. The Map Info scrollbar sits at x=1117 vs Java 1118.
- The Custom Game/Maps search icon draws at 24 px; Java uses the 30 px natural size, as Core
  Database already does.
- Round M flagged these dialogs `button_overlay` without a twin check: Join, Schematics, Mod
  Browser, Effects and the in-editor Map Load. Twin each one (title rule at y=30–32, body 3 px
  lower).
- The planet page Back and Tech Tree buttons sit 6 px right and 2 px up of Java's
  (`PlanetDialog.java:312,316` row placement).
- The editor menu grid sits about 2 px lower than Java's.
- Core Database:
  - The scroll knob is longer in Godot (95..250 vs Java 92..219), so the content is shorter.
    Find which category or entries are missing or shorter.
  - The `ARC_BAR_SQUEEZE` emulation also applies when a search makes the content non-scrolling.
    Java drops the 24 px narrowing then.
- The editor `SearchField` does not hide its "search" placeholder while focused
  (`MindWidgets.field` does).
- The weather "always" toggle (`custom_rules_dialog.gd:1322`) should be a plain Arc `CheckBox`
  (`CustomRulesDialog.java:566` `f.check(...)`), not the grayPanel `Elems.check`.
- Settings:
  - Compare the hover tooltip timing and placement with Java.
  - A slider knob was seen in its highlighted (yellow) state with the pointer elsewhere after a
    wheel scroll, and it cleared on the next pointer move
    (`runs/20261019-M-final/diff/07-graphics3-side.png`). Check that hover state follows the
    content when it scrolls under a still pointer, as Arc does.

Accept: one twin pair per bullet, at the same Java and Godot positions.

## T9 — Wheel routing edge cases — LOW

- **Nested panes.** In Godot the innermost hovered pane takes the wheel. In Arc the event goes to
  the scroll-focused pane and bubbles to the parent panes (`ScrollPane` `scrolled` listener,
  `ScrollPane.java:197-204`). Test this on the logic editor or custom rules, which have nested panes.
- **UI scale.** The Arc smooth-scroll minimum speed (200 px/s) is in canvas units. Check it at a UI
  scale other than 100%.
- **Block palette.** The wheel scrolled the palette on neither client. Confirm that is Java's
  behaviour (palette `ScrollPane` focus); if it is, record it and close.

## T10 — Map previews and minimap — LOW

- `generate_preview_from_tiles` (save-slot and generated previews) always passes team 0, because
  `MapSource` has no per-tile team. Java uses `tile.team()`, so those previews show buildings as
  derelict grey.
- Java's sandbox preview has a 1-pixel dot that Godot lacks
  (`runs/20261019-M-fix-followups/diff/m2-sbplay-zoom8.png`). Find which tile it is and why Godot
  drops it.
- Minimap units are a fixed 8 widget px. Java uses `Scl.scl(1) * tilesize * 3` world units
  (`MinimapRenderer.java:157-166`), so their size follows the zoom.
- Stale previews. Godot and Java only regenerate a preview when the cache file is missing, so
  existing users keep previews from older pixel paths. The Godot cache name is now `_v4` (one bump
  per pixel-path change; it never touches Java's files), which busts the pre-round-M team colours
  and the pre-tint `ColoredFloor`/`ColoredWall` colours.

## T11 — World draws that were fixed in code but not seen live — LOW

Stage each one and capture a twin pair:
- The core-protection overlay colour (now `Team.color`): a survival map with an enemy core, placing
  near its no-build radius.
- The unit walk dust tinted by the floor `mapColor` (now from `block_colors.png`): the alpha mech
  walking on stone and on sand.
- The landing dust after the round-M `camera.rs` change (`mapColor` alpha forced to 1).
- After landing, the alpha mech spawns at the core's top-right in Godot and bottom-left in Java.
  Compare `CoreBuild.requestSpawn` / `playerSpawn` placement with the Godot spawn path.

## T12 — Placement fragment leftovers — LOW

- Category glyphs are slightly thinner than Java's icon images (icon-font glyph vs texture region).
- Requirement rows on the placement header are ordered and laid out differently from Java
  (`runs/20261019-M-fix-followups/diff/s2-side.png`; `PlacementFragment.java:398-421`).
- The ledger probe paths in `parity-ledger.json`/`chains.json` still point at
  `.../placement/Panel/Layout/Left/InputBar/...`. Change them to `Panel/Frame/Layout/Left/Rows/InputBar`.

## T13 — Runtime panics seen during gates — LOW

- `MindWorldRenderer::on_notification` panics at boot with "Gd<T>::bind_mut() failed, already
  bound". The frame loop continues.
- `MindHud::process` panics with "access to instance … after it has been freed" at the exit of
  `ui_widgets_check.gd`, after the checks finish.

Both are mind-gdext lifetime bugs. Fix the borrow/ownership in each so the gates and boot log run
clean, and add a log assertion to `tools/ci.sh` that fails on a godot-rust panic.

---

## Order

T1 first: it moves text everywhere, and T5/T8 depend on its metrics. Then T3 and T4 (largest visual
effect), T5, T7, then the LOW tasks in any order. Each task ends with its twin pairs in
`runs/<date>-M01-T<n>/` and a one-line result in `docs/M01-twin-residuals_summary.md`.
