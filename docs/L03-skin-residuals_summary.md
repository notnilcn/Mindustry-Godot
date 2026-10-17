<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L03 — Skin and presentation residuals (wave 2: T1, T2; T3–T5 pending)

Session A (session 2: worktree `Mindustry-Godot-session2`, branch `parity/session-2`, display `:11`, bridge 6980).

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T1 slider skin (PAR-0413) | **done** | Settings slider rows are full-height 500x42 `sliderBack` boxes with the 29x42 knob, as in Java. `ui_widgets_check.gd` asserts the box's minimum height. |
| T2 planet page (PAR-0420 orbit, PAR-0414 buttons) | **done** | Rail pinned top-left. Orbit centre moved into the planet's mesh frame (Rust, built through the flock). Globe size measured: matches, `camLength` unchanged. |
| T3 editor shell skin | pending (wave 3) | |
| T4 HUD skip/wave wrap/hover box (PAR-0386) | pending (wave 3) | |
| T5 Game Over pane / tech-tree clip | pending (wave 3) | Game Over layout landed in L01 T2 (≤ 3 px vs Java); T5 re-checks the pane skin. |

## Commits

```
90ca8f77 L03 T1 prep: ui_widgets_check green and run by tools/ci.sh   (PAR-0423, see the L01 summary)
2e1cc2e6 L03 T1: settings slider rows are full-height sliderBack boxes
9ca2138c L03 T2: planet orbit centre in the mesh frame; planet rail top-left
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

## Done criteria (wave 2 scope)
T1: the Graphics rows match `java/36-graphics.png`, with the widget-check assertion in place. T2: the rail is
top-left, the orbit transform is fixed with a Rust test, and the globe was measured. T3–T5 are scheduled for wave 3.
