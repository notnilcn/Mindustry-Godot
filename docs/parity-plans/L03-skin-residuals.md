<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L03 — Skin and presentation residuals

**Goal.** Close the presentation mismatches that survived round J. Each one is small, but they
are visible on high-traffic screens.

**Depends on.** L01 (dialog centring) for T5. Run T1 first: the slider style is shared.

**Build.** This plan is pure GDScript (`client/ui/theme/theme_builder.gd`, `client/ui/`,
`client/scenes/`): no cargo build, no `.so` reload — verify in the running editor.

## T1 — Slider skin = `Styles.defaultSlider` at full row height — HIGH

- Java:
  - `SettingsMenuDialog.SliderSetting` (`ui/dialogs/SettingsMenuDialog.java:840-870`)
    `table.stack(slider, content)`;
  - `Styles.defaultSlider` (`ui/Styles.java:374-378`): `background = sliderBack`,
    `knob = sliderKnob`, `knobOver`/`knobDown`.
  - The `sliderBack` nine-patch fills the whole ~32 px row (a dark box with a grey border), and
    the knob is a full-height accent-tinted block (`java/36-graphics.png`).
- Port (round L, `godot/36-graphics.png`): a ~10 px outlined track runs through the middle of
  the title text, with a slim grey grabber. J06 T3 (`65dd3052`) mapped the styles, but the
  stylebox's content margins/expand margins and the grabber icon size do not reproduce the
  nine-patch geometry.
- Change in `client/ui/theme/theme_builder.gd`:
  - Build the `HSlider` `slider` stylebox from the `sliderBack` region as a
    `StyleBoxTexture` with Java's nine-patch splits, so it **expands to the control height**.
  - Set `grabber_area`/`grabber_area_highlight` to empty.
  - Size the `grabber`/`grabber_highlight` icons from the `sliderKnob*` regions at their atlas
    size × `Scl`.
  - Set the `center_grabber` / `grabber_offset` constants so the knob sits inside the box.
  - Make `_add_slider_row` give the slider the full stack height.
- Accept: the Graphics page matches `java/36-graphics.png` (row box, knob, title/value
  overlay). Extend `ui_widgets_check.gd` to assert the slider's stylebox min height equals
  the row height.
- Ledger: PAR-0413 (drop the "extra Bloom Intensity row" clause: Java has that row).

## T2 — Planet page: orbit orientation and planet button column — MEDIUM

- Orbit:
  - Java `PlanetRenderer.renderOrbit` (`graphics/g3d/PlanetRenderer.java:171-179`) draws
    `Angles.circleVectors(points, radius, (cx, cy) -> center + (cx, 0, cy))`, a circle in the
    parent's **XZ plane**, seen through the planet camera (`:60-75`).
  - In Java's frame the arc runs from the top-left down through the planet
    (`java/02-planet.png`). Godot draws a near-horizontal line (`godot/02-planet.png`), so the
    port's plane, the camera up vector or the parent position is wrong.
  - Fix: compare the port's orbit vertices and camera basis with Java for Serpulo
    (`orbitRadius`, `parent = sun`), then fix the transform.
- Button column:
  - Java `PlanetDialog` puts the planet list + Difficulty column **top-left**
    (`ui/dialogs/PlanetDialog.java:733-745`: `t.top().left()`, the Difficulty button `size(208, 40)`).
  - The port centres it (`ui/dialogs/planet_dialog.gd`). Move it (PAR-0414).
  - Also compare the globe size: K said Godot's is smaller, but in the round-L pair it looks
    comparable. Measure the radius in both frames before changing `camLength`.
- Accept: the planet page matches `java/02-planet.png` apart from per-profile sector state.

## T3 — Editor shell skin — MEDIUM

From `diff/31-editor-open-side.png`:
1. Tool buttons: Java `Styles.squareTogglei` (tools, grid) and `Styles.flati` (menu, undo,
   redo, rotate) on the dialog's `Styles.black` background (`MapEditorDialog.java:57,554,630-661`),
   with an accent frame on the selected tool. Godot uses grey-filled cells and the selected tool has
   no frame.
2. "Show Blocks/Terrain/Floor" checks: Java `mid.check(...)` with the default round check
   style, and Center is a `Styles.flatt` button (`MapEditorDialog.java:711-722`); Godot
   uses square check glyphs (J06 T9 fixed the generic `MindCheck`, but the editor builds its
   own).
3. "Brush" row: Java has a framed `Brush` text button plus a size slider; Godot has a dot + text.
4. Map view frame: Java draws the map bounds with `Lines.rect` in `Pal.accent`
   (`editor/MapView.java:253-325`);
   Godot has a thin red border.
5. Editor menu Resize icon: Java `Icon.resize`. Add the glyph alias to `MindIcons` if the icon
   font has it, otherwise record the deviation.

Java: `editor/MapEditorDialog.java` (tool table, `addBlockSelection`), `editor/MapView.java`.
Accept against `java/31-editor-open.png` and `java/32-edmenu.png`.

## T4 — HUD details — LOW

1. Wave skip button: Java draws `Icon.play` in a clear (`cleari`-style) cell inside the wave
   panel. Godot draws a black rounded pill with a white rim (`diff/11-sandbox-side.png`
   top-left).
2. Wave panel wrap (PAR-0386): Java wraps "2 Enemies / Remaining" left-aligned at the
   panel's fixed width (`HudFragment` wave table).
3. Hovered-block info box placement: Godot's box sits flush to the right edge above the
   palette; Java's starts at the palette's left edge (`diff/49-hovercore-side.png`). Compare
   `PlacementFragment`'s `topTable` anchoring.

## T5 — Game Over pane and tech-tree clip — LOW (after L01 T2)

1. Game Over: Java wraps the stats in `t.pane(...).setBackground(Styles.black3)` with
   `margin(13)`, `grow().pad(12).top()`, and `minWidth(370).maxSize(600, 550)`
   (`ui/dialogs/GameOverDialog.java:72-95`). Godot's rows start right under the title with no
   pane (`diff/44b-side.png`).
2. Tech tree: Godot clips the node view above the bottom button row; Java draws the tree under
   the buttons (`diff/40-tech-side.png`).

## Done criteria

T1–T3 landed and matched against the named Java frames. T4/T5 landed or recorded. Summary in
`docs/L03-skin-residuals_summary.md`.
