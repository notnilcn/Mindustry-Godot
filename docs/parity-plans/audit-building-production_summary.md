# Execution summary — audit + execution plan: building, production/logistics and block config

- **Plan:** `docs/parity-plans/audit-building-production.md` (absolute: `/home/clintonnguyen/g/code_examples/Mindustry-Godot/docs/parity-plans/audit-building-production.md`)
- **Session:** 4 — worktree `../Mindustry-Godot-session4`, branch `parity/session-4`, display `:13`, bridge `7000`
- **Dates:** 2026-10-12 → 2026-10-13
- **Start state:** branch tip `06340c4` (main, fast-forwarded per the OBSOLETE §0.1 note; the audit fixes were already on main)
- **End state:** `d1ef222`, 14 commits ahead of main; worktree tree clean; no pushes/merges

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T0 building-inventory seams | **done** | `debug_add_building_items` + `building_items_json`; in-engine verified |
| T1 `shouldConsume()` | **done** | hook + `Building.should_consume` + power demand + 8 block overrides; in-engine verified |
| T2 unit factories | **done** | power gate, `unitBuildSpeed`, progress reset, output-unit adoption into `UnitRuntime`; in-engine verified |
| T3 reconstructor command + move-in | **done** | `ControllerSlot.command`, `spawn_output_unit_with_command`, `ReconstructorBuild.move_in`; headless only (no payload seam) |
| T4 reconstructor command panel | **done** | 10 command options + `current` readback; in-engine verified with a real tap |
| T5 plasma bore facing | **done** | per-tick `refresh_facing_drills` index; headless only (no Erekir sector) |
| T6 overlap filter | **done** | elevation-based grounded + `allow_leg_step`; headless |
| T7 solid pumps | **done** | attribute names, `valid_tiles`/`boost`, canPlaceOn gate, 24 floor affinities ported; headless |
| T8 wheel guard | **done** | hovered-control ancestor walk; in-engine verified |
| T9 switch click sound | **done** | `play_ui_sound("click")` via MindUi; structural evidence only |
| T11 `placed()` hooks | **done** | power auto-link (`getNodeLinks`/`get_potential_links`) + bridge chaining; in-engine verified |
| T12 tap-to-link config | **not reached** | large gdext work; see follow-ups |
| T13 door tap toggle | **done** | tap branch + unit-in-door check + 60-tick throttle; in-engine verified |
| T14 PAR-0089 sublimate | **done** | activation latch + per-tick drain; the turret log had no existing fix; headless only |
| T15 schematic copy mode | **not reached** | see follow-ups |
| T16 construction beam/ghost drawing | **handoff** | rendering owner (see below) |
| T17 rebuild ghosts / `rebuild_select` | **not reached** | see follow-ups |
| T18 schematic via plans | **done** | rotation through `SimCommand::Place` + `queue_player_place`; headless |
| §5 UNVERIFIED rows | **not reached** | see follow-ups |
| T10 illuminator light | **handoff** | rendering owner |

## Commits (`git log --oneline main..HEAD`)

```
d1ef222 Audit fix T18: schematic placement goes through player plans
75e8595 Audit fix T14 (PAR-0089): sublimate fuel latch and per-tick drain
734025e Audit fix T8: wheel guard walks only the hovered control chain
64cdd93 Audit fix T13: doors toggle on tap with the 60-tick throttle
0dbd8a0 Audit fix T9: switch tap plays the click sound
d639ea4 Audit fix T5: rescan the facing edge of beam drills every tick
bbe1af8 Audit fix T7: solid pumps read the floor attribute table
a43aaa9 Audit fix T6: legged units do not block construction
b1056f7 Audit fix T11: Building.placed() power auto-link and bridge chaining
5e9545d Audit fix T4: reconstructor command panel
955c9f0 Audit fix T3: reconstructor applies its command and moves the payload in
c94c27c Audit fix T2: unit factories gate on power and unitBuildSpeed
b1ea869 Audit fix T1: honour Building.shouldConsume (consumer + power gating)
4f9018d Audit fix T0: add building-inventory playtest seams
```

Commit-message note: the tasks are audit-found gaps, not numbered ledger items (the plan's `Fixes PAR-XXXX` form has no id for them), so the repo's existing `Audit fix T<n>:` convention was used (cf. `21093b3 Audit fix: ...`).

## Tests (exact commands, run from `client/rust/` behind the host-wide `flock /tmp/opencode/mg-build.lock`)

```
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib
  -> 1947 passed; 0 failed; 4 ignored
CARGO_BUILD_JOBS=6 cargo test -p mind-headless --lib
  -> 130 passed; 2 failed; 0 ignored
     failing: campaign_scenarios::tests::sector_golden_matches,
              units_scenarios::tests::committed_goldens_match   <-- the 2 pre-existing failures the plan says to ignore
CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   -> no output (clean)
cargo fmt -p mind-core -p mind-gdext -- --check                                            -> clean
CARGO_BUILD_JOBS=6 tools/build.sh                                                          -> Finished dev profile; sync_scenarios 11 files
```

New tests added: `reconstructor_idle_requests_no_power`, `full_crafter_stops_consuming`,
`full_drill_stops`, `unit_factory_needs_power`, `reconstructor_applies_command`,
`content_config_resolves_unit_commands`, `node_autolinks_on_place`,
`consumer_autolinks_to_node_in_range`, `bridges_chain_when_player_placed`,
`water_extractor_scales_with_floor`, `oil_extractor_needs_oil_floor`,
`plasma_bore_stops_when_ore_removed`, `door_toggles_and_throttles`,
`legged_unit_does_not_block_construction`, `sublimate_activation_window`,
`queue_player_place_keeps_rotation`.

## In-engine verification (Ground Zero, sector 170, imported profile nodes 375/unlocked 282)

Evidence files/screenshots under `runs/20261012-session4-bp/` (shared symlink):

| Task | Evidence | Shows (vs Java) |
|---|---|---|
| T0 | `t0-t1-evidence.json` | `debug_add_building_items(...,silicon,10)` -> `10.0`; `building_items_json` -> `{"silicon":10}` |
| T1 | `t0-t1-evidence.json`, `t0-t1-after.png` | graphite-press with 10/10 graphite + 4 coal: after 4 s `building_items_json` `{"graphite":10,"coal":4}`, `building_debug` efficiency 0.0 / potential 1.0 (Java `update = false`) |
| T2 | `t2-evidence.json`, `t2-after.png` | unpowered factory keeps 10 silicon + 10 lead over 20 s; powered factory produces one dagger (type 0, commandable) and consumes both stacks; graph needed 1.2 |
| T4 | `t4-evidence.json`, `t4-reconstructor-panel.png` | real tap opens the panel with 10 `command.*` options; `configure_building(...,"content","rebuild")` -> `{"type":"content","content":"unitCommand","id":2}`; re-opened panel `current: "rebuild"` |
| T8 | `t8-t9-t13-evidence.json` | wheel over the world zooms 4 -> 5; wheel over an open dialog leaves the zoom unchanged |
| T9 | same file | switch tap still toggles without a panel; `play_ui_sound("click")` emitted (audio not assertable) |
| T11 | `t11-evidence.json`, `t11-after.png` | player-built power node (150,58) shares graph 9 with the instant combustion-generator (150,60); instant laser-drill (148,58) joins graph 9, status 1.0 |
| T13 | same file | real tap toggles the door; two configures in one call leave it closed (second throttled) |

Headless-only (the plan itself says in-engine is blocked): T5, T6, T7, T14, T18, plus T3
(no payload-injection seam; the config half of T3 is exercised by T4's tap).

## T10/T16/T17 handoffs and cross-area notes

- **T10 illuminator light (rendering owner):** `LightBehavior` stores the configured color
  (`light_color(x,y)` seam); the light pass is not drawn with it. Java
  `LightBlock.java:126-129` `drawLight` (radius `200 * min(smoothTime, 2)`, brightness
  `0.9 * efficiency`). Not implemented here per the plan.
- **T16 construction ghost/beam (rendering owner):** plan drawing and the build beam data
  seams exist (`player_build_plans_json`, `MindInput.placement_preview_json`,
  `MindSimHost::construct_draw/construct_at`); Java `BuilderComp.java:337-386`,
  `ConstructBlock.java:251-275`, `InputHandler.drawPlan/drawBreaking`. Not implemented here.
- **T17 rebuild ghosts (not reached):** `rebuild_select` key edge + `queue_team_rebuild` +
  `BlockPlan` push on building death are untouched. Note: `Building.placed()`/lifecycle
  hooks now run on construct-finish (T11), so a rebuild implementation can hook `on_destroyed`.
- **T12 tap-to-link (not reached):** the selection state, `on_configure_build_tapped` and
  the minimal spec for bridge/node/driver taps are untouched.
- **T15 schematic copy mode (not reached).**
- **§5 UNVERIFIED rows (not reached):** laser drill water boost, impact-drill burst,
  separator distribution, thermal/nuclear generators, plastanium stack conveyor, unloader,
  mass driver, constructor payload, core upgrade, launch pad. One test each per the table.
- **Power findings worth a follow-up:** during T2/T7 debugging, a power source placed so
  that its 1x1/2x2 footprint *overlaps* a multiblock (only reachable through the ungated
  `place_block`) leaves the merged graph without the consumer; ordinary adjacency merges
  correctly (T11 tests). Also, the vanilla **sandbox `power-source` is 1x1** — test recipes
  that assume a 2x2 source leave a gap.
- **Block runtime vs campaign research:** the audit's note stands — behaviors must not trust
  the runtime registry's `unlocked()`.

## Blockers

- None that stopped a task: no plan precondition failed. T5/T14 in-engine halves are blocked
  by design (no launchable Erekir sector in the imported profile), exactly as the plan states.

## Follow-ups for the coordinator

1. **Merge order:** `parity/session-4` is a clean fast-forward from `06340c4`; nothing else
   was touched. T14 edits the shared turret files, so if the combat executor also fixes
   PAR-0089 there will be a trivial conflict (config/state fields + `update_continuous`).
2. **Remaining tasks:** T12, T15, T17 and the §5 test rows are untouched on this branch
   (each is independent; T12's `link_valid` port from T11 is already in place).
3. **Goldens:** no golden file changed (the content parity tests pass unchanged).
4. **In-engine evidence location:** `runs/20261012-session4-bp/` (shared with the main
   checkout through the `runs/` symlink).
5. If a later session revisits T11: the `SimRuntime.facing_drills` index pattern (T5) is a
   good template for grid-needing per-tick scans.
