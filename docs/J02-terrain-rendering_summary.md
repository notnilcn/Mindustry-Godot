<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J02 — Terrain, floor and menu-background rendering (session 3, wave A)

Branch `parity/session-3`, base `fe8fb0e4`, session 3 (display :12, bridge 6990).
Wave-A scope: T1, T3, then T2 if time. **T1 and T3 landed and are verified
in-engine; T2 was not started (wave-A budget after T1/T3 ran out).** T4/T5 are
wave B by policy.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — Floor edge blending (`Floor.drawEdges`/`drawNonLayer`/`drawBlended`) + floor-pass orientation | done (verified in-engine) | `b33dce04` |
| T3 — Preview/minimap colours = `MapIO.colorFor` (`useColor` + ore `mapColor`) | done (verified in-engine) | `a7b1aff0` |
| T2 — Autotiled/tiled floors + liquid-overlay alpha | **pending** (not started; see Blockers) | — |

## Commits

- `b33dce04` `J02 T1: floor edge blending (Floor.drawEdges/drawNonLayer/drawBlended) + floor-pass orientation`
- `a7b1aff0` `J02 T3: preview and minimap colours = MapIO.colorFor (useColor + ore mapColor)`
- docs commit (this file + the learnings entry) follows.

## T1 — what changed

Java spec: `world/blocks/environment/Floor.java:249-262` (`drawBase` order:
`drawMain` → `drawEdges` if `drawEdgeIn` → `drawOverlay` → liquid-overlay alpha),
`:343-361` (`drawNonLayer`), `:363-386` (`drawEdges`), `:388-407`
(`drawBlended`), `:409-414` (`realBlendId`), `:416-428` (`edges`/`edge`/`doEdge`),
`graphics/FloorRenderer.java:318-336` (per-layer draw decision).

1. **Content model** (`mind-core`): `BlockSpec`/`BlockDef` gained
   `blend_group` (`Floor.java:62`, resolved to `blendGroup.id` per
   `Floor.java:190`), `draw_edge_in` (`:92`), `draw_edge_out` (`:94`) and a
   derived `has_edge` (the `<name>-edge` region exists; upstream generator
   `tools/Generators.java:827-847`: non-overlay floors with `blendGroup == this
   && drawEdgeOut`). Blocks.java overrides carried verbatim:
   `crater-stone`/`char`/`stone-vent` → `stone` (`:352,:356,:535`),
   `hotrock`/`magmarock`/`basalt-vent` → `basalt` (`:367,:377,:540`),
   `rhyolite-crater`/`rhyolite-vent` → `rhyolite` (`:423,:505`),
   `ferric-craters` → `ferric-stone` (`:451`), `carbon-vent` → `carbon-stone`
   (`:510`), `arkyic-vent` → `arkyic-stone` (`:515`), `yellow-stone-vent` →
   `yellow-stone` (`:520`), `red-stone-vent` → `dense-red-stone` (`:525`),
   `crystalline-vent` → `crystalline-stone` (`:530`); `space` gets only
   `drawEdgeOut = false` (`:342`); `metal-tiles-1..13` and `colored-floor` clear
   both flags (`:844-949`); `EmptyFloor` clears `drawEdgeOut`
   (`EmptyFloor.java:15`). The liquid floors gained their `cacheLayer`
   (`Blocks.java:219,231,243,258,294,309,326,337,404,495`) so `drawNonLayer`'s
   layer split is real. The mod JSON block parser accepts
   `blendGroup`/`drawEdgeIn`/`drawEdgeOut`.
2. **`mind-core::render::floor_edges`** ports `realBlendId`, `doEdge`,
   `drawEdges`, `drawNonLayer` and `drawBlended` as pure functions returning the
   ordered `<blendGroup>-edge` 3×3 cells for a tile; tests pin sand|stone,
   core-zone|stone, water|sand (cache-layer split), the liquid+overlay packed
   blend id and the blend-group/availability rules.
3. **`mind-gdext::render::floor::tile_quads`** emits base → edges (`drawEdgeIn`)
   → overlay (`OverlayFloor.drawBase` draws the variant only,
   `OverlayFloor.java:22-24`; a plain floor overlay runs the full `drawBase`)
   and the `drawNonLayer` branch for other cache layers. `PendingQuad.crop`
   selects a cell (`Floor.edge` → `edges[rx][2 - ry]`, `Floor.java:421-423`);
   the world and editor bakers apply it to the resolved UVs. The editor renderer
   (`editor_renderer.rs`) needed only the `crop_uv` pass-through in its floor
   quad conversion.
4. **Floor-pass orientation fix (pre-existing bug, found while verifying T1).**
   The port's floor/wall pass rendered every sprite vertically mirrored
   against Java (`v_flip = false` through the y-flip camera; `atlas_bind`
   documents the required v swap for world sprites). Evidence: the twin's
   pre-fix `godot/11-sandbox.png` stone tile matched `vflip(atlas)` at SSD 0.5
   while `normal` scored 3.7; Java's tile matched the atlas upright. After the
   fix my fresh capture matches Java upright (SSD 0.2-0.3). Fixed by
   `v_flip: true` on the base-floor, edge and wall quads (overlays already had
   it; the building cache/`blocks` pass already used it). The same fix applies
   to the editor because it shares `tile_quads`.

### T1 tests

```bash
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check        # OK
cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core -p mind-gdext --all-targets -- -D warnings   # OK
cargo test --manifest-path client/rust/Cargo.toml -p mind-core floor_edges   # 7 passed
cargo test --manifest-path client/rust/Cargo.toml -p mind-core                # 2709 passed, 4 ignored; blocks_golden::family_counts_match_golden fails (pre-existing, see below)
cargo test --manifest-path client/rust/Cargo.toml -p mind-gdext              # 67 passed
```

### T1 in-engine evidence (`runs/20261016-s3-J02-T1/`)

Real-input walk (clicks through Play → Custom Game → card → Play, per the
harness recipes); screenshots via `godot_screenshot`; references are the stored
twin frames (Godot-only §3b standard).

- Sandbox custom game, core strip live (`godot/02-sandbox-core-fixed.png`):
  all four `core-zone` edge tiles (stone N/E/S/W of the core) now match
  `java/11-sandbox.png` at SSD 0.2-1.0 (`diff/corezone-neighbours-ab.png`,
  top Java / bottom port; pre-fix the same tiles scored 10-22).
- Ancient Caldera survival, crop at the reference coordinates
  (`godot/03-caldera-fixed.png`): the sand/stone boundary matches the left half
  of `diff/46-edges-zoom.png` (`diff/caldera-edges-side.png`); whole-frame
  alignment to `java/42-survival.png` is SSD 0.96 on a texture patch (ores and
  the twin-built conveyor differ — F1 map filters are J01's scope).
- Pre-fix captures kept for contrast: `godot/00-sandbox-before-flipfix.png`,
  `godot/01-caldera-before-flipfix.png`, `diff/pre-flip-mirror-evidence.png`.
- Editor re-check (PAR-0109 shared path): `godot/04-editor-maps-dialog.png`
  (Maps list + previews) and `godot/05-editor-caldera.png` (editor world view,
  floors/water render correctly, y-up, no dark chunks; matches the Java editor
  frame family `java/29..31-editor-open.png`).

## T3 — what changed

Java spec: `io/MapIO.java:229-237` (`colorFor`), `:169-174,220-244` (preview
generation), `Block.java:183` (`useColor = true`), `OverlayFloor.java:13` /
`AirBlock.java:14` (false), `OreBlock.java:22-33` (`mapColor = ore.color`,
`useColor = true`; `setup` at `:37-41`).

- `BlockSpec`/`BlockDef.use_color` with the class defaults above; the
  `BlockPalette` snapshots it and `color_for` is now exact (the previous
  `has_color` approximation made ore overlays fall back to the floor colour).
- Ore `mapColor` from the item colour at content build (`OreBlock.java:22,40`).
- Preview cache name bumps `_v2.png` → `_v3.png` (`maps/preview.rs`,
  `io/fs.rs`) so stale colour caches regenerate.
- Not touched (remaining `colorFor`-family gaps, not in T3 scope):
  `Block.minimapColor(tile)` overrides (`Cliff.java:30`, `ColoredFloor.java:202`,
  `ColoredWall.java:91`, `Sorter.java:52`, `DuctRouter.java:66`,
  `LightBlock.java:61`) are still unported; no vanilla acceptance frame in T3
  depends on them.

### T3 tests

```bash
cargo test --manifest-path client/rust/Cargo.toml -p mind-core io::map::preview   # 8 passed (incl. ore_overlays_use_their_map_color)
cargo test --manifest-path client/rust/Cargo.toml -p mind-core                    # 2710 passed; same pre-existing blocks_golden failure
```

### T3 in-engine evidence (`runs/20261016-s3-J02-T3/`)

- Port preview `godot/sandbox_v3.png` vs Java's own `ref/java-sandbox_v2.png`
  (`diff/sandbox-previews-side.png`): both show the blue water circle **and the
  grey scrap circle** (pre-fix the grey circle was missing).
- Map Play (`godot/01-mapplay-sandbox.png`, `diff/mapplay-side.png` vs
  `java/10-mapplay.png`) and the custom-game list (`godot/02-custom-list.png`)
  show the same preview; the editor Maps dialog shows it too.
- Live minimap (`godot/03-minimap-ore.png`, `diff/minimap-side.png` vs the
  Java minimap in `java/11-sandbox.png`): the grey scrap circle is drawn; the
  structured probe `MinimapProvider.minimap_color_at(120,95)` == `0x777777ff`,
  exactly the scrap item colour (`ore-scrap` tile: stone floor + ore-scrap
  overlay, verified via `tile_debug`).

## Deviations (with Java file:line)

1. **Preview cache name `_v3`** (T3): Java keeps `previews/<name>_v2.png`
   (`maps/Map.java:75`); the port bumps the name so caches written by the
   pre-`useColor` colour path regenerate on first use. Old `_v2` files stay on
   disk untouched.
2. **T1 orientation fix is a port-only correction of a port bug**, not an
   upstream deviation: after it the pass matches Arc `Draw.rect` orientation
   (`Floor.drawMain`/`Block.drawBase` draw regions with the image top at the
   world top) through the port's y-flip camera. Recorded here because no plan
   item named it.
3. **T1 3×3 invalidation**: the port's world/editor renderers already re-bake
   every chunk when the world/editor revision changes (`FloorRenderer::update`,
   `EditorRenderer::pump`), so tile changes invalidate the affected edges (the
   plan's "invalidate the 3x3 neighbourhood" is subsumed); no extra code added.

## Blockers

- **T2 not started.** Wave-A budget after T1 (including the orientation
  discovery and in-engine verification) and T3 did not leave room for the
  autotile/liquid-overlay work with its own in-engine capture. Suggested for
  wave B: `Floor.drawMain` (`Floor.java:264-292`) + `StaticWall.drawBase`
  autotile (`StaticWall.java:40-77`), `autotileVariants`/`autotileMidVariants`
  (`Blocks.java:842-952`; atlas regions `metal-tiles-<n>-<0..46>`,
  `metal-tiles-<n>-<v>-<i>`, `metal-tiles-13-mid-*` all confirmed present in
  `assets/sprites/sprites.atlas.json`), plus the liquid-overlay redraw
  (`Floor.java:257-261`, `overlayAlpha = 0.35f` at `Blocks.java:300`) — a fresh
  Java capture is needed for the metal-tiles acceptance frame (no stored twin
  frame shows world metal tiles).
- **Pre-existing `blocks_golden::family_counts_match_golden` failure**
  (`mind-core`): fails identically on the unmodified tree (verified by stashing
  my changes: actual `other 158 / production 51` vs the committed golden
  `164 / 45`) — not a J02 regression; the coordinator may want to regenerate
  that golden.
- **Pre-existing `audio_golden::audio_scenarios_match_committed_goldens`
  failure** (`mind-headless`): the committed `tests/golden/audio/events_sim.json`
  uses a `{format, events[], tick}` schema while the current binary emits
  `{dump, pass, scenario, events:<count>}`. My commits touch no
  `mind-headless`/`mind-core::audio` file (`git diff --stat fe8fb0e4..HEAD --
  client/rust/mind-headless client/rust/mind-core/src/audio` is empty), so the
  golden is stale against the current dump wrapper — not a J02 regression.
  Reported here instead of regenerating (out of J02 scope).

## Ledger changes to apply

- New item: "Floor transitions render as hard tile edges (no `Floor.drawEdges`)"
  → closed by `b33dce04` (T1), verified in-engine (sandbox core + Caldera crop).
- New item: "Floor/wall pass renders vertically mirrored vs Java" → closed by
  `b33dce04` (folded into T1; pre-existing, found while verifying T1).
- PAR-0114 (scrap ore) → fixing: closed by `a7b1aff0` (T3) together with
  J01 T2 (the remaining part of PAR-0114 — the three unimportable user maps —
  stays J01's).
- No chain/ledger files were edited by this session.

## Done criteria

- T1 and T3 landed as one commit each with Java citations and targeted gates;
  T2 explicitly pending (above).
- In-engine verification per the round-J standard: a Rust test that failed
  before the fix (T1 `floor_edges` set, T3 preview unit test) plus a real-input
  walk against the stored twin references (Godot-only §3b).
- Summary committed to `docs/J02-terrain-rendering_summary.md`; run evidence in
  `runs/20261016-s3-J02-T1/` and `runs/20261016-s3-J02-T3/`; branch left with a
  clean working tree after the docs commit.
