<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J07 — Planet page rendering and campaign residuals — summary (session 5, wave D)

Branch `parity/session-5`, base `7178b401` (wave-C merge), display `:14`,
bridge 7010. Profile note: the Godot session profile is partial (282/375
unlocks); the Java reference has the full tree. T1/T2 inspect the globe itself
(orbit/atmosphere/framing are unlock-independent); the sector markers differ by
campaign state in both profiles, so full-frame gate numbers are reported but
not used as pass/fail here. The plan's throwaway-profile import was not run
(it closes the client; the graded surfaces do not depend on unlocks).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — orbit lines (`PlanetRenderer.renderOrbit`) | **landed + in-engine verified** | `6b69bded` |
| T2 — atmosphere sphere/draw order + framing; clouds/glow residual recorded | **landed + in-engine verified (clouds + bloom tail recorded)** | `418a496b` |
| T3 — tech-tree backdrop, sector icons, framed root header | **landed + in-engine verified** | `aec5bde4` |
| T4 — Interplanetary Accelerator (PAR-0296) | **recorded** (multi-part feature; no seams for progress/candidates/launch mode/animator) | — |
| T5.4 — `scheduleToast` slot + logic gates | **landed + in-engine verified** | `9ab01188` |
| T5.1/2/3/5 — prebuildBase/allowCampaignRules, Erekir generator, abandon, land-info gates | **recorded with Java locations** (T5.5's announce verified to already exist) | `9ab01188` |

## T1 — orbit lines

The port already carried `mind_core::render::g3d::orbit` (`orbit_radii`,
`orbit_angle`, `world_position`, tested against the Java constructor order);
the payload exposed nothing and the view drew nothing. `planet_view` now emits
`orbits` for every visible parented planet of the current solar system (radius
+ parent centre in the view frame), and `planet_view.gd` draws one
`LINE_STRIP` loop per orbit in `Pal.gray` (454545) with `alpha = uiAlpha`,
hidden below 0.02 — `PlanetRenderer.java:171-179`.

In-engine (view frozen with `set_process(false)` to cancel the day/night light
drift; the earlier un-frozen toggle diff was 100 % noise): payload orbits =
Erekir 17.0 + Serpulo 57.24 around the sun at (12.85, 0, -55.78); toggling the
layer changes 279 sampled px (bbox 6,234–1150,438) and the arc's sky pixels
read exactly (68,68,68); zoom 2 → `uiAlpha` 0 → hidden.
Evidence: `runs/20261017-s5-J07-T1/`.

## T2 — atmosphere, clouds, framing

- Atmosphere: Java renders `MeshBuilder.buildHex(Color.white, 2, 1.5f)` (a
  radius-1.5 hex sphere) *after* the opaque planet (`Planet.java:537-550` in
  `renderTransparent`). The port drew a camera-facing quad at
  `render_priority -9` (before the body at -8) — the additive veil was hidden
  behind the planet (the twin's "thinner, darker rim"). Now: `SphereMesh`
  radius 1.5 at priority -7, no billboard rotation (a sphere is rotation
  symmetric). Radial purple-halo profile at zoom 1 (mean max(0, blue-green)
  per 20 px ring): 120–199 px → after 29/36/49/39 vs **java 31/45/43/30** vs
  before-fix 26/27/33/30 (before-fix peaked outside the shell at 200–219);
  `atmosphere` off collapses it to 3/3/10/22. Evidence:
  `runs/20261017-s5-J07-T2/`.
- Camera framing: the formula (`(radius + camRadius) * camLength + (zoom-1) *
  (radius + camRadius) * 2`, radius 1, camRadius 0, camLength 4, fov 60) is
  identical in both; the J-round "globe smaller" matched the pre-I04 billboard
  body and is not reproducible with the hex body. Full-frame diff vs
  `java/02-planet.png` is dominated by campaign-state markers + the orbit
  angle (time) + the glow tail below.
- **Recorded residuals**: `drawClouds` (`Planet.cloudMesh`: Serpulo's two
  `HexSkyMesh` layers, `Planets.java:127-130`, `HexSkyMesh.java:16-63`) is not
  ported — clouds are `alpha = 1 - uiAlpha` (`HexSkyMesh.java:47,62`), i.e. 0
  at the acceptance pose (zoom 1), so the frame cannot show them; a zoomed-out
  twin pair is the follow-up. Java's own `Bloom` (threshold 0.8, 6 blur
  passes, intensity 2.5, `PlanetRenderer.java:31-34`) leaves an outer tail
  (23–27 at r > 200 px) that the port's WorldEnvironment glow approximation
  reproduces weaker (14–20) — recorded, not tuned (I04 T15 territory).

## T3 — tech tree

- Backdrop: Java's `ResearchDialog` is a plain `BaseDialog` with no background;
  the port's `defaultDialog` panel painted an opaque slate. The research
  dialog now clears its panel stylebox (dim kept). Right-band region means:
  java (13.7, 11.3, 7.3) std 44.4 / godot before (44.8, 48.5, 53.0) std 24.6 /
  after (13.7, 11.3, 7.2) std 44.4 — byte-level match with Java.
- Sector icons: `SectorPreset.java:134` overrides `uiIcon` to
  `Core.atlas.find("sector-" + name, Icon.terrain.getRegion())`; the payload
  emitted `sector-<name>-ui` (absent), so nodes fell back to generic glyphs.
  `content_icon_name` now emits `sector-<name>` for the port's `sector`
  content type (roots, nodes, content view); `MindWidgets.content_icon`
  supplies the `terrain` fallback. Payload check:
  `"icon":"sector-groundZero"`; `icon_texture("sector-groundZero")` resolves
  (49 `sector-*` atlas regions present).
- Root header: the `titleTable` root button is always present in Java; the
  port hid it unless a tree had multiple roots. Now visible whenever a tree is
  loaded; the `defaultt` frame was already correct (grey 69 border measured at
  rect (426,4,300,60)). Evidence: `runs/20261017-s5-J07-T3/`.

## T4 — Accelerator: recorded (see runs/20261017-s5-J07-T4/notes.md)

All Java anchors + the port's gaps are itemised there. The key finding: the
plan's acceptance premise is wrong — **vanilla never populates
`Planet.launchCandidates`** (no writer in the whole Java tree; `serpulo.json`
carries only `presets` + `attackSectors`), so the Accelerator launches the
core to **another sector of the same planet** (`PlanetDialog.canLaunchTo`'s
`allowSelfSectorLaunch` branch, `:610`); Erekir is reached from the planet
page's normal launch. A follow-up needs `CampaignState.progress` + an
accelerator item filter, a `canLaunch`/`config_spec` branch, a launch mode in
the planet dialog (or the LaunchPad picker seam), the core/resource carry and
the `LaunchAnimator` cutscene. No in-engine frame exists (both clients would
need a walked charged accelerator).

## T5

Landed: the `scheduleToast` slot (`HudFragment.java:729-743`) + the
`LExecutor.java:1939-1942` message gates; in-engine three queued toasts fired
on the slot stamps 40827/44327/47827 with the box samples confirming the
schedule. Recorded with Java locations: `prebuildBase`
(`Planet.java:133`, `Control.java:230-…` progressive rebuild missing),
`allowCampaignRules` (`Planet.java:139,221` → `PlanetDialog.java:739`,
`GameOverDialog.java:115`; today's port gate is match-level
`allow_edit_rules`), the Erekir generator residuals
(`maps/planet/erekir.rs:7-13`), abandon semantics
(`PlanetDialog.java:1166-1177` vs the port's record delete), and the
land-info gates (`CoreBlock.java:414-417`; the announce itself already exists
in `landing_core.gd:97-134`).

## Exact test commands and results

```bash
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check      # clean
cargo clippy ... -p mind-gdext --all-targets -- -D warnings             # clean
cargo test -p mind-core --lib render::                                 # 196 passed (run after the wave-D changes)
bash tools/godot.sh --headless --editor --quit --path client           # parse/import gate clean (0 script errors)
bash tools/session-play.sh 5                                           # full editor restart after every build (a --play alone keeps the old .so mapped)
```

In-engine checks are listed per task above; all captures/evals are in
`runs/20261017-s5-J07-T{1,2,3,4,5}/`.

## Deviations (with Java locations)

1. Atmosphere mesh: smooth sphere vs Java's 2-subdivision hex sphere
   (`PlanetRenderer.java:35`); the shader reads the mesh surface position, so
   the per-fragment ray is the same surface ray.
2. Clouds unported (`Planet.cloudMesh`/`drawClouds`,
   `PlanetRenderer.java:150`); invisible at the acceptance pose (alpha =
   1 - uiAlpha).
3. The outer glow tail is weaker than Java's planet `Bloom`
   (`PlanetRenderer.java:31-34`) — pre-existing I04 T15 approximation.
4. Orbit layer draws under the sector grid (grid priority 4 vs orbit 2);
   Java draws the grid before the atmosphere/orbit
   (`PlanetRenderer.java:152-168`) — a small ordering difference, not measured
   as visible.
5. T4 and the T5.1/T5.2/T5.3 pieces are recorded, not ported.

## Blockers

None for the landed tasks. T4's land-side needs the seams listed in its notes.
The Java-profile unlock normalisation for full-frame gates was skipped (the
import closes the client); the graded surfaces above are unlock-independent.

## Ledger changes to apply (coordinator; `chains.json` untouched)

- **PAR-0296** (Accelerator stub): keep `seeded`/`fixing` with the T4 record —
  add the notes: vanilla `launchCandidates` empty (self-planet launch is the
  real semantics); port gaps = progress/item-capacity state, `canLaunch` +
  config spec, planet-launch mode, core/resource carry, `LaunchAnimator`
  cutscene; run `runs/20261017-s5-J07-T4/`.
- **New (J07 T1/T2/T3)**: record `verified`-style notes for the orbit lines,
  the atmosphere sphere/order, and the tech-tree backdrop/icons/header with
  the commits + runs above; keep the cloud/bloom tails as residual notes.
- **J-round row 02-03** (planet page PARTIAL): the missing orbit arcs and the
  thinner/darker atmosphere rim are fixed (`6b69bded`, `418a496b`); the
  remaining planet-page residuals are the cloud layer (unported, invisible at
  zoom 1) and the glow tail.
- **J-round row 40** (tech tree PARTIAL): backdrop/icons/header fixed
  (`aec5bde4`); remaining = sector-node state differences.
- **T5 notes**: `prebuildBase`, `allowCampaignRules`, the Erekir generator,
  the abandon semantics and the land-info gates are recorded with Java
  locations (see `runs/20261017-s5-J07-T5/notes.md`); `scheduleToast` closed
  (`9ab01188`).

## Done criteria (plan)

"T1, T2 and T4 landed and matched. T3/T5 landed or recorded with Java
locations." — T1/T2/T3 landed and verified in-engine against the stored Java
frames/metrics; T4 is recorded with Java locations and a concrete follow-up
plan (the plan's acceptance premise corrected); T5's toast slot landed, the
other four items recorded with Java locations. `docs/K-round-twin-verification.md`
(the round-K re-walk) is J10 T3's, not this plan's.
