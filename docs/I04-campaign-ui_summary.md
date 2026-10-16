# I04 — Campaign UI (session 2) — summary

Session 2, worktree `Mindustry-Godot-session2`, branch `parity/session-2`,
bridge 6980, display :11. Plan: `docs/parity-plans/I04-campaign-ui.md` (17
tasks). Executed 2026-10-16.

Status: **T1–T7 done (8 commits). T8–T17 not reached (budget).** The branch is
clean and fully committed; all gates green at the last task.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T1 palette rebuild on world load/unlock | done (incl. extra fix) | runs/20261016-s2-I04-T1 (onset palette = ducts, back to GZ = conveyors) |
| T2 unlock toast | done | runs/20261016-s2-I04-T2 (scrap-wall research toast + merged 2-icon grid) |
| T3 hint predicates/events | done | runs/20261016-s2-I04-T3 (`hint_state_json` events ["factorycontrol"]) |
| T4 logistics/storage/logic/defense stats | done | runs/20261016-s2-I04-T4 (titanium-conveyor Move Speed 10) |
| T5 unit factory/payload stats | done | runs/20261016-s2-I04-T5 (additive-reconstructor pairs) |
| T6 turret+ammo / unit weapons | done (2 commits) | runs/20261016-s2-I04-T6 + -T6b (duo ammo vs java-26-duo-info.png; dagger weapons) |
| T7 schematics dialog (Fixes PAR-0143) | done | runs/20261016-s2-I04-T7 (menu cards + GZ card = 100 select plans + info dialog) |
| T8 planet globe drag/zoom/last sector/reveal | not reached | — |
| T9 sector rename + icon picker | not reached | — |
| T10 sector stats dialog + Abandon | not reached | — |
| T11 database planet tabs (Fixes PAR-0144) | not reached | — |
| T12 dialog fade | not reached | — |
| T13 planet hex mesh (PAR-0142 p1) | not reached | — |
| T14 Serpulo emissive glow (PAR-0142 p2) | not reached | — |
| T15 planet bloom (Fixes PAR-0142) | not reached | — |
| T16 campaign UI cosmetics | not reached | — |
| T17 enemy bases (PAR-0112 step 3, headless) | not reached | — |

No task was skipped for a cross-plan dependency; T8–T17 are simply beyond the
session budget.

## Commits (`git log --oneline main..HEAD`)

```
9f07b0de Fixes PAR-0143: schematics cards, tags row and info dialog per SchematicsDialog
30f726c6 Gameplay I04 T6: unit weapon and ability content stats
ff3037b1 Gameplay I04 T6: turret and ammo content stats
6224b980 Gameplay I04 T5: unit factory and payload content stats
28c2bd32 Gameplay I04 T4: logistics, storage, logic and defense content stats
67a6ae8a Gameplay I04 T3: remaining hint predicates and hint events
8d213c5e Gameplay I04 T2: unlocking content shows the HUD unlock toast
90355c94 Gameplay I04 T1: placement palette rebuilds on world load and block unlock
```

## Tests (last run, all green)

```
cargo test -p mind-core --lib --features msav-import
  test result: ok. 2657 passed; 0 failed; 4 ignored
cargo test -p mind-headless --lib      -> ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib         -> ok. 51 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> clean
cargo fmt -p mind-core -p mind-gdext -- --check -> clean
cargo run -q -p mind-headless -- parity goldens -> 97 PASS / 0 other
bash tools/godot.sh --headless --editor --quit --path client | grep SCRIPT ERROR|Parse Error -> nothing
```

Task tests: T3 `factory_command_records_hint`, `locked_upgrade_blocks_and_fires_hint`;
T4 `conveyor_stats`, `mender_stats`, `liquid_junction_has_no_liquid_capacity`;
T5 `additive_reconstructor_lists_unlocked_upgrades`; T6
`duo_ammo_lists_copper_graphite_silicon`, `dagger_weapon_stats`.
Goldens regenerated: `client/rust/mind-core/tests/goldens/ui/campaign.json`
(T7: view fields `power_consumption`/`power_production` + filtered loadouts +
fixture change) with `harness_ui_campaign` sha256 updated. Panic count 0 after
every in-engine step.

## In-engine evidence (paths under the shared `runs/`)

* T1: `runs/20261016-s2-I04-T1/{gz-conveyor-selected,onset-after-load,gz-back-conveyors}.png`;
  Onset palette = 7 ducts, `block` null after load, GZ back to conveyor.
* T2: `runs/20261016-s2-I04-T2/{unlock-toast,unlock-toast-merged}.png`;
  `_unlock_grid` [2, [24.0, 24.0]].
* T3: `runs/20261016-s2-I04-T3/{factory-command.png,hint-events.json}`;
  `hint_state_json` events `["factorycontrol"]`, fabricators 1.
* T4: `runs/20261016-s2-I04-T4/{database,titanium-conveyor-info}.png`;
  `itemsMoved` = `["10"," items/second"]`.
* T5: `runs/20261016-s2-I04-T5/additive-reconstructor-{info,scrolled}.png`.
* T6: `runs/20261016-s2-I04-T6/duo-info-ammo.png` (copper 9 damage / 2 ammo/item,
  inaccuracy 2 degrees) matches `runs/20261014-s3-verify3/java-26-duo-info.png`;
  `runs/20261016-s2-I04-T6b/dagger-info.png`.
* T7: `runs/20261016-s2-I04-T7/{schematics-menu,schematic-info,gz-card-placement}.png`;
  card press -> dialog hidden + `placement_preview_json()` = 100 plans.

## Deviations from Java (by task)

* T1: `MindCampaign.block_catalog_json` keys the campaign planet off the loaded
  world (`last_sector_rules().planet`) when one exists — Java's `state.rules`
  is the loaded world's from `World.loadSector` on, while the port's facade
  session flips only in `start_sector` (the posted `WorldLoadEvent` rebuild
  would otherwise read the old planet; `PlacementFragment.java:78-84`).
* T2: the shared `HudFragment.scheduleToast` 3.5 s slot is not ported (the port
  shows the toast at once when no box lives; `HudFragment.java:729-740`); the
  toast icon resolves by `<type>-<name>-ui` region candidates (the signal
  carries the name, not `content.uiIcon`).
* T4: plan's `duct_junction_has_no_item_capacity` unbuildable — Java
  `DuctJunction` has no vanilla block (mod `ClassMap` only; same in the port);
  test uses `LiquidJunction` (`LiquidJunction.java:21`). `CoreBlock` unit panel
  drops the `?` button/`console` name row. `ForceProjector` coolant panel not
  ported (`ForceProjector.java:117-140`).
* T5: plan's "dagger -> mace first" is Java-reversed — `Blocks.java:6345-6352`
  lists nova -> pulsar first; the test asserts the Java order.
* T6: turret coolant booster panels (`ReloadTurret.java:17`,
  `LaserTurret.java:26`) not ported; frag/interval panels render expanded (no
  collapse button, `StatValues.java:906-945`); `statusText` emoji skipped
  (`StatValues.java:229`); `reloadWhileCharging` absent (Java default true).
  Unit `mineTier` omits the `StatValues.drillables` list
  (`UnitType.java:824-833`); abilities render as name+knob panels (Java calls
  each ability's own `display`).
* T7: Tags pencil adds a tag through the shared text prompt (Java's
  `showAllTags` editor and `checkTags` are not ported); `pencilSmall` is
  approximated by `pencil`.
* Incidental fixes found while landing the above:
  `content_info_dialog.gd` rendered `Cell::Icon` null amounts as `<null>` and
  flattened nested panel rows onto one line (T5/T6); the schematics dialog's
  `_in_game()` used `== "playing"`, so in a `should_pause` match every card
  click opened the info dialog instead of starting placement (T7, folded into
  the PAR-0143 fix).

## Blockers / notes

* **Merge with main**: `main` advanced to `872a7430` (I06 pass 7) while this
  session ran. `git merge --no-edit main` conflicted in
  `client/rust/mind-gdext/src/campaign.rs` (both sides edited `campaign.rs`);
  per policy the merge was aborted (`git merge --abort`) and the branch left
  clean. The coordinator must resolve that conflict when merging
  `parity/session-2` (likely both keep-additions in `campaign.rs`; my hunks
  are the T1 palette planet source, the T2 unlock producers + `StatCtx.campaign`
  and the T7 `list_schematics`-adjacent code).
* The T7 fixture change (`vanilla_fixture` keeps one file-backed schematic)
  changes the `harness_ui_campaign` golden; regenerated per env §2.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0143 | verified (T7) |
| new | "Placement palette keeps the previous planet's blocks" -> verified (T1) |
| new | "Unlock toast never shows (no `MindHud.push_unlock` producer)" -> verified (T2) |
| note | T3: `cannotupgrade`/`factorycontrol` now have producers; the hint JSON `cores_destroyed` reads the session stats |
| note | T4/T5/T6 content-stats families landed (blocks/units); remaining stats deviations listed above |
| note | `content_info_dialog` null-amount + panel-row rendering bugs fixed (T5/T6) |
| note | schematics `_in_game()` paused-state bug fixed under PAR-0143 (T7) |
| PAR-0144, PAR-0142, PAR-0112 | **not reached** (T11, T13-T15, T17 remain for a follow-up session) |

## Done criteria

Not met in full: T1–T7 are committed with their checks, suites/goldens/parse
gate are green, and the T7 golden was regenerated; T8–T17 remain for a
follow-up session (the branch is clean and ready for the coordinator merge).
