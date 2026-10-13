# Verification round 1 — session 3 (H02 blocks, H08 input) — 2026-10-14

Worktree `Mindustry-Godot-session3` @ `ffbe84dd` (ff-merged from main), bridge 6990, display `:12`.
Godot `--import` profile (`{"nodes": 375, "unlocked": 282}`), Ground Zero entered with the
"Go" recipe. Evidence: `runs/20261014-s3-verify/` (PNGs + `H02-evals.json`, `H08-evals.json`).

**Player unit.** Pre-H00 `rules.env` is 31 (scorching bit set), and `debug_spawn_player`
returns `true` but the alpha is killed on the next tick by the env check
(`entities/comp/unit/environment.rs:233-240`), so the H-environment workaround does NOT work.
`MindCampaign.set_rules_json` refuses campaign edits (`allowEditRules`). I applied H00 T1's
`env_bit` table locally (uncommitted, `core/game/planet.rs`), rebuilt, and ran every check with
the real alpha (env 233). The patch was reverted and rebuilt at the end.

## H02 — blocks

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 core incineration | CONFIRMED | `H02-T1-core-full-belt.png`; belt (131,55) stuck at `{"copper":3}` | Recipe rewritten (belt + source; a source against the core cannot show a backlog) |
| T2 sync_build_rules / validBreak | PARTLY | `H02-T2-core-broken.png` | Last core breakable (destructive, Sector Lost autosaved). Enemy break not visible (builder drops it). Recipe warning + enemy staging |
| T3 validPlace / kind_rotates | CONFIRMED | `H02-evals.json` T3 | Enemy conveyor replaced by player titanium conveyor. Deep water and enemy core radius → headless only (none in playable area). Free-ground map fixed in §2 intro |
| T4 core upgrade | CONFIRMED | `H02-T4-core-upgrade-lost.png` | Foundation accepted on free ground; over the shard it destroys the core (Sector Lost). "Run only after the fix" warning, block id 340 |
| T5 config wire / bridge hang | PARTLY | `H02-T5-bridge-int.png`, `H02-T5-door-sorter.png` | Hang NOT reproduced; door/sorter/bridge configs already read back. Mangling is code-only. Sorter sprite lacks its item icon (H03). Coords fixed (old ones were walls), unlink readback documented |
| T6 tap-to-link | CONFIRMED | `H02-T6-tapA.png`, `H02-T6-tapB.png`, `H02-T6-node-tap.png` | Coords fixed; B placed north of A (the panel covers right/below A); node check via config, not `graph_id` |
| T7 force projector | PARTLY (main symptom NOT reproduced) | `H02-T7-force-projector.png` | Efficiency 1.0 and the dome is drawn. Only the live absorption gap (code) remains. Coords + dagger spawn fixed; inventory row annotated |
| T8 drill knobs | NOT CHECKABLE (headless) | code | — |
| T9 overdrive | NOT CHECKABLE (headless) | code | — |
| T10 liquids | NOT CHECKABLE (headless) | code | — |
| T11 unitOn | CONFIRMED | `H02-evals.json` T11 | Coords fixed (x=150 wall) |
| T12 walls | NOT CHECKABLE (headless) | code | — |
| T13 save bodies | NOT CHECKABLE (headless) | code | — |
| T14 verification tests | NOT CHECKABLE | — | — |
| T15 beam node | NOT CHECKABLE (Erekir) | code | — |

## H08 — input

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 line rotation | CONFIRMED + new latch bug | `H08-T1-vertical-drag.png`, `java-06-drag-up.png` | An upward drag gives rotation 3. `override_line_rotation` is never reset (reset made mandatory). Wheel expectation corrected: Java gives ALL steps `rotation`; use an upward drag, expect all 2 |
| T2 line flags / HostWorld | CONFIRMED | `H08-evals.json` T2 | Recipe → L-shaped chain (a straight row cannot tell upgradeLine from pathfind) |
| T3 junction/bridges/nodes | CONFIRMED (junction) | `H08-evals.json` T3 | Node drag coords fixed |
| T4 tap/deselect | CONFIRMED (a,b,c) | `H08-T4-enemy-sorter-tap.png` | Coords fixed; place-op field names verified |
| T5 hotkeys | CONFIRMED with corrections | `H08-T5-f1-sorter.png`, `H08-T5-f1-conveyor.png` | Q does not deselect; pick already selects (only the category switch is missing); F1 symptom only on configurable blocks |
| T6a RTS keys | CONFIRMED + new root cause | `H08-evals.json` T6a | `g` selects nothing: infinite `SelectRect` → NaN bounds (`input/rts.rs:51-66`); change 4 amended |
| T6b RTS orders | NOT CHECKED (code only) | — | — |
| T7 command mode guard | CONFIRMED | `H08-evals.json` T7 | No `elevation` field in `unit_debug_json` |
| T8 possession | CONFIRMED (+ PAR-0138 works) | `H08-evals.json` T8 | Java drops payload at the carrier, not the cursor (port matches); 40 px closest-unit pitfall |
| T9 mouse binds/F5/F11 | CONFIRMED (F5, back); F11 code only | `H08-evals.json` T9 | — |
| T10 camera | CONFIRMED (paused WASD) | `H08-evals.json` T10 | — |
| T11 derelict repair | NOT CHECKABLE (needs H02 T3) | — | — |
| T12 onlyDepositCore | NOT CHECKABLE (headless) | code | — |
| T13 GUI clicks | SKIP: not reproduced | `H08-T13-menu.png`, `H08-T13-menu-after.png` | Recipe documented in the SKIP note |
| V1 re-verification | PARTLY (V1.1 only) | `H08-evals.json` V1.1 | — |

## Java leg

The Java client ran on `:12` with the worktree `parity-java-home` (pid 295906, killed at the
end; session 2's Java on `:11` was not touched). The route Play (410,186) → Campaign
(640,186) → Sectors list (940,45) → Ground Zero (890,143) → Go (640,678) reached Ground Zero in
about 3 minutes, at window +190+10, 900x700.

The upward conveyor drag preview points up (`java-06-drag-up.png`). A pynput scroll during a
drag zoomed the camera instead of rotating, twice, and the game stayed paused after a Space
press. So the wheel-during-drag behaviour was not observed in Java. The plan's correction for
it comes from source: IH:2486-2500 and DI:762-768.

Junction replacement, core incineration, Ctrl+click on a turret, the hotkey combos and the
power-node tap were not observed in Java (time budget; the source is unambiguous for each).

## Hiccups

- `debug_spawn_player` does not work before H00: the unit is killed on the next tick by the env check.
- `set_rules_json` rejects campaign edits.
- H02's "free ground (140..158, 55..80)" is half static wall, and many plan coordinates at
  x >= 150 were wrong.
- `center_on_tile` is overridden by the player follow every frame. Use `$C.set_process(false)`.
- One eval (`get_node(...).visible = false; $C.center_on_tile(...)`) timed out and pinned the
  frame loop (frames stopped). `--play` recovered it.
- `clear_player_build_plans()` before the queued commands apply is a no-op: the old plans came back.
- After a clean re-import a "Campaign Changes Notice" (`full_text`) dialog sits on the stack.
  Close it with `MindUi.close_top_dialog()`.
- Team-2 daggers spawned for tests attack the core. Two lancers, each next to a power-source,
  killed them (a lancer one tile away from its source stays at efficiency 0, which is correct).
- Two destructive recipes (H02 T2 core break, H02 T4 core upgrade) caused Sector Lost and an
  autosave. Both needed a clean re-import. The plans now carry warnings.

## For the coordinator to decide

1. **H-environment needs fixing** (I did not edit it):
   - §4.1 says `debug_spawn_player` is the pre-H00 workaround; it does not work while env is 31.
   - Add the free-ground facts and the `$C.set_process(false)` camera recipe to §4.1/§4.2.
   - §4 "synthetic clicks on GUI Controls may not arrive (H08 owns that finding)" is outdated:
     they arrive (H08 T13 SKIP).
2. **PAR-0129** (force projector efficiency 0) did not reproduce on `ffbe84dd`. Close that half,
   or re-scope it to "no live bullet absorption".
3. **H02 T5 bridge hang** did not reproduce. Decide whether T5 keeps the hang investigation or
   drops it.
4. **New bugs found:**
   - H08 T1: the `override_line_rotation` latch.
   - H08 T6a: select-all NaN rect, so select-all selects nothing.
   - H02 T5: the sorter does not draw its configured item (H03 render).
5. **Not checked this round:**
   - H08 T6b in-engine. Command taps need drag-select plus a right-click on an enemy building.
   - V1.2–V1.5.
   - F11 (it would fullscreen the editor-embedded game).
