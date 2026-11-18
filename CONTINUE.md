<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Continue the twin-verification sweep

State at **2026-11-03, 02:00 local**: `main` is committed and clean at `f5676f53`
plus the phase-2 queue commit. **266 / 467 items terminal** (261
`reference-verified`, 5 `twin-verified`); the session started at 23/450.
The counts on this page are historical — read the live queue with
`python3 tools/ai-tools/twin/twin_verify.py stats --root .`.

Read first:

- [`docs/TWIN-PHASE1-REPORT.md`](docs/TWIN-PHASE1-REPORT.md) — queue close-out:
  status counts, batch-verification groups + evidence, deferrals.
- [`docs/TWIN-PHASE2-REPORT.md`](docs/TWIN-PHASE2-REPORT.md) — the parallel
  gap hunt: every fixed item, its commit and its Java-derived test; the
  recorded-but-unfixed gaps; branches `gap/a`/`gap/b`/`gap/c` (merged).
- [`docs/TWIN-RUNS.md`](docs/TWIN-RUNS.md) — evidence layout and the two
  terminal statuses; [`docs/TWIN-CONTINUATION.md`](docs/TWIN-CONTINUATION.md) —
  the older sequential checker/fixer loop and environment notes.

## What is left (201 non-terminal items)

1. **Recorded gaps, ready to fix** — `PAR-0465` (splash damage never applies the
   bullet status to blast victims; `status_area` unfiltered, and Java's
   `if(heals())` heal branch is missing), `PAR-0466` (editor resize never shifts
   relative build configs; needs plan-07 config representation), `PAR-0467`
   (block `emitLight` ignores per-block `lightRadius`/`lightColor`).
2. **Deep draw residuals** (`fixing`): `PAR-0126` (unit-draw umbrella; its shadow
   family is now covered by PAR-0461), `PAR-0296` (accelerator stub), `PAR-0363`
   (turret parts), `PAR-0366` (ArmorPlate/UnitSpawn/MoveLightning abilities),
   `PAR-0369` (carried-unit payload shadow/rotation re-aim), `PAR-0380` (bespoke
   configs), `PAR-0394` (map data assets), `PAR-0407` (J08 residual bundle).
3. **Fresh Java legs** for `PAR-0439` and `PAR-0442` (code fixed in phase 1,
   Godot-verified, only the twin pair is missing) and for the phase-2 fixes if
   you want them upgraded from `reference-verified` to `twin-verified`.
4. **Layout deltas** (`mismatch`): `PAR-0426`, `PAR-0433`, `PAR-0435`,
   `PAR-0436` — small measured UI differences.
5. **`verified` (173)** — live UI/render/campaign items not re-run since the
   native-UI cutover; batch them per screen with one representative leg.
6. **Deferred by design** — multiplayer (`PAR-0121/0122/0124/0125`), campaign
   loadout (`PAR-0024..0027`, progression-gated), `PAR-0057` (needs a loaded
   sector), the 4 `wontfix` re-checks.

## Working commands

```bash
bash tools/parity-session.sh                 # Xvfb :10 + editor + bridge 6970
tools/godot.sh -e                            # or open the editor directly
python3 tools/ai-tools/twin/twin_verify.py next --root .   # first non-terminal item
python3 tools/ai-tools/twin/twin_verify.py stats --root .  # status counts
python3 tools/ai-tools/twin/twin_verify.py list --status mismatch --root .
bash tools/build.sh && bash tools/parity.sh         # build + oracle gate (self-queues)
cargo test --manifest-path client/rust/Cargo.toml -p mind-core
python3 tools/ai-tools/twin/twin_verify.py check --root .  # queue validity
```

`TWIN-VERIFICATION.json` is edited only through
`tools/ai-tools/twin/twin_verify.py set-status` (exclusive lock + atomic
replace); `check` validates it. Evidence is created with the packaged
`twin_item.sh` under `runs/PAR-####/` (gitignored, never `runs/archive/**`);
export `TWIN_REFERENCE_LEG=java TWIN_PORT_LEG=godot` before capturing.

Suggested opening prompt:

> Continue the Mindustry→Godot parity work in
> `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (branch main, clean).
> Read `AGENTS.md`, `docs/TWIN-PHASE2-REPORT.md` and `docs/TWIN-RUNS.md`, then
> pick the highest-value non-terminal items: start with PAR-0465/0466/0467,
> then the `fixing` draw residuals and the `mismatch` layout deltas. Use
> `python3 tools/ai-tools/twin/twin_verify.py` for every queue edit, run
> `bash tools/build.sh` and the affected tests before committing, and mark an
> item `reference-verified` only with Java-source/oracle evidence,
> `twin-verified` only with a fresh Java+Godot frame pair.
