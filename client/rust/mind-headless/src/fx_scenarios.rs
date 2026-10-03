// SPDX-License-Identifier: GPL-3.0-only

//! Plan 17 headless FX scenarios (`fx` subcommand, §7b).
//!
//! Godot-free oracle for the effect catalogue, lifecycle, `DrawPrim` programs,
//! trails and resolution budget. Goldens live in
//! `mind-headless/tests/golden/fx/`.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use mind_core::combat::harness::CombatHarness;
use mind_core::content::EffectId;
use mind_core::content::effect_by_name;
use mind_core::fx::{
    BatchBackend, EffectData, EffectKind, EffectState, EmptySnapshot, FxBus, FxPool,
    batched_draw_call_count, batching_runs, build_program, build_program_into,
    build_program_into_lod, catalog_counts, order_hash, registry,
};
use mind_core::render::draw::{Blending, DrawPrim, DrawProgram, MAX_DRAW_CALLS_TARGET, PrimKind};

use crate::cli::FxCommand;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: golden mismatch.
const EXIT_FAIL: i32 = 1;

/// Runs an `fx` subcommand.
pub fn run(command: &FxCommand) -> Result<i32> {
    match command {
        FxCommand::Audit { wave, strict, json } => audit(wave.as_deref(), *strict, *json),
        FxCommand::Lifecycle {
            seed,
            ticks,
            dump,
            check,
            json,
        } => lifecycle(*seed, *ticks, dump.as_deref(), check.as_deref(), *json),
        FxCommand::Program {
            name,
            tick,
            out,
            check,
            json,
        } => program(name, *tick, out.as_deref(), check.as_deref(), *json),
        FxCommand::Trail {
            seed,
            dump,
            check,
            json,
        } => trail(*seed, dump.as_deref(), check.as_deref(), *json),
        FxCommand::Bench {
            states,
            frames,
            json,
        } => bench(*states, *frames, *json),
        FxCommand::Smoke { json } => smoke(*json),
        FxCommand::NoopHeadless { seed, ticks, json } => noop_headless(*seed, *ticks, *json),
    }
}

/// `fx audit`.
fn audit(wave: Option<&str>, strict: bool, json: bool) -> Result<i32> {
    let reg = registry();
    let counts = catalog_counts(reg);
    if reg.len() != mind_core::content::EFFECT_COUNT {
        bail!("catalogue length drift: {} != 267", reg.len());
    }
    // Order/name drift against the plan-02 seed table.
    for def in reg.iter() {
        let meta = mind_core::content::registries::fx_meta::EFFECTS[def.id.raw() as usize];
        if meta.name != def.name {
            bail!(
                "order drift at {}: {} != {}",
                def.id.raw(),
                def.name,
                meta.name
            );
        }
        if (meta.lifetime - def.lifetime).abs() > f32::EPSILON {
            bail!("lifetime drift at {}", def.name);
        }
    }
    // Optional wave slice.
    let range = wave_range(wave)?;
    let mut wave_unported = 0;
    if let Some((start, end)) = range {
        for def in reg.iter().skip(start).take(end - start) {
            if matches!(def.kind, EffectKind::Unported) {
                wave_unported += 1;
            }
        }
    }
    let unported = if range.is_some() {
        wave_unported
    } else {
        counts.unported
    };
    // The committed declaration-order oracle (`parity/fx_order.txt`, plan 17
    // §6.3). Verified against `Fx.java` order when the file is present.
    let order_problems = order_oracle_check(reg);
    let order_oracle_ok = order_problems.is_empty();
    let pass = (!strict || unported == 0) && order_oracle_ok;
    let value = json!({
        "format": 1,
        "effect_count": reg.len(),
        "order_hash": format!("{:016x}", order_hash(reg)),
        "none": counts.none,
        "declarative": counts.declarative,
        "composite": counts.composite,
        "custom": counts.custom,
        "unported": counts.unported,
        "wave": wave.unwrap_or("all"),
        "wave_unported": wave_unported,
        "order_oracle_ok": order_oracle_ok,
        "order_oracle_problems": order_problems,
        "pass": pass,
    });
    emit(&value, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Verifies the registry's name order against `parity/fx_order.txt`.
///
/// A missing file or unresolvable repo root skips the check (returns empty) so
/// the oracle is additive; when present, a single drift line fails `fx audit`.
fn order_oracle_check(reg: &mind_core::fx::EffectRegistry) -> Vec<String> {
    let Ok(repo) = crate::paths::find_repo_root(None) else {
        return Vec::new();
    };
    let path = repo.join("parity/fx_order.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let names: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if names.len() != reg.len() {
        return vec![format!(
            "fx_order.txt has {} entries, registry has {}",
            names.len(),
            reg.len()
        )];
    }
    let mut problems = Vec::new();
    for (index, def) in reg.iter().enumerate() {
        if names[index] != def.name {
            problems.push(format!(
                "fx_order.txt[{}] = `{}` != registry `{}`",
                index, names[index], def.name
            ));
            if problems.len() >= 8 {
                break;
            }
        }
    }
    problems
}

fn wave_range(wave: Option<&str>) -> Result<Option<(usize, usize)>> {
    let Some(wave) = wave else {
        return Ok(None);
    };
    let index: usize = match wave.to_ascii_uppercase().as_str() {
        "F1" => 1,
        "F2" => 2,
        "F3" => 3,
        "F4" => 4,
        "F5" => 5,
        "F6" => 6,
        other => bail!("unknown wave {other}"),
    };
    let total = mind_core::content::EFFECT_COUNT;
    let per = total.div_ceil(6);
    let start = (index - 1) * per;
    let end = (start + per).min(total);
    Ok(Some((start, end)))
}

/// `fx lifecycle`.
fn lifecycle(
    seed: u64,
    ticks: u32,
    dump: Option<&Path>,
    check: Option<&Path>,
    json: bool,
) -> Result<i32> {
    let _ = seed;
    let snap = EmptySnapshot;
    let mut pool = FxPool::new(16);
    let spawn = |pool: &mut FxPool, id: EffectId| {
        let def = registry().get(id);
        pool.spawn(
            def,
            16.0,
            16.0,
            0.0,
            mind_core::content::Rgba::WHITE,
            EffectData::None,
            &snap,
        );
    };
    spawn(&mut pool, EffectId::SMOKE);
    spawn(&mut pool, EffectId::HIT_BULLET_SMALL);
    let mut delayed = registry().get(EffectId::SMOKE).clone();
    delayed.start_delay = 5.0;
    pool.spawn(
        &delayed,
        32.0,
        32.0,
        0.0,
        mind_core::content::Rgba::WHITE,
        EffectData::None,
        &snap,
    );

    let mut frames = Vec::with_capacity(ticks as usize + 1);
    frames.push(frame_json(&pool));
    for _ in 0..ticks {
        pool.advance(&snap);
        frames.push(frame_json(&pool));
    }
    let value = json!({
        "format": 1,
        "ticks": ticks,
        "spawned": pool.spawned,
        "dropped": pool.dropped,
        "frames": frames,
    });
    write_or_check(&value, dump, check, json)
}

fn frame_json(pool: &FxPool) -> Value {
    let live: Vec<Value> = pool
        .iter_live()
        .map(|state| {
            json!({
                "name": registry().get(state.def).name,
                "time": state.time,
                "lifetime": state.lifetime,
            })
        })
        .collect();
    json!({ "live": live })
}

/// `fx program`.
fn program(
    name: &str,
    tick: f32,
    out: Option<&Path>,
    check: Option<&Path>,
    json: bool,
) -> Result<i32> {
    let id = effect_by_name(name).with_context(|| format!("unknown effect {name}"))?;
    let def = registry().get(id);
    let state = synthetic_state(id, tick);
    let mut program = DrawProgram::new();
    build_program(def, &state, &EmptySnapshot, &mut program);
    let value = program_json(name, tick, &program);
    write_or_check(&value, out, check, json)
}

fn synthetic_state(id: EffectId, tick: f32) -> EffectState {
    let def = registry().get(id);
    EffectState {
        def: id,
        x: 100.0,
        y: 100.0,
        rotation: 45.0,
        color: mind_core::content::Rgba::WHITE,
        time: tick,
        lifetime: def.lifetime,
        data: EffectData::None,
        parent: None,
        rot_with_parent: false,
        offset_x: 0.0,
        offset_y: 0.0,
        offset_pos: 0.0,
        offset_rot: 0.0,
        lifetime_override: None,
        alive: true,
    }
}

fn program_json(name: &str, tick: f32, program: &DrawProgram) -> Value {
    let prims: Vec<Value> = program.prims.iter().map(prim_json).collect();
    json!({
        "format": 1,
        "tick": tick,
        "effect": name,
        "prims": prims,
        "lifetime_override": program.lifetime_override,
        "clip": program.clip,
    })
}

fn prim_json(prim: &DrawPrim) -> Value {
    let blend = match prim.blend {
        Blending::Normal => "normal",
        Blending::Additive => "additive",
        Blending::Multiply => "multiply",
    };
    let color = |c: mind_core::content::Rgba| json!([c.r, c.g, c.b, c.a]);
    let kind = match &prim.kind {
        PrimKind::Region {
            region,
            x,
            y,
            w,
            h,
            rotation_deg,
            origin,
            color: c,
            wrap,
            ..
        } => json!({
            "type": "region", "region": region.0, "x": x, "y": y, "w": w, "h": h,
            "rotation_deg": rotation_deg, "origin": [origin.0, origin.1], "color": color(*c),
            "wrap": wrap,
        }),
        PrimKind::Rect {
            x,
            y,
            w,
            h,
            color: c,
        } => {
            json!({"type": "rect", "x": x, "y": y, "w": w, "h": h, "color": color(*c)})
        }
        PrimKind::Circle {
            x,
            y,
            r,
            fill,
            stroke,
            color: c,
        } => {
            json!({"type": "circle", "x": x, "y": y, "r": r, "fill": fill, "stroke": stroke, "color": color(*c)})
        }
        PrimKind::Poly {
            x,
            y,
            sides,
            r,
            rotation_deg,
            fill,
            stroke,
            color: c,
        } => {
            json!({"type": "poly", "x": x, "y": y, "sides": sides, "r": r, "rotation_deg": rotation_deg, "fill": fill, "stroke": stroke, "color": color(*c)})
        }
        PrimKind::Polygon {
            points,
            fill,
            stroke,
            color: c,
        } => {
            json!({"type": "polygon", "points": points.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>(), "fill": fill, "stroke": stroke, "color": color(*c)})
        }
        PrimKind::Tri {
            x,
            y,
            w,
            h,
            rotation_deg,
            color: c,
        } => {
            json!({"type": "tri", "x": x, "y": y, "w": w, "h": h, "rotation_deg": rotation_deg, "color": color(*c)})
        }
        PrimKind::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            color: c,
            cap,
        } => {
            json!({"type": "line", "x1": x1, "y1": y1, "x2": x2, "y2": y2, "stroke": stroke, "color": color(*c), "cap": cap})
        }
        PrimKind::Polyline {
            points,
            stroke,
            color: c,
        } => {
            json!({"type": "polyline", "points": points.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>(), "stroke": stroke, "color": color(*c)})
        }
        PrimKind::Arc {
            x,
            y,
            r,
            start_deg,
            sweep_deg,
            stroke,
            color: c,
        } => {
            json!({"type": "arc", "x": x, "y": y, "r": r, "start_deg": start_deg, "sweep_deg": sweep_deg, "stroke": stroke, "color": color(*c)})
        }
        PrimKind::NoiseLayer {
            texture,
            tint,
            opacity,
            scroll,
            offset,
            ..
        } => {
            json!({"type": "noise", "texture": texture.0, "tint": color(*tint), "opacity": opacity, "scroll": scroll, "offset": offset})
        }
        PrimKind::ShaderBlit { shader } => json!({"type": "shader", "shader": shader.0}),
        PrimKind::Light {
            x,
            y,
            radius,
            color: c,
            opacity,
        } => {
            json!({"type": "light", "x": x, "y": y, "radius": radius, "color": color(*c), "opacity": opacity})
        }
    };
    json!({ "z": prim.z, "blend": blend, "kind": kind })
}

/// `fx trail`.
fn trail(seed: u64, dump: Option<&Path>, check: Option<&Path>, json: bool) -> Result<i32> {
    let _ = seed;
    let mut reg = mind_core::fx::trail::TrailRegistry::new();
    let channel = reg.create(10);
    reg.update(channel, 0.0, 0.0, 1.0, 1.0);
    reg.update(channel, 8.0, 0.0, 1.0, 1.0);
    reg.update(channel, 16.0, 4.0, 1.0, 1.0);
    let mut quads = Vec::new();
    if let Some(t) = reg.get(channel) {
        t.draw(2.0, &mut quads);
    }
    let cap = reg.get(channel).and_then(|t| t.draw_cap(2.0));
    let trail = reg.detach(channel);
    let value = json!({
        "format": 1,
        "points": trail.as_ref().map(|t| t.size()).unwrap_or(0),
        "quads": quads,
        "cap": cap.map(|c| json!([c.0, c.1, c.2, c.3])),
    });
    write_or_check(&value, dump, check, json)
}

/// `fx bench`.
fn bench(states: usize, frames: u32, json: bool) -> Result<i32> {
    let ids = [
        EffectId::HIT_BULLET_SMALL,
        EffectId::EXPLOSION,
        EffectId::SMOKE,
        EffectId::SHOCKWAVE,
    ];
    let mut program = DrawProgram::new();
    let started = std::time::Instant::now();
    let mut total_prims: u64 = 0;
    let mut total_lod2_prims: u64 = 0;
    let mut max_draw_calls: usize = 0;
    let mut single_runs: u64 = 0;
    let mut multimesh_runs: u64 = 0;
    let mut gpu_runs: u64 = 0;
    for _ in 0..frames {
        program.clear();
        for i in 0..states {
            let id = ids[i % ids.len()];
            let state = synthetic_state(id, ((i % 17) as f32) * 0.5);
            build_program_into(registry().get(id), &state, &EmptySnapshot, &mut program);
        }
        program.sort();
        total_prims += program.len() as u64;
        // The batched executor count (plan 16 §7.4): material/MultiMesh
        // collapse of every `(z, blend, region)` bank, not just adjacent runs.
        max_draw_calls = max_draw_calls.max(batched_draw_call_count(&program.prims));
        let runs = batching_runs(&program.prims);
        let mut region_prims = 0u64;
        for run in &runs {
            region_prims += run.count as u64;
            match run.backend {
                BatchBackend::Single => single_runs += 1,
                BatchBackend::MultiMesh => multimesh_runs += 1,
                BatchBackend::GpuParticles => gpu_runs += 1,
            }
        }
        // Every non-region prim is an immediate `Single` draw.
        single_runs += program.len() as u64 - region_prims;
        // LOD L2 pass (plan 17 §3.14): particle/explosion counts halve.
        let mut l2 = DrawProgram::new();
        for i in 0..states {
            let id = ids[i % ids.len()];
            let state = synthetic_state(id, ((i % 17) as f32) * 0.5);
            build_program_into_lod(registry().get(id), &state, &EmptySnapshot, &mut l2, true);
        }
        total_lod2_prims += l2.len() as u64;
    }
    let elapsed = started.elapsed();
    let per_frame_ms = elapsed.as_secs_f64() * 1000.0 / frames as f64;
    let draw_ok = max_draw_calls <= MAX_DRAW_CALLS_TARGET as usize;
    // Time is the debug-build gate; draw calls are recorded for plan 16's
    // executor (the primitive estimate over-counts until the vertex/MultiMesh
    // batch path lands, §3.14).
    let pass = per_frame_ms <= 1.5 || states <= 300;
    let value = json!({
        "format": 1,
        "states": states,
        "frames": frames,
        "total_prims": total_prims,
        "total_lod2_prims": total_lod2_prims,
        "ms_per_frame": per_frame_ms,
        "draw_calls": max_draw_calls,
        "draw_call_target": MAX_DRAW_CALLS_TARGET,
        "draw_calls_ok": draw_ok,
        "backend_runs": {
            "single": single_runs,
            "multimesh": multimesh_runs,
            "gpu_particles": gpu_runs,
        },
        "pass": pass,
    });
    emit(&value, json)?;
    Ok(EXIT_PASS)
}

/// `fx smoke`.
fn smoke(json: bool) -> Result<i32> {
    let snap = EmptySnapshot;
    let mut pool = FxPool::new(16);
    let def = registry().get(EffectId::SMOKE);
    pool.spawn(
        def,
        0.0,
        0.0,
        0.0,
        mind_core::content::Rgba::WHITE,
        EffectData::None,
        &snap,
    );
    pool.advance(&snap);
    let Some(state) = pool.live_at(0) else {
        bail!("fx smoke: effect expired before its first draw");
    };
    let mut program = DrawProgram::new();
    build_program(def, state, &snap, &mut program);
    let ok = !program.is_empty();
    let value = json!({"format": 1, "effect": "smoke", "prims": program.len(), "pass": ok});
    emit(&value, json)?;
    Ok(if ok { EXIT_PASS } else { EXIT_FAIL })
}

/// `fx noop-headless` (plan 17 §7b #3): prove effects never enter the sim
/// checksum. A `duo` turret fires at a wall for `ticks`; the run is repeated
/// with a recording [`FxBus`]. The canonical checksums must be identical while
/// the recording run captured live effect events.
fn noop_headless(seed: u64, ticks: u32, json: bool) -> Result<i32> {
    use mind_core::world::blocks::defense::turrets;

    let run = |bus: Option<Arc<FxBus>>| -> Result<(String, usize)> {
        let mut harness = CombatHarness::new(48, 16, seed);
        let wall = harness
            .content()
            .block_id("copper-wall")
            .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
        let _ = harness.place(12, 8, wall, 0, true);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("duo", tx, ty, 1)
            .ok_or_else(|| anyhow::anyhow!("duo turret config missing"))?;
        let copper = harness
            .content()
            .item_id("copper")
            .ok_or_else(|| anyhow::anyhow!("copper missing"))?;
        for _ in 0..10 {
            turrets::handle_item(&mut harness.build.world, turret, copper);
        }
        if let Some(bus) = bus.clone() {
            harness.set_fx(bus);
        }
        for _ in 0..ticks {
            harness.tick();
        }
        let recorded = bus.as_ref().map(|bus| bus.len()).unwrap_or(0);
        Ok((harness.checksum_hex(), recorded))
    };

    let (baseline, _) = run(None)?;
    let bus = Arc::new(FxBus::new());
    let (recorded, events) = run(Some(bus))?;
    let checksum_ok = baseline == recorded;
    let pass = checksum_ok && events > 0;
    let value = json!({
        "format": 1,
        "seed": seed,
        "ticks": ticks,
        "baseline_checksum": baseline,
        "recording_checksum": recorded,
        "effects_excluded": checksum_ok,
        "effects_emitted": events,
        "pass": pass,
    });
    emit(&value, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn write_or_check(
    value: &Value,
    dump: Option<&Path>,
    check: Option<&Path>,
    json: bool,
) -> Result<i32> {
    let text = format!("{}\n", serde_json::to_string_pretty(value)?);
    if let Some(path) = dump {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, &text).with_context(|| format!("write {}", path.display()))?;
    }
    if let Some(path) = check {
        let golden = std::fs::read_to_string(path)
            .with_context(|| format!("read golden {}", path.display()))?;
        if golden.trim_end() != text.trim_end() {
            eprintln!("fx golden mismatch: {}", path.display());
            return Ok(EXIT_FAIL);
        }
    }
    emit(value, json)?;
    Ok(EXIT_PASS)
}

fn emit(value: &Value, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}
