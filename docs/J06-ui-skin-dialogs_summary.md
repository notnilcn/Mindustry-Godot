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
