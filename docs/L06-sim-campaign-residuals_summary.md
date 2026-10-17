<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L06 — Simulation and campaign residuals — summary

Owner: session B (session 3 worktree, branch `parity/session-3`). Round L.

| Task | Item | Status | Wave | Commit |
|---|---|---|---|---|
| T1 Unit assembler output gate, hand-off, commandPos | PAR-0402 | **DONE** | 2 | bfc8d589 |
| T2 Router ControlBlock + `Turret.playerControllable` | PAR-0402 | **DONE** | 2 | f9403276 |
| T3 Bullet suppression + pierce/laser length pre-pass | PAR-0402 | **DONE** | 2 | fcf2e192 (+ fc5774fc doc fix) |
| T4 Tethered units / `EmptyDataAbility` I/O | PAR-0402 | **DONE** | 3 | b71cec34 |
| T5 `DrawLiquidOutputs` side regions | PAR-0402 | **DONE** | 3 | ebe92178 |
| T6.0 Generated sector layout (Arc `Simplex.perm`) | PAR-0424 | **DONE** | 3 | 341d9f74 |
| T6.1 `prebuildBase` / `allowCampaignRules` + difficulty gate | PAR-0391 | **DONE** | 3 | b73b95a0 |
| T6.2 Erekir generator rules | PAR-0391 | **DONE** | 3 | 878e2bb3 |
| T6.3 Abandon = game-over path | PAR-0391 | **DONE** (real-input walk) | 3 | 5e70eed4 |
| T6.4 `showSectorLandInfo` + land-info gate | PAR-0391 | **DONE** | 3 | f6ca1456 |
| T6.5 Planet clouds | PAR-0391 | **DONE** (twin frames) | 3 | 3b64692d |
| T7 Interplanetary Accelerator | PAR-0296 | **DONE** headless + Godot frames; Java capture blocked; tap hook pending (session A file) | 3 | eeedfbb1, 4d52cb3b, ca701a9c, 52e6e6c4 |
| stretch: repair tower heal + build-turret suppression | PAR-0437 | **DONE** | 3 | 245a0fe9 |

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

## T4 — unit save revision 4 (b71cec34, predecessor)

`SIM_UNIT_REVISION 4` appends `TypeIO.writeAbilities` (`u8 len` + `f data`, TypeIO.java:446-451)
and `UnitTetherComp.spawnerUnitId` (`i32`, UnitTetherComp.java:17); load copies the data
index-aligned (`EmptyDataAbility` placeholders on a count mismatch, UnitComp.java:549-557) and
resolves the spawner after the id restore (`afterRead`, :20-24). Revisions 1-3 still load. Test:
`io::sim_io::tests::unit_abilities_and_tether_round_trip`. No vanilla unit carries
`UnitTetherc` (the test stages one).

## T5 — `DrawLiquidOutputs` (ebe92178, predecessor)

`DrawDesc::LiquidOutputs` (DrawLiquidOutputs.java:16-26, `load` :46-55) and the electrolyzer's
`DrawMulti` chain (Blocks.java:1356-1398). Test: `render::draw_desc::tests::liquid_outputs_draw_per_side_regions`.
Deviation: the port's `LiquidTile` draws the building's current liquid, not specifically water.

## T6 — campaign rules and generators

- **T6.0 (341d9f74, PAR-0424):** Arc `Simplex.perm(seed, x)` = `x & 255`, unsigned shifts
  (javap of the pinned jar); Simplex/Ridged/rawNoise match Java at five probe points. The
  editor Sector Generate's missing ruins are L07 T3.2 (session A).
- **T6.1 (b73b95a0):** `PlanetDef.prebuild_base` (Planet.java:133) and `allow_campaign_rules`
  (:139, `isVanilla()` :221), mod JSON; the difficulty button follows
  `allowCampaignRules && mode != planetLaunch` (PlanetDialog.java:739; the planetLaunch half
  landed with T7.2). Recorded: the `prebuildBase` base rebuild (Control.java:230-288) itself.
- **T6.2 (878e2bb3):** `ErekirGenerator::place_structures` writes `rules.env`,
  `placeRangeCheck`, `!waves`, `!hideSpawns` around the loadout (ErekirPlanetGenerator.java:
  423-433). The plan's `Waves.generate` does not apply to Erekir.
- **T6.3 (5e70eed4):** `abandonSectorConfirm` for the played sector (PlanetDialog.java:1149-1170):
  the paused dialog (and planet page) hide, `CampaignRuntime.pending_abandon_kill` counts 7 ticks
  (`Time.runTask(7f)`), every `player_cores` core is killed, and the campaign `checkGameState`
  (Logic.java:329-334) -> `apply_sector_loss` plays. The off-play branch (record clear) is
  unchanged; the old delete-and-quit stays only as a no-runtime fallback. Test:
  `sim::production_tests::abandon_played_sector_kills_cores_and_plays_game_over`.
  Deviation: Java's `Time.runTask` counts real frames; the port counts sim ticks (the game is
  running in both because the pause dialog hides first).
- **T6.4 (f6ca1456):** `SectorPresetDef.show_sector_land_info` (SectorPreset.java:34, mod JSON),
  `SHOW_SECTOR_LAND_INFO` (Vars.java:191) and `shows_sector_land_info` = CoreBlock.java:414's gate;
  `get_sector_state.showLandInfo`; landing_core.gd skips the announce otherwise (custom-map landings
  no longer announce). Test: `mods::json::tests::sector_show_land_info_flag_and_gate`.
- **T6.5 (3b64692d):** `render::g3d::hex_sky` (`HexSkyMesh`, HexSkyMesh.java:16-63): layer tables
  for Serpulo/Erekir (Planets.java:49-52,127-130), `build` = `buildHex(..., planet.radius, radius)`,
  `skip` noise, `rel_rot`, `cloud_alpha` / `clouds_hidden`; `MindCampaign.planet_clouds`;
  planet_view.gd draws one shell per layer (priority -7/-6, atmosphere moved -7 -> -5 to keep
  Java's clouds -> atmosphere order); clouds.gdshader decodes gamma like planet.gdshader. Test:
  `cloud_layers_skip_like_java` pins kept-tile counts + id hashes from a jar probe
  (`runs/20261018-s3-L06-T6.5/probe/CloudProbe.java`: serpulo 567/724, erekir 840/862 of 2432)
  and `cloud_shell_height_colour_and_gates`.

## T7 — Interplanetary Accelerator (PAR-0296)

Same-planet core launch (vanilla never fills `Planet.launchCandidates`; J07 T4 record).

- **T7.1 (eeedfbb1, mind-core):** `Accelerator.init()` capacities/consume (core-nucleus
  requirements, itemCapacity 25000); `AcceleratorDef` (launchBlock, 13000 extra capacity, 1e6
  power buffer, 120/220/120 durations); Java `updateTile` (heat/statusLerp/time/launchHeat,
  `progress += efficiency / buildDuration`), `version() = 1` progress I/O,
  `acceptItem`/`getMaximumAccepted`; `Sim::accelerator_can_launch` (:239-241),
  `accelerator_config_tapped` (`Trigger.acceleratorUse`), `accelerator_launch` (consume, carry,
  `useBatteries`, `progress = 0`, launching), `accelerator_end_launch`;
  `SectorView.accelerator_selectable` (`canSelect` in planetLaunch, PlanetDialog.java:414-435);
  `AcceleratorLaunchAnimator` (zoomLaunch :394-412); `Schematic::single_core`
  (`Universe.updateLoadout(CoreBlock)`). Tests: `accelerator_builds_core_gates_and_launches`,
  `accelerator_requires_campaign_and_writes_progress`, `accelerator_launch_select_rules`,
  `accelerator_launch_zoom_phases`.
- **T7.2 (4d52cb3b) + T7.3 (ca701a9c) + 52e6e6c4:** sim_host `accelerator_tapped` (whole tap:
  no panel, picker when `canLaunch`), `set_pending_launch_core` / `place_launch_core`
  (single-core loadout for generated and preset sectors); `MindCampaign.accelerator_launch_to`
  (sim half, camera `show_accelerator_launch`, beginLaunch fx/sounds, the chargeDuration burst +
  `Effect.shake(10, 14)`, and after 334 ticks `clearLoadoutInfo` / `updateLaunchResources` /
  `updateLoadout(core-nucleus)` / prime + load + start the sector); planet_dialog.gd
  `planetLaunch` mode (zoom 1, uiAlpha 0, difficulty hidden, `@sectors.launch`,
  `listener.get(sector)` + hide, tap gated on `accelerator_selectable`); `canLaunchTo`
  (`allowSelfSectorLaunch`, PlanetDialog.java:610).
- **Not ported (recorded):** the launch lightning (20 bolts + per-tick charge bolts, damage 40,
  Accelerator.java:364-367,423-429), `coreLandDust` rings (:369-385), `drawLaunch` rings/clouds
  and the launching-core sprite in `draw()` (:173-204,440-556), the white fade image
  (:343-354), `unlockedOnLand` (empty on Serpulo), the `@sectors.launchselect` info text and
  the hover cancel glyph; mod `launchBlock` / `launchCandidates` (vanilla values only).
- **Cross-session hook (blocker for a real-input tap):** `input/desktop.rs` `config_spec`
  (session A) still opens the generic item panel for an accelerator. The hook:
  `if def.kind == BlockKind::Accelerator { drop(guard); host.bind_mut().accelerator_tapped(x, y); return None; }`.
- **Java capture blocked:** the block is `campaignOnly` (Blocks.java:6874) and `canLaunch`
  needs a campaign (:240); a capture would rewrite reference-profile sector saves
  (L-environment rule 7). Accepted on headless tests + Godot frames.

## PAR-0437 — repair tower (245a0fe9, stretch)

`update_repair_tower` = `RepairTowerBuild.updateTile` (RepairTower.java:57-86): 6-tick refresh
while `potentialEfficiency > 0`, same-team damaged units within `range + hitSize / 2`,
`checkSuppression` -> `warmup = 0`, heal `healAmount * edelta()`, warmup/totalProgress,
`shouldConsume = targets.size > 0`; unit-repair-tower range 100 / heal 1.5 (Blocks.java:
6612-6621). `build_turret_efficiency` = BuildTurret.java:132-134. Tests:
`repair_tower_heals_units_and_respects_suppression`, `suppressed_build_turret_has_zero_efficiency`
(the combat harness cannot drive a `Building`-backed build turret, so that gate is tested at the
helper `update_build` calls).

## In-engine evidence (wave 3, session 3, display :12, bridge 6990)

Fresh flock build before the first walk (4d52cb3b), rebuilt for ca701a9c.

| Task | Run dir | Input | Result |
|---|---|---|---|
| T6.5 | `runs/20261018-s3-L06-T6.5/` | real clicks + 12 real wheel ticks on both clients | both show the two HexSkyMesh shells once zoomed out (`diff/03-zoomed-out-side.png`); Godot cells read lighter (linear vs gamma blending) |
| T6.3 | `runs/20261018-s3-L06-T6.3/` | real clicks: Ground Zero -> Esc -> Abandon -> OK | confirm "core(s) will self-destruct", the core dies, "Sector … lost!" game-over pane |
| T7 | `runs/20261018-s3-L06-T7/` | staging: place accelerator/batteries/power/items + `accelerator_tapped`; real: sector pick + Launch | charge zoom, launch zoom-out, sector 63 founded with a core-nucleus carrying 500 titanium; second launch lands normally |

Godot only for T6.3 and T7 (Java would change the reference profile's campaign state).

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

## Test commands (end of wave 3, branch head 52e6e6c4)

```
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check                       # clean
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings  # clean
cargo test -p mind-core --no-fail-fast      # 2802 passed, 0 failed, 4 ignored (fully green)
cargo test -p mind-headless --no-fail-fast  # 142 passed, 0 failed
cargo test -p mind-gdext                    # 77 passed
mind-headless parity goldens                # 97 PASS / 0 FAIL
mind-headless run-all --tier T0             # 4/4
tools/regen_goldens.sh --check              # 0 FAIL
bash tools/godot.sh --headless --editor --quit --path client                 # exit 0, no script errors
bash tools/godot.sh --headless --path client --script res://tools/shaders_check.gd   # total=29 failed=0
```

## Golden moves (batched regen, L08 T1 protocol)

Regenerated in five commits (467b6081, 28a4f20c, 7d343932, 643169f6, 3fef57cd), each attributed by
scratch-worktree runs at the boundary commits; details in the L08 summary (T1, wave-3 batch).
Movers: 38dc36f1 (L02 T1 `Floor.decoration`), 341d9f74 (T6.0 `Simplex.perm`), b73b95a0 (T6.1
`allow_campaign_rules` in the hashed campaign views), eeedfbb1 (T7.1 `Accelerator.init` capacity,
`accelerator_selectable`). T4, T5, T6.2-T6.5, T7.2-T7.3 and PAR-0437 moved none.

## Findings

1. ~~Repair tower healing is unported~~ - fixed (PAR-0437, 245a0fe9).
2. ~~`BuildTurret` heal-suppression efficiency cut unported~~ - fixed (245a0fe9).
3. The campaign game-over title reads the preset id ("Sector groundZero lost!") instead of
   `Sector.name()` = the localized "Ground Zero" (`GameOverDialog`; pre-existing, seen in the
   T6.3 walk, `runs/20261018-s3-L06-T6.3/godot/06-game-over.png`).
4. The planet page keeps the sector grid/selection visible while zoomed out (Java fades it with
   `uiAlpha`); seen in the T6.5 frames, pre-existing.
5. The accelerator tap still opens the generic item panel until session A adds the `config_spec`
   hook (T7 above).

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0402 | close: T1-T5 all landed (T4 tether/ability I/O b71cec34, T5 liquid output sides ebe92178) | commits + tests above |
| PAR-0391 | close: T6.1 prebuildBase/allowCampaignRules, T6.2 Erekir rules, T6.3 abandon game-over path, T6.4 showSectorLandInfo, T6.5 clouds; residual note: the `prebuildBase` base rebuild (Control.java:230-288) is unported | b73b95a0, 878e2bb3, 5e70eed4, f6ca1456, 3b64692d; runs/20261018-s3-L06-T6.3, -T6.5 |
| PAR-0424 | close (generated Serpulo layout matches Java; ruins in the editor's Sector Generate = L07 T3.2) | 341d9f74 |
| PAR-0296 | landed (headless + Godot frames); keep open for (a) the session-A `config_spec` tap hook, (b) a Java capture with a throwaway campaign profile, (c) the recorded launch visuals (lightning, rings, dust, fade) | eeedfbb1, 4d52cb3b, ca701a9c, 52e6e6c4; runs/20261018-s3-L06-T7 |
| PAR-0437 | close: repair tower heals; repair tower and build turret honour heal suppression | 245a0fe9 |
| new | Game-over title shows the preset id instead of the localized sector name (finding 3) | runs/20261018-s3-L06-T6.3/godot/06-game-over.png |
| new | Planet page grid/selection not faded by uiAlpha when zoomed out (finding 4) | runs/20261018-s3-L06-T6.5/godot/03-zoomed-out.png |

## Done criteria

T1, T3, T6.1-T6.3 and T7 landed with tests (T2, T4, T5, T6.0, T6.4, T6.5 too). Moved goldens are
regenerated in the batched pass with attribution; `cargo test -p mind-core` is fully green.
Open: the T7 tap hook (session A file) and the T7 Java capture (profile constraint).
