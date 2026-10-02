// SPDX-License-Identifier: GPL-3.0-only

//! Plan 18 headless audio scenarios (§7b).
//!
//! Deterministic state-machine and event dumps used as the audio oracle. The
//! in-engine MCP sweep is deferred to the single-editor mutex (plan 18 §7c).

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, anyhow};
use mind_core::audio::math::Listener;
use mind_core::audio::{
    ActiveVoice, Admission, AmbientPoller, AmbientProvider, AmbientSnapshot, AudioEvent, AudioSink,
    LoopMixer, LoopOutput, MusicContext, MusicOutput, MusicPlayer, MusicRef, MusicRules,
    PlanetMusic, RecordingAudioSink, SeededAudioRng, SharedAudioLog, SoundId, SoundLoopState,
    SoundPriorityTable, admit,
};
use mind_core::combat::CombatHarness;
use mind_core::world::blocks::defense::turrets;
use mind_core::world::update::BuildClock;

use crate::cli::AudioCommand;
use crate::paths;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// Dispatches an `audio` subcommand.
pub fn run(command: &AudioCommand) -> anyhow::Result<i32> {
    match command {
        AudioCommand::Events {
            scenario,
            dump,
            golden,
            json,
        } => events(scenario, dump.as_deref(), golden.as_deref(), *json),
        AudioCommand::Music {
            scenario,
            frames,
            seed,
            dump,
            golden,
            json,
        } => music(
            scenario,
            *frames,
            *seed,
            dump.as_deref(),
            golden.as_deref(),
            *json,
        ),
        AudioCommand::Loops {
            scenario,
            ticks,
            dump,
            golden,
            json,
        } => loops(scenario, *ticks, dump.as_deref(), golden.as_deref(), *json),
        AudioCommand::Policy {
            requests,
            dump,
            golden,
            json,
        } => policy(
            requests.as_deref(),
            dump.as_deref(),
            golden.as_deref(),
            *json,
        ),
        AudioCommand::Bench {
            ticks,
            voices,
            loops,
            ambient,
            json,
        } => bench(*ticks, *voices, *loops, *ambient, *json),
    }
}

/// Audio golden directory under the repo root.
fn golden_dir() -> anyhow::Result<PathBuf> {
    Ok(paths::find_repo_root(None)?.join("client/rust/mind-headless/tests/golden/audio"))
}

/// Writes/compares the produced text, returning `true` on pass.
fn persist_and_check(
    name: &str,
    text: &str,
    dump: Option<&Path>,
    golden: Option<&Path>,
) -> anyhow::Result<bool> {
    if let Some(path) = dump {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, text).with_context(|| format!("writing `{}`", path.display()))?;
    }

    let golden_path = match golden {
        Some(path) => Some(path.to_path_buf()),
        None => golden_dir()
            .ok()
            .map(|dir| dir.join(format!("{name}.json"))),
    };
    if let Some(path) = golden_path
        && path.is_file()
    {
        let expected = std::fs::read_to_string(&path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        if expected.trim_end() != text.trim_end() {
            log::error!("audio {name}: golden mismatch at `{}`", path.display());
            return Ok(false);
        }
    }
    Ok(true)
}

fn report(pass: bool, json: bool, value: serde_json::Value, summary: &str) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        println!("{summary} -> {}", if pass { "PASS" } else { "FAIL" });
        println!("{}", serde_json::to_string(&value).unwrap_or_default());
    }
}

/// `audio events`: scripted one-shot/loop/music emission through the sink.
fn events(
    scenario: &str,
    dump: Option<&Path>,
    golden: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    if scenario == "audio_events_sim" {
        return events_sim(dump, golden, json);
    }
    if scenario != "audio_events_blocks" {
        return Err(anyhow!(
            "audio events --scenario {scenario}: `audio_events_blocks` or `audio_events_sim`"
        ));
    }

    let script: &[(u64, AudioEvent)] = &[
        (
            0,
            AudioEvent::at(SoundId::BLOCK_PLACE1, 128.0, 128.0, 1.0, 1.0, true),
        ),
        (
            0,
            AudioEvent::at(SoundId::BLOCK_PLACE2, 136.0, 128.0, 1.02, 1.0, true),
        ),
        (
            4,
            AudioEvent::at(SoundId::BLOCK_BREAK1, 128.0, 128.0, 1.0, 1.0, true),
        ),
        (
            4,
            AudioEvent::at(SoundId::BLOCK_REPAIR, 136.0, 128.0, 1.0, 0.9, true),
        ),
        (
            8,
            AudioEvent::at(SoundId::SHOOT_DUO, 200.0, 150.0, 1.03, 1.0, true),
        ),
        (
            12,
            AudioEvent::loop_add(SoundId::LOOP_CONVEYOR, 160.0, 160.0, 1.0, 1.0),
        ),
        (
            12,
            AudioEvent::loop_instance(17, SoundId::LOOP_MINE_BEAM, 160.0, 160.0, true, 0.8),
        ),
        (16, AudioEvent::loop_camera(SoundId::RAIN, 0.35)),
        (
            20,
            AudioEvent::play(SoundId::CORE_LAUNCH, 1.0, 1.0, 0.0, false, true),
        ),
        (24, AudioEvent::music_play(MusicRef::new("game1"), true)),
    ];

    let mut sink = RecordingAudioSink::new();
    let mut last_tick = u64::MAX;
    for (tick, event) in script {
        if *tick != last_tick {
            sink.set_tick(*tick);
            last_tick = *tick;
        }
        sink.emit(event.clone());
    }

    let text = sink.dump_json();
    let pass = persist_and_check("events_blocks", &text, dump, golden)?;
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    report(
        pass,
        json,
        serde_json::json!({
            "scenario": scenario,
            "events": sink.events.len(),
            "pass": pass,
            "dump": value,
        }),
        &format!("audio events: {} events", sink.events.len()),
    );
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// `audio events --scenario audio_events_sim`: runs the real plan-07/10
/// build/combat harness with a [`SharedAudioLog`] installed and dumps the
/// events emitted by the sim call sites (plan 18 §7b / M5).
///
/// Phases: instant place, construction finish, deconstruct finish, turret
/// shoot, bullet hit, bullet despawn. `BuildClock` is advanced between finish
/// phases so `ConstructBlock.shouldPlay` (32 ms) admits each block sound.
fn events_sim(dump: Option<&Path>, golden: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let log = SharedAudioLog::new();
    let mut harness = CombatHarness::new(48, 16, 37);
    harness.build.set_audio_sink(log.clone());

    let wall = harness
        .content()
        .block_id("copper-wall")
        .ok_or_else(|| anyhow!("copper-wall missing"))?;

    // Phase 1: instant place -> `ConstructBlock.constructFinish` place sound.
    set_build_clock(&mut harness, 0.0);
    log.set_tick(0);
    harness.place(8, 8, wall, 0, true);

    // Phase 2: construction finish -> place sound (clock advanced past 32 ms).
    set_build_clock(&mut harness, 4.0);
    log.set_tick(1);
    harness.place(10, 8, wall, 0, false);
    harness.build.construct_tick(10_000.0);

    // Phase 3: deconstruct finish -> break sound (advance the clock between the
    // instant place and the finish so the 32 ms gate admits the break).
    set_build_clock(&mut harness, 8.0);
    log.set_tick(2);
    harness.place(12, 8, wall, 0, true);
    set_build_clock(&mut harness, 12.0);
    harness.build.break_block(12, 8, false);
    harness.build.construct_tick(10_000.0);

    // Phase 4: turret shoot sounds (`duo` ammo has `shootSound`).
    set_build_clock(&mut harness, 16.0);
    log.set_tick(3);
    let (tx, ty) = CombatHarness::tile_center(4, 8);
    let turret = harness
        .spawn_test_turret("duo", tx, ty, 1)
        .ok_or_else(|| anyhow!("duo turret missing"))?;
    let copper = harness
        .content()
        .item_id("copper")
        .ok_or_else(|| anyhow!("copper missing"))?;
    for _ in 0..10 {
        turrets::handle_item(&mut harness.build.world, turret, copper);
    }
    for _ in 0..30 {
        harness.tick();
    }

    // Phase 5: bullet hit sound (`fuse` carries a hit sound).
    set_build_clock(&mut harness, 80.0);
    log.set_tick(4);
    let _ = harness.spawn_bullet("fuse", tx, ty, 0.0, 1);
    // Step until the fuse hits the wall.
    for _ in 0..40 {
        harness.step_bullets_only();
    }

    let text = log.dump_json();
    let pass = persist_and_check("events_sim", &text, dump, golden)?;
    let events = log.len();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    report(
        pass,
        json,
        serde_json::json!({
            "scenario": "audio_events_sim",
            "events": events,
            "pass": pass,
            "dump": value,
        }),
        &format!("audio events sim: {events} events"),
    );
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Rewinds/advances the harness `BuildClock` (ticks) so the block-sound rate
/// gate sees a >32 ms delta between phases.
fn set_build_clock(harness: &mut CombatHarness, ticks: f32) {
    if let Some(mut clock) = harness.build.world.get_resource_mut::<BuildClock>() {
        clock.time = ticks;
    }
}

#[derive(Default)]
struct TraceMusicOutput {
    playing: Option<MusicRef>,
    frame: u32,
    events: Vec<serde_json::Value>,
}

impl TraceMusicOutput {
    fn record(&mut self, action: &str, extra: serde_json::Value) {
        let mut value = serde_json::json!({"frame": self.frame, "action": action});
        if let serde_json::Value::Object(map) = extra
            && let serde_json::Value::Object(ref mut target) = value
        {
            for (key, item) in map {
                target.insert(key, item);
            }
        }
        self.events.push(value);
    }
}

impl MusicOutput for TraceMusicOutput {
    fn play(&mut self, track: &MusicRef, volume: f32, looping: bool) {
        self.playing = Some(track.clone());
        self.record(
            "play",
            serde_json::json!({"track": track.0, "volume": volume, "looping": looping}),
        );
    }
    fn set_volume(&mut self, _volume: f32) {}
    fn stop(&mut self) {
        self.playing = None;
        self.record("stop", serde_json::json!({}));
    }
    fn is_playing(&self) -> bool {
        self.playing.is_some()
    }
    fn fade_filter(&mut self, wet: f32, duration_s: f32) {
        self.record(
            "fade_filter",
            serde_json::json!({"wet": wet, "duration": duration_s}),
        );
    }
    fn set_paused(&mut self, paused: bool) {
        self.record("set_paused", serde_json::json!({"paused": paused}));
    }
    fn on_game_state_change(&mut self, playing: bool) {
        self.record("game_state", serde_json::json!({"playing": playing}));
    }
}

/// `audio music`: scripted menu/game/dark/boss/pause timeline.
fn music(
    scenario: &str,
    frames: u32,
    seed: u64,
    dump: Option<&Path>,
    golden: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    if scenario != "audio_music_select" {
        return Err(anyhow!(
            "audio music --scenario {scenario}: only `audio_music_select` is defined"
        ));
    }

    let mut player = MusicPlayer::new();
    let mut out = TraceMusicOutput::default();
    let mut rng = SeededAudioRng::new(seed);
    let context = MusicContext {
        is_menu: true,
        is_game: false,
        ..MusicContext::default()
    };

    let run_frame = |player: &mut MusicPlayer,
                     out: &mut TraceMusicOutput,
                     rng: &mut SeededAudioRng,
                     context: &mut MusicContext,
                     frame: u32| {
        out.frame = frame;
        context.now_ms = f64::from(frame) * (1000.0 / 60.0);
        player.update(context, rng, out);
    };

    for frame in 0..frames {
        let mut context = context.clone();
        match frame {
            0 => {
                context = MusicContext {
                    is_menu: true,
                    is_game: false,
                    ..context
                };
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            60 => {
                player.stop(&mut out);
                context.is_menu = false;
                context.is_game = true;
                context.rules_music.always = true;
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            120 => {
                player.stop(&mut out);
                context.core_hp_fraction = Some(0.5);
                player.play_random(&context, &mut rng, &mut out);
            }
            180 => {
                player.stop(&mut out);
                context.live_boss = true;
                player.play_random(&context, &mut rng, &mut out);
            }
            240 => {
                player.play_music(Some(MusicRef::new("boss1")), true, &context, &mut out);
            }
            300 => {
                context.dialog = true;
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            330 => {
                context.dialog = true;
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            360 => {
                context.dialog = false;
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            420 => {
                context.paused = true;
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            480 => {
                player.keep_silent();
                context.paused = false;
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            540 => {
                context = MusicContext {
                    is_menu: true,
                    is_game: false,
                    rules_music: MusicRules::new(),
                    planet_music: PlanetMusic::default(),
                    ..context
                };
                run_frame(&mut player, &mut out, &mut rng, &mut context, frame);
            }
            _ => {}
        }
    }

    let text = serde_json::to_string_pretty(&serde_json::json!({
        "format": 1,
        "scenario": scenario,
        "frames": frames,
        "seed": seed,
        "trace": out.events,
    }))?;
    let pass = persist_and_check("music_select", &text, dump, golden)?;
    report(
        pass,
        json,
        serde_json::json!({
            "scenario": scenario,
            "frames": frames,
            "seed": seed,
            "actions": out.events.len(),
            "pass": pass,
        }),
        &format!("audio music: {} actions", out.events.len()),
    );
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

#[derive(Default)]
struct TraceLoopOutput {
    voices: Vec<(u64, SoundId, f32, f32)>,
    playing: Vec<u64>,
    next: u64,
}

impl LoopOutput for TraceLoopOutput {
    fn start_loop(&mut self, sound: SoundId, volume: f32, _pitch: f32, pan: f32) -> u64 {
        self.next += 1;
        let id = self.next;
        self.voices.push((id, sound, volume, pan));
        self.playing.push(id);
        id
    }
    fn update_loop(&mut self, _voice: u64, _volume: f32, _pitch: f32, _pan: f32) {}
    fn stop_loop(&mut self, voice: u64) {
        self.playing.retain(|id| *id != voice);
    }
    fn is_playing(&self, voice: u64) -> bool {
        self.playing.contains(&voice)
    }
}

/// `audio loops`: aggregation + `SoundLoop` + weather per-tick rows.
fn loops(
    scenario: &str,
    ticks: u32,
    dump: Option<&Path>,
    golden: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    if scenario != "audio_loop_aggregate" {
        return Err(anyhow!(
            "audio loops --scenario {scenario}: only `audio_loop_aggregate` is defined"
        ));
    }
    let listener = Listener::new(0.0, 0.0, 100.0);
    let mut mixer = LoopMixer::new();
    let mut output = TraceLoopOutput::default();
    let mut mine = SoundLoopState::new(SoundId::LOOP_MINE_BEAM, 0.8, 17);
    let mut rows = Vec::with_capacity(ticks as usize * 2);

    for tick in 0..ticks {
        let conveyor_volume = if tick < 60 { 1.0 } else { 0.0 };
        mixer.accumulate(
            SoundId::LOOP_CONVEYOR,
            -25.0,
            0.0,
            conveyor_volume,
            1.0,
            listener,
        );
        mixer.accumulate(
            SoundId::LOOP_CONVEYOR,
            25.0,
            0.0,
            conveyor_volume * 0.5,
            1.0,
            listener,
        );
        mixer.accumulate(SoundId::LOOP_DRILL, 0.0, 50.0, 0.8, 1.0, listener);
        // Weather camera-centred loop (`LoopCamera`).
        mixer.accumulate(SoundId::RAIN, listener.x, listener.y, 0.35, 1.0, listener);

        let play = tick < 80;
        let action = mine.update(0.0, 0.0, play, 1.0, listener);
        mine.apply(action);

        mixer.update(1.0, true, false, listener, &mut output);

        for sound in [SoundId::LOOP_CONVEYOR, SoundId::LOOP_DRILL, SoundId::RAIN] {
            if let Some(data) = mixer.data(sound) {
                rows.push(serde_json::json!({
                    "tick": tick,
                    "sound": sound.name(),
                    "volume_raw": data.volume,
                    "cur_volume": data.cur_volume,
                    "pan": if data.total == 0.0 { 0.0 } else { (data.sum_x / data.total - listener.x) / (listener.width / 2.0) },
                    "pitch": if data.total_volume == 0.0 { 1.0 } else { data.pitch / data.total_volume },
                    "voice": if data.voice.is_some() { "live" } else { "none" },
                }));
            }
        }
    }

    let text = serde_json::to_string_pretty(&serde_json::json!({
        "format": 1,
        "scenario": scenario,
        "ticks": ticks,
        "rows": rows,
    }))?;
    let pass = persist_and_check("loops_aggregate", &text, dump, golden)?;
    report(
        pass,
        json,
        serde_json::json!({
            "scenario": scenario,
            "ticks": ticks,
            "rows": rows.len(),
            "pass": pass,
        }),
        &format!("audio loops: {} rows", rows.len()),
    );
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// `audio policy`: pure admission/eviction over a synthetic request stream.
fn policy(
    requests: Option<&Path>,
    dump: Option<&Path>,
    golden: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    let table = SoundPriorityTable::build();
    let _ = requests;

    // Synthetic stream: 100 requests against a max-7 sound (beamPlasma), a
    // group (shootFlame), and a min-interrupt window (mechStep).
    let plan: &[(&str, u64, f32)] = &[
        ("beamPlasma", 0, 1.0),
        ("beamPlasma", 1, 1.0),
        ("beamPlasma", 2, 1.0),
        ("beamPlasma", 3, 1.0),
        ("beamPlasma", 4, 1.0),
        ("beamPlasma", 5, 1.0),
        ("beamPlasma", 6, 1.0),
        ("beamPlasma", 7, 1.0),
        ("shootFlame", 8, 1.0),
        ("shootFlamePlasma", 9, 1.0),
        ("shootMissile", 10, 1.0),
        ("mechStep", 11, 1.0),
        ("mechStep", 12, 1.0),
        ("mechStep", 13, 1.0),
    ];
    let mut active: Vec<ActiveVoice> = Vec::new();
    let mut next_voice = 0u64;
    let mut rows = Vec::new();

    for (name, frame, volume) in plan {
        let Some(sound) = SoundId::by_name(name) else {
            continue;
        };
        let spec = table.spec(sound);
        let now_ms = *frame as f64 * (1000.0 / 60.0);
        let request = mind_core::audio::PlayRequest {
            sound,
            priority: spec.priority,
            group: spec.group,
            max_concurrent: spec.max_concurrent,
            min_interrupt: spec.min_interrupt(1.0),
        };
        let decision = admit(&active, &request, now_ms);
        match decision {
            Admission::Play | Admission::Replace { .. } => {
                next_voice += 1;
                if let Admission::Replace { voice } = decision {
                    active.retain(|v| v.voice != voice);
                }
                active.push(ActiveVoice {
                    voice: next_voice,
                    sound,
                    group: spec.group,
                    priority: spec.priority,
                    started_ms: now_ms,
                    min_interrupt: request.min_interrupt,
                    volume: *volume,
                });
            }
            Admission::Deny => {}
        }
        rows.push(serde_json::json!({
            "frame": frame,
            "sound": name,
            "decision": match decision {
                Admission::Play => "play",
                Admission::Replace { .. } => "replace",
                Admission::Deny => "deny",
            },
            "active": active.len(),
        }));
    }

    let text = serde_json::to_string_pretty(&serde_json::json!({
        "format": 1,
        "scenario": "audio_policy",
        "requests": rows.len(),
        "decisions": rows,
    }))?;
    let pass = persist_and_check("policy", &text, dump, golden)?;
    report(
        pass,
        json,
        serde_json::json!({
            "requests": rows.len(),
            "pass": pass,
        }),
        &format!("audio policy: {} requests", rows.len()),
    );
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// `audio bench`: mix-update timing for the §7d budget.
fn bench(
    ticks: u64,
    voices: usize,
    loops: usize,
    ambient: usize,
    json: bool,
) -> anyhow::Result<i32> {
    if ticks == 0 {
        return Err(anyhow!("audio bench --ticks must be greater than zero"));
    }
    let ticks_usize = usize::try_from(ticks).context("--ticks does not fit in memory")?;
    let listener = Listener::new(0.0, 0.0, 100.0);
    let mut mixer = LoopMixer::new();
    let mut output = TraceLoopOutput::default();
    let mut player = MusicPlayer::new();
    let mut music_out = TraceMusicOutput::default();
    let mut context = MusicContext::default();
    let mut rng = SeededAudioRng::new(1);

    // Ambient snapshot.
    let provider = BenchProvider { loops: ambient };
    let mut snapshot = AmbientSnapshot::default();
    snapshot.build_from([&provider as &dyn AmbientProvider]);
    let mut poller = AmbientPoller::new(50.0);

    // Warm up.
    for tick in 0..1000u64 {
        let _ = tick;
        mixer.accumulate(SoundId::LOOP_CONVEYOR, 0.0, 0.0, 1.0, 1.0, listener);
        mixer.update(1.0, true, false, listener, &mut output);
        music_out.frame = 0;
        player.update(&context, &mut rng, &mut music_out);
    }

    let mut samples: Vec<u64> = Vec::with_capacity(ticks_usize);
    let mut ambient_scratch: Vec<mind_core::audio::AmbientSoundData> = Vec::new();
    for tick in 0..ticks {
        let start = Instant::now();
        for i in 0..loops {
            #[allow(clippy::cast_precision_loss)]
            let x = (i as f32) - (loops as f32 / 2.0);
            mixer.accumulate(SoundId::LOOP_CONVEYOR, x, 0.0, 1.0, 1.0, listener);
        }
        if ambient > 0 {
            ambient_scratch.clear();
            ambient_scratch.extend_from_slice(poller.update(
                1000.0 / 60.0,
                false,
                true,
                &snapshot,
                listener,
            ));
            mixer.merge_ambient(&ambient_scratch);
        }
        mixer.update(1.0, true, false, listener, &mut output);
        context.now_ms = tick as f64 * (1000.0 / 60.0);
        player.update(&context, &mut rng, &mut music_out);
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[(samples.len() * 99) / 100];
    // Budgets are release-build targets (plan 18 §7d); debug runs report only.
    let within_budget = cfg!(debug_assertions) || p99 as f64 / 1000.0 <= 250.0;
    let report_value = serde_json::json!({
        "ticks": ticks,
        "voices": voices,
        "loops": loops,
        "ambient": ambient,
        "p50_ns": p50,
        "p99_ns": p99,
        "p50_us": p50 as f64 / 1000.0,
        "p99_us": p99 as f64 / 1000.0,
        "budget_p99_us": 250.0,
        "release": !cfg!(debug_assertions),
        "pass": within_budget,
    });
    report(
        within_budget,
        json,
        report_value,
        &format!("audio bench: p50 {p50} ns / p99 {p99} ns"),
    );
    Ok(if within_budget { EXIT_PASS } else { EXIT_FAIL })
}

struct BenchProvider {
    loops: usize,
}

impl AmbientProvider for BenchProvider {
    fn is_valid(&self) -> bool {
        true
    }
    fn should_ambient_sound(&self) -> bool {
        true
    }
    fn ambient_volume(&self) -> f32 {
        1.0
    }
    fn ambient_sound(&self) -> SoundId {
        SoundId::LOOP_HUM
    }
    fn position(&self) -> (f32, f32) {
        let _ = self.loops;
        (0.0, 0.0)
    }
}
