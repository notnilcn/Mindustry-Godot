<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L06 — Simulation and campaign residuals — summary

Owner: session B (session 3 worktree, branch `parity/session-3`). Round L.

| Task | Item | Status | Wave | Commit |
|---|---|---|---|---|
| T1 Unit assembler output gate, hand-off, commandPos | PAR-0402 | **DONE** | 2 | bfc8d589 |
| T2 Router ControlBlock + `Turret.playerControllable` | PAR-0402 | **DONE** | 2 | f9403276 |
| T3 Bullet suppression + pierce/laser length pre-pass | PAR-0402 | **DONE** | 2 | fcf2e192 (+ fc5774fc doc fix) |
| T4 Tethered units / `EmptyDataAbility` I/O | — | pending | 3 | — |
| T5 `DrawLiquidOutputs` side regions | — | pending | 3 | — |
| T6 Campaign rules and generators (+ PAR-0424 generated sector layout) | PAR-0391, PAR-0424 | pending | 3 | — |
| T7 Interplanetary Accelerator | PAR-0296 | pending | 3 | — |

Every landed task has a failing-first mind-core test (the state/functions did not exist or
behaved differently before the change).

## T1 — unit assembler (`world/blocks/units/UnitAssembler.java`)

- **Output gate:** `wasOccupied = checkSolid(getUnitSpawn(), false)` (`:515`) holds product
  progress (`:523`). `checkSolid` (`:679-685`) ported as
  `units::behavior::assembler_check_solid`: a ground output overlapping a solid tile
  (`EntityCollisions.overlapsTile` + `EntityCollisions::solid`, `EntityCollisions.java:98-120`),
  or an added unit's `hitboxTile` (`HitboxComp.java:69-76`) in the `hitSize * 1.4` square
  (`Units.anyEntities`, `Units.java:199-209`), skipping `spawnedByCore` units and units outside
  the output's layer. The behavior cannot see the live grid, so
  `Sim::refresh_assembler_occupancy` fills the transient `UnitAssembler.was_occupied` right
  before the scheduled building pass.
- **`spawned()`** (`:556-574`) as `deliver_assembled_unit`: a commandable product takes
  `commandPos` (`CommandAI.commandPosition`); it goes into the same-team payload block under
  the spawn point (`unit.buildOn()`, `acceptPayload`/`handlePayload`) or is added; the
  `UnitCreateEvent` fires either way.
- **`areaSize = 13`** for tank/ship/mech-assembler (`Blocks.java:6566,6581,6596`; class default
  11, `UnitAssembler.java:45`): the port used 11, so the spawn point and drone ring were one
  tile short.
- Tests: `occupied_output_holds_assembler_progress`, `assembler_output_area_occupancy`,
  `assembled_unit_takes_command_pos_and_payload_handoff`; the two existing assembler tests'
  helper now reads `area_size`.
- Deviations: Java's `overlapsTile` radius reads an unrelated temp rect
  (`EntityCollisions.r1`); the port scans `round(hitSize/8) + 1` (always covers the rect; the
  overlap test is exact). The `Mathf.range(0.001f)` spawn jitter (`:563`) is not reproduced.

## T2 — router ControlBlock + `playerControllable`

- `RouterBuild implements ControlBlock` (`Router.java:30-53`): `RouterBuild.block_unit`, the
  `block` proxy created on first use like Java's lazy `unit()` (`:37-44`), never added to the
  unit pass, despawned with the router; `canControl()` = `size == 1` (the distributor is not).
  While possessed, `getTileTarget` (`:102-117`) syncs the proxy, takes the aimed quadrant and,
  only while shooting, returns the accepting `nearby(angle)` building; otherwise the item waits.
- `Sim::control_block_unit_id` (the ControlBlock branch of `InputHandler.selectedUnit()`,
  `InputHandler.java:2159-2161`) now covers routers and gates turrets on `playerControllable`;
  it is `&mut` (lazy proxy), so the host seam and the desktop caller bind mutably.
- `BlockDef.player_controllable` (default true; no vanilla turret clears it,
  `Turret.java:103`) parsed from mod JSON `playerControllable`.
- Logic `@controlled` reads a possessed router.
- Tests: `router_is_a_control_block`, `possessed_router_routes_toward_aim_while_shooting`
  (through the real `possess_unit` path), `turret_player_controllable_parses`.
- In-engine: not walked this wave (headless covers the Ctrl+click possession path end to end
  from `control_block_unit_id` to `possess_unit`); a router-possession frame can be taken in
  L08 T6.

## T3 — suppression + length pre-pass

- **Length pre-pass** (`Damage.java`): `collideLine` first applies `findLength` (`:256-258`,
  `:155-163`) = `findPierceLength` (`:180-218`) with a pierce cap, else `findLaserLength`
  (`:165-174`) for lasers. `collide_line_bullet` gained the `laser` flag; `collide_laser`
  (`:221-228`) sets `fdata` to the cut length. Callers follow Java: laser/shrapnel
  `collideLaser(..., laserAbsorb, pierceCap)`, continuous `collideLine(..., laserAbsorb,
  pierceCap)`, rail `collideLine(..., false, false, pierceCap)`. Before, lasers pierced
  plastanium walls (`absorbLasers`, `Blocks.java:1735,1744`) and `fdata` was the full length.
- **Heal suppression** (`BulletType.java:572-575`, `Damage.java:46-60`,
  `BuildingComp.java:485-495`): `projectors::apply_suppression` (non-privileged other-team
  buildings `within(range + hitSize/2)`, `BlockIndexer.java:435-454`, get
  `applyHealSuppression(reload + 1)` = `max(t, Time.time + amount)` on the `BuildClock`);
  `is_heal_suppressed` = `suppressable && Time.time <= healSuppressionTime`. Bullet hit applies
  it when `suppressionRange > 0`; the mend projector (`MendProjector.java:94,112`) and regen
  projector (`RegenProjector.java:134-137,151`) honour it; `SuppressionFieldAbility` now uses
  the same helper (before, it stored a relative duration nothing read).
- Tests: `laser_stops_at_enemy_absorber`, `laser_without_absorber_keeps_length` (control),
  `pierce_cap_cuts_rail_length`, `suppression_bullet_stops_enemy_mender`.
- Not ported (recorded): the view-only `regenSuppressSeek`/particles; the repair tower does not
  heal in the port at all (separate gap); the build turret's `checkSuppression` efficiency cut
  needs its build-speed seam.

## Test commands (end of wave 2)

```
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check                       # clean
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings  # clean
cargo test -p mind-core --no-fail-fast     # 2770 passed; 3 failed = deferred generation checksums (L02 T1)
cargo test -p mind-headless --no-fail-fast # green
cargo test -p mind-gdext                   # 77 passed
mind-headless parity goldens               # 97 PASS; run-all --tier T0 4/4
bash tools/godot.sh --headless --editor --quit --path client   # exit 0
godot --headless --path client --script res://tools/shaders_check.gd   # exit 0
```

## Golden moves for the batched regen

L06 T1–T3 moved **no** golden. The batch (after L06 T4–T7, wave 3) so far covers only
`mind-core maps::planet::{serpulo,erekir,asteroid}::tests::*_generation_checksum` (L02 T1).

## Findings

1. Repair tower healing is unported (`RepairTower.java:62-80`); the port only ticks its
   refresh timer (`units/behavior.rs` `RepairTowerBehavior`).
2. `BuildTurret` heal-suppression efficiency cut (`BuildTurret.java:132-134`) unported.

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0402 | T1 (assembler gate/hand-off/commandPos/areaSize), T2 (router ControlBlock, playerControllable) and T3 (suppression, laser/pierce length) landed; remaining PAR-0402 parts: tether I/O (T4), liquid output sides (T5) | bfc8d589, f9403276, fcf2e192 + tests |
| new | Repair tower never heals in the port | finding 1 |

## Done criteria

T1 and T3 landed with tests (T2 too). T6.1–T6.3 and T7 pending (wave 3).
