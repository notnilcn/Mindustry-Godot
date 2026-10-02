# JVM golden diff (NUD-10)

- Rust gate: `source-derived fallback (NUD-10): mind-headless content dump over parity/tools generators` (v146)
- JVM dump: `parity/java/DumpContent.java`
- Produced by `parity/tools/jvm_golden_diff.py`.

## Per-type counts

| type | rust | jvm | note |
|---|---|---|---|
| block | 447 | 447 |  |
| bullet | 112 | 202 | plan 10 registers turret ammo (`Blocks.java` `ammoTypes`) bullets |
| item | 22 | 22 |  |
| liquid | 11 | 11 |  |
| planet | 7 | 7 |  |
| sector | 46 | 95 | plan 19 `SectorSubmissions.registerSectors()` auto-presets (46 authored + 49 submitted in JVM) |
| status | 23 | 23 |  |
| team | 0 | 0 |  |
| unit | 65 | 70 | plan 10 registers turret-created missile units (scathe missiles, build tower) |
| unitCommand | 10 | 10 |  |
| unitStance | 30 | 30 |  |
| weather | 6 | 6 |  |

## Block field diffs

- Names: rust-only 0, jvm-only 0
- Blocks with field diffs: 56 (remaining generator/default reconciliation; the plan-02 CI gate uses the source-derived golden until these are closed)

| block | field: rust -> jvm |
|---|---|
| air | `build_time`: 20.0 -> -1, `health`: 40 -> -1, `liquid_capacity`: 10.0 -> -1, `scaled_health`: 40.0 -> -1 |
| heat-reactor | `scaled_health`: 112.0 -> 112.00001 |
| force-projector | `liquid_capacity`: 10.0 -> 60 |
| shockwave-tower | `liquid_capacity`: 10.0 -> 15 |
| conveyor | `item_capacity`: 10 -> 3 |
| titanium-conveyor | `item_capacity`: 10 -> 3 |
| armored-conveyor | `build_time`: 3.9 -> 3.8999999, `item_capacity`: 10 -> 3 |
| unit-cargo-unload-point | `solid`: False -> True, `update`: False -> True |
| combustion-generator | `has_items`: False -> True |
| steam-generator | `has_items`: False -> True |
| rtg-generator | `has_items`: False -> True |
| neoplasia-reactor | `has_items`: False -> True |
| laser-drill | `liquid_capacity`: 47.0 -> 48 |
| large-plasma-bore | `scaled_health`: 115.99999 -> 116 |
| duo | `has_liquids`: False -> True |
| scatter | `has_liquids`: False -> True |
| scorch | `has_liquids`: False -> True |
| hail | `has_liquids`: False -> True |
| lancer | `has_liquids`: False -> True |
| arc | `has_liquids`: False -> True |
| swarmer | `has_liquids`: False -> True |
| salvo | `has_liquids`: False -> True |
| fuse | `has_liquids`: False -> True |
| ripple | `has_liquids`: False -> True |
| cyclone | `has_liquids`: False -> True |
| foreshadow | `has_liquids`: False -> True |
| spectre | `has_liquids`: False -> True |
| meltdown | `has_liquids`: False -> True |
| breach | `has_liquids`: False -> True |
| diffuse | `has_liquids`: False -> True |
| disperse | `has_liquids`: False -> True |
| scathe | `has_liquids`: False -> True |
| smite | `has_liquids`: False -> True |
| ground-factory | `item_capacity`: 10 -> 60 |
| air-factory | `item_capacity`: 10 -> 60 |
| naval-factory | `item_capacity`: 10 -> 70 |
| additive-reconstructor | `has_items`: False -> True, `item_capacity`: 10 -> 80 |
| multiplicative-reconstructor | `has_items`: False -> True, `item_capacity`: 10 -> 260 |
| exponential-reconstructor | `has_items`: False -> True, `item_capacity`: 10 -> 1700 |
| tetrative-reconstructor | `has_items`: False -> True, `item_capacity`: 10 -> 2000 |
| repair-turret | `has_liquids`: False -> True, `liquid_capacity`: 10.0 -> 96 |
| tank-fabricator | `item_capacity`: 10 -> 100 |
| ship-fabricator | `item_capacity`: 10 -> 140 |
| mech-fabricator | `item_capacity`: 10 -> 140 |
| tank-refabricator | `has_items`: False -> True, `item_capacity`: 10 -> 80 |
| ship-refabricator | `has_items`: False -> True, `item_capacity`: 10 -> 120 |
| mech-refabricator | `has_items`: False -> True, `item_capacity`: 10 -> 100 |
| prime-refabricator | `has_items`: False -> True, `item_capacity`: 10 -> 200 |
| tank-assembler | `has_items`: False -> True, `solid`: False -> True, `update`: False -> True |
| ship-assembler | `has_items`: False -> True, `solid`: False -> True, `update`: False -> True |
| mech-assembler | `has_items`: False -> True, `solid`: False -> True, `update`: False -> True |
| basic-assembler-module | `scaled_health`: 122.0 -> 122.00001 |
| item-void | `solid`: False -> True, `update`: False -> True |
| target-dummy | `configurable`: False -> True, `update`: False -> True |
| interplanetary-accelerator | `item_capacity`: 8000 -> 25000 |
| hyper-processor | `liquid_capacity`: 47.0 -> 48 |

## Known harness/loop artifacts

- Localized names: JVM harness `Core.bundle` is uninitialized, so the JVM golden's `localized` is the internal name. The Rust gate boots `parity/bundle_keys.json` and records real localized names; plan 03 covers bundle loading.
- `air` reports raw `-1` fields because the upstream build's `@OverrideCallSuper` codegen did not rewrite `AirBlock.init()` (bytecode check: no `super.init()` call); the Rust port derives `health/scaledHealth/buildTime/liquidCapacity` as upstream does with working codegen.

