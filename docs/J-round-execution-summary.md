<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round J — execution summary (2026-10-16 → 2026-10-17)

Coordinator record for the round-J plans in `docs/parity-plans/`. Base commit `fe8fb0e4`; every
wave merged through `main` with the per-plan summaries in `docs/J<NN>-*_summary.md` and the
verification record in `docs/K-round-twin-verification.md` (J10 T3).

## 1. Execution model

Four parity sessions ran the J-INDEX schedule in parallel (`parity/session-2`..`session-5`,
displays `:11`..`:14`, bridges `6980`..`7010`), with the coordinator (session 1 / main) reviewing
diffs, running the merge gates, merging each branch, and applying the ledger changes to
`chains.json` / `parity-ledger.json`.

| Wave | Session 2 | Session 3 | Session 4 | Session 5 |
|---|---|---|---|---|
| A | J01 T1–T2 | J02 T1–T3 | J03 T1–T3 | J10 T1 + T2.6 |
| B | J01 T3–T6 → J04 T1 | J02 T4–T5 → J06 T1–T3 | J03 T4–T7 | J10 T2.1–T2.5 |
| C | J04 T2–T6 | J06 T4/T6–T9 | J05 T1–T5 | J02 T2 (backfill) |
| D | — | J06 T5 → J08 T1–T3 (+records) | J09 T1–T3/T5 (+T4 blocker) | J07 T1–T3/T5.4 (+T4 record) → J10 T3 |

Merges (first-parent order): `a4f1aa6d` J10-A, `a5930c7a` J02-A, `15977d27` J01-A, `a58b8bf0`
J10-B, `59ac6d6a` J03-B, `1523d173` J02/J06-B, `1c6ba37a` J01/J04, `2fbddff1` J05, `4f6fe3a3`
J06-C, `d8455268` J07, `0403cdd3` J09, `3b68bfca` J04-C, `4137ea80` J06-T5/J08, `ab8b0595`
J02-T2. Ledger commits interleave (`fa71e270`, `b8c534ab`, `a5b18973`, `5b810821`, `99ef19cf`,
`1225f9cf`, `b4f42445`, `b6e51926`, `7178b401`, `3a653e4e`, `6f949a27`, `8903fb12`), each
applying a plan summary's "Ledger changes to apply" list.

Baseline repair by the coordinator: `313ec6b1` replaced two `expect()`s on
`Entity::from_raw_u32` in `mind-headless` (clippy 1.98 `expect_used`, no behaviour change); the
main-checkout learnings writes were captured in `ca55ff5c`, `2f6d74be`.

## 2. Per-plan status

| Plan | Status | Landing commits |
|---|---|---|
| **J01** world load, import, saves | **complete** (T1–T6) | `396c9886` T1, `1b844069` T2, `a17dda0f` T3, `44e959f6` T4, `6e07cd0f` T5, `b08e17f1` T6 |
| **J02** terrain rendering | **T1, T2, T3, T4 landed; T5 verification-only** | `b33dce04` T1 (+floor/wall mirror), `a7b1aff0` T3, `5e3b3ed9` T4, `847f0d4f` T2 (Java capture pending) |
| **J03** turret/unit draw | **T1–T7 landed; documented residuals** | `16d2a336` T1, `69e24f94` T2, `11a04001` T3, `9f381d49` T4, `44401634` T5, `f11231d7` T6, `54d13776` T7 |
| **J04** combat/production sim | **T1–T5 landed; T6 partial (2 landed, rest recorded)** | `e5b94549` T1, `fefc574` T2, `d236e4a7` T3, `501f2727` T4, `60e34db5` T5, `8986959a` T6 |
| **J05** block config UI | **T1, T2, T5 complete; T3/T4 partial + recorded** | `0a7f40cc` T1, `3069c447` T2, `2ed429de` T3, `584a0e30` T4, `fbddeab2` T5 |
| **J06** skin/dialogs | **T1–T9 complete** (T9.5/T9.6 recorded) | `0cf55155` T1, `988ec70d` T2, `65dd3052` T3, `8b197956` T4, `23828d01` T5, `9da3b3b2` T6, `37ef2a8d` T7, `a35307a8` T8, `9eb88e02` T9 |
| **J07** planet/campaign | **T1–T3 landed; T4 recorded (premise corrected); T5.4 landed** | `6b69bded` T1, `418a496b` T2, `aec5bde4` T3, `9ab01188` T5.4 |
| **J08** editor/logic | **T1–T3 landed (T3 partial); T4/T6–T8 recorded; T5 blocked** | `061ddaba` T1, `d2176386` T2, `ddeae35d` T3 |
| **J09** mods/content | **T1–T3, T5 complete; T2 partial; T4 blocker recorded** | `f63ef648`/`13548c98` T1, `f45ed9ac` T3, `9f33ceaa` T5, `45e1a55e` golden maintenance |
| **J10** harness/re-walk | **T1, T2 items 1–6 complete; T3 in `K-round-twin-verification.md`** | `574e085a` T1, `78f42bf5` T2.6, `5325720c` T2.1, `3afd8b70` T2.3 |

## 3. Gates

Run on the merged tree after each wave (flock-serialised cargo; `CARGO_BUILD_JOBS=6`):

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean after `313ec6b1`.
- `cargo test -p mind-core` — 2756 lib tests pass at the final wave; only the pre-existing
  `blocks_golden::family_counts_match_golden` failure remains (PAR-0362).
- `cargo test -p mind-gdext` — 77 pass; `cargo test -p mind-headless --lib` — 133 pass;
  `cargo test -p mind-tools` — clean.
- `mind-headless parity goldens` — 97 PASS / 0 FAIL (includes the J09 `units_legs_ik` regen and
  the J02 render goldens).
- Godot import/parse gate (`tools/godot.sh --headless --editor --quit --path client`) — clean;
  `tools/shaders_check.gd` — 29 shaders / 0 failed (includes the new `bloom.gdshader`).

**Known baseline failures (pre-existing, tracked as PAR-0362):** `blocks_golden`
family counts (6 blocks reclassified `other`→`production` without a golden regen),
`audio_golden::audio_events_sim` (`shootDuo` muzzle moved per H01 T1; the tick-4
`explosionArtillery` event delta still needs attribution), `input_golden`, `logistics_golden`,
`maps_editor_golden`. Not regenerated blindly; `units_legs_ik` was the one genuine drift and was
moved to the Java IK invariant in J09 (with manifest hash + reason).

## 4. Ledger applied by the coordinator (all in `chains.json`)

- Verified/closed: PAR-0338, PAR-0345, PAR-0347, PAR-0348, PAR-0349, PAR-0359, PAR-0360,
  PAR-0361, PAR-0363 (partial→`fixing` for residuals), PAR-0364, PAR-0370–PAR-0376,
  PAR-0381–PAR-0385, PAR-0387–PAR-0390, PAR-0392, PAR-0393, PAR-0395–PAR-0398, PAR-0400,
  PAR-0401, PAR-0403–PAR-0406. PAR-0114 was **closed** (scrap ore + 3 map previews),
  PAR-0089 closed as already-present, PAR-0092 re-evidenced with the Java panels.
- Reopened: PAR-0126 (legs/treads/wrecks landed; segments/payloads open).
- Seeded residuals: PAR-0362, PAR-0365–PAR-0369, PAR-0377–PAR-0380, PAR-0386, PAR-0391,
  PAR-0399, PAR-0402, PAR-0407, PAR-0408 (`fixing` for the Java capture only).
- Blocked: PAR-0394 (map data-asset host/decorator — gates J08 T5).
- Wontfix: PAR-0346 (round-I claim retracted).
- `parity/ledgers/mod_fields.md`: `consumes`, `type`, `aiController` rows closed; the rest have
  anchors for the next wave.
- `parity/golden_manifest.json`: `units_legs_ik` and `mod_classmap` hashes updated with reasons.

## 5. Residuals and next queue

1. **PAR-0362** — regenerate the five drifted golden suites with the owning task's Java reason
   (do not blanket-regen); attribute the `audio_events_sim` delta first.
2. **PAR-0408** — the fresh Java capture for J02 T2's metal-tiles field (exact recipe in the
   J02 summary; the Godot side is verified).
3. **J03 residuals** — PAR-0365 (unit parts), PAR-0366 (ability draws), PAR-0367 (weapon
   layerOffset), PAR-0368 (crawl segments), PAR-0369 (payload draws); PAR-0126 stays open.
4. **J04/J07/etc. seeds** — PAR-0399 (explosion view half), PAR-0402 (J04 T6 records),
   PAR-0391 (campaign generator/abandon residuals), PAR-0379/PAR-0380 (hover bars / bespoke
   configs), PAR-0386 (wave panel wrap / editor highlight), PAR-0407 (J08 logic/editor records).
5. **J09 T4** — the `ModDataManager` owner+decorator is the one recorded blocker; landing it
   unblocks J08 T5 (map assets dialog) and map `patches`.
6. **J10 T3** — the round-K re-walk verdicts and any new ledger findings are in
   `docs/K-round-twin-verification.md`.

## 6. Deviations recorded (summary)

All per-plan deviations with Java locations are in the `docs/J<NN>-*_summary.md` files. The
cross-cutting ones: name-stem save slots kept (J01 T6); filter application before `endMapLoad`
instead of a literal `FilterContext` (J01 T1); preview cache `_v3` bump (J02 T3); flyer icon
interim + single-pass bloom (J02 T4, J10 T2.1); `ItemSelection` icon placement via
`CenterContainer` and next-tick config application while paused (J05 T1); the explosion view half
waiting on an `ExplosionSink` host (J04 T4); Accelerator self-planet semantics (J07 T4).
