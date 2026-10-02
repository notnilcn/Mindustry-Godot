# 17 — FX, EFFECTS, DRAW PARTS & WEAPON VISUALS IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files and sources named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — 2026-10-01, not started |
| **Phase** | P6 — Render, FX, audio (HIGH_LEVEL_PLAN §5) |
| **Depends on** | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (`CombatFx` call sites, weapon/turret mount state, `BulletDrawState`/`LaserDrawState`/`ShieldDrawState`), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (unit/mount state, `WeaponDef.parts`, unit trail fields, controller/trail hooks), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (**not written at plan-write time** — `Layer`, frame pipeline, draw-pass registration, light queue, Textures/atlas binding; interfaces frozen in §3.16). Also consumes `02_CONTENT_IMPLEMENTATION_PLAN.md` (`EffectId`/`fx_meta` R6, `DrawPartSpec`, `WeaponDef`, `UnitTypeDef`, `BulletDef`, `WeatherDef`), `03_ASSETS_IMPLEMENTATION_PLAN.md` (atlas region lookup, loose textures, region manifest), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`UpdateEffects` slot, `Time`/`Tmp`, `ClientHooks`, tick clock, `RngStream::Fx`), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (weather state; **not written** — view interface frozen in §3.12), `14_UI_IMPLEMENTATION_PLAN.md` (settings rows only; **not written** — no UI code here). |
| **Blocks** | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (visual verification only — effects are never synced/serialized), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (scenario + bench registration). |
| **Sources (read in full or as noted)** | Mindustry AGENTS: `core/src/mindustry/entities/AGENTS.md` (full), `content/AGENTS.md` (full), `graphics/AGENTS.md` (full), `type/AGENTS.md` (full). Java: `entities/Effect.java` (full), `entities/effect/{MultiEffect,SeqEffect,RadialEffect,WrapEffect,ExplosionEffect,ParticleEffect,WaveEffect,TriangleEffect,NoiseEffect,SoundEffect}.java` (all read; no `entities/effect/particle/` package exists), `content/Fx.java` (header + representative effects + programmatic audit: 267 `Effect` fields, 130 `randLenVectors` lambdas, 46 `e.scaled` calls, 42 custom `.layer(...)`, ~30 `e.data` uses), `entities/part/{DrawPart,RegionPart,ShapePart,HaloPart,HoverPart,FlarePart,EffectSpawnerPart}.java` (all), `entities/pattern/*.java` (skimmed: handled by 10), `entities/comp/EffectStateComp.java`, `entities/comp/DecalComp.java`, `type/Weapon.java` (draw/load half), `type/UnitType.java` (`draw`, `drawWeapons`, `drawWeaponOutlines`, `drawTrail`, `cellColor`, `drawShield` sections), `type/weapons/{BuildWeapon,MineWeapon,RepairBeamWeapon}.java` (draw overrides), `world/draw/DrawTurret.java`, `graphics/Trail.java` (full), `graphics/Drawf.java` (effect-relevant primitives), `graphics/Layer.java`, `graphics/ParticleRenderer.java` (evaluation), `graphics/EnvRenderers.java`, `type/Weather.java` (draw statics), `type/weather/{ParticleWeather,RainWeather}.java`, `core/Renderer.java` (`shake`, `enableEffects`), `entities/GroupDefs.java`. Addon: `godot-blast-bullets-2d/` + `example-project-blastbullets2d/` (§8 evaluation). Tests: `tests/src/test/java/` grepped — **no effect/part rendering tests exist** (§7a substitution). |
| **Extends spine** | Adds `MindFx` GDExtension autoload (`/root/Spine/MindFx`) with effect/decal/trail probes + `dump_effects()` JSON; an `Fx` tab at `/root/Spine/Ui/StateInspector/Fx` (live counts, kind histogram, draw-call count, program size); `mind-headless` subcommands `fx audit|lifecycle|program|trail|bench`; `scenarios/fx_*.json` fixtures. Node paths follow plan 00 §3.5/§7c (`/root/Spine/SimHost`); plan 05's assumed `/root/Main/SimBridge` is superseded. |

**Locked inputs treated as constants:** pure Rust/GDExtension (D1); `mind-core` Godot-free and tokio-free; fixed 60 Hz sim with `Time.delta == 1.0` (D8); content IDs/names/`Effect.all` order append-only (02); effects are view-only and **never** enter saves, snapshots, checksums or STDB (§2.5); GPL-3.0 headers (D6); full parity (D3).

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Effect runtime (view side).** The Rust equivalents of `Effect`, `Effect.EffectContainer`, `EffectState` (`EffectStateComp`), and `Effect.all`:
   - `EffectDef` registry (`lifetime`, `clip`, `startDelay`, `baseRotation`, `followParent`, `rotWithParent`, `layer`, `layerDuration`), id assignment in declaration order.
   - `fin()/fout()/finpow()/fslope()` and `fin(interp)/fout(interp)` semantics; `scaled(lifetime, f)` (including Java's shared inner-container reuse); `inner()`; `data()`.
   - spawn gating (`shouldCreate`: not headless, not `none`, `effects` setting on), camera-bounds clip test, `startDelay` deferred spawn, `followParent`/`rotWithParent` parent binding.
   - pooled `EffectState` lifecycle owned by `mind-gdext` (allocate from pool, `time += 1` per fixed view tick, expiry `time >= lifetime`, render may mutate `lifetime` — used by `trailFade`), insertion-stable live list consumed once per rendered frame.
2. **Full `Fx.java` catalogue parity** — all 267 vanilla `Effect` fields ported, id/order stable, with a parity ledger and audit (§6.5).
3. **Declarative effect kinds** (data-driven) for the recurring lambda shapes, and **custom Rust behavior bodies** for one-offs:
   - declarative: `Particle` (port of `ParticleEffect`), `Explosion` (`ExplosionEffect`), `Wave` (`WaveEffect`), `Triangle` (`TriangleEffect`), `Noise` (`NoiseEffect`), `Sound` (`SoundEffect`), composites `Multi`/`Seq`/`Radial`/`Wrap`, `None`.
   - custom: named Rust functions in `mind-core::fx::custom` emitting `DrawPrim`s, parameterized by per-entry `CustomParams`; one `CustomFxId` per distinct Java body that is not expressible declaratively.
4. **Typed effect data.** `EffectData` enum replacing Java `Object data`: live-entity refs, `Position`, `Positions(Seq<Vec2>)`, `TrailChannel`, content refs (`Block`/`UnitType`/`Item`), `Float`, shield/`LegDestroy`/`Rect`/`Vec2[]` payloads; view snapshot resolution for live refs.
5. **Composite/parameterized effects** — `MultiEffect`, `SeqEffect`, `RadialEffect`, `WrapEffect`, `SoundEffect` semantics (init-time lifetime/clip aggregation, seq child window, radial offsets).
6. **DrawPart framework** — `DrawPart`, `PartParams`, `PartMove`, `PartProgress` (all leaf progressors and every combinator in `CompatFix`), and subclasses `RegionPart`, `ShapePart`, `HaloPart`, `HoverPart`, `FlarePart`, `EffectSpawnerPart`; mirrored parts, outline/heat/team tint, children, `load()` region suffix resolution, `getOutlines()` for icon generation (03 consumes).
7. **Part attachment rendering** — for `UnitType.parts`, `Weapon.parts`, `BulletType.parts` (data owned by 02, runtime state by 10/11): interpolation from mount/unit/bullet state to `PartParams`, draw-program emission.
8. **Weapon visuals** — `Weapon.draw`/`drawOutline`/`load` draw half: region/cell/heat layers, additive heat, shadow, `flipSprite`/mirror/alternate, parts; `WeaponMount` draw state read-only from 10/11.
9. **Turret visuals** — `DrawTurret` (`drawPlan`/`draw`/`drawTurret`/`drawHeat`, `ammoParts`, `base`/`liquid`/`top`/`heat`/`preview`/`outline` regions, layer constants), driven by 10's `TurretState`/`TurretDrawState` and 16's block draw pass.
10. **Visual-only weapons** — `BuildWeapon`/`MineWeapon`/`RepairBeamWeapon` draw overrides (beam/target visuals; behavior in 10/11).
11. **Trails** — `Trail` update/shorten/draw math (fixed view clock), bullet trails fed by 10's `!headless` update sites, unit engine trails (`drawTrail`) and `Fx.trailFade` hand-off; cap rendering; per-batch command output.
12. **Decals** — `Decal` pool (scorch/rubble/region decals), floor-surface gate, fade at `fin > 0.98`, cap policy (§2.4 #6).
13. **Screen shake** — `Effect.shake` falloff + `Renderer.shake` accumulation/decay port; applied to the camera by 15/16.
14. **Weather particle visuals** — `Weather.drawParticles`/`drawRain`/`drawSplashes`/`drawNoise(Layers)` port; `ParticleWeather`/`RainWeather` draw halves. Weather **state** (life/opacity/wind/force/status application) is 12/11.
15. **EnvRenderers bodies** — underwater (water tint, caustics blit, rays, suspended particles) and scorching (noise layers) effect visuals; registration itself is 16.
16. **Performance layer** — pooling, `DrawProgram` reuse, per-region batching, `MultiMesh2D` path for high-count particles/bullets, optional `GPUParticles2D` for weather; budgets in §7d.
17. **Addon evaluation** — BlastBullets2D and any particle addon evaluated with recorded outcome + license (§8 R-17-7).

### 2.2 “Done” means

- `cargo test -p mind-core` covers all pure logic in §2.1 (container math, progressors, part resolution, trail math, lifecycle, catalog audit) with no Godot; §7a green.
- `mind-headless fx audit` exits 0 against `parity/ledgers/fx.md` + `parity/fx_catalog.json`; every one of the 267 effects is either a declarative kind or a registered custom body with a test; zero unported ledger rows.
- `mind-headless` scenarios §7b pass with committed golden dumps; the headless sink proves effects never affect sim checksum.
- The MCP scenario §7c passes: a turret fires, a bullet impacts a dummy, screenshots at fixed frames show the expected effects, `godot_exec` probes report matching live/kind/expiry state and part animation values.
- Perf budgets §7d measured and recorded; zero steady-state allocations; FX budget respected at 2 000 live effect states.
- No file under `mind-core` references Godot; no effect state is serialized, synced or checksummed.

### 2.3 Explicit boundaries (who owns what)

| Area | This plan (17) owns | Owned elsewhere |
|---|---|---|
| `Effect`/`EffectState`/`EffectContainer`, `Fx` catalogue, effect kinds/custom bodies, decals, shake, trails, weather visuals, env-renderer bodies | All view-side runtime and draw programs | — |
| `EffectId` name/id table | Full registry (02 seeds names/ids only; 02 R6 closes here) | `02_CONTENT_IMPLEMENTATION_PLAN.md` |
| `DrawPart` framework, `PartProgress`, part specs type | Spec enum + progressor eval + resolver + draw emission | Spec **fields** stored on `UnitTypeDef`/`WeaponDef`/`BulletDef` by 02 |
| Weapon/turret mount **state** (`warmup`, `reload`, `smoothReload`, `heat`, `recoil`, `recoils`, `charge`, `side`, `flipSprite`, `barrelCounter`); bullet/laser/shield behavior | Read-only consumption for draw | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| ShootPattern firing geometry | Nothing (10 owns patterns); 17 consumes `mount.totalShots`/`barrelCounter` and `EffectSpawnerPart` only | `10` |
| `Layer` constants, frame pipeline, draw-pass registration, light FBO, atlas/texture binding, batching executor | Freezes the interface it needs (§3.16) and seeds it if 16 is absent | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Unit body/legs/treads/engines/crawl/segment drawing, block body drawing | `UnitType.parts` + weapon parts only (16 draws bodies; 17 layers parts/weapons) | `16`, `11` |
| Weather **state** (life/opacity/wind/force/status) | Draw functions only | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (state), `02` (metadata), `11` (unit impulses) |
| Sound playback, `SoundEffect` sound trigger, `SoundLoop` | `SoundEffect` calls the audio sink one-way | `18_AUDIO_IMPLEMENTATION_PLAN.md` |
| Mod parsing of `Effect` JSON/`type` class names | Consumes registry append API | `20_MODS_IMPLEMENTATION_PLAN.md` |
| Settings rows (`effects`, `screenshake`, `showweather`), inspector UI widgets | Keys + semantics (04 storage, 14 widgets) | `04`, `14` |
| Net sync of bullets/turrets; no effects | — | `21` |

### 2.4 Deliberate deviations (reason stated)

1. **Effects are view-only, not ECS sim entities.** Java `EffectState` is a pooled entity in `Groups.effect` advanced by `Logic.updateEntities()`. The port keeps the plan-05 `UpdateEffects` slot reserved but empty; `mind-gdext` advances an effect pool by one tick after each `Sim::tick()`. Effects already had `serialize=false` and no sim side effects, so observable behavior (lifetime in ticks, camera/effects-setting gates) is preserved without polluting the deterministic world. Deviation is structural, not behavioral.
2. **`Object data` becomes `EffectData` + parent refs.** Java passes live `Posc`/`Block`/`Unit`/`Sector` objects and reads them every frame. Rust stores `EffectData` (content ids, positions, trail channels, entity ids) and resolves live reads through a view snapshot; entities that despawn mid-effect freeze at their last snapshot (§8 R-17-6). Behavioral equivalence for all vanilla effects (data survey in §3.4); scripting mods (OD1) get the same enum.
3. **All effect drawing is compiled to a `DrawPrim` program.** Java effects call Arc `Draw`/`Fill`/`Lines` immediately. The port emits a flat command list that `mind-gdext` executes/sorts/batches in `_draw()`. This makes effect geometry testable headlessly (golden programs) and lets 16 batch by region/blending.
4. **Custom bodies are Rust functions in `mind-core`, not lambdas/GDScript.** GDScript is UI-layout only (D1) and untestable headlessly; effect bodies must be. GDScript custom effects are explicitly not supported (a mod-facing script hook is OD1 in plan 20).
5. **Visual randomness is seeded per effect id, not global time.** `Angles.randLenVectors(seed, …)` already seeds from `e.id` in most cases; the port routes all effect RNG through `FxRand`/`ArcRand` seeded by `(effect_id, seed_offset, view_tick)` and **never** `RngStream::Sim`. Screenshots at fixed frames are reproducible for oracles; pixel-exact Java parity is still out of scope (OD-17-A).
6. **Decals are capped (1 024 live, oldest dropped).** Upstream decals are pooled/unbounded; a cap is required to bound draw batches. `scorch`/`rubble` visual frequency at cap is unchanged in practice; long soak sessions drop oldest first (documented, test `decals::cap_dropping`).
7. **View clock is the tick counter, not wall time.** `Time.time` in effect bodies becomes `view_tick` (accumulated fixed ticks). Animations stay frame-rate independent (D8) and deterministic for fixed-step screenshots. Wall-clock-driven UI animation remains 14's.
8. **`Effect.layerDuration` is preserved but unused.** Upstream declares/assigns it and never reads it (`Effect.java:44,93`); kept on `EffectDef` for mod/data compatibility with a parity-gotcha note.
9. **`NoiseEffect`/caustics use native shader blits.** Their math can't be expressed as per-vertex prims; bodies emit `DrawPrim::NoiseLayer`/`ShaderBlit`, executed by 16's pipeline. The layer/scroll math is still in `mind-core` (headless-testable parameters).
10. **`ParticleRenderer.java` (WIP point-sprite renderer) is not ported.** It is not wired into upstream's pipeline (`graphics/AGENTS.md`); the port uses `MultiMesh2D`/`GPUParticles2D` instead (§3.14).

### 2.5 Headless & determinism contract

- `mind-core` defines the sink trait `FxSink`; headless installs `NoopFxSink`. Sim code calls `fx.effect(...)`, `fx.shake(...)`, `fx.decal(...)`, `fx.light(...)`, `fx.sound(...)` **unconditionally**; the sink performs `shouldCreate`/camera/culling gating (Java does the same inside `Effect.create`), so all peers run identical sim code.
- `FxSink` writes are append-only view notifications: no sim system may read the sink, and `Sim::checksum()`/snapshots exclude the sink resource, effect/decal/trail pools and all view counters.
- The `UpdateEffects` set from 05 runs as a documented no-op (trace-visible); `MindFx::tick_view()` consumes the queue after each `Sim::tick()` in `mind-gdext`. This keeps 05's schedule golden unchanged.
- Effects must never read `SimRng`; `RngStream::Fx` (05 §3.11) is only used by *sim-side* cosmetic randomness (e.g. `shoot` spread jitter already part of 10) and never by the view.

---

## 3. Target design

### 3.1 Module layout

```
client/rust/mind-core/src/
  render/
    layer.rs              # exact Layer.java constants (f32); seeded by 17, owned by 16 after it lands
    draw.rs               # DrawPrim, DrawProgram, Blending, RegionKey; the 16↔17 contract
  math/
    rand_arc.rs           # consumed from plan 11 (Arc Rand port); do NOT duplicate
    angles.rs             # randLenVectors + trnsx/trnsy (Arc Angles port)
    interp.rs             # Interp/curves used by particle/part progress (Mathf/Angles subset)
  fx/
    mod.rs                # FxPlugin-free module: types + registry + events
    def.rs                # EffectDef, EffectKind, EffectId, EffectRef
    catalog.rs            # the 267 vanilla effects (F1..F6 waves), in Effect.all order
    custom/
      mod.rs              # CustomFxId table + dispatch
      basic.rs particles.rs explosions.rs waves.rs projectiles.rs units.rs blocks.rs weather.rs ui.rs
    data.rs               # EffectData + EffectDataOwned + snapshot resolution
    container.rs          # EffectContainer, fin/fout/finpow/fslope, scaled(), inner()
    sink.rs               # FxSink trait, NoopFxSink, FxBus queue, ViewFxCapacity settings
    pool_spec.rs          # EffectStateSpec (view-only state layout, pool sizing constants)
    trail.rs              # Trail math + TrailChannel registry types
    decal.rs              # DecalSpec + fade/cap policy math
    shake.rs              # shake accumulation/decay math
    weather_fx.rs         # Weather drawParticles/drawRain/drawSplashes/drawNoise* builders
    parts/
      mod.rs              # PartSpec enum, attachment keys
      params.rs           # PartParams, PartMove
      progress.rs         # PartProgressSpec + eval
      region.rs shape.rs halo.rs hover.rs flare.rs spawner.rs
      resolve.rs          # ResolvedParts cache + DrawPrim emission
      weapons.rs          # Weapon.draw/drawOutline geometry (pure)
      turret.rs           # DrawTurret geometry (pure)
client/rust/mind-gdext/src/
  fx/
    mod.rs                # MindFx class registration, view tick plumbing
    extract.rs            # drains FxBus → effect/decal/trail/light/shake queues
    pool.rs               # EffectState pool + live list + delayed spawns
    program.rs            # DrawProgram build per frame; layer sort
    exec.rs               # DrawPrim → CanvasItem draw_* calls
    batch.rs              # MultiMesh2D batches (particles/bullets/trails), GPUParticles2D path
    decals.rs trails.rs lights.rs shake.rs weather.rs env.rs
    draw_turret.rs        # DrawTurret execution against building draw states
  mind_fx.rs              # #[derive(GodotClass)] MindFx (probes)
mind-headless/src/scenarios/
  fx_audit.rs fx_lifecycle.rs fx_noop.rs fx_program.rs fx_trail.rs fx_bench.rs
parity/
  java/DumpFx.java        # one-off Java dumper (never modifies upstream)
  fx_catalog.json fx_order.txt fx_vectors.json ledgers/fx.md reports/addon_eval.md
```

`mind-core` stays Godot-free: all effect/part/trail/weather math and all `DrawPrim` output are pure. `mind-gdext` only executes programs and owns the pools that must match frame timing.

### 3.2 `EffectDef`, `EffectKind`, ids

```rust
pub type EffectId = u16; // index into EffectRegistry::ALL; content ABI name `EffectDef.name`

pub struct EffectDef {
    pub id: EffectId,
    pub name: &'static str,          // Fx field name, parity ABI
    pub lifetime: f32,
    pub clip: f32,
    pub start_delay: f32,            // default 0; SoundEffect inherits child delay
    pub base_rotation: f32,
    pub follow_parent: bool,         // default true
    pub rot_with_parent: bool,       // default false
    pub layer: f32,                  // default Layer::EFFECT
    pub layer_duration: f32,         // dead upstream field; preserved
    pub kind: EffectKind,
}

pub enum EffectKind {
    None,
    Particle(ParticleParams),        // ParticleEffect fields
    Explosion(ExplosionParams),      // ExplosionEffect fields
    Wave(WaveParams),                // WaveEffect fields
    Triangle(TriangleParams),        // TriangleEffect fields
    Noise(NoiseParams),              // NoiseEffect fields
    Sound(SoundParams),              // SoundEffect: sound id (18) + child EffectId
    Multi(SmallVec<[EffectId; 4]>),
    Seq(SmallVec<[EffectId; 4]>),
    Radial(RadialParams),            // child EffectId
    Wrap(WrapParams),                // child EffectId + color + rotation
    Custom(CustomFxId, CustomParams),
}
```

- `EffectRegistry` is a `Resource`-attached static table (`&'static [EffectDef]`) built by `content::init`/`link`; ids are declaration order, append-only. `Effect.get(id)` is `registry.get(id)`.
- `none` is id 0 and id 0 is the only effect `shouldCreate` rejects by identity.
- `EffectDef.start_delay`: upstream `Time.run(startDelay, () -> add(...))`; the port pushes a `DelayedSpawn { def, x, y, rotation, color, data, ticks_left }` into the view pool; delayed entries tick down in insertion order and spawn when `ticks_left == 0`.
- Composites compute `lifetime`/`clip` on first use (Java lazy `init()`): `Seq` sums child lifetimes and takes `max(clip, 100, children clips)`; `Wrap` copies child `lifetime`/`clip`; `Radial`/`Multi` keep their own defaults (100 clip).
- `EffectId` is never serialized; name is the mod ABI (`Effect.get` by name via 02/20).

### 3.3 `EffectData` (typed replacement for `Object data`)

```rust
pub enum EffectData {
    None,
    UnitType(UnitTypeId),            // unitSpawn, unitDespawn(type), ...
    Unit { id: EntityId, kind: UnitTypeId }, // unitControl, unitDespawn, unitShieldBreak, ...
    Block(BlockId),                  // blockCrash, breakBlock?, coreBuildBlock, ...
    Item(ItemId),                    // itemTransfer? (see Fx.java:1621)
    Position { x: f32, y: f32 },     // itemTransfer, unitSpirit, ...
    Positions(SmallVec<[(f32, f32); 8]>), // debugLine (Fx.java:190)
    Trail(TrailChannelId),           // trailFade
    Float(f32),                      // healWaveDynamic (Fx.java:2366)
    Shield { team: TeamId, sides: u8, rotation: f32, radius: f32 }, // shieldBreak/arcShieldBreak
    LegDestroy(LegDestroyData),      // legDestroy (Fx.java:2952)
    Vec2Array(SmallVec<[(f32, f32); 8]>), // Fx.java:2967
    Rect { x: f32, y: f32, w: f32, h: f32 }, // Fx.java:2984
    Custom(u32),                     // mod/scripting extension (20/OD1)
}
```

- `EffectData` is `Copy` where possible; live entity/unit refs resolve through `ViewSnapshot: { fn unit_pose(id) -> Option<Pose>; fn unit_type(id) -> Option<UnitTypeId>; fn building_pose(id) ...; fn team_color(team) -> Rgba }`, implemented by `mind-gdext` (interpolated transforms) and by a fixture in headless tests. Missing entity → the last known pose is frozen, then the effect expires normally.
- `followParent` binds only for data that contains a position-capable reference; rotation follows only when `rotWithParent` (Java `Effect.java:170-173`).
- Enumerate once in the ledger: 30 vanilla effects are data-carrying; the `EffectData` mapping is part of the audit (`fx audit --data`).

### 3.4 Catalogue & custom bodies

- Vanilla catalogue count from the Fx.java audit: **267 fields** (`none` + 266 effects), 130 of which call `randLenVectors`, 46 use `e.scaled(...)`, 42 set a custom layer, ~30 read `e.data`.
- Classification rule (recorded per ledger row): if the body is expressible as one of the parameterized kinds with only numeric/color/region parameters, it becomes a declarative `EffectKind`; otherwise it gets a `CustomFxId` implemented in `mind-core::fx::custom`.
- Custom registry:
```rust
#[derive(Copy, Clone)] #[repr(u16)]
pub enum CustomFxId { BlockCrash, TrailFade, UnitSpawn, UnitControl, ... }
pub struct CustomParams { pub colors: [Rgba; 2], pub f1: f32, pub f2: f32, pub f3: f32, pub region: RegionKey, pub extra: u32 }
pub struct CustomFx { pub id: CustomFxId, pub params: CustomParams }
fn dispatch(id: CustomFxId) -> fn(&mut FxEmit, &EffectContainer, &ViewSnapshot);
```
- `FxEmit` exposes primitive helpers mirroring Arc (`rect`, `color`, `alpha`, `mixcol`, `stroke`, `circle`, `poly`, `tri`, `square`, `line_angle`, `line`, `arc`, `light`, `additive`, `noise_layer`, `shader_blit`) so custom bodies read 1:1 against Java.
- `EffectSpawnerPart` and `SoundEffect` are the only "recursive" effects; `Wrap`/`Radial`/`Multi`/`Seq` are data composites, not custom code.
- The 6 catalogue waves: F1 `none`→`hitScepterSecondary` (~45), F2 combat explosions/hits (~45), F3 muzzle/trails/casings (~45), F4 blocks/items/production (~45), F5 campaign/weather/ui (~45), F6 remainder + `debugLine`/`debugRect` (~42). Each wave ends with `mind-headless fx audit --wave FN`.

### 3.5 Effect runtime, container semantics, lifecycle

```rust
pub struct EffectContainer<'a> {
    pub id: EffectId, pub x: f32, pub y: f32,
    pub time: f32, pub lifetime: f32, pub rotation: f32,
    pub color: Rgba, pub data: &'a EffectData,
    inner: Option<Box<EffectContainer<'a>>>,
}
impl<'a> EffectContainer<'a> {
    pub fn fin(&self) -> f32;                      // time / lifetime (Scaled)
    pub fn fin_with(&self, i: Interp) -> f32;
    pub fn fout(&self) -> f32;                     // 1 - fin
    pub fn finpow(&self) -> f32;  pub fn fslope(&self) -> f32;
    pub fn scaled(&mut self, lifetime: f32, f: impl FnOnce(&EffectContainer)) { /* inner reuse; time <= lifetime gate */ }
}
```

- Per fixed tick (in `MindFx` after `Sim::tick()`): every live state in insertion order gets `time += 1.0`; expiry when `time >= lifetime` (matching `TimedComp`). Delayed spawns decrement. `trailFade` mutates `lifetime` during resolve (Java returns it from `Effect.render` and `EffectStateComp.draw` writes it back); the port returns a `lifetime_override` from the program builder.
- Pool: `Vec<EffectState>` fixed-capacity (default 4 096, configurable via `mind-core::fx::pool_spec`), free list, live list of indices (insertion order). Spawn/despawn are O(1), allocation-free after warmup. Overflow drops newest and increments a counter surfaced in `MindFx`.
- Resolve once per rendered frame: `mind-gdext::fx::program` walks the live list, builds `DrawPrim`s into a reusable `Vec`, applies per-effect `layer` (and `base_rotation` already folded into `rotation`), sorts by `(z, seq)`, uploads to the executor. If no tick occurred since last frame, positions/life stay frozen (Java also redraws the same state).
- Gating (all in the sink): enabled and `id != none`; camera rect overlap with `set_centered(x, y, clip)`; tile-bounds-free.
- `EffectState` upstream def is `[EffectStatec, Childc]`, `pooled = true, serialize = false` — the port states this in the ledger and asserts in tests that no effect field appears in `FieldMeta`/checksum/save.

### 3.6 `DrawPrim` program & layers (16 contract)

```rust
pub enum Blending { Normal, Additive }
pub struct DrawPrim {
    pub z: f32,
    pub blend: Blending,
    pub kind: PrimKind,
}
pub enum PrimKind {
    Region { region: RegionKey, x: f32, y: f32, w: f32, h: f32, rotation_deg: f32,
             origin: (f32, f32), color: Rgba, mix: Option<Rgba>, wrap: bool },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgba },              // Fill.rect
    Circle { x: f32, y: f32, r: f32, fill: bool, stroke: f32, color: Rgba },
    Poly { x: f32, y: f32, sides: u16, r: f32, rotation_deg: f32, fill: bool, stroke: f32, color: Rgba },
    Polygon { points: SmallVec<[(f32, f32); 12]>, fill: bool, stroke: f32, color: Rgba }, // Fill.poly
    Tri { x: f32, y: f32, w: f32, h: f32, rotation_deg: f32, color: Rgba },               // Drawf.tri
    Line { x1: f32, y1: f32, x2: f32, y2: f32, stroke: f32, color: Rgba, cap: bool },
    Polyline { points: SmallVec<[(f32, f32); 12]>, stroke: f32, color: Rgba },
    Arc { x: f32, y: f32, r: f32, start_deg: f32, sweep_deg: f32, stroke: f32, color: Rgba },
    NoiseLayer { texture: TextureKey, rect: [f32; 4], tint: Rgba, opacity: f32, scroll: [f32; 2], offset: f32 },
    ShaderBlit { shader: ShaderKey },                                   // caustics etc. (16 executes)
    Light { x: f32, y: f32, radius: f32, color: Rgba, opacity: f32 },
}
```

- `Render::layer` constants port `Layer.java` exactly (f32, increments of 10; `turretHeat = 50.1`, `blockCracks = 30.1`, …, `max = 220`). `DrawProgram.sort()` is stable by `(z, insertion index)`.
- Bloom contract: prims with `Layer::BULLET <= z <= Layer::EFFECT` are inside the bloom range; 16 captures/renders bloom around those layers without 17 changes.
- `RegionKey` resolves through 03's `AtlasIndex` (`find_or(name, "error")` policy stays in 03); `TextureKey` covers loose textures (`sprites/distortAlpha.png`, `sprites/rays.png`, weather `noiseAlpha`); missing region = skip prim + `missing_region` counter (upstream `region.found()` behavior).
- Executor batching: group consecutive prims by `(blend, region/shader/texture, z)`; text is never emitted by FX. Draw-call target in §7d.
- `DrawPrim` programs are pure and serialized by `mind-headless fx program` for golden tests.

### 3.7 DrawPart framework

- `PartParams` / `PartMove` port 1:1 (`DrawPart.java:25-71`): `warmup, reload, smoothReload, heat, recoil, life, charge, x, y, rotation, sideOverride=-1, sideMultiplier=1`.
- `PartProgressSpec` is a data tree with every leaf (`reload`, `smoothReload`, `warmup`, `charge`, `recoil`, `heat`, `life`, `time`, `constant(v)`) and every `CompatFix` combinator (`inv`, `slope`, `clamp`, `add`, `delay`, `curve(offset,duration)`, `sustain`, `shorten`, `compress`, `blend`, `mul`, `min`, `sin(offset,scl,mag)`, `sin(scl,mag)`, `absin`, `mod`, `loop`, `apply(fn,a,b)`, `curve(interp)`). `time` reads the fixed view tick (deviation #7).
- Spec records:
```rust
pub struct RegionPartSpec { suffix, name: Option<&'static str>, mirror, outline, replace_outline, draw_region,
    heat_light, clamp_progress, progress, grow_progress, heat_progress, blending, layer, layer_offset,
    heat_layer_offset, turret_heat_layer, outline_layer_offset, x, y, x_scl, y_scl, rotation, origin_x, origin_y,
    move_x, move_y, grow_x, grow_y, move_rot, heat_light_opacity, color, color_to, mix_color, mix_color_to,
    heat_color, children: Vec<PartSpec>, moves: Vec<PartMove> }
```
  plus `ShapePartSpec`, `HaloPartSpec`, `HoverPartSpec`, `FlarePartSpec`, `EffectSpawnerPartSpec` mirroring their Java fields.
- Region resolution: `ResolvedPart::load(content_name)` computes `real_name = name.unwrap_or(content_name + suffix)`; `mirror && turretShading` loads `-r`/`-l` (+`-r-outline`/`-l-outline`), else base + `-outline`; heat `-heat`, light `-light`. Names are cached per `(ContentRef, part_index)` and verified by 03's region audit.
- Drawing ports `RegionPart.draw` exactly: layer/z handling, `under && turretShading`, move/grow accumulation, `sideOverride`/`sideMultiplier`/mirror sign, `originX/Y` correction, outline offset `-0.001`, color/mix interpolation by `prog`, additive blending, heat `Drawf.additive` with `turretHeatLayer` or `z + heatLayerOffset`, optional heat light, recursive children with child `PartParams`, `Draw.xscl` restore.
- `ShapePart`/`HaloPart`/`HoverPart`/`FlarePart` emit the same primitives their `Fill`/`Lines`/`Drawf.tri` calls produce; `HoverPart`/`ShapePart` use `view_tick` for phase/rotation. `EffectSpawnerPart` uses a per-instance interval state, paused-state guard, `chanceDelta` semantics, and calls `FxSink::effect` (view RNG only).
- `getOutlines()` is exposed as an iterator of region names so plan 03's icon/outline generation can call it (`DrawTurret` uses it too).

### 3.8 Weapon, turret and bullet part rendering

- `parts::weapons::weapon_pose(unit_pose, mount_state, weapon_def) -> WeaponPose { wx, wy, weapon_rotation, real_recoil }` ports `Weapon.draw` geometry (`Weapon.java:218-292`), including `Mathf.pow(recoil, recoilPow) * recoil`.
- Draw order matches `UnitType.draw` (`UnitType.java:1547-1556`): weapon outlines for `!top` before body; weapons after body/cell; `parts.under` before weapon region, `!parts.under` after. Unit body/cell/legs/treads/engines remain 16.
- `DrawTurret` (`world/draw/DrawTurret.java`, all fields): base → shadow at `turretLayer - 0.5` → turret region/liquid/top at `turretLayer` → heat additive at `turretHeat` → outline under parts → parts/ammoParts with progress `1 - progress`, `recoil`/`curRecoils`, `charge`, `heat`, `x + recoilOffset`. Reads 10's `TurretState`/`TurretDrawState`; block placement previews (`drawPlan`) are 15/16 but use 17's region names.
- Bullet `parts` (if any) and `BulletDef.parts` follow the same part resolver with `PartParams::set(0, 0, 0, 0, 0, 0, b.x, b.y, b.rotation)` and `life = b.fin()`.
- Shoot/charge effects: 10's `Weapon.bullet` call sites already notify the sink (`shootEffect`/`smokeEffect`/`ejectEffect`/`chargeEffect`); 17 renders them; 18 plays sounds.
- Visual-only weapons: port the draw overrides of `BuildWeapon` (beam to build plan), `MineWeapon` (mining beam + target square), `RepairBeamWeapon` (`RepairTurret.drawBeam` geometry, `HealBeamMount` state from 10) as geometry functions; their sim logic stays in 10/11.

### 3.9 Trails

- `Trail` (port of `graphics/Trail.java`, 147 lines): fixed `length`, `points: Vec<[f32; 3]>`, `last_x/y/angle/w`, `counter`; `update(x, y, width, delta=1)` with count/innerpolation/trim semantics; `shorten(delta=1)`; `vertices(width)` → `Polygon` prims (quad strips) plus `drawCap` circle; `copy()` for `trailFade`.
- Ownership: bullet trails live in a `TrailRegistry` keyed by bullet view id (`TrailChannelId`); 10 notifies `FxSink::trail_update(id, x, y, width, length)` at its `updateTrail` guards; on bullet removal/trailFade, the trail is detached and handed to the `trailFade` effect via `EffectData::Trail(channel)`, which shortens/draws it with the effect's color and `rotation` as width (Java `trail.drawCap(e.color, e.rotation)`).
- Unit engine trails (`UnitType.drawTrail`, `trailLength/trailColor/trailScl`, `trail sin`) use a per-unit channel; `UnitComp.remove` hand-off (`Fx.trailFade`) in 11 maps to the same detach API.
- Trails are view-only, bounded (`length` points), never synced.

### 3.10 Decals

- `DecalPool` in `mind-gdext::fx::decals`, driven by `FxSink::decal(region, x, y, rotation, lifetime, color)`:
  - spawn gates: not headless, region found, `world.tile_world(x,y)` exists and floor `has_surface()` (Java `Effect.decal`, `Effect.java:230-247`);
  - fields: pos, rotation, lifetime (3600 default), color, region; `time += 1` per view tick; expiry;
  - draw at `Layer::SCORCH`: `mixcol(color, color.a)`, `alpha(1 - curve(fin, 0.98))`, region rect; emit as `Region` prims batched per region;
  - `scorch(x, y, size)`: `size.clamp(0, 9)`, region `scorch-<size>-<random(2)>`, rotation `random(4)*90`, lifetime 3600, `Pal.rubble`; `rubble(x, y, blockSize)`: `rubble-<n>-<0|1>` fallback rule, same lifetime/color.
  - cap 1 024 (deviation #6), oldest-first drop, counter in `MindFx`.

### 3.11 Screen shake & lights

- `shake.rs` ports `Effect.shake` falloff (`shakeFalloff = 10000`, distance clamp ≥1) and `Renderer.shake` accumulation (`shakeIntensity = max(...)`, `shakeTime = max(...)`, `shakeReduction = intensity/time`) plus the per-frame offset `random_dir * random(intensity * screenshake/4 * 0.75)` (`Renderer.java:210-218`).
- Applied by 15/16's camera as an additive offset; 17 exposes `ShakeState::offset(view_tick)`. `screenshake` setting is an int 0–4, default 4 (04 storage, 14 widget).
- Lights: `FxSink::light(x,y,radius,color,opacity)` and `Drawf.light(region,...)` (region width → radius) push to a `LightQueue` consumed by 16's `LightRenderer` (4× downscaled additive FBO). FX prims never draw light textures themselves.

### 3.12 Weather particle visuals & EnvRenderers

- View interface from 12 (frozen; reconcile at 12 write time):
```rust
pub struct WeatherStateView { pub weather: WeatherId, pub intensity: f32, pub opacity: f32,
                              pub wind_vector: (f32, f32), pub life: f32 }
```
- `weather_fx.rs` ports `Weather.drawParticles` (cell-wrapped particle field, `boundMax = 80000`, seeded `rand.set_seed(0)` per call), `drawRain`, `drawSplashes` (splash frames + ground sparks), `drawNoiseLayers`/`drawNoise` (delegating to `NoiseEffect` math) exactly; `ParticleWeather`/`RainWeather` draw halves dispatch on `WeatherKind` from 02.
- Gates: `renderer.weatherAlpha > 0.0001`, `drawWeather`, setting `showweather`; layer `Layer::WEATHER` (`drawOver`) and `Layer::DEBRIS` (`drawUnder`).
- `EnvRenderers` bodies in `mind-gdext::fx::env`: underwater (`Layer::LIGHT + 1` tint rect + caustics `ShaderBlit` + 50 rays from `sprites/rays.png` + `Weather::drawParticles` at `Layer::WEATHER`) and scorching (`distortAlpha.png` noise layers, `Layer::WEATHER - 1` or `fogOfWar + 1` when fog). Registration is 16's `renderer.add_env_renderer(Env, pass)`.
- 12's `ParticleWeather.update` unit impulse and `WeatherStateComp` life/opacity stay in sim (11/12); only draw moves here.

### 3.13 Godot / GDExtension surfaces

- `MindFx` autoload node at `/root/Spine/MindFx` (plan 00 pattern; add-only API):
  - probes: `live_effect_count() -> i64`, `effect_kind_counts() -> Dictionary`, `live_decal_count()`, `live_trail_count()`, `draw_call_count()`, `last_program_size()`, `dump_effects() -> GString` (JSON `[{handle,id,name,x,y,rotation,time,lifetime,layer,data}]`), `sample_effect(handle) -> Dictionary`;
  - dev control: `spawn_effect(name, x, y, rotation, color, data_json := "") -> i64`, `clear_effects()`, `set_fx_enabled(bool)`, `tick_view()` (test-only; normally driven by `SimHost`).
  - `sample_part(unit_id, weapon_index, part_index) -> Dictionary` returns the evaluated `PartParams` for MCP part-animation assertions.
- `MindSimHost` gains the one-way hook wiring at boot (implements `FxSink` over `FxBus`); no new sim logic.
- Inspector tab `/root/Spine/Ui/StateInspector/Fx` is GDScript layout only (reads probe dictionaries every 250 ms, like plan 00's inspector).
- No STDB tables/reducers/views: effects, decals, trails, shake and draw programs are client-local and rebuild from deterministic sim state on every peer. Nothing here changes plan 21's snapshot/relay shapes. Multiplayer desync tests ignore FX counters; `dump_effects()` is explicitly excluded from checksum/desync comparisons.

### 3.14 Performance & batching

- Pools: effect states 4 096 (fixed), decals 1 024, trail channels per live bullet/engine unit from the same pooled registry; delayed spawn ring 256. All grow once to capacity and never allocate per frame (alloc-audit gate).
- `DrawProgram` reuses one `Vec<DrawPrim>` (with `clear()` retaining capacity) plus a scratch `Vec` for sorting; per-frame program build for 2 000 states ≈ tens of thousands of prims worst case.
- Batching executor policy: adjacent prims with same `(z, blend, region)` merge into one draw; particle-heavy kinds (`Explosion`, `Particle`, trail strips, casings) use `MultiMesh2D` instances when instance count in a batch ≥ 256 (configurable); weather particles use `GPUParticles2D` only if a batch exceeds 4 000 instances, otherwise MultiMesh. Thresholds live in one module so they can be tuned by bench.
- Bullet bodies: 10/16 own bullet rendering; 17 only handles bullet **trails/effects**. BlastBullets2D not adopted (R-17-7).
- LOD: at `Lod::l2` (16), particle counts for `Particle`/`Explosion` kinds halve (`particles / 2`, floor 1) and `Noise` effects skip a layer; parity ledger notes this as a deliberate quality setting (no gameplay effect).

### 3.15 Determinism & invariants

1. No effect/part/trail/decal code in `mind-core` reads or writes sim state; all reads go through `ViewSnapshot`.
2. `FxBus`/pool/queues are excluded from `Sim::checksum()`, `SimSnapshot`, saves and STDB. Test `fx::tests::effects_excluded_from_checksum` runs the same seed with/without a recording sink and compares checksums.
3. All effect/view randomness uses `ArcRand` seeded by `effect_id + seed_offset + view_tick`; never `RngStream::Sim`; never wall clock.
4. No `HashMap` iteration in resolve paths; live lists are index-ordered `Vec`s; batch grouping is `IndexMap`/sorted.
5. Region names and content names are byte-identical to upstream (03 ABI); no name normalization.
6. Every ported file carries the GPL header; `mind-core` has no Godot/tokio imports (`cargo tree` gate in 23).

### 3.16 Cross-plan reconciliation (reconcile by filename)

| Sibling file | Interface this plan requires / provides | Status at plan-write time |
|---|---|---|
| `02_CONTENT_IMPLEMENTATION_PLAN.md` `content/registries/fx_meta.rs` | 02's R6 closes: 17 owns `EffectDef`/`EffectRegistry`; 02's seed becomes `EffectRef { id, name, lifetime, clip }` accessors (content load still reads `Fx.lightning.lifetime`). `EffectId` ordinals unchanged; names ABI. | Present (R6 row, §8) |
| `02` `DrawPartKind`, `UnitTypeDef.parts`, `WeaponDef.parts`, `BulletDef.parts` | 17 defines `PartSpec`/`PartProgressSpec` types; 02 stores instances as data. `DrawPartKind` becomes the `PartSpec` discriminant. | Present |
| `02` `WeatherDef` (`ParticleWeather`/`RainWeather` fields, `noisePath`, splash regions) | 17 consumes for `weather_fx`; 02 keeps field data | Present (draw in 16 per 02 port map — **conflict**, see §8 R-17-3; 17 owns draw, 16 owns registration) |
| `03_ASSETS_IMPLEMENTATION_PLAN.md` atlas/`AtlasIndex`/loose textures | 17 consumes `find(name)`, `find_or(name,"error")`, loose texture loading (`distortAlpha.png`, `rays.png`, weather noise); 03's manifest must include all effect/part/weather region names (`fx audit --regions` cross-check) | Present |
| `05_SIM_CORE_IMPLEMENTATION_PLAN.md` | `UpdateEffects` slot stays a documented no-op; `ClientHooks` extended with `FxSink` access; `Time`/`Tmp`/`ArcRand` reuse; `RngStream::Fx` never used by view. 05 §7.2 trace golden must be updated only if the no-op set label changes (it does not). | Present |
| `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` | 10's `combat/view.rs` `CombatFx` trait is superseded by `mind-core::fx::sink::FxSink` (17 owns definition; 10 re-exports); all 10 hooks (`effect`, `shake`, `light`, `trail`, `sound`, `sound_loop`, `tether_beam`, `laser`) are implemented by 17's gdext sink; `BulletDrawState`/`LaserDrawState`/`ShieldDrawState` feed 17 draw programs. | Present (§3.14 row) |
| `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` | `WeaponMount` fields read-only; `UnitTypeDraw`/`WeaponDef.parts`; `Fx.trailFade` on unit removal → `TrailChannel` detach; unit `trail`/`parts` fields exposed in draw state. | Present |
| `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | `WeatherStateView` (see §3.12) + `WeatherEntry` runtime; weather sound loop handled by 18. | **Not written**; contract frozen here |
| `14_UI_IMPLEMENTATION_PLAN.md` | No FX/parts UI; only settings rows `effects`, `screenshake`, `showweather` and the `Fx` inspector tab shell. | **Not written**; no dependency |
| `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` | Owns `Layer` file, frame pipeline, draw-pass registration, `LightRenderer`, atlas/texture binding, batching executor, bloom brackets; 17 emits `DrawPrim` programs + registers the FX pass; seeds `render/layer.rs`/`render/draw.rs` if 16 lands after 17. | **Not written**; interface frozen here |
| `18_AUDIO_IMPLEMENTATION_PLAN.md` | `SoundEffect`/`ejectSound`/shoot sounds → audio sink one-way; 17 never plays audio. | Not written |
| `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` | Effects excluded from snapshots/checksums; `dump_effects()` excluded from desync reports. | Not written |
| `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` | Scenario/bench registration names in §7. | Continuous |

---

## 4. Port map

Legend: **owner** = plan that ships the code; 17 = this plan. Paths are relative to `client/rust/`.

| Mindustry source | Target Rust / Godot | Notes |
|---|---|---|
| `entities/Effect.java` | `mind-core/src/fx/def.rs`, `container.rs`, `sink.rs` | All `at` overloads, `create` gating/clip/startDelay, `add`, `render(id,...) → lifetime`, `shake`, `floorDust`, `decal`/`scorch`/`rubble`, `shockwaveDust`, `EffectContainer`. |
| `entities/effect/MultiEffect.java` | `fx/def.rs` `EffectKind::Multi` | Parallel children; `shouldCreate` once. |
| `entities/effect/SeqEffect.java` | `EffectKind::Seq` | Lazy `init` sums lifetimes; render window `life <= child.lifetime + sum`; child layer ignored. |
| `entities/effect/RadialEffect.java` | `EffectKind::Radial` | `rotation_spacing/offset`, `length_offset`, `effect_rotation_offset`, `amount`; uses `Angles::trnsx/y`. |
| `entities/effect/WrapEffect.java` | `EffectKind::Wrap` | Overrides `create` color/rotation; `init` copies child lifetime/clip; `render` empty. |
| `entities/effect/ExplosionEffect.java` | `EffectKind::Explosion` | Wave/smoke/spark params; `scaled(waveLife)`; `randLenVectors` seeds `id`, `id+1`. |
| `entities/effect/ParticleEffect.java` | `EffectKind::Particle` | Region/line particles, interps, spin, widths/heights, casing flip, light; `init` clip extension. |
| `entities/effect/WaveEffect.java` | `EffectKind::Wave` | Poly/circle shockwave, `sides <= 0 → circleVertices`, light with `lightInterp`. |
| `entities/effect/TriangleEffect.java` | `EffectKind::Triangle` | Position/width/height interps, flippable, spin, light. |
| `entities/effect/NoiseEffect.java` | `EffectKind::Noise` + `PrimKind::NoiseLayer` | Multi-layer screen noise; `clip = f32::MAX`; loose texture. |
| `entities/effect/SoundEffect.java` | `EffectKind::Sound` | Calls `FxSink::sound` (18 plays) + child effect; inherits child `startDelay` when unset. |
| `content/Fx.java` (267 fields) | `mind-core/src/fx/catalog.rs` (waves F1–F6) + `fx/custom/*` | Every field mapped 1:1; ledger per entry; no automatic translation. |
| `entities/part/DrawPart.java` | `fx/parts/mod.rs`, `params.rs`, `progress.rs` | `PartParams`, `PartMove`, every `PartProgress` leaf/combinator. |
| `entities/part/RegionPart.java` | `fx/parts/region.rs` | Mirror `-r`/`-l`, outline, heat, children, origin correction, additive heat. |
| `entities/part/ShapePart.java` | `fx/parts/shape.rs` | Poly/circle fill/hollow, rotate speed, lerped radius/stroke. |
| `entities/part/HaloPart.java` | `fx/parts/halo.rs` | `shapes` orbit, tri mode, halo rotate/lerp. |
| `entities/part/HoverPart.java` | `fx/parts/hover.rs` | Pulsing circles, phase/stroke. |
| `entities/part/FlarePart.java` | `fx/parts/flare.rs` | `sides` × 2-color `Drawf.tri` fans, spin. |
| `entities/part/EffectSpawnerPart.java` | `fx/parts/spawner.rs` | Interval/chance spawns, mirror rect, paused guard, debug rect. |
| `entities/pattern/*.java` | **10** `mind-core/src/weapons/pattern.rs` | Firing geometry owned by 10; 17 consumes barrel counters for parts only. |
| `entities/comp/EffectStateComp.java` | `mind-gdext/src/fx/pool.rs` (+ `fx/pool_spec.rs`) | `serialize=false`, `pooled=true`, lifetime/timed semantics, `clipSize`. |
| `entities/comp/DecalComp.java` | `mind-gdext/src/fx/decals.rs` (+ `fx/decal.rs`) | Scorch-layer fade/`mixcol`; floor-surface gate. |
| `type/Weapon.java` (draw/load half) | `fx/parts/weapons.rs` + `mind-gdext/src/fx/exec.rs` | `drawOutline`/`draw`, region/cell/heat/parts/top/flipSprite/layerOffset; behavior half stays 10. |
| `type/UnitType.java` (draw subset: `drawWeapons`, `drawWeaponOutlines`, `parts`, `drawTrail`, `cellColor`, `drawShield` call sites) | `fx/parts/weapons.rs`, `trail.rs`, shield prims | Body/legs/treads/engines/crawl/segment/items/mining beam stay 16/11; `drawShield`/`drawLight` emit prims consumed by 16. |
| `type/weapons/BuildWeapon.java`, `MineWeapon.java`, `RepairBeamWeapon.java` (draw overrides) | `fx/parts/weapons.rs` (`build_beam`, `mine_beam`, `repair_beam`) | Sim halves in 10/11. |
| `world/draw/DrawTurret.java` | `mind-core/src/fx/parts/turret.rs` + `mind-gdext/src/fx/draw_turret.rs` | DrawBlock wrapper registered by 07/16; regions/ammo parts/liquid/top/heat. |
| `graphics/Trail.java` | `mind-core/src/fx/trail.rs` | Pure math + `Polygon` prims; `trailFade` via `EffectData::Trail`. |
| `graphics/Drawf.java` (FX subset: `shadow`, `additive`, `light`, `tri`, `squareShadow`, `spikes`, `laser`, `flame*`) | `fx/parts/*` helpers → `DrawPrim` | Non-FX `Drawf` text/selection stays 16. |
| `graphics/Layer.java` | `mind-core/src/render/layer.rs` | Exact constants; 16 takes file ownership when it lands. |
| `graphics/EnvRenderers.java` (bodies) | `mind-gdext/src/fx/env.rs` | Underwater/scorching visuals; registration API owned by 16. |
| `type/Weather.java` draw statics + `type/weather/ParticleWeather.java`, `RainWeather.java` draw halves | `mind-core/src/fx/weather_fx.rs` + `mind-gdext/src/fx/weather.rs` | Particle/rain/splash/noise builders; state in 12. |
| `graphics/ParticleRenderer.java` | **not ported** (evaluation only) | WIP, unwired upstream; replaced by MultiMesh/GPUParticles2D (§3.14, §8). |
| `core/Renderer.java` (`shake`, `enableEffects`, `weatherAlpha/drawWeather`) | `fx/shake.rs`, settings keys | Camera application by 15/16; settings storage 04, UI 14. |

---

## 5. Milestones & task breakdown

Order is strict; each milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, the named harness command and a Changelog entry with evidence.

**M0 — Vertical slice: one effect end-to-end.**
- `fx/sink.rs` (`FxSink`, `NoopFxSink`, `FxBus`), `render/draw.rs`/`layer.rs` minimal, `fx/def.rs` with `EffectDef`/`EffectKind::{None,Custom}`, one ported effect (`Fx.smoke` or `Fx.hitBulletSmall`) as a custom body; `mind-gdext/src/fx/{pool,program,exec,mod}.rs`; `MindFx` probes; `SimHost` wiring.
- Verify: `cargo test -p mind-core fx::tests::first_effect_program`; headless `run fx_smoke`; MCP: spawn via `MindFx.spawn_effect`, screenshot shows pixels.
- Smallest slice is deliberately one custom effect + one screenshot: proves sink → pool → program → Godot draw before any catalogue work.

**M1 — Effect runtime core.**
- `container.rs` (fin/fout/finpow/fslope/scaled/inner), `data.rs` + `ViewSnapshot`, `sink.rs` full API (`effect/shake/light/decal/trail/sound/laser/tether`), pool lifecycle with delayed spawns, camera clip, `shouldCreate` gating, settings flags.
- Verify: `fx::tests::{container_math, should_create_gates, lifecycle_spawn_expire_order, start_delay_deferred, camera_clip, follow_parent, effects_excluded_from_checksum}`; `mind-headless run fx_lifecycle --seed 5`.

**M2 — Composite + data effects.**
- `EffectKind::{Multi,Seq,Radial,Wrap,Sound,Noise}`, `EffectData` full enum, custom bodies for the ~30 data-carrying effects (F1–F2 subsets), `Expr` no — direct fns; `NoiseLayer` prim; `FxRand`/`angles.rs`.
- Verify: `fx::tests::{multi_parallel, seq_window, radial_offsets, wrap_color_rotation, sound_forwards, noise_params, data_enum_covers_ledger}`; `mind-headless fx program --name <each data effect>`.

**M3 — Catalogue waves F1–F6 + ledger.**
- Port all 267 effects in six waves; run `mind-headless fx audit` after each; `DumpFx.java` golden (`fx_catalog.json`, `fx_order.txt`, `fx_vectors.json`); ledger complete.
- Verify: audit exits 0; `fx::tests::catalog_matches_ledger`, `rand_len_vectors_golden`; per-wave screenshot spot-checks via MCP.

**M4 — DrawPart framework.**
- `parts/{params,progress,region,shape,halo,hover,flare,spawner}.rs`; `ResolvedParts` cache; `getOutlines()` iterator for 03.
- Verify: `parts::tests::{progress_combinators_golden, region_suffix_resolution, mirror_side_override, children_params, effect_spawner_interval, outlines_list}`; `mind-headless fx program --parts <weapon>` golden.

**M5 — Weapon/turret/unit/bullet part rendering.**
- `parts/weapons.rs`, `parts/turret.rs`, `draw_turret.rs`; unit weapon outlines/parts order; bullet parts; visual-only weapon beams; team/cell/outline colors from 16's draw state.
- Verify: `parts::tests::{weapon_pose_recoil_heat, unit_weapon_layer_order, turret_draw_order, ammo_parts_switch, build_mine_repair_beam_geometry}`; MCP part-animation probes (§7c step 7).

**M6 — Trails, decals, shake, weather, env.**
- `trail.rs` + registry/detach, `decals.rs` + cap/fade, `shake.rs`, `weather_fx.rs` + `weather.rs`, `env.rs`; `Fx.trailFade` hand-offs from 10/11.
- Verify: `trail::tests::{update_shorten_vertices_golden, fade_handoff}`, `decals::tests::{floor_gate, fade_and_cap}`, `shake::tests::falloff_and_decay`, `weather::tests::particle_counts}`; MCP weather toggle screenshot.

**M7 — Batching, perf, audit, exit.**
- MultiMesh/GPUParticles2D paths behind thresholds, LOD gating, `fx_bench`, allocation audit, addon evaluation write-up, §7 checklist.
- Verify: budgets §7d recorded; `mind-headless bench fx --states 2000 --assert-alloc 0`; MCP scenario §7c fully green with screenshots in Changelog.

---

## 6. Data & formats

### 6.1 Declarative kind parameters (selected field sets; full fields = Java source-faithful)

- `ParticleParams { color_from, color_to, particles, rand_length, casing_flip, cone, length, base_length, interp, size_interp, color_interp, offset_x, offset_y, light_scl, light_opacity, light_color, spin, size_from, size_to, size_change_start, width_change_start, height_change_start, use_rotation, offset, region, width_from/to, height_from/to, width_interp, height_interp, line, stroke_from/to, len_from/to, cap }` (`ParticleEffect.java`).
- `ExplosionParams { wave_color, smoke_color, spark_color, wave_life, wave_stroke, wave_rad, wave_rad_base, spark_stroke, spark_rad, spark_len, smoke_size, smoke_size_base, smoke_rad, smokes, sparks }` (`ExplosionEffect.java`).
- `WaveParams { color_from, color_to, light_color, size_from/to, light_scl, light_opacity, sides, rotation, stroke_from/to, interp, light_interp, offset_x/y }`.
- `TriangleParams { color_from/to, flippable, interp, width/height/color interps, start/end x/y, light_scl, light_opacity_from/to, light_color, width/height from/to, use_rotation, spin, offset }`.
- `NoiseParams { noise_path, color, noise_scl, opacity, base_speed, intensity, wind_x, wind_y, layers, layer_speed_mul, layer_alpha_mul, layer_scl_mul, layer_color_mul }`.
- `SoundParams { sound: SoundId, min_pitch, max_pitch, min_volume, max_volume, effect: EffectId }`.
- `RadialParams { effect, amount, rotation_spacing, rotation_offset, effect_rotation_offset, length_offset }`; `WrapParams { effect, color, rotation }`.

### 6.2 `DrawPrim` program snapshot (`mind-headless fx program`)

```json
{"format": 1, "tick": 120, "effect": "hitBulletSmall",
 "prims": [{"z": 100.0, "blend": "normal",
            "kind": {"type": "circle", "x": 512.0, "y": 512.0, "r": 3.0, "fill": true, "color": [255,255,255,255]}}],
 "lifetime_override": null, "clip": 50.0}
```

Sorted by `(z, insertion)`; stable across runs for a fixed seed/tick; golden files at `tests/golden/fx_*.json`.

### 6.3 Fx parity ledger (`parity/ledgers/fx.md`)

```markdown
# Ledger — Fx effects (upstream `content/Fx.java`, 267 entries, `Effect.all` order)
| id | name | life | clip | layer | kind | custom_body | data | wave | ported | notes |
|----|------|------|------|-------|------|-------------|------|------|--------|-------|
| 0 | none | 0 | 0 | effect | None | — | — | F1 | [x] | rejected by shouldCreate |
| 1 | blockCrash | 90 | 50 | effect | Custom | BlockCrash | Block | F1 | [x] | 2 rects, e.fin alpha |
| 2 | trailFade | 400 | 50 | effect | Custom | TrailFade | Trail | F1 | [x] | mutates lifetime |
| 3 | unitSpawn | 30 | 50 | effect | Custom | UnitSpawn | UnitType | F1 | [x] | mixcol/scale |
...
- Effects: 267/267 ported (0 unported)
- Declarative kinds: 141 (52.8%)  ·  Custom bodies: 118  ·  Composites: 8
- Data-carrying: 30/30 `EffectData` cases verified (`fx audit --data`)
- Layers: 42 overridden — all values match upstream
- Regions referenced: 214 — all present in `parity/asset_manifest.json` (03 cross-check)
- `fx_order.txt` sha256: <hash>  (must equal DumpFx output)
- Deviations: decal cap, view clock, LOD particle halving (all recorded)
```

`mind-headless fx audit` regenerates the counts + region check and exits non-zero on any missing port, unknown custom id, `Effect.all` order mismatch, missing region, or data-case gap. `parity/java/DumpFx.java` emits `fx_catalog.json` (name/order/lifetime/clip/layer) and `fx_vectors.json` (first 32 `randLenVectors` outputs for seeds 1..16) once from the upstream checkout; it never modifies the upstream repo.

### 6.4 Files & settings

| File | Producer | Committed? |
|---|---|---|
| `parity/fx_catalog.json`, `parity/fx_order.txt`, `parity/fx_vectors.json` | `DumpFx.java` (one-off) | yes |
| `parity/ledgers/fx.md` | hand-maintained + `fx audit` verification | yes |
| `parity/reports/addon_eval.md` | this plan's addon evaluation | yes |
| `tests/golden/fx_*.json` | `mind-headless fx program/lifecycle/trail` | yes |
| `bench/baselines.json` FX entries | plan 23 | yes (23-owned) |

Settings (port of `Renderer.java:171` + `showweather`/`screenshake`): `effects: bool` (default true), `screenshake: int 0..=4` (default 4), `showweather: bool` (default true). Stored by 04; UI rows in 14. Headless ignores all three.

### 6.5 View capacity constants

```rust
pub const EFFECT_POOL_CAPACITY: usize = 4096;
pub const DECAL_CAPACITY: usize = 1024;
pub const DELAYED_SPAWN_CAPACITY: usize = 256;
pub const MULTIMESH_THRESHOLD: u32 = 256;
pub const GPUPARTICLES_THRESHOLD: u32 = 4000;
pub const MAX_DRAW_CALLS_TARGET: u32 = 200;
```
All live in `mind-core::fx::pool_spec`/`render::draw` so `mind-headless` benches and `mind-gdext` share the same numbers.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (no JUnit rendering tests exist — substitutions)

Upstream `tests/src/test/java` has **no** effect/part/trail/weather rendering tests (verified by grep for `Effect|Fx|DrawPart|Trail|weather|part`). The port therefore uses a Java-side golden dumper plus deterministic headless extraction:

| Upstream test / behavior | Rust test (crate `mind-core`) | Notes |
|---|---|---|
| `ApplicationTests.initialization` (content map present) | `fx::tests::catalog_registers_all` | `EffectRegistry::len() == 267` + names unique; `none` id 0. |
| `Effect.EffectContainer.fin/fout/scaled` (Arc `Scaled` semantics) | `fx::tests::container_math` | `scaled` gate, inner reuse, `fin(interp)`/`fout(interp)`/`finpow`/`fslope` exact values. |
| `Effect.shouldCreate` (`Effect.java:141-143`) | `fx::tests::should_create_gates` | headless/none/effects-off rejected; camera-clip culling exact rect test. |
| `EffectState` pooled/timed lifecycle (`EffectStateComp`, `TimedComp`) | `fx::tests::lifecycle_spawn_expire_order` | insertion order preserved; expiry tick `ceil(lifetime)`; render lifetime override honored next tick. |
| `Effect.startDelay` (`Time.run`) | `fx::tests::start_delay_deferred` | spawn tick equals `start_delay`; order stable. |
| `Effect.add` parent binding (`Effect.java:170-173`) | `fx::tests::follow_parent_rot_with_parent` | position/rotation tracking through a fake `ViewSnapshot`. |
| `Angles.randLenVectors` | `fx::tests::rand_len_vectors_golden` | vs `parity/fx_vectors.json` (Arc Rand port from 11 reused). |
| Composite `init()` aggregation | `fx::tests::{multi_parallel, seq_window, radial_offsets, wrap_color_rotation}` | exact Java arithmetic. |
| `NoiseEffect.drawNoiseLayers` parameters | `fx::tests::noise_params` | iteration/offset/color-mult chain. |
| `DrawPart.PartProgress` + `CompatFix` | `parts::tests::progress_combinators_golden` | one assertion per combinator + leaf. |
| `RegionPart.draw` geometry/order | `parts::tests::{region_suffix_resolution, mirror_side_override, origin_correction, children_params, outlines_list}` | draw-program order/positions golden. |
| `EffectSpawnerPart` interval/chance | `parts::tests::effect_spawner_interval` | deterministic view-tick RNG; paused guard. |
| `Trail.update/shorten/draw` (`Trail.java`) | `trail::tests::update_shorten_vertices_golden` | counter/interpolation/trim/quad vertices vs committed trace. |
| `DecalComp.draw` fade + `Effect.decal` gate | `decals::tests::{floor_gate, fade_and_cap}` | `alpha = 1 - curve(fin, 0.98)`; cap drop order. |
| `Renderer.shake` accumulation (`Renderer.java:85-88,210-218`) | `shake::tests::falloff_and_decay` | distance falloff, max-combine, decay per tick, `screenshake` multiplier. |
| `Weather.drawParticles/drawRain/drawSplashes` | `weather::tests::{particle_counts, mod_wrap, splash_gate}` | seed 0 layout; `boundMax=80000`; tile liquid gate. |
| `Weapon.draw` geometry | `parts::tests::{weapon_pose_recoil_heat, flip_sprite_mirror}` | recoil pow, layerOffset, heat alpha. |
| `DrawTurret.draw` order | `parts::tests::{turret_draw_order, ammo_parts_switch}` | base→heat→outline→parts order. |
| — (new) | `fx::tests::effects_excluded_from_checksum` | recording sink changes no checksum; `dump_effects` not in `SimSnapshot`. |
| — (new) | `fx::tests::draw_program_deterministic_hash` | same tick ⇒ same sorted program hash. |

Assertions use exact float comparisons where upstream arithmetic is deterministic; `1e-5` epsilon for trig-derived positions. Tests marked `#[ignore = "plan NN"]` where a sibling plan is required, with the owner recorded for 23.

### 7b. Headless harness scenarios (`mind-headless`, no Godot)

1. `fx audit [--wave F1..F6] [--regions --manifest ...] [--data]`
   - Asserts the 267-entry ledger: names/ids/order match `fx_order.txt`, every entry has a kind/custom body, every custom body has a test, every referenced region exists in the 03 manifest, every `EffectData` case is exercised. Exit code is the gate.
2. `fx lifecycle --seed 5 --ticks 600 --dump out/fx_lifecycle.json`
   - Script: spawn `smoke`(60), `hitBulletSmall`(15) with `startDelay` variants at fixed ticks; assert live list per tick: insertion order, exact spawn/expiry ticks, delayed order, pool reuse without id change; golden JSON checksum.
3. `fx noop-headless --seed 7 --ticks 1200`
   - Runs the same scenario with `NoopFxSink`; asserts zero effect states, zero allocations in `UpdateEffects`, and `checksum == baseline` (effects-excluded proof).
4. `fx program --name <effect> --tick 3 --data <fixture> --out out/fx_program.json`
   - Builds the sorted `DrawPrim` program for any catalogue entry at a fixed time; golden-compares against `tests/golden/fx_program_*.json`; used per catalogue wave in CI (all 267).
5. `fx trail --seed 3 --dump out/fx_trail.json`
   - Deterministic trail update/detach/fade trace; golden vertices + `trailFade` lifetime override.
6. `bench fx --profile {light,mid,stress} --states {300,2000,5000} --frames 3600 --assert-alloc 0 --json out/fx_bench.json`
   - Numbers from §7d; checksum unchanged with/without recording sink.

Scenario registration names (`fx_audit`, `fx_lifecycle`, `fx_noop`, `fx_program`, `fx_trail`, `fx_bench`) are handed to 23.

### 7c. MCP playtest scenario (open-godot-mcp) — turret/impact + fixed-frame screenshots

Preconditions: plan 00 spine (`res://scenes/spine.tscn`, `/root/Spine/SimHost`), plan 07 placement + plan 10 combat turret/dummy with `set_ammo`, milestone M9 of 10 and M2–M5 of this plan; scenario file `res://scenarios/fx_turret_impact.json` (turret at (64,64), dummy at (80,64), 64×64 flat world).

1. `godot_health check` → `{ok:true}`.
2. `godot_game play(scene="res://scenes/spine.tscn", frozen=false)`; pid-stamp via `godot_exec eval {pid, tick}`.
3. `godot_exec call /root/Spine/SimHost load_scenario ["res://scenarios/fx_turret_impact.json"]` → `true`; `set_paused [true]`.
4. Baseline: `godot_exec call /root/Spine/MindFx clear_effects []`; `godot_screenshot game` → `screens/fx_pre.png`; eval `get_node("/root/Spine/MindFx").live_effect_count()` == 0.
5. Fire and step: `set_paused [false]`; `godot_exec call /root/Spine/SimHost step [1]` repeatedly; after each of ticks 1..12 grab `godot_exec eval`:
   ```gdscript
   var fx = get_node("/root/Spine/MindFx")
   return {"tick": get_node("/root/Spine/SimHost").get_tick(),
           "live": fx.live_effect_count(), "kinds": fx.effect_kind_counts(),
           "draws": fx.draw_call_count(), "program": fx.last_program_size()}
   ```
   Assert: at the shot tick `kinds` contains the turret's `shootEffect`/`smokeEffect`/`ejectEffect` and bullet `hitEffect` when it lands; live count rises then returns to 0 within `max(lifetime)` ticks + pool; `draws <= 200`.
6. Screenshot at fixed frames: after tick 2 (muzzle) and tick `hit_tick` (impact) call `godot_screenshot region` over the turret/impact rect (`screens/fx_shot_t2.png`, `screens/fx_hit.png`); pixel-diff each against `screens/fx_pre.png` must show changed non-background pixels inside the rect (Godot `Image` compare via a small GDScript helper or `godot_screenshot burst` + committed reference).
7. Part animation via `godot_exec`: `var fx = get_node("/root/Spine/MindFx"); return fx.sample_part(<turret_unit_id>, 0, 0)` at ticks 1..10; assert `warmup` non-decreasing to 1 while firing, `recoil` spikes to 1 on shot then decays per `recoilTime`, `heat` decays after fire, `charge` rises when `firstShotDelay > 0` (use a `lancer` variant for the charge leg).
8. Burst determinism: `var fx = get_node("/root/Spine/MindFx"); return fx.dump_effects()` at tick `hit_tick`; compare the JSON list (ids/names/positions/life) with `tests/golden/fx_mcp_impact.json` generated by `mind-headless fx lifecycle` for the same seed; positions may use the `ViewSnapshot` freeze rule (no live interpolation in the fixture).
9. Weather pass (M6): `godot_exec call /root/Spine/MindFx spawn_effect ["rain", ...]` is not valid (weather is 12 state); instead `godot_exec call /root/Spine/SimHost set_weather ["rain", 1.0]` (dev probe from 12) and screenshot; assert `live_effect_count` unchanged and particle prim count grows (`MindFx.last_program_size()`), river/rain pixels differ from baseline.
10. Negative: `set_paused [true]`; `step [600]`; `live_effect_count()` unchanged (paused effects do not tick); `set_fx_enabled [false]` clears and rejects new effects.
11. Logs: `godot_log get source=game count=200` → no `panic`/`fx error`; `godot_game stop`; attach screenshot paths + probe JSON to the Changelog.

### 7d. Performance budget & measurement

Baseline from HLP §7.4: 16.6 ms frame at 60 fps. FX is client-only; sim budgets are untouched.

| Profile | Load | Budget (P95, dev machine) |
|---|---|---|
| `light` | 300 live effect states, no weather | program build ≤ 0.25 ms/frame; draw ≤ 0.4 ms GPU-submit; ≤ 40 draw calls |
| `mid` | 2 000 live effect states (mix of particles/explosions/trails), 3 000 live trail points, 256 decals | program build ≤ 1.5 ms/frame; resolve+submit ≤ 3.0 ms/frame; ≤ 200 draw calls |
| `stress` | 5 000 live effect states, 10 000 particle instances, 1 024 decals, weather on | program build ≤ 3.0 ms; resolve+submit ≤ 5.0 ms; ≥ 55 fps sustained; drop-oldest counters increment, no stutter spike > 8 ms |
| Steady-state allocations | any profile, after 600-frame warmup | 0 bytes/frame (alloc-audit), pool growth 0 |
| Pool churn | 1 000 spawn+expire/s | ≤ 2 µs/effect average; no `Vec` reallocation after warmup |

- Measurement: `mind-headless bench fx --states N --frames 3600 --assert-alloc 0 --json` (CPU resolve/program with a fake snapshot + sink), `criterion` bench `cargo bench -p mind-core --bench fx_resolve`, and in-engine `godot_profiler snapshot` + `Performance.get_monitor(TIME_PROCESS)` sampled by `MindFx.draw_call_count()`/`last_program_size()`.
- CI records numbers; 23 turns them into hard gates. A regression over budget blocks the P6 gate.

### 7e. Exit criteria checklist

- [x] `cargo test -p mind-core` green without Godot/network; every §7a row implemented or `#[ignore = "plan NN"]` with an owner recorded for 23.
- [x] `cargo fmt --check` + `cargo clippy -p mind-core -- -D warnings` clean; no `HashMap` iteration in `fx`/`render` resolve paths.
- [x] `mind-headless fx audit` exits 0: 267/267 ledger rows ported, order hash matches the pinned `94f48259c1412fef`, zero unported bodies, zero unknown custom bodies. (Region-manifest and data-case sub-checks remain plan-03/`--regions` follow-ups; `fx_order.txt` was not generated — see ledger Java-oracle note.)
- [ ] §7b scenarios pass with committed goldens; `fx noop-headless` checksum equals the baseline (effects excluded from sim). (`fx lifecycle`/`program`/`trail` goldens pass via `fx_golden`; the dedicated `fx noop-headless` subcommand is still open.)
- [ ] §7c MCP scenario executed end-to-end with screenshots at fixed frames, `sample_part` values matching mount state, `dump_effects` matching the headless golden; logs clean. **DEFERRED — single-editor mutex, editor not running.**
- [ ] §7d budgets measured and recorded (light/mid/stress + alloc 0); draw calls ≤ target. (`fx bench` numbers recorded (debug) incl. `total_lod2_prims` + `backend_runs`; LOD gating lands in `build_program_into_lod`. In-engine profiler + plan-16 executor draw-call gate remain open — MCP mutex.)
- [x] Decal cap, view clock, LOD particle halving and `layerDuration` dead-field handling documented in the ledger/changelog.
- [x] Addon evaluation recorded (`parity/reports/addon_eval.md`): BlastBullets2D MIT, not adopted; sim stays Rust; `THIRD_PARTY_NOTICES.md` untouched unless reversed.
- [ ] Cross-plan reconciliations from §3.16 applied: 02 R6 closed, 10 `CombatFx` re-export switched to `FxSink`, 03 region manifest covers FX regions, 05 trace golden unchanged, 12 weather view interface frozen, 16 `DrawPrim`/`Layer` file ownership note, 21 exclusion documented. (02/05/10 rows done; 03 `--regions`, 16 ownership, 21 exclusion remain owner-side.)
- [ ] Every ported file carries the GPL header; `cargo tree -p mind-core` shows no `godot`/`tokio`; `MindFx` probe API documented in the repo playtest skill. (Headers + boundary verified; playtest-skill probe docs remain.)

---

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Flag |
|---|---|---|---|
| OD-17-A | **Visual parity acceptance.** Arc/Java effect rendering (GL blending, `Rand`, float trig) cannot be pixel-identical in Godot 4.7 across platforms; HLP §9 only mandates Rust↔Rust sim determinism. | Screenshot oracles use tolerance: SSIM ≥ 0.995 and ≤ 0.5% changed pixels in the FX rect; geometry (draw-program values) compared exactly via goldens. | **NEEDS USER DECISION** — if strict pixel parity is required, scope grows substantially (software raster parity harness, ignored today). |
| OD-17-B | **Custom body location / testability.** Bodies can emit `DrawPrim`s from `mind-core` (headless goldens, no arbitrary Godot calls) or be native `mind-gdext` code (max fidelity, untestable headless). | `mind-core` + `DrawPrim` for all bodies; only `NoiseLayer`/`ShaderBlit`/MultiMesh submission live in gdext (their parameters still computed in core). | **NEEDS USER DECISION** — affects how much of the catalogue is covered by headless goldens; default maximizes verifiability. |
| OD-17-C | **BlastBullets2D adoption** (HLP §8). MIT, C++ GDExtension, precompiled; its runtime owns bullet motion/homing/orbit/timers, duplicating 10's sim authority, and couples to a godot-cpp ABI. | **Not adopted.** Bullet sim stays Rust; rendering uses a custom MultiMesh batcher. Evaluation recorded with license (MIT © 2025 nikoladevelops). | OD6 (no user) |
| R-17-1 | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` missing: `Layer`, `DrawPrim`, pass registration, light queue. | Freeze in §3.6; 17 seeds `mind-core/src/render/{layer,draw}.rs` and a minimal `MindFxLayer`; 16 takes file ownership at merge (same pattern as 10's frozen 07 interfaces). | Orchestrator reconcile |
| R-17-2 | `02` R6 `fx_meta` vs full registry. | 17 owns `EffectRegistry`; 02 keeps a seed accessor; names/ids unchanged. | Close 02 R6 in its changelog |
| R-17-3 | `02` port map says weather draw goes to 16; assignment says 17 owns weather particle visuals. | 17 owns draw bodies + prim builders; 16 owns registration/pipeline. Document in both plans. | Orchestrator reconcile |
| R-17-4 | `12`/`14` not written: weather view interface and settings rows. | Contract frozen (§3.12); settings keys stored by 04; 14 only renders rows. | Reconcile at 12/14 write time |
| R-17-5 | Mod effects append to `Effect.all`; numeric ids shift only for later builds. | Ids append-only per build; `Effect.get(id)` valid only within a build; mods reference by name (20); ledger hash pins vanilla order. | Accepted |
| R-17-6 | Live entity referenced by an effect despawns mid-life (e.g. `unitControl` on a dead unit). | `ViewSnapshot` returns the last known pose (frozen) for the remaining lifetime; never panics; test `fx::tests::dead_parent_freezes`. | Accepted |
| R-17-7 | Godot `_draw` per-prim cost at 5 000 effects. | Batching executor + MultiMesh/GPUParticles thresholds (§3.14); if still over budget, lower `EFFECT_POOL_CAPACITY` and LOD counts; bench gate catches it. | Mitigation owned here |
| R-17-8 | Java decals are unbounded; cap changes soak visuals. | Cap 1 024 oldest-drop + counter; documented deviation; revisit only if a parity test fails. | Accepted |
| R-17-9 | `screenshake`/`effects`/`showweather` settings keys could drift from 04/14 naming. | Keys pinned here: `effects`, `screenshake`, `showweather` (upstream names). | Reconcile with 04/14 |
| R-17-10 | `sprites/rays.png`/`distortAlpha.png`/weather noise are loose textures, not atlas regions. | 03 exposes loose-texture lookup (already in its scope); `fx audit --regions` checks both namespaces. | Reconcile with 03 |
| R-17-11 | `SeqEffect`/`ParticleEffect`/`TriangleEffect`/`NoiseEffect`/`SoundEffect` are not used in vanilla `Fx.java` (grep: only `MultiEffect` 36, `ExplosionEffect` 10, `WaveEffect` 13, `RadialEffect` 4, `WrapEffect` 6 call sites across core), so declarative coverage depends on mods/other files. | Port all classes anyway (HLP D3 full parity); they are exercised by mods (20) and by tests; catalogue classification still uses them where behavior matches. | Accepted |
| R-17-12 | `EffectSpawnerPart` runs inside draw and uses `Vars.state.isPaused()`; view tick timing could diverge from sim pause. | The viewer ticks only when `Sim::tick()` runs; pause therefore pauses part spawners exactly like upstream. | Accepted |

## 9. References

### Mindustry sources read

- `core/src/mindustry/entities/AGENTS.md` (full)
- `core/src/mindustry/content/AGENTS.md` (full)
- `core/src/mindustry/graphics/AGENTS.md` (full)
- `core/src/mindustry/type/AGENTS.md` (full)
- `core/src/mindustry/entities/Effect.java` (full)
- `core/src/mindustry/entities/effect/{MultiEffect,SeqEffect,RadialEffect,WrapEffect,ExplosionEffect,ParticleEffect,WaveEffect,TriangleEffect,NoiseEffect,SoundEffect}.java` (all)
- `core/src/mindustry/content/Fx.java` (audited: 267 fields; representative bodies + statistical scan)
- `core/src/mindustry/entities/part/{DrawPart,RegionPart,ShapePart,HaloPart,HoverPart,FlarePart,EffectSpawnerPart}.java` (all)
- `core/src/mindustry/entities/pattern/*.java` (skimmed; owned by plan 10)
- `core/src/mindustry/entities/comp/{EffectStateComp,DecalComp}.java`, `entities/GroupDefs.java`
- `core/src/mindustry/type/Weapon.java` (draw/load half), `type/UnitType.java` (`draw` region), `type/weapons/{BuildWeapon,MineWeapon,RepairBeamWeapon}.java`
- `core/src/mindustry/world/draw/DrawTurret.java`, `world/blocks/defense/turrets/Turret.java` (draw hooks)
- `core/src/mindustry/graphics/{Trail,Drawf,Layer,ParticleRenderer,EnvRenderers}.java`
- `core/src/mindustry/type/Weather.java`, `type/weather/{ParticleWeather,RainWeather}.java`
- `core/src/mindustry/core/Renderer.java` (`shake`, `enableEffects`, `weatherAlpha`)
- `tests/src/test/java/**` (grepped: no rendering/effect tests)

### Project plans read

- `HIGH_LEVEL_PLAN.md` (§0, §2, §3 plan-17 row, §4, §6–§9)
- `PRELIMINARY_PLAN.md`
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.5 spine node map, §7c MCP recipe)
- `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `05_SIM_CORE_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`
- Addon: `godot-blast-bullets-2d/` (`LICENSE.md` MIT, `README.md`, `blastbullets2d.gdextension`, `src/`, precompiled `bin/`), `example-project-blastbullets2d/`

## Changelog

- 2026-10-03 — **M7 completion — LOD gating, backend histogram, audit/exit (lane/17-fx).**
  - `mind-core::fx::resolve`: new `build_program_into_lod(def, state, snapshot, program, l2)` threads the LOD flag through `render_def`/`seq`/`particle`/`explosion`; at `l2`, `Particle`/`Explosion` counts go through `batch::lod_particle_count` (halve, floor 1) — quality-only, no gameplay effect. `build_program_into` delegates with `l2=false` (ABI unchanged). Test `lod_halves_particle_prims` (8 particles → 16 prims full / 8 prims L2).
  - `mind-headless fx bench` now reports `total_lod2_prims` and a `backend_runs` histogram (`single`/`multimesh`/`gpu_particles`) from `batching_runs`; non-region prims count as immediate `Single` draws. Recorded (debug, 2000 states × 20 frames): 297 660 prims, ~6.3 ms/frame, `backend_runs.single 297660`; vanilla catalogue bodies are all custom (no declarative particle clusters), so LOD2 equals full for this synthetic set — expected and documented.
  - Addon evaluation confirmed present (`parity/reports/addon_eval.md`: BlastBullets2D MIT, not adopted). §7e checklist re-audited; remaining open items are owner-side (plan-03 `--regions`, plan-16 executor draw-call gate, plan-21 exclusion) or the single-editor-mutex MCP run.
  - **Evidence:** `cargo test -p mind-core` **1256 lib** + integrations / 2 ignored; `cargo test -p mind-headless` **73 lib** + goldens green; `cargo check -p mind-gdext` clean; workspace clippy `-D warnings` + `cargo fmt` clean; `fx audit` 267/267, `order_hash 94f48259c1412fef`.

- 2026-10-03 — **M6 completion — weather splashes/noise, env bodies, per-channel trail colors (lane/17-fx).**
  - `mind-core::fx::weather_fx`: added `SplashGround` + `WeatherFx::build_splashes` (exact `Weather.drawSplashes`: wrapped field, `life*4` overlap gate, liquid frame `mul(1.5).a(opacity)`, clear-floor `slope(life)` spark lines, solid/null skip) and `WeatherFx::build_noise_layers` (`NoiseEffect.drawNoiseLayers` layer chain with speed/alpha/scale/color multipliers). New tests `noise_layers_emit_one_prim_per_layer`, `splashes_branch_on_ground`.
  - `mind-core::fx::env_fx` (new): `underwater_prims` (water tint `Layer::light+1` + `caustics` `ShaderBlit` + 50 `rays` quads `Layer::light+2` + suspended `particle` field `Layer::weather`, exact `EnvRenderers` math) and `scorching_prims` (`distortAlpha` 4-layer scarlet noise at `Layer::weather-1` / `fogOfWar+1`). New tests `underwater_emits_tint_caustics_rays_and_particles`, `scorching_emits_four_noise_layers_at_fog_or_weather`.
  - `mind-gdext/src/fx/env.rs` (new): `EnvFxInput` + `build_env_prims` selecting through the plan-16 `EnvRegistry`; `MindFx` `set_env`/`clear_env`/`env_active` probes and `_draw` append.
  - `TrailRegistry`: per-channel ribbon tint (`create_colored`/`color`/`iter_live_colored`); `MindFx` now tints trail ribbons/caps instead of hard-coded white. Test `channels_carry_ribbon_color`.
  - **Evidence:** `cargo test -p mind-core --lib fx::` **89 passed**; `cargo test -p mind-core` **1255 lib** + integrations / 2 ignored; `cargo test -p mind-headless` **73 lib** + goldens green; `cargo check -p mind-gdext` clean; workspace clippy `-D warnings` + `cargo fmt` clean; `fx audit` 267/267, `order_hash 94f48259c1412fef`. **Deferred / reconciliation:** the frozen plan-10 `FxSink::trail(x,y,rotation,color,width,length)` carries no channel id, so `FxEvent::Trail` still does not feed `TrailRegistry` and the `basic::trail_fade` body cannot reach the gdext-owned registry — needs a one-field `trail(id, …)` addition to the plan-10 sink (shared-file, orchestrator-owned); in-engine MCP weather/env screenshots remain deferred to the single-editor mutex.

- 2026-10-03 — **M5 completion — gdext turret adapter + unit weapon layer order (lane/17-fx).**
  - `mind-core::fx::parts::weapons`: added `UnitWeapon` + `cell_color` (exact `UnitType.cellColor` `black.lerp(team, f + absin(time, max(f*5,1), 1-f))`), rewrote `draw_weapon_outlines` to `UnitType.drawWeaponOutlines` semantics (per-weapon `z + layerOffset`, `applyColor`/`applyOutlineColor` tint), and added `draw_unit_weapons` (`UnitType.drawWeapons`: apply body color once, then `Weapon.draw` per mount). `PartEmit::set_z` added so non-core callers can push a band layer. New tests `unit_weapon_layer_order_outlines_before_body_weapons_after`, `cell_color_matches_java`, `bullet_parts_drive_from_life_fin` → `fx::parts` **27 passed**.
  - `mind-gdext/src/fx/draw_turret.rs` (new): `TurretDrawInput` (owned snapshot of `TurretDraw` + plan-10 `TurretDrawState`), `as_draw()`, `part_params()`, and `build_turret` (appends the exact `DrawTurret.draw` prims). `MindFx` now owns `turrets: Vec<TurretDrawInput>`, renders them each `_draw` at `Layer::Block` via `AllRegions`, and exposes `spawn_turret_draw`/`clear_turrets`/`live_turret_count`; `sample_part` returns real `PartParams` (`warmup`/`reload`/`smooth_reload`/`heat`/`recoil`/`charge`, `status: ok|missing`) instead of the placeholder.
  - **Evidence:** `cargo test -p mind-core` **1250 lib** (+ integrations) / 2 ignored; `cargo test -p mind-headless` green; `cargo check -p mind-gdext` clean; workspace clippy `-D warnings` + `cargo fmt` clean; `fx audit` 267/267, `order_hash 94f48259c1412fef`. **Deferred (unchanged):** region/atlas execution (03/16), live plan-10 push into `MindFx` (MCP mutex).

- 2026-10-01 — Draft v1 written (this file). No milestones started. Open decisions OD-17-A (visual parity tolerance) and OD-17-B (custom-body location) require user input before M3/M4 respectively; R-17-1/2/3 need orchestrator reconciliation with 16/02/12.

- 2026-10-03 — **M6 — gdext trail/decal/weather wiring + M7 — batching/alloc/budgets (lane/17-fx).**
  - **M6:** `mind-gdext::fx::MindFx` now appends decals (Scorch layer, `1 - curve(fin,0.98)` fade) and trail ribbons/caps to the frame program (`TrailRegistry::iter_live` added additively), and draws a weather field via the plan-12 `WeatherStateView` (`set_weather`/`clear_weather`/`weather_active` probes; `ParticleWeather`/`RainWeather` dispatch through `WeatherFx::build_particles`/`build_rain`). `MindSimHost` caches the `../MindFx` sibling at boot and calls `tick_view()` once per fixed sim tick — the one-way sim→view hook (no new sim logic; the sim never reads the view). In-engine run recorded **DEFERRED** (single-editor mutex).
  - **M7:** new `mind-core::fx::batch` — `batching_runs`/`choose_backend` (`Single`/`MultiMesh` ≥256/`GpuParticles` ≥4000) and `draw_call_count` (adjacent same-`(z,blend,region)` merge); `lod_particle_count` (L2 halves, floor 1); `program_reuse_is_allocation_free_after_warmup` proves 2000-state frames reuse one `DrawProgram` buffer with zero capacity growth. `mind-headless fx bench` now accumulates a full sorted frame program and reports `draw_calls`/`draw_calls_ok`/`draw_call_target`. Addon evaluation unchanged (BlastBullets2D MIT, not adopted). Recorded debug numbers: 2000 states → ~3.2 ms/frame program build (debug), 297 660 prims/20 frames; the primitive-level draw-call estimate exceeds the 200 target until plan 16's vertex/MultiMesh executor lands (recorded, not a gate here).
  - **Evidence:** `cargo test -p mind-core --lib fx::` **77 passed**; `cargo check -p mind-gdext` clean; workspace clippy `-D warnings` + fmt clean; `fx bench --states 2000 --frames 20` emits the new fields.

- 2026-10-03 — **M5 — weapon/turret/bullet part geometry (lane/17-fx).** New `fx::parts::{weapons,turret}`: `weapon_pose` ports `Weapon.draw` (`rotation`/`baseRotation`/`pow(recoil,recoilPow)*recoil`/mount offsets), `draw_weapon` ports the exact `Weapon.draw` order (shadow `circle-shadow`, `top` outline, `under` parts → region → `cell` → additive `heat` → non-under parts, `flipSprite` mirror), `draw_weapon_outline`, `part_recoil`, `draw_weapon_outlines` (unit body order), `draw_bullet_parts` (`life = fin`). `turret::draw_turret` ports `DrawTurret.draw` order (base → shadow at `turretLayer-0.5` → region/liquid/top at `turretLayer` → additive heat at `turretLayer` → outline at `-0.01` → `parts` + selected `ammoParts`), `draw_turret_plan`, `TurretDraw` input struct. `weapons::beams` ports `BuildWeapon`/`MineWeapon`/`RepairBeamWeapon` geometry (line + target square + pulse circle). Tests: `weapon_pose_recoil_and_rotation`, `draw_weapon_emits_region_cell_heat_in_order`, `under_parts_draw_before_region_and_others_after`, `beams_emit_lines_and_pulse`, `turret_draw_order`, `ammo_parts_switch_adds_prims`, `plan_draw_uses_base_preview_top` (+ existing). Evidence: `cargo test -p mind-core --lib fx::parts` **24 passed**; workspace clippy `-D warnings` + fmt clean. **Deferred:** gdext `draw_turret.rs` adapter pulling live plan-10 turret state (M6), region/atlas execution (03/16).

- 2026-10-03 — **M4 complete — DrawPart draw emission (lane/17-fx).** New `mind-core::fx::parts::draw` compiles plan-02 part data (`content::registries::units::parts::DrawPartSpec`) into `DrawPrim`s 1:1 from `entities/part/*`: `RegionPart.draw` (layer/`layerOffset`, `under`/`turretShading`, progress + `growProgress` + `moves`, mirror `-r`/`-l`, outline at `outlineLayerOffset`, color/`colorTo` lerp, `mixcol`, blending, additive heat at `turretHeat`/`heatLayerOffset`, heat light, origin correction, recursive children with child `PartParams`), `ShapePart.draw` (fill/hollow poly/circle, `rotateSpeed`, lerp radius/stroke), `HoverPart.draw` (pulsing circle strokes), `FlarePart.draw` (2× `sides` tri fans). `HaloPartSpec`/`EffectSpawnerPartSpec` rewritten to mirror Java exactly; `draw_halo` + `draw_spawner` (interval `effectIntervalState`, `chanceDelta`, mirror rect, paused guard, view-seeded `ArcRand`). Region resolution via `RegionNames`/`intern_region` (mirror `-r`/`-l`/`-outline`/`-heat`/`-light`) + `RegionLookup` (`AllRegions`/`MapRegions`); `get_outlines()` walks children for plan 03. Progress converter maps every plan-02 `PartProgressSpec` (+ `InterpKind`, `AbsinTime`) to the evaluator tree. **Tests:** 10 new in `fx::parts::draw` (suffix/mirror/outline resolution, side override, shape/hover/flare geometry, children, outlines, progress conversion, spawner cadence) + existing 6 parts tests. **Additive shared-file edits:** `DrawPartSpec` gained `origin_x/origin_y/clamp_progress/replace_outline/turret_heat_layer/outline_layer_offset/heat_light_opacity` (all defaulted; no constructor sites affected). Evidence: `cargo test -p mind-core --lib fx::parts` **16 passed**; full `mind-core` suite green; workspace clippy `-D warnings` + fmt clean.

- 2026-10-03 — **M3 wave F3/F4 batch (lane/17-fx, from `main` @ d9db1a9).** Ported **56** more `Fx.java` bodies 1:1 (34 → **90 custom**; unported 232 → **176**): `commandSend`, `upgradeCoreBloom`, `coreLaunchConstruct`, the smoke/dust/pickup family (`fallSmoke`…`pickup`), `sparkExplosion` + `titanExplosion*`, `coreExplosion`, `smokeAoeCloud`, `scatheExplosion*`/`scatheSlash`/`scatheLight*`, `dynamicSpikes`/`greenBomb`/`greenLaserCharge*`/`greenCloud`, the wave family (`healWaveDynamic`/`heal`/`dynamicWave`/`shieldWave`/`shieldApply`), the `hit*` family (`hitSquaresColor`…`hitMeltHeal`), and `instBomb`/`instTrail`/`instShoot`/`instHit`. Added exact Arc primitives `Lines.lineAngle(offset)` + `Lines.spikes` (`fx::custom::emit`) and `Scaled.foutpow()` (`fx::container`). New determinism test `fx::resolve::tests::every_ported_custom_body_emits_deterministically` (≥80 bodies); ledger counts updated. Evidence: `fx audit --json` → `custom: 90, unported: 176, order_hash 94f48259c1412fef`; `cargo test -p mind-core --lib fx::` **53 passed**; fmt + clippy clean.

- 2026-10-03 — **M0–M2 complete + M4 partial (lane/17-fx, from `main` @ 377451a).** Landed the Godot-free FX core and the single playback seam per HLP §12 C6:
  - **Core (`mind-core`):** new `render::draw` (`DrawPrim`/`DrawProgram`/`Blending`/`RegionKey`/`TextureKey`/`ShaderKey`, stable `(z, seq)` sort, fnv1a program hash); `math::interp` (full `Interp` + `curve`/`slope`); `fx::{def,container,data,sink,pool,pool_spec,resolve,catalog,custom,angles,decal,shake,trail,weather_fx,parts}`. `FxSink` is re-exported from plan-10 `combat::view` (already named `FxSink`; no edit needed) and plan 17 owns the concrete `FxBus`/`NoopFxSink`/`FxSettings` impl. Effect registry builds all **267** entries from the plan-02 seed table (`none` id 0); composites expand at spawn (Multi/Radial/Wrap/Sound), `Seq` renders inline; `trailFade` uses `DrawProgram::lifetime_override`. Container `fin/fout/finpow/fslope/scaled` + `fout(margin)` match `Scaled` exactly. Pool is insertion-ordered, fixed-capacity, O(1), alloc-free after warmup, with `ChildComp` parent follow + `EffectData` freeze rule (R-17-6). Decal cap + fade, shake falloff/decay, trail channel registry/detach, weather particle/rain builders, and `PartParams`/`PartMove`/`PartProgressSpec` (every `CompatFix` combinator) are implemented and tested.
  - **Catalogue:** 34/267 effects ported as custom bodies (F1/F2 subset: `blockCrash`…`explosion`, `hit*`, `shootSmall/Big`, `casing1`, `smoke`, `shockwave`, `healWave`, `hitLaser`); remaining 232 are `EffectKind::Unported`, tracked by `parity/ledgers/fx.md` and rejected by `fx audit --strict`. **Upstream note:** all 267 `Fx.java` fields are plain `new Effect(...)` (grep for `new ExplosionEffect|MultiEffect|WaveEffect|RadialEffect|WrapEffect` = 0), so declarative/composite kinds exist for mods/inline effects and are covered by unit tests, not the vanilla catalogue (R-17-11 confirmed).
  - **Headless:** `mind-headless fx {audit,lifecycle,program,trail,bench,smoke}` + committed goldens `tests/golden/fx/{fx_lifecycle,fx_program_hitBulletSmall,fx_program_explosion,fx_trail}.json` + `tests/fx_golden.rs`.
  - **GDExt:** `mind-gdext::fx::MindFx` (`/root/Spine/MindFx`, `Node2D`) drains `FxBus` after each sim tick, advances the pools, compiles/sorts/executes `DrawPrim`s in `_draw`, and exposes the MCP probes (`live_effect_count`/`effect_kind_counts`/`live_decal_count`/`live_trail_count`/`draw_call_count`/`last_program_size`/`dump_effects`/`spawn_effect`/`clear_effects`/`set_fx_enabled`/`deferred_prim_count`/`sample_part`). Region binding + lights + noise/shader prims are deferred to plan 03/16 (debug quads in the interim).
  - **Evidence:** `cargo test -p mind-core --lib fx::` **52 passed**; `cargo test -p mind-headless` green incl. `fx_golden` (2 tests); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `cargo check -p mind-gdext` clean; `fx audit --json` → 267 entries, order hash `94f48259c1412fef`, custom 34 / unported 232.
  - **Deferred (owners):** M3 waves F3–F6 (this plan); M4 region/shape/halo/hover/flare/spawner draw emission + `getOutlines` + weapons/turret geometry M5 (this plan); M6 trail/decal/shake/weather MCP wiring (this plan); M7 batching/perf/alloc-audit (this plan); §7c MCP scenario (single-editor mutex, orchestrator); region/light execution (plan 16); loose-texture + atlas lookup (plan 03); `SoundEffect` playback (plan 18).
  - **Shared-file edits (minimal, additive):** `mind-core/src/lib.rs`, `mind-core/src/math/mod.rs`, `mind-core/src/render/mod.rs`, `mind-gdext/src/lib.rs`, `mind-headless/src/{cli,exec,lib}.rs`, `client/scenes/spine.tscn`. No changes to plan-10/11/16 files.

- 2026-10-03 — **M3 complete — catalogue F5–F6: 267/267 ported (lane/17-fx, from `main` @ `8895c13`).**
  Ported the remaining **176** `content/Fx.java` bodies 1:1 in new
  `mind-core/src/fx/custom/wave_f56.rs`, dispatched by effect name from
  `CustomFxId::Catalogue` (the registry keeps each effect's id/name/order ABI;
  `order_hash` unchanged at `94f48259c1412fef`). `catalog.rs` now maps every
  non-`none` entry to `Custom` — **0 `Unported`**. New/updated tests:
  `fx::catalog::tests::catalog_matches_ledger`,
  `fx::angles::tests::rand_len_vectors_golden`,
  `fx::custom::wave_f56::tests::every_wave_body_dispatches_without_panic`; the
  existing `fx::resolve::tests::every_ported_custom_body_emits_deterministically`
  now covers 266 bodies. `parity/ledgers/fx.md` rewritten to 267/267 with the
  F5–F6 batch table. **Java oracle:** `parity/java/DumpFx.java` was not available
  in this environment, so `fx_catalog.json`/`fx_order.txt`/`fx_vectors.json` were
  **not** generated; the ledger is source-derived from the upstream `Fx.java`
  field audit and `rand_len_vectors_golden` pins the Rust Arc `Rand` sequence
  (documented in the ledger).
  **Evidence:** `cargo run -q -p mind-headless -- fx audit --json` →
  `custom 266, unported 0, order_hash 94f48259c1412fef, pass true`; `fx audit
  --wave F5`/`F6` → `wave_unported 0`; `cargo test -p mind-core` **1198 passed /
  2 ignored** (lib) + integration (`blocks_golden` 3, `combat_golden` 5,
  `sim_core_determinism` 2, `sim_core_meta` 2, `sim_core_schedule` 1); `cargo
  test -p mind-headless` **73 passed** incl. `fx_golden`; workspace clippy
  `-D warnings` + `cargo fmt --check` clean.
  **Deferred / non-gating (OD-17-A):** §7c in-engine MCP (single-editor mutex;
  editor not running), plan-16 draw-call executor, plan-03 atlas/loose-texture
  binding, plan-18 sound playback.

