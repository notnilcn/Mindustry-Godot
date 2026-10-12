<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G00 — Integrate sessions 2-5 into `main` and reset the baseline (2026-10-13)

Self-contained. Read `00-environment.md` (same directory) first; this plan is
the ONE exception to its "never merge into main" rule: G00 is run by a single
executor, in the MAIN checkout, before any other G-plan starts. No other
session may run while G00 runs (check `pgrep -af "godot --path"` shows
nothing and no other agent is working).

Why: the four session branches from the 2026-10-12 round are NOT merged into
`main` (`git merge-base --is-ancestor parity/session-N main` is false for
N = 2..5). They carry 58 commits. Every G01-G08 plan was audited against the
merged result of `main` + these branches, so they must land first.

The coordinator already did a trial merge of exactly this sequence in a
throwaway worktree on 2026-10-13 and recorded every result below. If what you
see differs from what is written here, STOP and report; do not improvise.

---

## T1. Merge the four branches (main checkout)

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
git status --short          # expect ONLY untracked/deleted entries under docs/parity-plans/ and .opencode/ ; no modified tracked code files
git log --oneline -1        # expect 06340c4 chains.json: apply the 2026-10-12 audit verdicts
for N in 2 3 4 5; do git rev-list --count main..parity/session-$N; done   # expect 18, 20, 14, 6
```
If the counts differ, a session added commits after 2026-10-13: still merge,
but list the extra commits in your summary.

Merge in this order (each one a merge commit):
```bash
git merge --no-ff --no-edit parity/session-2      # expect: clean
git merge --no-ff --no-edit parity/session-3      # expect: clean
git merge --no-ff --no-edit parity/session-4      # expect: clean
git merge --no-ff parity/session-5                # expect: CONFLICT in 2 files
```

### Resolving the session-5 conflicts (the only two expected)

1. `client/rust/mind-gdext/src/sim_host.rs`: one conflict hunk around line
   2070. `HEAD` adds `pub fn debug_add_building_items(...)` (session 4); the
   `parity/session-5` side adds `pub fn place_launch_loadout(...)` and
   `fn player_core_tile(...)`. Keep BOTH. The `HEAD` side ends without the
   closing `}` of `debug_add_building_items` (the shared trailing `}` after
   the hunk closes whichever function comes last). Resolve as:
   ```
   <HEAD side, unchanged>
       }

   <session-5 side, unchanged>
   ```
   i.e. delete the three marker lines, and insert a line `    }` plus an empty
   line between the HEAD side and the session-5 side. Check:
   `grep -n "fn debug_add_building_items\|fn place_launch_loadout\|fn player_core_tile" client/rust/mind-gdext/src/sim_host.rs`
   prints three lines, and `grep -c "<<<<<<<\|>>>>>>>" client/rust/mind-gdext/src/sim_host.rs` prints `0`.
2. `parity/golden_manifest.json`: one hunk, the `content_ids` sha256. Take
   either side for now (T2 sets the real value):
   `git checkout --theirs parity/golden_manifest.json` is NOT safe (other
   hunks merged cleanly); edit the hunk by hand and keep one `"sha256": ...`
   line.
3. `git add client/rust/mind-gdext/src/sim_host.rs parity/golden_manifest.json && git commit --no-edit`
4. Do NOT create the marker commit yet; T6 creates it as the last G00 commit.

Compile check: `cd client/rust && flock /tmp/opencode/mg-build.lock cargo check -p mind-gdext` → no errors
(the coordinator's trial merge compiled cleanly).

## T2. Fix the golden manifest hashes

Facts found by the coordinator (trial merge):
* `parity/golden_content.json` after the text merge is byte-identical to a
  fresh regeneration from the merged code: sha256
  `e54e4be3749fd621f276819e5429cc91a402b9c1d76bd5ae79712df716823394`.
* `parity/bundle_keys.json` was changed by session 2 (`5c63c1d`, four
  `unit.scathe-missile*.name` keys added for the new scathe missile units) but
  its manifest hash was not updated: the file's sha256 is now
  `ec84fe4abb2b80dd8596e8c91e68b7a80161c2e77dc5968bf76636c9b13ba6bd`
  (manifest still says `59cf4d5d...`).

Steps:
```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
flock /tmp/opencode/mg-build.lock cargo run -q --manifest-path client/rust/Cargo.toml -p mind-headless -- content dump --out /tmp/g00-content.json --bundle parity/bundle_keys.json --log-file -
sha256sum /tmp/g00-content.json parity/golden_content.json parity/bundle_keys.json
#   expect the first two lines to show the SAME hash e54e4be3...; the third ec84fe4a...
git diff 06340c4 -- parity/bundle_keys.json      # expect exactly 4 added lines: unit.scathe-missile-phase.name, unit.scathe-missile-surge-split.name, unit.scathe-missile-surge.name, unit.scathe-missile.name
```
Then edit `parity/golden_manifest.json`: entry `"id": "content_ids"` sha256 →
`e54e4be3749fd621f276819e5429cc91a402b9c1d76bd5ae79712df716823394`; entry
`"id": "bundle_keys"` sha256 →
`ec84fe4abb2b80dd8596e8c91e68b7a80161c2e77dc5968bf76636c9b13ba6bd`.
If the regenerated content hash differs from the merged file, copy
`/tmp/g00-content.json` over `parity/golden_content.json` only after
`diff <(jq -S . parity/golden_content.json) <(jq -S . /tmp/g00-content.json)`
shows nothing but changes caused by the merged sessions (scathe units,
research requirements); otherwise stop and report.

Commit: `G00 T2: golden manifest hashes for the merged content and bundle keys`
(body: the two facts above).

## T3. Regenerate the campaign sector golden (rules checksum)

`mind-headless` test `campaign_scenarios::tests::sector_golden_matches` has
failed since 2026-10-11 and earlier plans listed it as "pre-existing". The
coordinator diffed it: the ONLY difference is
`"rules_checksum": "613fa92157ae0537"` (golden) vs `"4781866c0cf4b1d8"`
(code). Cause: commit `7b785a3` (PAR-0139 round 2) made `TeamRule` skip
default-valued fields when serialising (mirrors Arc `usePrototypes=false`;
needed because the 256-team rules string exceeded the u16 save-meta cap), so
the serialized rules text, and therefore its checksum, changed. That is an
intended change, so the golden is stale.

```bash
flock /tmp/opencode/mg-build.lock cargo run -q --manifest-path client/rust/Cargo.toml -p mind-headless -- campaign sector --planet serpulo --sector groundZero --ticks 600 --log-file - --dump /tmp/g00-sector.json
diff client/rust/mind-headless/tests/golden/campaign/sector_cycle.json /tmp/g00-sector.json
#   expect exactly:  18c18  < "rules_checksum": "613fa92157ae0537"  > "rules_checksum": "4781866c0cf4b1d8"
cp /tmp/g00-sector.json client/rust/mind-headless/tests/golden/campaign/sector_cycle.json
sha256sum client/rust/mind-headless/tests/golden/campaign/sector_cycle.json
```
Put that sha256 into `parity/golden_manifest.json` entry
`"id": "harness_campaign_sector_cycle"`.
If the diff shows any other line, STOP and report.

Commit: `G00 T3: regenerate the campaign sector golden for the compact TeamRule serialisation`.

Do NOT touch the other long-standing failure,
`units_scenarios::tests::committed_goldens_match` (scenario
`units_base_build`: golden `queued_plans: 28, pass: true`, code
`queued_plans: 0, pass: false`). That is a real enemy base-builder AI
regression; plan G01 owns it.

## T4. Regenerate the server content seed

Session 2 added four scathe missile unit types, so the STDB content catalog
(`client/rust/mind-core/src/mods/catalog.rs` pins 720 entries) and
`server/spacetimedb/src/main/content_seed.rs` must agree.
```bash
flock /tmp/opencode/mg-build.lock bash server/gen_content_seed.sh
git status --short server/
```
If the seed file changed, commit it:
`G00 T4: regenerate the STDB content seed for the scathe missile units`.
If it did not change, note "seed already current" in your summary.

## T5. Test gates and baseline (record exact numbers)

From `client/rust` (each behind `flock /tmp/opencode/mg-build.lock`, `CARGO_BUILD_JOBS=6`):
```bash
cargo test -p mind-core --lib --features msav-import    # trial merge: 1998 passed; 0 failed; 4 ignored
cargo test -p mind-headless --lib                       # expect exactly ONE failure: units_scenarios::tests::committed_goldens_match
cargo test -p mind-gdext --lib                          # session 3 saw 26 passed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   # no output
cargo fmt -p mind-core -p mind-gdext -- --check         # no output
cargo run -q -p mind-headless -- parity goldens         # all PASS
```
Then `CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh`
(expect `Finished` + `sync_scenarios`), and the GDScript parse gate
(`00-environment.md` §2).

In-engine smoke (session 1 = main checkout, bridge 6970, display :10):
```bash
tools/session-play.sh 1 --import       # expect {"nodes": 375.0, "unlocked": 282.0}
export OPEN_GODOT_MCP_PORT=6970
```
then the Ground Zero recipe (`00-environment.md` §4.1) and:
* `$MC.get_hud_state()` → `hasCore: true`, `sectorName: "groundZero"`.
* `$H.place_block(130,48,"mechanical-drill")`, `$H.place_block(132,48,"duo")`,
  `$MC.run_wave()`, then poll `$H.unit_debug_json()` every ~10 s for 90 s
  (`$H.set_paused(false)`): the wave daggers die, the core survives.
* Screenshot `runs/20261013-g00/gz-after-merge.png`; it must show belts,
  core, the player alpha with an outline, the minimap.
* `grep -c panic` on the newest log = `0`.
* Stop the session afterwards: `bash tools/parity-session.sh --stop`
  (session 1) so the four G-plan sessions have the RAM.

If anything fails, STOP and report; do not "fix forward" in G00.

## T6. Marker and worktree hygiene

1. Only after T1-T5 all passed, create the empty marker commit:
   ```bash
   git commit --allow-empty -m "G00: integrate sessions 2-5 (merge marker)" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
   ```
   Every G-plan checks `git log main --oneline | grep -m1 'G00: integrate'`.
2. The session worktrees are on their old branches. Each later plan
   fast-forwards them with `git merge --ff-only main`; that works because
   each branch is now an ancestor of `main`. Verify:
   `for N in 2 3 4 5; do git merge-base --is-ancestor parity/session-$N main && echo ok-$N; done`
   → four `ok` lines.
3. `git worktree prune` (removes the stale `/tmp/mg-head` and, once the
   coordinator has deleted it, `/tmp/mg-audit` entries). Never delete a
   session worktree directory.
4. Do not push.

## T7. Ledger changes for the coordinator (do not edit chains.json yourself)

Write this table into your summary (copy it, adding the merge SHAs). The
coordinator applies it to `chains.json`. Status values are the ledger's:
`seeded` / `reference` / `fixing` / `verified` / `blocked`.

| Item | New status | Note to append (2026-10-13, G00) |
|---|---|---|
| PAR-0025 | verified | Configure Loadout Max toggle, valid gate, rows, capacity landed in `44b0f7b` (session 5 T1); 200 px SchematicImage toggles tracked by G05. |
| PAR-0026 | verified | same as PAR-0025. |
| PAR-0027 | verified | same as PAR-0025. |
| PAR-0085 | verified | `44b0f7b` + `fe9381b` (loadout validity + schematic core); in-engine evidence `runs/20261012-session5-campaign-import/t1-*.png`, `t2-launch-foundation.png`. |
| PAR-0131 | fixing | landing sprite/fade/announce/timing/locks landed `659146d`; remaining: thruster flames + hiding the real core under the landing sprite (G03). |
| PAR-0160 | verified | research requirements materialised `2fe5800`; `[223,174]` rows with costs; dialog lists all nodes. |
| PAR-0162 | verified | `Schematics.load` of `schematics/*.msch` + post-sort reindex landed in `44b0f7b`. |
| PAR-0163 | verified | launch capacity in the plan JSON (2000 shard / 4500 foundation) `44b0f7b`. |
| PAR-0088 | verified | scathe missiles `5c63c1d` (headless `scathe_launches_missile_and_kills`), under-bullet rule `487f79f`, adopt perf `b0d194c`; anthicus verified in-engine earlier (`1be95d2`). |
| PAR-0091 | fixing | `afaeebb` building `dynamicExplosion`; unit-death explosion, ExplosionShield, `rules.fire` gating remain (G01). |
| PAR-0127 | fixing | PlayEvent cooldown re-arm `9d64483`; still never seen in-engine (G07). |
| PAR-0137 | fixing | boosting mechs lift off `0e984ce` (headless); in-engine check in G08/G01. |
| PAR-0138 | fixing | payload key path, building pickup, revision-2 unit save `4b7067b`; in-engine carrier check pending (G01/G08). |
| PAR-0140 | (unchanged) | every command id accepted/stored `95fced5`; dispatch for Suicide/Builder/Boost `e927900`/`9d041bb`/`3e50373`; other AI kinds in G01. |
| PAR-0076 | verified | power gate/unitBuildSpeed `c94c27c`, command + move-in `955c9f0`, command panel `5e9545d` (in-engine). |
| PAR-0089 | fixing | sublimate latch + drain `75e8595` (headless only; needs an Erekir sector to see in-engine). |
| PAR-0098 | fixing | schematics through plans `d1ef222`; remaining: plan/beam drawing (G03), rebuild select (G08). |
| PAR-0094 | verified | key-combo line `52b4d42`, palette accent `8bf1b7f`, cursor hover `aade84d` (all in-engine, session 3). |
| PAR-0126 | verified | flying shadow + soft shadow `100ac50`, outlines `5dcd208` (in-engine vs Java crop). |
| PAR-0070 | verified | FX producer + regions + polyline `11ce662` (in-engine `runs/20261012-s3-t12/`); shake wiring structural only. |
| PAR-0128 | verified | same commit as PAR-0070 (regions drawn). |
| PAR-0097 | verified | F6 block status markers `d5adb63` (in-engine). |
| PAR-0096 | fixing | minimap indicators + unit icons `b03aed9`; the sim never emits `BuildDamageEvent` (G01 emits, G03 drains). |
| PAR-0157 | fixing | selection overlay `6be52fd` (in-engine); attack reticle needs an entity-id resolver (G01/G03). |
| PAR-0129 | fixing | domes/bubbles `4b160e3` (code); force projector stays at efficiency 0 in-engine (G02) and the animated shader needs the effect buffer (G03). |
| PAR-0130 | (unchanged) | belt autotile/animation `e031438`, item order `5aeb51f`. |
| PAR-0152 | (unchanged) | blockbuild shader `5075b87`, per-part drill/turret `0bd9e5d`. |

New items to file (title → status):
* "Research dialog listed only the root row: ResearchView rows carried no tree name" → verified (`2fe5800`).
* "Enemy base-builder AI queues no plans (units_base_build golden: 0 of 28)" → seeded (owner G01).

## Summary

Write `REPO/docs/parity-plans/G00-integrate-sessions_summary.md` (do not
commit it): merge commit SHAs, conflict resolution notes, every test number
from T5 (this becomes the baseline every G-plan compares against: put the
exact `mind-headless` failure list there), in-engine smoke evidence, the T7
table.

## Done criteria

* `main` contains all four session branches and the `G00: integrate` marker.
* `mind-headless` fails exactly one test (`units_scenarios::tests::committed_goldens_match`).
* `parity goldens` all PASS; clippy/fmt clean; mind-core suite green.
* Ground Zero boots, the wave dies to a duo, no panic.
