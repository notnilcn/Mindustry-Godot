# AGENTS.md — mind-core/src/fx (effect registry, pools and draw programs)

`mind-core/src/fx` is the Godot-free half of the FX system: the vanilla `Fx.java` catalogue, the
`EffectDef`/`EffectKind` data surface, the view-only effect/decal/trail/shake state machines, the
effect → `DrawPrim` program compiler, draw-part/turret/weapon geometry, batching policy and the
weather/env visual builders. It is view-only: nothing here is saved, synced or checksummed.
Simulation code emits through the single [`combat::view::FxSink`](../combat/AGENTS.md) seam
(re-exported here as `sink::FxSink`); the host installs `FxBus` (recording) or `NoopFx` (headless).
Geometry leaves as `render::draw::DrawPrim`/`DrawProgram` and Godot executes it in
[`MindFx`](../../../mind-gdext/AGENTS.md). Read the crate map [`mind-core/AGENTS.md`](../../AGENTS.md)
and the render-data guide [`render/AGENTS.md`](../render/AGENTS.md) first.

## Layout

| File / subfolder | Responsibility |
|---|---|
| `mod.rs` | Module list and public re-exports; `From<[f32; 4]> for Rgba`. |
| `catalog.rs` | `build_registry` in `Effect.all` order (267), `kind_for`/`layer_for`, `CatalogCounts`, `order_hash`. |
| `def.rs` | `EffectDef`/`EffectKind`, declarative `Particle`/`Explosion`/`Wave`/`Triangle`/`Noise`/`Sound`/`Radial`/`Wrap` params, `EffectRegistry`, process-wide `registry()`. |
| `sink.rs` | `FxSink` re-export, `FxBus`/`FxEvent`/`FxBusState`, `FxSettings`, `NoopFxSink`, `emit_named`. |
| `pool.rs` | `FxPool`/`EffectState`/`PendingSpawn` lifecycle, parent follow, `FxGate`, `in_camera`. |
| `pool_spec.rs` | Locked capacities (`EFFECT_POOL_CAPACITY` 4096, `DECAL_CAPACITY` 1024, `DELAYED_SPAWN_CAPACITY` 256) and decal lifetime/fade. |
| `container.rs` | `EffectContainer` plus `Scaled` math (`fin`/`fout`/`finpow`/`fout_margin`/`scaled`/`scaled_view`). |
| `resolve.rs` | `build_program` (clear+sort) / `build_program_into` / `build_program_into_lod`: state → `DrawProgram`; declarative kinds, custom dispatch and `Seq` windows. |
| `batch.rs` | Batching plan over sorted prims: `choose_backend`, `batching_runs`, `draw_call_count`, `batched_draw_call_count`, `lod_particle_count`. |
| `data.rs` | `EffectData` payload union, `ViewSnapshot`/`EmptySnapshot`, `ViewEntityId`, `Pose`, `TrailChannelId`, `LegDestroyData`. |
| `angles.rs` | Arc `Angles`/`Mathf` trig (`trnsx`/`trnsy`/`angle`/`sign`) and the `randLenVectors` family. |
| `trail.rs` | `TrailRegistry` channels over `render::trail::Trail`: create/tint/update/shorten/detach. |
| `decal.rs` | `Decal`/`DecalPool` (capped, oldest dropped), `scorch`/`rubble` region names and fade. |
| `shake.rs` | `shake_falloff`/`shake_at` and the `ShakeState` add/decay/camera-offset math. |
| `env_fx.rs` | `EnvRenderers` bodies: `underwater_prims` (tint, caustics, rays, particles) and `scorching_prims`. |
| `weather_fx.rs` | `WeatherFx` particle/rain/splash/noise builders, `weather_fx_for`, `noise_uv`, `WeatherStateView`/`WeatherView`. |
| `custom/` | `CustomFxId` + `dispatch`, the `FxEmit` primitive vocabulary, wave F1–F4 bodies (`basic.rs`) and the remaining F5–F6 bodies (`wave_f56.rs`). |
| `parts/` | `PartSpec` tree and `PartParams`/`PartMove` (`mod.rs`, `params.rs`), `PartProgressSpec` (`progress.rs`), program emission (`draw.rs`), `DrawTurret` (`turret.rs`) and `Weapon.draw`/unit weapons (`weapons.rs`). |

## ABI / parity gates

- **Effect ids are `Effect.all` declaration order, append-only.** `EffectId` indexes
  `content::registries::fx_meta::EFFECTS`; `EFFECT_COUNT` is a hard `267` (`none` id 0, `debugRect`
  id 266) and `catalog::build_registry` maps index → id without renumbering. Seed names, lifetimes
  and clips are the frozen ABI; bodies and layer overrides are not.
- `parity/fx_order.txt` (267 names, two `#` header lines) is the declaration-order oracle;
  `mind-headless fx audit` compares it line-by-line with the registry and fails on count/name drift.
  `parity/golden/fx/fx_order.json` (`count: 267`, upstream commit and source sha256) and the four
  `mind-headless/tests/golden/fx/` goldens are sha256-pinned in `parity/golden_manifest.json`.
- `catalog::order_hash` (FNV-1a-64 over names) is pinned at `94f48259c1412fef` by
  `catalog_matches_ledger`; `fx audit` also checks every name/lifetime against `fx_meta::EFFECTS`,
  and `--strict` fails on any `EffectKind::Unported` (the ledger holds 0).

## Host split

- `mind-core::fx` owns data and math: catalogue/registry, `EffectDef`/`EffectKind`, `EffectData`/
  `ViewSnapshot`, the pool/decal/trail/shake state machines, `EffectContainer`, `build_program*`,
  batching/LOD planning, draw-part specs/emission and weather/env prim builders — no Godot types.
- `mind-gdext::MindFx` owns instances and execution: `FxPool`, `DecalPool`, `TrailRegistry`,
  `ShakeState`, `FxSettings`, a reusable `DrawProgram` and `view_tick`. It drains `FxBus` in
  `tick_view` (skipping `EffectId::NONE` and disabled `effects`), resolves `RegionKey` to atlas
  textures, executes CPU prims in `_draw` (NoiseLayer/ShaderBlit stay plan-16), forwards lights/shake
  to the light pass and `MindCamera2D`, and is installed on `UnitRuntime::fx` by
  `MindSimHost::install_fx_sink`.
- Execution stays sideways: queue/layers/bands in [`render/AGENTS.md`](../render/AGENTS.md), GPU
  submission in `mind-gdext`; `batch.rs` only plans batching over an already-sorted program.

## Verification

Run from `client/rust/`. These are the invocations `mind-headless/tests/fx_golden.rs` gates.

```bash
cargo test -p mind-core fx::

cargo run -p mind-headless -- fx audit --json
cargo run -p mind-headless -- fx audit --strict --json
cargo run -p mind-headless -- fx lifecycle --ticks 120 --check mind-headless/tests/golden/fx/fx_lifecycle.json --json
cargo run -p mind-headless -- fx program hitBulletSmall --tick 3 --check mind-headless/tests/golden/fx/fx_program_hitBulletSmall.json --json
cargo run -p mind-headless -- fx program explosion --tick 3 --check mind-headless/tests/golden/fx/fx_program_explosion.json --json
cargo run -p mind-headless -- fx trail --check mind-headless/tests/golden/fx/fx_trail.json --json
cargo run -p mind-headless -- fx noop-headless --json
cargo test -p mind-headless --test fx_golden
```

## Gotchas

- **`none` is id 0 and must not spawn.** `FxGate::should_create` rejects it and the host's bus drain
  skips `EffectId::NONE`; `FxPool::spawn` itself is ungated and consumes a slot for one tick.
  `EffectRegistry::get` clamps an out-of-range id to `debugRect`, so validate ids at the seam.
- **Never reorder or insert into the catalogue.** `EffectId` *is* the registry index and the
  `fx_order.txt` line number. Appending an effect means bumping `EFFECT_COUNT` and regenerating
  `fx_order.json`/`fx_order.txt` plus their manifest hashes in the same change.
- **`spawn` is not one state.** `Multi`, `Radial`, `Wrap` and `Sound` expand into child spawns and
  return `None`; `Seq` owns one state and selects a child window in `resolve::seq`; `none` renders
  nothing. Do not count `spawn()` calls as live effects.
- **Headless defaults to `NoopFx`.** `UnitRuntime` and `CombatHarness` start with `noop_fx()`; only
  the host installs `FxBus`. Effects are excluded from saves/sync/checksum, and `fx noop-headless`
  proves the same scenario yields identical checksums with and without a recording bus.
- **Delayed spawns age on `advance`.** `start_delay > 0` queues into the 256-entry pending buffer
  and fires during the next `FxPool::advance` (which also follows parents and expires states). A
  delayed spawn goes live only on that tick; decals cap at 1024 and drop the oldest (deviation #6).
