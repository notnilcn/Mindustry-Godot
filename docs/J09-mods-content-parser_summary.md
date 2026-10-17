<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J09 — Mod content parser coverage and mod content everywhere (wave D) — summary

Session: parity session 4 (`parity/session-4`, worktree `Mindustry-Godot-session4`),
display `:13`, bridge `7000`. Date: 2026-10-17. Wave D covers T1–T5.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — `ClassMap` aliases mods use | **covered** (consumers slice landed; the remaining scope aliases registered with explicit warnings where no runtime half exists) | `f63ef648`, `13548c98` |
| T2 — `mod_fields.md` TODO rows | **partial** (`consumes`, `type` aliases, `aiController` closed; the rest stay TODO with anchors below) | `f63ef648`, `13548c98` |
| T3 — mod content on every surface | **landed** (quick-select catalog + map previews; remaining `create_base_content` sites are tests) | `f45ed9ac` |
| T4 — data-asset client half | **recorded blocker with the precise seams** (plan-sanctioned alternative) | notes in `runs/20261017-s4-J09-T4/` |
| T5 — script mods | **verified** (deviation kept; reason in the `getStateText` position) | `9f33ceaa` |

## T1 — `ClassMap` coverage

Landed (see `runs/20261017-s4-J09-T1/notes.md` + the two commit messages):

- **Consumers** (23 aliases, `ClassScope::Consume` + the `consumes` JSON parse with Java defaults:
  `ConsumeItems`/`ConsumeLiquid`/`ConsumeLiquids`/`ConsumePower`/`ConsumeCoolant`,
  `Consume.update = true`, `ConsumeCoolant.java:4-6` defaults). Filter/dynamic/payload/
  power-condition classes are recognized and warned about; `ConsumeItemExplode` names its
  **J04 T2 dependency** (its runtime half is not in the merged main).
- **Bullets**: six aliases that were unreachable despite resolver arms (`ContinuousBulletType`,
  `ContinuousFlameBulletType`, `InterceptorBulletType`, `MultiBulletType`, `PointBulletType`,
  `PointLaserBulletType`; `EmptyBulletType` was already handled).
- **Effects** (7 aliases), **AIs** (7 -> `AiControllerKind::Default`, so `aiController` no longer
  errors), **Abilities** (`ArmorPlateAbility`/`EmptyDataAbility` warn + skip; `parse_ability`
  returns `Option`), **Blocks** (20 base classes -> behaviour kinds: `PowerGenerator` ->
  `ConsumeGenerator`, `Turret` -> `ItemTurret`, `BlockProducer` -> `Constructor`,
  `HeatBlock` -> `HeatProducer`, `TiledFloor` -> `Floor`, …; `ControlBlock`/`UnitTetherBlock`
  fail as "not a concrete block class"), **Data** (10 aliases + stack `type` validation),
  **Drawers** (29 aliases) and **Parts** (11 aliases) registered.
- `parity/mod_classmap.json` regenerated: **342 entries** (`mind-tools mods classmap --check` clean).

Fixtures + tests: `parity/mod_fixtures/consumers/` and `parity/mod_fixtures/classes/`
(`mind-headless mods content --fixture … --json` evidence);
`block_consumes_classes_parse_with_java_defaults`, `consumers_fixture_parses_consume_classes`,
`classmap_scopes_resolve_the_t1_aliases`, `block_base_classes_and_unsupported_abilities`,
`classes_fixture_maps_base_classes`.

Residual (recorded, next wave): parse arms (not just aliases) for the Drawer/Part scopes, the
inline-effect classes (`ParticleEffect`…), `ConsumeItemExplode`'s runtime once J04 T2 lands, and
per-scope fixtures for bullets/effects/drawers/parts (the current fixtures cover consumers and the
cross-scope classes).

## T2 — `mod_fields.md`

Closed: `consumes` (JSON parse), `type` (base classes), `aiController` (the standard AIs).
Still TODO (unchanged, anchors in the ledger): `UnlockableContent` -> `UnlockFields`,
`drillMultipliers` parse, `assembler plans`, weapon effect/draw parts, inline
`MultiEffect`/array effects, `draw` (drawer) parsing.

## T3 — mod content everywhere

1. `MindInput.load_catalog` reads `MindUi.block_catalog_json()` (live registry + mods + campaign
   store) and falls back to base content.
2. `MindPreview` uses `content_boot::build_registry` (base + enabled mods).
3. Audit: every other `create_base_content` call in `mind-gdext` is inside `#[cfg(test)]`
   (`campaign.rs`, `editor_renderer.rs`, `sim_host.rs`) and stays base-only by design.

## T4 — recorded blocker (`runs/20261017-s4-J09-T4/notes.md`)

Seams enumerated: owner node (nothing holds `ModDataManager`), the image decorator (runtime atlas
packing/region registration), the audio decorator (`AudioStream` registration), the bundle merge
seam, and the map `patches` load/save (`maps/mod.rs:463` writes `None`; `WriteContext.patches`
exists but nothing fills it). **J08 T5 stays blocked on this item.**

## T5 — script mods

Deviation kept; `ModListEntry` carries the `reason`, `mind-headless mods list` runs
`Mods.load_scripts`, and the in-engine Mods dialog shows "script mods are not supported in this
build" in Java's `getStateText` position. Fixture `parity/mod_fixtures/scripts/`;
`expected_list.json` regenerated (`--check` exits 0).

## Cross-wave findings (reported to the coordinator)

- **J03 T4 follow-up (fixed here):** `cargo test -p mind-headless` was not in wave B's gate list, so
  `units_scenarios::tests::committed_goldens_match` went stale when T4 moved the legs to Java's
  `solve(legLength/2, legLength/2, foot - hip)` geometry. This wave aligned the scenario's check to
  the Java invariant (both segments equal, per-leg `legLengthScl`; foot within
  `legLength * legMaxLength` of the hip) and regenerated `tests/golden/units/units_legs_ik.json`.
- **Pre-existing at the wave base (`7178b401`):** `mind-headless --test audio_golden`
  (`audio_events_sim` golden mismatch) fails identically before any wave-D change (verified by
  checking out the base and re-running).
- The `parity/golden_manifest.json` `mod_classmap` sha256 is updated for the regenerated manifest
  (342 entries; the `committed_manifest_hashes_match` / `full_check_set_passes` tests pass again).

## Gates

`cargo fmt --all -- --check`, `cargo clippy -p mind-core -p mind-gdext -p mind-headless --all-targets
-- -D warnings`, `cargo test -p mind-core` (2745+ passing), `cargo test -p mind-headless --lib`
(133 passing), `mind-tools mods classmap --check` (342 entries), `tools/build.sh` — clean at the
last run; the only remaining red is the pre-existing audio golden above.
