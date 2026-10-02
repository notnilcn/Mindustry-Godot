# Plan 18 — Audio parity ledger

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
> Source inventory: `core/src/mindustry/audio/**`, Arc `arc/audio/{Sound,Music,Audio,AudioBus}`.
> Status 2026-10-02: **M0–M4 core landed + headless oracles; M5/M6 partial; in-engine MCP deferred.**

## Ported behaviors (0 unported)

| Upstream | Port | Oracle |
|---|---|---|
| `Sound.calcFalloff` / `calcPan` | `mind_core::audio::math::{calc_falloff,calc_pan}` | `audio::math::tests::falloff_and_pan_values` |
| `Mathf.lerpDelta` (delta 1) | `math::lerp_delta` | `audio::math::tests::sanitize_and_lerp` |
| `Mathf.chance` NaN/edge semantics | `math::AudioRng::chance` | `audio::math::tests::chance_edges` |
| `Seq.random(exclude)` | `math::random_index_excluding` | `audio::math::tests::random_excluding_maps_around_exclusion` |
| `SoundPriority.init()` table | `priority::SoundPriorityTable::build` | `audio::priority::tests::table_matches_sound_priority_init` |
| SoLoud concurrency/interrupt | `priority::{admit, SoundPlayState}` | `audio::priority::tests::{admission_policy,group_limits,interrupt_window_and_priority_replacement,check_frame_boost}` |
| `SoundControl` playlists/override chain (incl. boss→dark quirk) | `music::MusicPlayer::{ambient_playlist,dark_playlist,boss_playlist}` | `audio::music::tests::playlist_overrides_and_boss_quirk` |
| `SoundControl.isDark` | `music::MusicPlayer::is_dark` | `audio::music::tests::dark_heuristic_golden` |
| `play`/`playOnce`/`playMusic`/fades/`silence`/`stop`/`keepSilent` | `music::MusicPlayer` | `audio::music::tests::{fade_crossfade_math,play_once_never_interrupts,play_music_interrupt,should_play_zero_volume,stop_and_keep_silent}` |
| Menu/planet/editor/game selection | `music::MusicPlayer::update` | `audio::music::tests::{menu_selection,editor_selection,game_random_chance_and_interval}` |
| `WaveEvent` 8–15 s delayed replay + boss group | `music::MusicPlayer::{on_wave_event,update_wave}` | `audio::music::tests::wave_event_delay_and_boss` |
| `volumeMultiplier = musicvol/100 × clamp(rules.volume)` | `music::MusicPlayer::{volume_multiplier,should_play}` | `audio::music::tests::volume_multiplier_and_should_play` |
| `SoundControl.loop` accumulation | `loops::LoopMixer::accumulate` | `audio::loops::tests::{aggregation_math,weighted_centroid_pan}` |
| `updateLoops` lerp/reset/voice lifecycle + menu clear/pause freeze | `loops::LoopMixer::update` | `audio::loops::tests::{lerp_and_reset_and_voice_lifecycle,menu_clear_and_pause_freeze}` |
| `AudioThread.doLoop` (20 FPS, silent→audible gate) | `loops::{AmbientSnapshot,AmbientPoller}` | `audio::loops::tests::{ambient_merge_silent_gate,zero_volume_poll_no_output}` |
| `SoundLoop` fade 0.05/frame + stop ≤ 0.001 | `loops::SoundLoopState` | `audio::loops::tests::sound_loop_fade` |
| `SoundControl.findMusic` chain | `audio::ids::find_music_chain` | `audio::ids::tests::find_music_chain_resolution` |
| `SoundId` by-name ABI + `none`/`unset` sentinels | `content::registries::sound_meta::SoundId` | `audio::events::tests::sound_id_serde_by_name` |
| UI-bus routing (`sounds/ui/*`, forced `coreLaunch`) | `audio::ids::bus_for_sound` + `priority` table | `audio::ids::tests::bus_routing` |
| Godot bus layout + Sound lowpass slot 0 | `mind_gdext::audio::buses` | MCP §7c (deferred) |
| Music stream cache (normal/looping variants) | `mind_gdext::audio::streams::StreamCache` | MCP §7c (deferred) |
| Pooled one-shot/loop voices (128+16) | `mind_gdext::audio::voices::VoicePool` | MCP §7c (deferred) |

## Headless goldens (plan 18 §7b)

- `audio events` → `client/rust/mind-headless/tests/golden/audio/events_blocks.json`
- `audio music` → `music_select.json`
- `audio loops` → `loops_aggregate.json`
- `audio policy` → `policy.json`
- `audio bench` → informational (`--json`); release budget p99 ≤ 250 µs, debug reports only.

## Deliberate deviations

1. Sim emits `AudioEvent`s through an `AudioSink`; `NoopAudioSink` is the headless default (plan 18 §2.4-1).
2. Main-thread 20 Hz `AmbientPoller` over an `AmbientSnapshot`; no ECS access off-thread (§2.4-2, OD-18-A).
3. Lowpass "wet" is a 20500→500 Hz cutoff sweep on the `Sound` bus (§2.4-3, OD-18-B).
4. Pause mirror is per-voice `stream_paused` (Music/UI exempt) (§2.4-4).
5. Pan via `AudioStreamPlayer2D` node offset with `attenuation = 0`; falloff exact in Rust (§2.4-5, OD-18-C).
6. Audio uses a frame clock, never `SimClock` (§2.4-6).
7. Loop variants duplicate the stream with `loop = true` (§2.4-7).

## Deferred / open

- **M5 complete:** `findMusic` mod/`dp-` chain is landed; **audition** (`MindAudio.audition_*`) and data-audio overlay (plan 20) are **not** implemented in `mind-gdext` yet.
- **M6:** `audio bench` exists; alloc-audit + inspector `Audio` row + `bench/baselines.json` registration (plan 23) deferred.
- **MCP §7c:** all in-engine checks deferred to the orchestrator's single-editor mutex. Copy-pasteable evals live in the plan §7c.
- Content sound/music fields on `BlockDef`/`UnitTypeDef`/`WeaponDef`/`BulletDef`/`WeatherDef`/`PlanetDef` are plan 02/12 ownership; plan 18 consumes them via adapters (`MusicRules`/`PlanetMusic`) and the `AudioEvent` constructors.
