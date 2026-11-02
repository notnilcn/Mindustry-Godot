<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin parity — phase 2 gap hunt report (2026-11-03)

Phase 2 ran three orchestrators in parallel worktrees after the phase-1 queue
close-out (see [`TWIN-PHASE1-REPORT.md`](TWIN-PHASE1-REPORT.md)). Each one ran a
checker → record/claim → fixer loop against the shared `TWIN-VERIFICATION.json`,
with one Godot session per branch; multiplayer stayed deferred.

## Result

| Status | Session start | Session end |
|---|---:|---:|
| `reference-verified` | 18 | 261 |
| `twin-verified` | 5 | 5 |
| terminal total (of 450 → 467 items) | 23 | **266** |

Every fix below is `reference-verified` on Java-source-derived unit tests plus
`bash tools/build.sh`; no fresh Java+Godot frame pair was run, so none is
`twin-verified`. 12 new gap items were recorded (`PAR-0451..0467`), 9 fixed,
3 left `seeded` with precise evidence for the next session.

Gates on merged `main`: `bash tools/build.sh` exit 0, `cargo test -p mind-core`
exit 0 (2844 lib tests + all integration binaries), `bash tools/parity.sh`
exit 0 (goldens PASS, smoke 4/4), `python3 tools/twin_verification.py check` ok
(467 items).

## Fixed

Branch `gap/a` — simulation/gameplay core (merged `4ed896de`):

| Item | Fix | Commit |
|---|---|---|
| PAR-0451 | turret shot-pattern delays dropped (`Time.run` queueing; scatter/salvo/swarmer bursts, lancer charge) | 4e6b02fe |
| PAR-0455 | duct-router / duct-unloader use the content 4f speed override instead of class defaults | 51f30190 |
| PAR-0457 | `canBoost` mechs land on grounded units (missing `canLand` hover term) | c9c31bf9 |
| PAR-0458 | logic `sensor @id/@color/@name` on `@sharded`/team values returned null | 8626c710, 94347c19 |
| PAR-0460 | thorium reactor cryofluid intake half Java's (0.02 vs 0.04/tick) | 5e774f04 |
| PAR-0462 | anti-air-only turrets targeted ground units and never aircraft (inverted accept predicate + elevation semantics) | 4186a24b |

Branch `gap/b` — campaign/game flow/data (merged `aea2db56`):

| Item | Fix | Commit |
|---|---|---|
| PAR-0452 | sector shield gate inverted: megabases 246/244/242 locked forever, Stormrider open | 43f454da |
| PAR-0453 | threat label compared difficulty to 10 (eradication) instead of 13 (`unreasonable`) | 9d6b6ea1 |
| PAR-0456 | autosave 60× too frequent (`saveinterval` is minutes in Java) | e60b0dad |
| PAR-0459 | editor Save never replaced a same-name map (duplicate entries, unsanitized name) | 188d732c |
| PAR-0464 | editor resize anchored content to the old origin instead of re-centring | bd745b60 |

Branch `gap/c` — view/UI/FX/audio (merged `f5676f53`):

| Item | Fix | Commit |
|---|---|---|
| PAR-0454 | unit weapon `<weapon>-heat` additive overlay never drawn | 2bf7309b |
| PAR-0367 | non-top weapon outlines missed `Weapon.layerOffset` z | 2bf7309b |
| PAR-0448 | hover-bar unit icon rendered literal `[icon name=…]` text | e0dffcbc |
| PAR-0443 | planet selection/hover overlays ignored `uiAlpha` | 16d4ec78 |
| PAR-0461 | per-weapon `Weapon.shadow` circle shadow missing | d37a8148 |
| PAR-0463 | `skip_wave` keybinding was dead | 195b29fa |
| PAR-0379 | HeaterGenerator `bar.heat` row missing (family rows already landed) | 96b1873a |
| PAR-0368, 0399, 0408, 0420, 0365, 0445 | audited complete against the Java implementations; tests confirmed | 92ba15aa, 9e51cfb4, 847f0d4f, 9ca2138c, 1e0003af, a053dc9a |

## Recorded, not fixed (next session)

- **PAR-0465** (`seeded`) splash damage never applies the bullet status
  (burning/blasted) to blast victims; `status_area` has no call sites and lacks
  Java's air/ground/hittable filters. Also noted: `createSplashDamage` omits
  Java's `if(heals())` allied-building heal branch.
- **PAR-0466** (`seeded`) editor resize never shifts relative build configs
  (Java `BuildPlan.pointConfig` + `ignoreResizeConfig`); needs the editor config
  representation, plan-07 territory.
- **PAR-0467** (`seeded`) block `emitLight` uses a fixed 60 px white light;
  Java's per-block `lightRadius`/`lightColor` are plumbed but unused.

Known residuals carried from phase 1: `PAR-0369` carried-unit payload soft shadow
and rotation re-aim, `PAR-0366` ArmorPlate/UnitSpawn/MoveLightning abilities,
`PAR-0126` unit-draw umbrella (shadow family now covered by PAR-0461), plus the
173 `verified` live UI/render/campaign items, the 5 `mismatch` deltas, the 3
`seeded` UI gaps, the 4 `wontfix` re-checks, `PAR-0057` (needs a loaded sector),
and fresh Java legs for PAR-0439/PAR-0442.

## Branches

`gap/a`, `gap/b`, `gap/c` are merged into `main` (one `--no-ff` merge each) and
kept for reference. Their worktrees were removed after the merge; `runs/` keeps
the per-item evidence (gitignored).
