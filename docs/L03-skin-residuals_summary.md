<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L03 — Skin and presentation residuals (final: T1–T5)

Session A (session 2: worktree `Mindustry-Godot-session2`, branch `parity/session-2`, display `:11`, bridge 6980).

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T1 slider skin (PAR-0413) | **done** | Settings slider rows are full-height 500x42 `sliderBack` boxes with the 29x42 knob, as in Java. `ui_widgets_check.gd` asserts the box's minimum height. |
| T2 planet page (PAR-0420 orbit, PAR-0414 buttons) | **done** | Rail pinned top-left. Orbit centre moved into the planet's mesh frame (Rust, built through the flock). Globe size measured: matches, `camLength` unchanged. |
| T3 editor shell skin (PAR-0421) + Map Info frame (PAR-0429) | **done (wave 3)** | Java style variations, round checks, Brush slider box, Center icon, accent 3 px map frame, black background, Resize icon; Map Info rebuilt to `MapInfoDialog.setup`. Engine fix: colspan width. |
| T4 HUD details (PAR-0386) | **done (wave 3)** | `buttonRight` skip button, left-aligned wave text, placement frame flush to the corner (hover box x 840–1152 = Java). |
| T5 Game Over pane / tech-tree clip | **done (wave 3)** | Pane layout from L01 T2; here: HUD hidden behind Game Over, centred stat labels, tech tree unclipped under the buttons. |

## Commits

```
90ca8f77 L03 T1 prep: ui_widgets_check green and run by tools/ci.sh   (PAR-0423, see the L01 summary)
2e1cc2e6 L03 T1: settings slider rows are full-height sliderBack boxes
9ca2138c L03 T2: planet orbit centre in the mesh frame; planet rail top-left
2193f78f L03 T3: editor shell skin; Map Info frame; colspan width like Arc   (wave 3)
8ad9823b L03 T4: HUD skip button, left-aligned wave text, palette frame        (wave 3)
9a06374b L03 T5: Game Over hides the HUD, centred stat labels; tech tree unclipped (wave 3)
```

## T1 — slider skin

### What changed
- `theme_builder.gd`: Godot's `HSlider` draws its track at the stylebox's **minimum** height, centred. Arc's row is
  the 42 px `sliderKnob` tall, and the `sliderBack` 9-patch fills it (`Styles.java:374-378`). The box had a 19 px
  minimum, so a thin track ran through the row title. The box now has a 42 px minimum height
  (`SLIDER_ROW_HEIGHT`). The knob stays `sliderKnob`/`sliderKnobOver` (29x42).
- `_add_slider_row` already gives the slider the full stack height (`SIZE_EXPAND_FILL`), so no change was needed
  there.

### Tests
`godot4 --headless --path client --script res://tools/ui_widgets_check.gd` → `UICHECK: failed=0`. New assertions:
the `defaultSlider` box minimum height is 42, the knob region is 42 px, and a settings slider is 42 px tall. The
first one would fail against the old 19 px box.

### In-engine evidence (`runs/20261018-s2-L03-T1/`, real input)
Settings → Graphics (`GC`), pointer parked off the rows (`GP`). Against Java's `36-graphics.png`: row boxes x 314–814,
42 px tall, 46 px pitch, full-height knob — MATCH. Residual: Java's first row sits at y=67 (title rule 27) and the
port's at y=58 (rule 30). I found no source for Java's extra 9 px (`prefs.margin(14)`, `SettingsTable`); it is
recorded below.

## T2 — planet page

### What changed
- **Orbit (PAR-0420).** `PlanetRenderer.renderOrbit` draws the loop around the parent in **world** space
  (`PlanetRenderer.java:171-179`). Java rotates the planet mesh (`Planet.getTransform`, `Planet.java:407`) and the
  camera target (`Planet.lookAt`, `:692-694`) by `getRotation()`. The port renders the globe and the camera in the
  planet's mesh frame but passed the raw world offset as the loop centre, so the orbit sat off by the planet's spin.
  New `mind_core::render::g3d::orbit::orbit_center_in_planet_frame` turns the offset by `+getRotation()`, like
  `planet_light`'s sun direction (`HexMesh.java:22-27`). `mind-gdext/src/campaign.rs` uses it.
- **Rail (PAR-0414).** The planet list + Difficulty table is `t.top().left()` (`PlanetDialog.java:732-735`). The
  planet view now pins its root table top-left (`MindTable.table_top/table_left`, added in L01 T3). The rail is at
  x=3; before this change it was at x=471.
- **Globe size.** FOV 60 (`PlanetRenderer.java:42`) and `camLength` 4 are identical. Limb-to-limb diameter is
  ~340 px in Java and ~352 px in Godot, where the atmosphere rim glows all round. `camLength` was not changed.

### Tests
`cargo test -p mind-core --lib orbit` → 6 passed. New `serpulo_orbit_center_is_the_sun_in_mesh_frame`: Serpulo's
own orbit centre (the sun) lies along the mesh-frame light direction at the orbit radius for three universe times.
`cargo check -p mind-gdext` is clean and `cargo fmt --check` is clean for both crates. The `.so` was rebuilt with
`CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh`, then the editor was restarted
(`tools/session-play.sh 2`).

### In-engine evidence (`runs/20261018-s2-L03-T2/notes.md`)
Play → Campaign (real clicks). The Godot profile looks at an equatorial sector (`cam_pos.y = 0`), so the orbit is
edge-on: a horizontal line through the globe, which is geometrically correct for that camera. After a **real X-pointer
drag** on the globe (`cam_pos.y = −0.41`), the orbit is an inclined line passing behind the planet, the same geometry
as Java's arc (`godot/02-planet-tilted.png`). The exact angle depends on each profile's selected sector and universe
time, so a like-for-like frame needs a shared profile.

## T3 — editor shell skin (wave 3, `runs/20261018-s2-L03-T3/notes.md`)
- Theme (`Styles.java:306-344`): `flati` (black / flatOver), `squareTogglei` (black, `flatDown` accent frame on the
  checked tool), `cleari`; `CheckBox` draws no background in any state (Arc `CheckBoxStyle`).
- Shell (`MapEditorDialog.java:57, 630-722`): Show Blocks/Terrain/Floor use `defaultCheck`; Brush is the
  `defaultSlider` box with the centred label; Center is `Icon.move` + label (`flatt`, margin 9); background
  `Styles.black`; editor menu Resize uses `Icon.resize`.
- Map view (`MapView.java:282-283, 323-325`): 3 px `Pal.accent` frame drawn last; brush outline `Pal.accent`.
- Map Info (PAR-0429, `MapInfoDialog.java:33-126`): two-column field table with `padTop(15)`, wrapped underline
  `areaField` description (base `TextEdit` style, `Styles.java:424-433`), flat route buttons inside one `Tex.button`
  frame, body scrolls (Data Patches below the fold, like Java).
- Engine: `MindTable` colspan cells now add only their width deficit to the spanned columns (Arc
  `Table.computeSize`); `ui_layout_check` case 6 (4 failures on the old engine). The old split widened Map Info's
  label column to 242 px (Java ~127).
- Residual: palette `spawn` sort (L07 T1 / PAR-0110); Map Info rows ~10 px above Java.

## T4 — HUD details (wave 3, `runs/20261018-s2-L03-T4/notes.md`)
- Skip button: `rightStyle` `buttonRight*` 9-patches (`HudFragment.java:493-509`).
- Wave text (PAR-0386): left-aligned `labelWrap` (`HudFragment.java:1066`).
- Placement (`PlacementFragment.java:282, 677, 697`): the outer frame has no background or margins, so the hover
  box and palette hug the bottom-right corner (hover box x 840–1152 = Java); blocks column on `Tex.pane2`,
  categories on `black6`.
- Residual: wave panel ~5 px right/down of Java; the hover box lacks Java's bevelled corner; "Enemies Remaining"
  wrap not paired like-for-like (needs a live wave).

## T5 — Game Over pane / tech-tree clip (wave 3, `runs/20261018-s2-L03-T5/notes.md`)
- Game Over: the `black3` pane, `margin(13)` and `maxSize(600, 550)` layout landed in L01 T2 (≤ 3 px). Here:
  `ui.hudfrag.shown = false` while shown — `MindUi.hud_set_visible` only stored a flag, so the dialog now hides
  the visible HUD fragments and restores them on hide; `StatLabel` text centred in the 50 px bar.
- Tech tree: the canvas no longer clips (`ResearchDialog.java:110-114`, unclipped `View` under the button row).
- Staging: Game Over via `debug_kill_building(130, 32)`.

## Deviations (with Java locations)
- Godot's `HSlider` has no pressed-grabber icon slot, so `sliderKnobDown` (white) is not shown while dragging
  (`Styles.java:374-378` `knobDown`).
- The orbit frame cannot be pixel-paired across the two profiles (different last sector and universe time).

## Blockers
None.

## Ledger changes to apply
| Item | Change | Evidence |
|---|---|---|
| PAR-0413 (slider skin) | close: rows MATCH; drop the "extra Bloom Intensity row" clause (Java has the row) | `runs/20261018-s2-L03-T1/` |
| PAR-0414 (planet buttons) | close: rail top-left | `runs/20261018-s2-L03-T2/godot/planet.png` |
| PAR-0420 (orbit plane) | fixed in code (mesh-frame centre); verify like-for-like in L08 T6 with a shared profile | `runs/20261018-s2-L03-T2/notes.md` |
| new | Settings Graphics list starts 9 px higher than Java (y 58 vs 67) | `runs/20261018-s2-L03-T1/diff/36-graphics-side.png` |
| new | Planet atmosphere rim drawn on the dark side (Java glows only on the lit limb) | `runs/20261018-s2-L03-T2/diff/planet-side.png` |
| PAR-0421 (editor shell skin) | close | `runs/20261018-s2-L03-T3/` (editor-view / edmenu / ed-info side-by-sides) |
| PAR-0429 (Map Info frame) | close | `runs/20261018-s2-L03-T3/diff/ed-info-side.png` |
| PAR-0386 (wave wrap) | close (left-aligned wrap; a live-wave pair can confirm in L08 T6) | `runs/20261018-s2-L03-T4/` |
| new | HUD skip button / placement frame skin | fixed, `runs/20261018-s2-L03-T4/` |
| new | Game Over left the HUD drawn behind it | fixed, `runs/20261018-s2-L03-T5/diff/44b-side.png` |
| new | Tech tree clipped above the button row | fixed, `runs/20261018-s2-L03-T5/diff/40-tech-side.png` |

## Done criteria
T1–T3 landed and matched against the named Java frames (T3 against a fresh `:11` editor batch). T4 and T5 landed
with their residuals recorded. L03 is complete.
