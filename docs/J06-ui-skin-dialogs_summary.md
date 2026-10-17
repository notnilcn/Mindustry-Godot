<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J06 — UI skin, dialog shells and per-dialog fidelity (session 3, wave B)

Branch `parity/session-3`, base after the wave-A merge `3bd1abe8`, session 3
(display :12, bridge 6990). Wave-B scope: T1, T2, T3 (all HIGH). **All three
landed and are verified in-engine against the stored twin frames.**

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — `TextButton` + icon layout and the `defaultt` skin | done (verified in-engine) | `0cf55155` |
| T2 — Dialog shells (`BaseDialog`, `showConfirm`, fill vs float) | done (verified in-engine) | `988ec70d` |
| T3 — Slider style (`Styles.defaultSlider`) | done (verified in-engine) | `65dd3052` |

## T1 — TextButton + `defaultt`

Java spec: `Table.button(text, image, runnable)` (`Table.java:536-550`) builds a
`TextButton` whose first cell is the icon at `image.imageSize()` and whose label
(`TextButton.java:30-34`, `add(label).grow().wrap()`, centred) grows over the
remaining width; `Fonts.icon` is a 30 px font (`Fonts.java:78-82`);
`Styles.defaultt`/`defaultb` use `Tex.button` (black body, grey border),
`buttonOver` on hover, `buttonDown` while pressed (`Styles.java:118-125,133-141`).

- `theme_builder.gd`: `defaultt`/`defaultb` map to the packed
  `button`/`button-over`/`button-down`/`button-disabled` nine-patches
  (`_button_nine_patch`; 12 px splits, which also pad the content like Arc's
  background padding) with Java's white/gray font colours. Other text-button
  styles keep their previous approximations.
- `mind_widgets.gd::icon_button` now reproduces Java's cell layout: the glyph at
  the icon font's 30 px size as the first cell and the label expanding over the
  rest, centred. Icon-only entries (empty text) stay centred like Java's
  `button(Icon.x, style, size)` image buttons and skip the label cell. The grep
  for remaining inline glyph+text labels found none — every `button(text, icon)`
  call site already routed through this factory.

Evidence: `runs/20261017-s3-J06-T1/` — the campaign pause menu matches
`ref/java-06-pause.png` (nine-patch bodies, 30 px icons left, centred labels);
the quit-confirm and editor-menu buttons match the same skin in
`ref/java-07-afterquit.png`/`ref/java-32-edmenu.png` (their shells are T2); the
editor menu's real-pointer hover shows the accent `buttonOver` border
(`diff/edmenu-hover-zoom.png`).

## T2 — dialog shells

Java spec: `BaseDialog.java:22` `setFillParent(true)`, `:24-25` accent title
rule, `:73-83` `addCloseButton` (210x64, `@back` + `Icon.left`);
`Styles.defaultDialog`/`fullDialog` = `stageBackground = black9`/`black` with
`background = windowEmpty` (no box); `UI.showConfirm`
(`core/UI.java:582-606`) = floating, text `width(500).wrap()` centred, buttons
`size(200,54).pad(2)`; Arc `Table.align = Align.center` (`Table.java:39`);
`PausedDialog.java:61,73-116`; `Control.java:412-420` + `Saves.addSave`/
`SaveSlot.save` for the `savecreate` autosave slot; `EditorMapsDialog.showMap`
(`ui/dialogs/EditorMapsDialog.java:100-163`).

- `theme_builder.gd`: `defaultDialog` also maps to an empty window
  (`windowEmpty`) — previously it filled a `grayPanel` sheet, which made every
  dialog a boxed slate panel.
- `mind_dialog.gd`: `fill_parent` (default true) applies the `BaseDialog` fill;
  `set_fill_parent(false)` keeps the floating content-sized mode.
  `add_close_button` builds the Java `addCloseButton` (defaultt `icon_button`,
  210x64) instead of the flat pane approximation.
- `mind_table.gd`: the whole grid centres inside the table bounds (Arc
  `Table.align`), which keeps a fill dialog's narrow button columns centred;
  grids that fill their cell are unaffected.
- `mind_dialog_base.tscn`: the content column centres vertically (Java `cont`
  semantics), so a short dialog no longer clips under the scroll body.
- `paused_dialog.gd`: the campaign branch drops the whole `@savegame`/
  `@loadgame` row (Java builds none), `@quit` keeps `dw + 10` = 230 and centres
  in its two-column cell, and the `tpad = -60f` bias is reproduced with a small
  bottom spacer. The `@save.quit` label now reads the real current slot
  (`MindCampaign.current_save_is_autosave`).
- `campaign.rs`: `Control.playMap`'s `savecreate` slot now registers through
  `Saves.addSave`/`SaveSlot.save` semantics — the fresh slot becomes current
  (autosave by default, `Saves.java:445-447`), so a custom game's pause menu
  reads `@save.quit` and `runExitSave` writes the same slot.
- `editor_map_dialog.gd`: the map-info popup is Java's 2x2 grid (300 px preview,
  300 px `Styles.black` info pane, two action buttons filling their columns);
  `editor_menu_dialog.gd` drops its fixed 560x512 panel min.

Evidence: `runs/20261017-s3-J06-T2/` — pause menu (`java/06-pause`) rows within
5-8 px at the same 65 px pitch; quit confirms (`java/07-afterquit`,
`java/26-quitconf`) floating with the 500 px text and 200x54 buttons; Map Info
(`java/30-mapinfo`), editor menu (`java/32-edmenu`) and Load Game (`java/39-load`)
render box-free with the full-width title rule. Residual: the load dialog's slot
previews still show the placeholder logo (J06 T5's preview work).

## T3 — slider style

Java spec: `Styles.defaultSlider` (`ui/Styles.java:374-378`) = `sliderBack` track
+ `sliderKnob`/`sliderKnobOver`/`sliderKnobDown` grabbers;
`SettingsMenuDialog.SliderSetting.add` (`ui/dialogs/SettingsMenuDialog.java:846-869`)
= a `stack(slider, content)` at `width(min(screen / 1.2, 500))` with the
title/value content margins `(3, 33, 3, 33)`.

- `theme_builder.gd`: `defaultSlider` maps to the packed sprites — `slider-back`
  as the track (`_nine_patch("slider-back", 4.0, 9.5)`; the explicit content
  margin is what makes Godot's `HSlider` size its track from the 27 px sprite
  instead of collapsing it to the 8 px border minimum),
  `grabber`/`grabber_disabled` = `slider-knob`, `grabber_highlight` =
  `slider-knob-over`, empty `grabber_area`/`grabber_area_highlight` (Java draws
  no filled progress bar). `_nine_patch` is the generalised factory behind
  `_button_nine_patch`.
- `settings_menu_dialog.gd`: the slider row's title/value labels centre
  vertically (`Styles.outlineLabel` cells are centred in Arc; a Godot `Label`
  defaults to top alignment and rode above the track).

Evidence: `runs/20261017-s3-J06-T3/` — the Graphics page renders every slider as
the Java dark track with the grey border and the knob at the value position
(`diff/graphics-side.png`, `diff/slider-row-compare.png`), and a real pointer
over the Screen Shake row shows the accent `sliderKnobOver` knob at the same
position as `ref/java-36-graphics.png` plus the description tooltip
(`godot/02-graphics-hover-tooltip.png`, `diff/graphics-hover-side.png`).
Residual: this save state lists the newer `bloomintensity` row the reference
frame predates, shifting the rows below it by one (the 46 px pitch matches).

## Gates

- `cargo fmt --check` clean (final tree), `cargo clippy -p mind-gdext
  --all-targets -- -D warnings` clean per task, `cargo test -p mind-core`
  2724 passed (the only failure is the pre-existing
  `blocks_golden::family_counts_match_golden` documented in the J02 wave-A
  summary), Godot headless editor gate (import/parse) clean after every
  GDScript/scene change.
- Shared files touched additively: `mind_widgets.gd` (`icon_button` layout),
  `theme_builder.gd` (`defaultt`/`defaultb`/`defaultDialog`/`defaultSlider`,
  `_nine_patch`), `mind_dialog.gd` (`fill_parent`, `add_close_button`,
  `set_fill_parent`), `mind_table.gd` (grid centring), `paused_dialog.gd`
  (row geometry + save slot), `editor_map_dialog.gd`,
  `editor_menu_dialog.gd`, `settings_menu_dialog.gd`, `mind_dialog_base.tscn`.

## Ledger changes to apply

- New item: "Text buttons render the glyph inline in the label with a flat slate
  fill (no `defaultt` nine-patch/cell layout)" → closed by `0cf55155` (T1),
  verified against frames 06/07/32.
- New item: "Dialogs draw boxed slate panels instead of the Java fill/floating
  shells" → closed by `988ec70d` (T2), verified against frames 06/07/26/30/32/39.
- New item: "Sliders render as thin Godot sliders through the text (no
  `defaultSlider` skin)" → closed by `65dd3052` (T3), verified against frame 36.
- Related twin findings that remain open elsewhere: frame 39's save-slot
  previews (J06 T5), frame 36's extra `bloomintensity` row (content/settings
  model, not skin).
- No chain/ledger files were edited by this session.

---

# Wave C — J06 T4, T6, T7, T8, T9 (T5 deferred)

Session 3, 2026-10-17. Branch `parity/session-3` fast-forwarded onto main
(`5b810821`, wave-B branches of J02/J06 merged) before this work. T5 (Load Game
dialog) stayed deferred per the coordinator: session 2 owns `load_dialog.gd`
while landing J01 T4/T5.

## Per-task status (wave C)

| Task | Status | Commit |
|---|---|---|
| T4 — Rebind Keys dialog | done (verified in-engine) | `8b197956` |
| T6 — Game Over dialog | done (verified in-engine) | `9da3b3b2` (+ `31302153` fmt) |
| T7 — Map Play clipping | done (verified in-engine, 27-map sweep) | `37ef2a8d` |
| T8 — Placement palette details | done (verified in-engine) | `a35307a8` |
| T9 — Small fidelity items | landed 1-5, 2 items recorded | `9eb88e02` |
| T5 — Load Game dialog | **deferred** (session 2 lands J01 T4/T5) | — |

## T4 — Rebind Keys (`KeybindDialog`)

`ui/dialogs/KeybindDialog.java:88-131`: category header + gray rule; rows
name (`padLeft 8`, `padRight 40`) → key cell (`left()`, `minWidth(90)`,
`padRight(20)`) → Rebind/Reset at `bw x bh` 140x40; the axis cell is
`min + " [red]/[] " + max` through Arc colour markup, coloured `Pal.accent`
(`Color.darkGray` unset); reset-all `button("@settings.reset", Icon.refresh,
Styles.grayt)`.

- `keybind_dialog.gd`: the key label translates the markup via
  `render_markup` before assigning it to the BBCode label (the raw
  `A [red]/[] D` string was shown literally) and uses the rich label's
  `default_color`; the key cell no longer `fill_x_axis`; the binds table keeps
  the 845 px width Java's table spans and centres; reset-all uses
  `icon_button("refresh", …)`.
- Evidence `runs/20261017-s3-J06-T4/`: keys in accent with the red `/`
  separator, left-aligned cells, centred ~845 px table (`diff/axis-markup-zoom.png`,
  `diff/rebind-keys-side.png` vs `ref/java-38-controls.png`).

## T6 — Game Over (`GameOverDialog`)

`ui/dialogs/GameOverDialog.java`: `super("@gameover")` + `titleTable.remove()`
(`:25-30`); a centred 600 px column (`:102`); a `Styles.black3` pane with one
50 px `StatLabel` per stat — `delay = 0.05 * i`, scale-in from x=0 and tint
`Pal.accent` → `Pal.darkestGray` over 0.3 s `pow3Out`, value counting up after a
further 0.3 s over 60 frames (linear below 10, `Interp.slowFast` = `pow2In`
above); only the wave row gated on `state.rules.waves`; a Time Played row while
a save slot is current (`Strings.formatMillis`); `@menu` 140x60 / `@continue`
170x60.

- **Stats path**: Java records the wave reached in the `GameOverEvent` listener
  (`Control.java:132-133`) and the remote `Logic.gameOver` (`Logic.java:435-437`);
  the port set `session.game_over` directly, so a core loss reported
  `stats.wave = 0`. `check_game_state`'s four detection sites now raise through
  `raise_game_over`, which sets `stats.waves_lasted = wave`.
- `GameStats::context_json` owns the `game_stats_json` keys (camelCase
  `STAT_ROWS` keys + the `waves` gate); `MindCampaign.current_save_playtime_ms`
  exposes `SaveSlot.getPlayTime()` for the Time Played row.
- `game_over_dialog.gd`: no title row, the 600 px centred block, the black3 pane
  (13 px margins, 12 px cells), the per-row tween as above, `Arc
  Strings.formatMillis` for the playtime value, 60 px buttons.
- Tests: `game::play::tests::core_loss_game_over_records_waves_lasted`,
  `game::stats::tests::game_over_context_keys_match_dialog_rows` (both fail
  before the stat fix).
- Evidence `runs/20261017-s3-J06-T6/`: sandbox core killed via
  `MindSimHost.debug_kill_building` → dark rows, `Waves Defeated 1` (was 0),
  `Time Played 0:00:58`, 140x60 Menu (`godot/01`, `godot/02` mid-animation,
  `diff/gameover-side.png` vs `ref/java-50-final.png`, `in-engine-probe.txt`).

## T7 — Map Play layout

Root cause: the floating dialog's panel is one reused `PanelContainer`
(`MapPlayDialog` is `setFillParent(false)`), so a rebuild lays out against the
previous content's width; the resize landing mid-sort was dropped by
`MindTable`'s `_sorting` guard, leaving the cells at the stale width's offsets.

- `mind_table.gd`: a `NOTIFICATION_RESIZED` during a sort is latched
  (`_resort_after_sort`) and re-queued when the sort finishes.
- `map_play_dialog.gd`: the build ends with a deferred `_resort_tables()`.
- The preview cell was already constrained (250x250 `custom_minimum_size`,
  `EXPAND_IGNORE_SIZE` + `STRETCH_KEEP_ASPECT_CENTERED`), matching Java's fixed
  cell with `Scaling.fit`.
- Evidence `runs/20261017-s3-J06-T7/`: `ref/java-41-caldera.png` side-by-side and
  a sweep of all 27 registered maps (`map-layout-sweep.txt`, `sweep-tool.py`)
  showing identical geometry (panel 428 px at x=362, image 250x250 at x=451,
  mode table at x=396).

## T8 — Placement palette

`ui/fragments/PlacementFragment.java:313-323`: each cell's per-frame
`button.update` tints the icon `Color.gray` unless
`state.rules.isInfiniteResources(player.team())` or the core holds every
requirement scaled by `buildCostMultiplier` **and** `player.isBuilder()`, with a
`Color.darkGray` override when `block.isPlaceable()` is false; the pane is a
`smallPane` scroll over a 4x46 px grid.

- `mind-core::world::build::palette_tint` owns the precedence (unplaceable wins)
  with a five-case unit test; `MindSimHost.palette_tints_json` computes the tint
  for every content block in one call from the same inputs as
  `block_requirements_json` (`player_core_items`, `player_can_build`,
  `supports_env_bits`).
- `placement_fragment.gd` re-applies the tints on the 10-frame HUD poll cadence
  through the per-state `icon_*_color` overrides (only the icon tints, so the
  checked `selecti` frame keeps its accent).
- `placement_fragment.tscn`: the ScrollContainer reserves the 8 px bar
  (min width 204) and the grid keeps its exact 196 px, so the bar no longer
  covers the fourth column.
- Deviation recorded: the plan's "category buttons with no cell background" note
  is inaccurate — Java's rail uses `clearTogglei` (`up = black6`,
  `Styles.java:345-350`) and the port already matches.
- Evidence `runs/20261017-s3-J06-T8/`: sandbox (infinite resources) → every cell
  white like Java's rule; Ground Zero → 202 white / 245 gray / 0 darkGray of 447
  blocks and the palette matches `ref/java-46-conv-sel.png`; geometry probe shows
  the bar beside the grid (`in-engine-probe.txt`).

## T9 — Small fidelity items

Landed:
1. Sidebar parent checked: `flatToggleMenut` (`Styles.java:210-219`,
   `MenuFragment.java:271,291`) — `flatOver` hover + the packed `flat-down-base`
   checked frame (was a translucent-white tint).
2. No keyboard focus frames: every button style gets an empty `focus` stylebox
   (the sector page's Go button showed one, `godot/03-sector.png`).
3. Core Database backdrop: verified translucent after T2's `windowEmpty`
   (`Styles.java:458-469`) — the menu shows through the black9 stage like Java.
4. Sector panel state lines ("Under attack!") centre (`planet_dialog.gd`).
5. `CheckBox` uses the round `check-on`/`check-off` indicators
   (`Styles.java:446-456`) instead of Godot's squares.

Recorded, not landed (`runs/20261017-s3-J06-T9/notes.md`): the wave panel's
fixed-width wrap (Java wraps "N Enemies Remaining" left-aligned; the port renders
the Rust `hud_text` block in one centred label, `diff/43-wave-side.png`) and the
editor palette's selected-cell highlight (`diff/31-editor-open-side.png`); Java's
checkbox hover icons have no Godot equivalent.

Evidence: `runs/20261017-s3-J06-T9/` (`godot/01-menu-parent-checked.png`,
`godot/02-sector-no-focus-frame.png`, `godot/03-coredb-translucent.png` +
`diff/` side-by-sides).

## Gates (wave C)

- `cargo fmt --check` clean; clippy `-D warnings` clean for `mind-core` +
  `mind-gdext` per task; `cargo test -p mind-core` 2732 passed (the only failure
  is the pre-existing `blocks_golden::family_counts_match_golden` documented in
  the J02 wave-A summary), `-p mind-gdext` 67 passed; Godot headless
  import/parse gate clean after every GDScript/scene change.
- Shared files touched additively: `mind_table.gd` (resort latch),
  `theme_builder.gd` (focus styleboxes, `CheckBox` icons, `_nine_patch` reuse),
  `menu_fragment.gd`, `planet_dialog.gd`, `placement_fragment.gd`/`.tscn`,
  `map_play_dialog.gd`, `game_over_dialog.gd`, `keybind_dialog.gd`;
  `load_dialog.gd` untouched (deferred T5).

## Ledger changes to apply (wave C)

- New item: "Rebind Keys shows raw `[red]`/`[]` markup and white keys" → closed
  by `8b197956` (T4), verified against frame 38.
- New item: "Game Over shows Waves Defeated 0, no Time Played row, bright accent
  rows and a duplicated title" → closed by `9da3b3b2` (T6), with the stats fix
  covered by two new tests; verified against frame 50.
- New item: "Map Play lays out against the previous map's panel width (portrait
  previews clip)" → closed by `37ef2a8d` (T7); the shared `MindTable` resort bug
  is the root cause, swept over all 27 maps.
- New item: "Placement palette ignores affordability and clips the fourth
  column under the scroll bar" → closed by `a35307a8` (T8), verified against
  frame 46 + a unit test.
- New item: "Sidebar parent button never stays checked / Godot focus frames /
  square checkbox indicators" → closed by `9eb88e02` (T9). Two residuals noted
  for the wave-D queue: the wave panel's fixed-width wrap and the editor
  palette's selected-cell highlight.
- No chain/ledger files were edited by this session.

---

# Wave D — J06 T5 (Load Game dialog)

Session 3, 2026-10-17 (branch fast-forwarded onto main `7178b401`).

## T5 — Load Game restyle — done

| Task | Status | Commit |
|---|---|---|
| T5 — Load Game dialog cards/buttons | done (verified in-engine) | `23828d01` |

Java `LoadDialog` (`ui/dialogs/LoadDialog.java:30-200`): `addCloseButton()`
(210x64) + `buttons.button("@save.import", Icon.add, …).fillX().margin(10f)`;
the search toggles are `emptyTogglei` `size(60)`; each card is a `Styles.grayt`
button with `title.add("[accent]" + name).width(230).wrap()`, four `size(40)`
`emptyTogglei`/`emptyi` icon buttons (autosave, delete, rename, export), a
160 px `BorderImage` preview and a 280 px meta column whose values carry the
`[lightgray]` markup.

- `theme_builder.gd`: `grayt` (up `grayPanel`, over/down `flatOver`, disabled
  `grayPanelDark`; `Styles.java:157-165`), `flatt` (`:147-155`),
  `emptyi`/`emptyTogglei` (`:272-282`: no background, per-state icon colours).
- `load_dialog.gd`: accent titles at 230 px, 40 px icon buttons, a 160x160
  preview cell, the 280 px meta column with `[lightgray]` values (`_fmt` now
  translates markup — it was rendering literally and widening the cards), 60 px
  filter toggles, `add_close_button()` + a 210x64 import button.
- Previews/ordering (J01 T4/T5) untouched and re-verified.

Evidence: `runs/20261017-s3-J06-T5/` (`godot/01-load-cards.png`,
`diff/load-side.png`, refs incl. the twin's pre-restyle frame).

## Ledger changes to apply (wave D, J06)

- New item: "Load dialog cards render white titles and a slate body with tiny
  buttons" → closed by `23828d01` (T5), verified against frame 39.
- No chain/ledger files were edited.
