// SPDX-License-Identifier: GPL-3.0-only

//! Plan 17 headless FX scenarios (`fx` subcommand, §7b).
//!
//! Godot-free oracle for the effect catalogue, lifecycle, `DrawPrim` programs,
//! trails and resolution budget. Goldens live in
//! `mind-headless/tests/golden/fx/`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use mind_core::content::EffectId;
use mind_core::content::effect_by_name;
use mind_core::fx::{
    EffectData, EffectKind, EffectState, EmptySnapshot, FxPool, build_program, catalog_counts,
    order_hash, registry,
};
use mind_core::render::draw::{Blending, DrawPrim, DrawProgram, PrimKind};

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
    let pass = !strict || unported == 0;
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
        "pass": pass,
    });
    emit(&value, json)?;
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
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
    for _ in 0..frames {
        for i in 0..states {
            let id = ids[i % ids.len()];
            let state = synthetic_state(id, ((i % 17) as f32) * 0.5);
            build_program(registry().get(id), &state, &EmptySnapshot, &mut program);
            total_prims += program.len() as u64;
        }
    }
    let elapsed = started.elapsed();
    let per_frame_ms = elapsed.as_secs_f64() * 1000.0 / frames as f64;
    let pass = per_frame_ms <= 1.5 || states <= 300;
    let value = json!({
        "format": 1,
        "states": states,
        "frames": frames,
        "total_prims": total_prims,
        "ms_per_frame": per_frame_ms,
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
