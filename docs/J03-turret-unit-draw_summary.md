<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J03 — Turret and unit draw fidelity (wave A: T1, T2, T3) — summary

Session: parity session 4 (`parity/session-4`, worktree `Mindustry-Godot-session4`),
display `:13`, bridge `7000`, base `fe8fb0e4`. Date: 2026-10-17.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — `DrawTurret` parity in the live block pass | **done with residuals** (see below) | `16d2a336` |
| T2 — idle turret head orientation | **done** (diagnosis: no sim bug; regression test + in-engine frame) | `69e24f94` |
| T3 — pack-time outlines (`Block.createIcons`) | **done** (pack arm already correct; guard test + Java-atlas diff evidence) | `11a04001` |

## Commits

- `16d2a336` `J03 T1: DrawTurret parity in the live block pass`
- `69e24f94` `J03 T2: pin the idle duo head orientation (drawrot 0)`
- `11a04001` `J03 T3: guard the pack-time in-place outlined-icon replacement`
- summary doc: this file (separate commit)

## T1 — live `DrawTurret`

What landed (Java citations in the commit body):

- Base chain `<name>-base` → `<basePrefix>block-<size>` with the Erekir
  `reinforced-` prefix (`DrawTurret.java:146-170`, `Blocks.java:4378-5959`).
- `mind-core::content::registries::blocks::turret_parts`: hand-ported
  `DrawTurret.parts`/`ammoParts` for duo, scatter, salvo, cyclone, diffuse,
  sublimate, titan, disperse, afflict (`Blocks.java:3321-5152,5616-5655`) plus
  `Turret.heatColor` overrides (`Turret.java:120`, `Blocks.java:3590,3651`).
- New live pass `mind-gdext::render::turret`: every visible turret runs the
  plan-17 `draw_turret` program each frame, fed from `TurretState`
  (`MindSimHost::turret_draw_input`), bound to the real atlas via `AtlasLookup`
  (4 px per world unit) instead of `AllRegions`. Normal node at
  `Layer.turret`, additive node at `Layer.turretHeat`; base stays in the block
  pass at `Layer.block`.
- Shadow (`Drawf.shadow(preview, …)`, `:83`), outline only when the drawer has
  parts (`:90-96`), liquid (`:127-130`), top (`:132-134`), additive heat
  (`:137-141`), parts/ammoParts in Java's two `if` blocks (`:90-119`) with the
  **raw** rotation for `DrawPart.params` (`:101`) and per-part
  `curRecoils` (`:104`).
- Two bugs were found only in-engine (kept fixed in this commit):
  parts drawn at `drawrot` (rotated -90) and a missing `!heat.found()` guard
  that emitted the `error` region for turrets without a `-heat` sprite.
- A `TurretRenderer.debug_last_prims` MCP probe was added as the evidence seam
  (kept; the `MindFx` probe stays as a test seam too).

Tests:

```bash
cargo test --manifest-path client/rust/Cargo.toml -p mind-core --lib
# -> 2705 passed; 0 failed; 4 ignored
cargo test --manifest-path client/rust/Cargo.toml -p mind-tools --lib
# -> 26 passed; 0 failed
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check        # clean
cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core -p mind-gdext -p mind-tools --all-targets -- -D warnings   # clean
```

New tests: `turret_parts_cover_the_vanilla_drawers`,
`turret_part_draw_order_matches_java` (z order + raw-rotation quads +
heat found-guard + `fuse` has no parts so no outline),
`turret_outline_table_matches_runtime_parts` (mind-tools).

In-engine (`runs/20261017-s4-J03-T1/`, load with `notes.md`):

- `pre-fix-duo-parts-rotated.png` / `zoom-duo.png` — the parts bug.
- `duo-idle-fixed-zoom.png`, `duo-isolated-idle.png` — idle duo at zoom 5,
  head upright, ring + parts, matching the Java composition
  (base + `duo-outline` + `duo` + barrels at 1.25 px/atlas-px).
- `lineup-idle.png` + `zoom-*.png` — the ten accept turrets (duo, lancer,
  fuse, salvo, cyclone, spectre, meltdown, foreshadow, breach, diffuse) idle
  on the sandbox.
- `duo-firing-prims.json` — the emitted prim set while the duo is firing
  (shadow `00000038`, outline at `turretLayer-0.01`, per-barrel recoil
  offsets, head 6.5x7 at `rot = rotation - 90`).
- `duo-firing-heat.json` — the pre-fix heat placeholder (`error`,
  `ab340065`).

Residuals (documented, not landed in wave A):

- scathe/smite/malign `HaloPart`/`ShapePart` trees and lustre's lambda heat
  progress are not ported; `turret_parts` says so. Wave-B/J04 can extend the
  module when those turrets are walked.
- No fresh *firing pixel* frame: the session-4 game kept ending (spawned
  team-2 enemies destroyed the core -> Game Over) during the capture passes;
  the firing state is covered by the prim dump + the game-over frame's
  "Enemies Destroyed 1" and by the mind-core order test.
- The heat overlay's post-fix pixel frame is pending; the guard is test-pinned.

## T2 — idle head orientation

Diagnosis (in order of the plan's checks):

1. Placement does **not** overwrite `TurretState.rotation` (grep +
   `placed_duo_idle_head_rotation_is_up`: 90 after placement and 60 ticks).
2. `push`/mesh rotations are degrees and the y-up camera is compensated; the
   emitted prim `rot` is 0 for an idle turret (`duo-firing-prims.json`).
3. The no-ammo branch returns before targeting (`Turret.java:571-605`), so no
   stale-target turn.

Root cause of the stored mismatch (`diff/24-zoom.png`): the twin pair is
state-mismatched — the Godot duos are fed and tracking a target (status
markers), the Java duos are idle; the angles differ for that reason. The idle
Godot head now renders upright.

Tests: `placed_duo_idle_head_rotation_is_up` (mind-core). Evidence:
`runs/20261017-s4-J03-T2/` (`duo-idle-godot-zoom.png`,
`duo-firing-prims.json`, `notes.md`).

**Deviation from the plan text:** the plan asked for "a `mind-core` test that
pins the part draw order for `lancer` (recoil part) and `fuse`". In Java,
`lancer` (`Blocks.java:3579`) and `fuse` (`:3937`) construct the default
`DrawTurret()` with no `parts`, so neither has a recoil part. The test pins
the order with `duo` (two per-barrel recoil parts) and `fuse` (part-free:
no outline, no parts).

## T3 — pack outlines

The `block-icons` pass already implemented both `Block.createIcons` arms
(`Block.java:1606-1630`); the arm was extracted into
`outline_icon_in_place` and is now pinned by
`outlined_icon_is_replaced_in_place` (padded in-place replacement, unpadded
outline returned for `block-<name>-full`).

Java-atlas diff (`runs/20261017-s4-J03-T3/atlas-diff/region-diff.txt`, method
in `notes.md`): `duo-preview` byte-identical (34x34 outlined), `duo-outline`,
`lancer`, `radar`, `mass-driver` all carry the ring; remaining pixel deltas are
the packer/AA rim (<= 123 px on a 98x98 region). No re-pack was needed — the
shipped atlas already matches Java; the missing live ring was the renderer
(T1). Deviation from the plan's T3 wording: "Java's live `duo` region already
carries the dark ring" is not literally what happens — Java draws the separate
`duo-outline` region (`turretLayer - 0.01`, `DrawTurret.java:90-96`); the
in-place replacement applies to `duo-preview` (`outlinedIcon = 1`,
`Turret.java:166`).

## Deviations (summary)

| Plan text | Reality (Java citation) | Handling |
|---|---|---|
| T1 test: `lancer` has a recoil part | `lancer`/`fuse` have no `parts` (`Blocks.java:3579,3937`) | test uses `duo` + `fuse`; documented here |
| T3: "Java's live `duo` carries the ring" | ring = separate `duo-outline` (`DrawTurret.java:90-96`); in-place replacement hits `duo-preview` (`Block.java:1606-1615`, `Turret.java:166`) | pack verified against Java; renderer draws the outline |

## Blockers / hiccups

- Session-4 runtime restarted twice during the walk: one
  `MindSimHost::set_paused` `Gd::bind_mut()` race (logged in
  `open-godot-mcp-learnings.md`, 2026-10-17) and two game-over resets from the
  staged enemies destroying the core. Evidence captured before each stop; the
  missing items are listed under the T1 residuals.
- Pre-existing base failure (not from this work): `cargo test -p mind-core`
  full run fails `tests/blocks_golden.rs::family_counts_match_golden`
  (`family other` 158 vs golden 164); it fails identically at base `fe8fb0e4`
  (`git stash` check).

## Ledger changes to apply (for the coordinator)

- Close/replace the T1 ledger candidate "Turrets draw without
  shadow/outline/parts/liquid (DrawTurret stub)" with a **partial** item:
  landed = base chain, shadow, outline, liquid/top, additive heat, parts
  (9 turrets) + ammoParts (titan), live state feed + real atlas binding;
  residual = scathe/smite/malign/lustre parts and the firing pixel frame.
- PAR-0126 / F3 (twin report): annotate that F3's renderer half is fixed as
  above; replace `reference`-style evidence with
  `runs/20261017-s4-J03-T1/` (T2 frame: `runs/20261017-s4-J03-T2/`,
  T3 atlas diff: `runs/20261017-s4-J03-T3/`).
- F3's "idle head orientation is wrong too" line can be dropped: diagnosed as
  a state mismatch (T2), pinned by a test.
- F3's "Java packs outlined turret icons" is confirmed but the live ring comes
  from `duo-outline`, not an in-place `duo` replacement (see deviations).

## Done criteria (plan)

- T1 landed with in-engine frames for the ten accept turrets (idle); the
  firing evidence is prim-level (documented gap).
- T2 landed with a regression test and an in-engine idle frame.
- T3 landed with a Java-atlas diff and a pack guard test.
- This summary is committed separately.

---

# Wave B — T4–T7 (same session, after merging main @ `3bd1abe8`)

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T4 — legged units (`UnitType.drawLegs`) + Java leg geometry | **done with residuals** | `9f381d49` |
| T5 — tanks / segments / wrecks / payloads | **treads + wrecks landed; segments + payloads recorded** | `44401634` |
| T6 — weapon outlines / unit parts / ability draws | **weapon `cellRegion` landed; parts + abilities recorded** | `f11231d7` |
| T7 — status marker shape + Arc `Fill.square` (PAR-0348) | **done** | `54d13776` |

## T4 — legged units

Java: `UnitType.drawLegs` (`UnitType.java:1826-1904`), `LegsComp`
(`LegsComp.java:112-220`), `InverseKinematics.solve` (`:211-213`).

- The sim's knee now uses Java's IK inputs (`legLength/2`, `legLength/2`,
  `foot - hip`) with the `legMinLength`/`legMaxLength` clamps; the hip anchor is
  `legOffset(i)` (`legBaseOffset` + `defaultLegAngle`) and the step destination
  `legAngle(i) × legLength * legLengthScl`. The old call passed the max/min
  lengths as the two segments, placing joints past the feet
  (`runs/20261017-s4-J03-T4/pre-fix-ik-legs-stretched.png`).
- The draw pass emits Java's order: front-first leg loop
  (`i = j%2==0 ? j/2 : n-1-j/2`), moving-foot shadows, feet, `legRegion` upper
  line, `legBaseRegion` lower line (`joint + inv(joint→foot)*legExtension`),
  joints, base joints, base — with `legBaseUnder` swap and
  `Mathf.sign(flip)` stroke mirrors.
- Tests: `leg_draw_ops_follow_java_order_and_geometry` (mind-core).
- In-engine: `runs/20261017-s4-J03-T4/` — atrax (id 11) + arkyid (13) via
  `spawn_unit`, zoom-6 captures vs the Java `unit-<name>-ui` previews; the twin
  frame `before-twin-no-legs.png` is the before half of PAR-0126.
- Residuals: `legSplashDamage` + step FX, `drownTime` (`invDrown = 1`), and
  Java's `lerpDelta` stage stepping with `curMoveOffset` (the port snaps on
  drift with Java's geometry).

## T5 — tanks, segments, wrecks, payloads

- **Treads landed**: `drawTank` (`UnitType.java:1806-1824`) — the `-treads`
  base + animated `-treads<i>-<frame>` slices (`frame = treadTime %
  treadFrames`) per rect/side; data added to `unit_draws_full`.
- **Wrecks landed**: `UnitComp.java:932-941` — three `<name>-wreck<i>` decals
  per death at a random offset within `hitSize/4`, `rotation - 90`; `FxSink`
  gained `decal` and `MindFx` now draws decals at the resolved region's natural
  size (was a hardcoded 32×32). Tests:
  `wreck_decals_emit_three_names_within_range`,
  `wreck_decal_offsets_are_seed_reproducible`.
- In-engine (`runs/20261017-s4-J03-T5/`): stell (id 38) + locus (39) with
  treads at zoom 5; after a real duo kill of a spawned dagger,
  `MindFx.live_decal_count() == 3` / `last_program_size() == 3`.
- **Recorded residuals**: `drawCrawl` segments (`renale`/`latum`,
  `UnitTypes.java:4316,4335`) and `drawPayload` (`Payloadc`) — sim state/atlas
  regions exist; the draw passes are not implemented (anchors in the T5 notes).

## T6 — weapon outlines, unit parts, ability draws

- Weapon `-outline` was already drawn for non-top weapons
  (`UnitType.java:1744-1761`); landed the `<weapon>-cell` overlay
  (`Weapon.java:261-265`) tinted by `cellColor`.
- **Recorded residuals** (`runs/20261017-s4-J03-T6/notes.md`): unit
  `UnitType.parts` after shields (content data exists; needs per-mount
  warmup/smoothReload/heat/charge in `unit_draws_full` + a pass like T1's),
  weapon `layerOffset`, and the seven ability draws (`ForceFieldAbility`,
  `EnergyFieldAbility`, `ArmorPlateAbility`, `UnitSpawnAbility`,
  `MoveLightningAbility`, `SuppressionFieldAbility`, `ShieldArcAbility`).

## T7 — status marker + PAR-0348

- **Decision**: Arc `Fill.square(x, y, radius, rotation)` =
  `Draw.rect(white, x, y, radius*2, radius*2, rotation)` (`Fill.java:361-363`)
  — a square of side `2*radius` rotated CCW; for `rotation = 45` (all vanilla
  sites) a **diamond with corners on the axes at `radius*sqrt(2)`**.
  `Lines.square` = `poly(4, radius, rotation - 45)` (`Lines.java:507-509`).
- Fixed `status.rs::square_points` (was an axis-aligned square — the twin's
  "axis-aligned squares with a dark border"), `FxEmit::square` (`r*sqrt2`,
  `rotation + 45`) and `FxEmit::square_line` (`rotation - 45`); the single
  emitter covers the ~30 call sites. Markers also take `Lod.alpha2`
  (`BuildingComp.java:1226-1228`, exposed via `stats_lod_alpha2`).
- Test: `arc_square_conventions` (mind-core). In-engine:
  `runs/20261017-s4-J03-T7/` — diamond markers at zoom 2 vs the before crop.

## Wave-B gates (final, this worktree)

```bash
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check          # clean
cargo clippy ... -p mind-core -p mind-gdext -p mind-tools --all-targets -- -D warnings  # clean
cargo test ... -p mind-core --lib   # 2728 passed; 0 failed; 4 ignored
cargo test ... -p mind-gdext --lib  # 67 passed; 0 failed
cargo test ... -p mind-tools --lib  # 26 passed; 0 failed
```

Pre-existing base failure unchanged: `blocks_golden::family_counts_match_golden`
(`other` 158 vs 164) also fails at `fe8fb0e4`/`3bd1abe8`.

## Wave-B blockers / hiccups

- The session-4 runtime reset twice more during the T4 captures (game over from
  staged enemies), and one `set_paused` `bind_mut` race (recorded in
  `open-godot-mcp-learnings.md`); evidence was captured before each stop and the
  gaps are listed per task.
- Budget: T6's unit-parts pass and T5's segments/payloads are recorded, not
  landed; they share the same pass shape as T1's turret parts.

## Ledger changes to apply (wave B)

- **PAR-0126** (reopened for legs/treads/wrecks/segments): now **partially
  fixed** — legs (all 10 legged units) and tank treads are drawn and the wreck
  decals fire; **segments (`renale`/`latum`) and payload draws are still
  missing**. Evidence: `runs/20261017-s4-J03-T4/`, `runs/20261017-s4-J03-T5/`.
  Keep it `fixing` with those two items as the remaining check.
- **PAR-0348** settled: Arc `Fill.square`/`Lines.square` convention decided
  (see T7) and applied to the marker + `FxEmit`; close it with
  `runs/20261017-s4-J03-T7/notes.md` as the record.
- **New item candidates**: "unit parts not drawn (`UnitType.parts`, 31 Erekir
  parts)" and "ability draws missing (7 ability classes)" — J03 T6 residuals;
  "weapon `layerOffset` not applied".

## Done criteria (plan, wave B)

- T4 landed with tests + in-engine atrax/arkyid evidence (preview fallback; no
  live Java leg captured).
- T5 landed for treads + wrecks (tests + in-engine); segments/payloads recorded
  with exact anchors.
- T6 landed the weapon cell; parts/abilities recorded with exact anchors.
- T7 landed with the marker + the PAR-0348 decision.
- This wave-B section is committed separately.
