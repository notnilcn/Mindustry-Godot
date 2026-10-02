# 18 — AUDIO IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.` (Rust) or `## Ported from Mindustry ... — GPL-3.0` (GDScript).
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | 🟢 **FINISHED 2026-10-03 on `lane/18-audio`: M0–M6 complete; only the in-engine §7c MCP sweep remains deferred to the single-editor mutex.** `mind-core::audio` (all state machines + `AudioSink`/`SharedAudioLog`), **sim call-site emitters routed to `AudioSink`** (`world::BuildHarness`, `turrets::shoot`, `combat::bullet::{hit,despawn}`, `unit::kill_unit`), `audio::sim` content→event helpers, `audition_name` `dp-` overlay chain, `mind-gdext::audio::MindAudio` (buses, stream cache, voice pool, `#[func]` API, `audition_*`, spine node), `mind-headless audio events/music/loops/policy/bench` + goldens (`events_blocks` + new `events_sim`), alloc-audit + inspector `Audio` row + `bench/baselines.json` budgets, `mind-headless/tests/audio_golden.rs`. Reconciled with the written siblings 00/02/03/04/05/10/11/12/13/14/16/17/20 (§3.12). OD-18-A (main-thread 20 Hz poller) and OD-18-B (cutoff-sweep lowpass) executed on their locked defaults; OD-18-D (`dp-` overlay) consumed. |
| **Phase** | P6 — Render, FX, audio (`HIGH_LEVEL_PLAN.md` §5). |
| **Depends on** | `02_CONTENT_IMPLEMENTATION_PLAN.md` (content records carrying `SoundId`/music fields: `BlockDef.ambientSound`/`destroySound`, `UnitTypeDef.*Sound*`, `WeaponDef.shoot_sound`, `BulletDef.hitSound/despawnSound/healSound`, `WeatherDef.sound*`, `PlanetDef.launchMusic/ambientMusic/darkMusic/alwaysPlayMusic`; `SoundId` is **owned by this plan**, §3.2), `03_ASSETS_IMPLEMENTATION_PLAN.md` (`sounds.index.json`, `Sounds`/`Musics` registries, `FileTree::resolve_sound`, lazy `AudioStream` cache, `AssetsReadyEvent`, `OverlaySound`/`dp-` hooks). Transitively: 00 (spine, event bus, MCP rig), 04 (`SettingsStore` keys), 05 (`EventBus`, `ResetEvent`/`WaveEvent`/`ClientLoadEvent`, `SimClock`), 12 (`Rules` music fields), 14 (`MindUi` dialog/menu/planet/editor visibility, settings rows), 16 (camera for pan/falloff, frame slot), 17 (`FxSink::sound` forwarding). |
| **Blocks** | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (no direct interface: sound events are local-only and never relayed; the no-op sink must be the default in every relay path) and `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (mobile audio session handling, export bus layout verification, dedicated-server audio-less policy). Also consumed (not blocked) by `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (audio scenario/golden registration). |
| **Sources (read in full unless noted)** | Mindustry AGENTS docs: `core/src/mindustry/audio/AGENTS.md`, `core/assets/AGENTS.md`, `core/src/mindustry/game/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/ui/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `core/AGENTS.md`. Java: `audio/{SoundControl,SoundPriority,AmbientSource,SoundLoop,MusicContainer}.java` (all in full), `core/Control.java` (update order, menu/core-landing music, 190–225 + 674–712), `core/GameState.java` (`boss()`, `enemies`, `isMenu/isGame/isPaused`), `game/Rules.java` music fields (170–179), `game/EventType.java` (`MusicRegisterEvent`, `WaveEvent`, `ResetEvent`, `ClientLoadEvent`), `core/Logic.java` (`WaveEvent` fire site 238/324), `type/Planet.java` (`launchMusic/ambientMusic/darkMusic/alwaysPlayMusic`, 143–149), `type/Weather.java` (`sound/soundVol/soundVolMin/soundVolOsc*`, `updateEffect` 83–105), `world/Block.java` (`ambientSound/ambientSoundVolume`, 356–358), `world/blocks/defense/turrets/Turret.java` (`soundLoop`, 304–320, 504–505), `entities/comp/{BuildingComp,UnitComp,FireComp,MinerComp,TankComp,BuilderComp,WeatherStateComp}.java` (sound call sites), `entities/effect/SoundEffect.java`, `entities/comp/PowerGenerator.java`, `ImpactReactor.java`, `Drill.java`, `BurstDrill.java` (`ambientVolume()` overrides), `logic/LExecutor.java` (`PlaySoundI`/`PlayMusicI`, 2230–2280), `editor/data/MapAudioView.java`, `ui/dialogs/SettingsMenuDialog.java` (sound table 368–371), `ClientLauncher.java` (95–164 loaders + `SoundPriority.init()` 239). Arc behavioral oracle (**not checked out**; fetched from `Anuken/Arc` `master`): `arc/audio/{Sound,Music,Audio,AudioSource,AudioBus,Filters}.java`. Tests: `Mindustry/tests/src/test/java/**` grepped — **no audio tests exist** (§7a substitution). |
| **Extends spine** | (a) `/root/Spine/MindAudio` (Rust `Node`) with the stable `#[func]` test API (§3.10); (b) `client/default_bus_layout` equivalents created programmatically (§3.5); (c) state-inspector `Audio` row (bus volumes, current track, fade, voices, loops); (d) `mind-headless audio *` subcommands + `RecordingAudioSink` event dumps; (e) `scenarios/audio_*.json`. Node paths follow plan 00 §3.5/§7c and plan 17 (`/root/Spine/*`); plan 05's assumed `/root/Main/*` and plan 12/14's `/root/Main/MindCampaign` are superseded (OD16-J), so `MindAudio` is `/root/Spine/MindAudio`. |
| **License** | GPL-3.0 (D6). Audio behavior and *names* are parity ABI; code is a port, not a copy. |

## 2. Scope & parity definition

### 2.1 In scope

1. **Sound registry binding.** Consume plan 03's `Sounds`/`Musics` registries (`assets/sounds.index.json`, append-only ids, `none`/`unset` dummies) and bind each id to Godot `AudioStream` resources lazily; own `SoundId`/`MusicRef` types; own `findMusic` (mod/`music/` prefix/`dp-` resolution chain, `MusicContainer` lazy cache).
2. **`SoundControl` parity (client frame state machine).** Playlists (`ambientMusic`/`darkMusic`/`bossMusic` defaults + `MusicRegisterEvent` + mod hooks), `reload()`, `play`/`playOnce`/`playMusic`/`silence`/`stop`/`keepSilent`, `finTime`/`foutTime` fades and crossfades, finished-track detection, menu → `Musics.menu` / planet UI → `Planet.launchMusic` / editor UI → `Musics.editor` / in-game rules-editor → `Musics.editor`, `alwaysPlayMusic()` (`Rules.alwaysPlayMusic` ∥ `alwaysmusic` setting ∥ `Planet.alwaysPlayMusic`), `disableMusic`, interval/chance gating (`musicInterval`, `musicChance`), `isDark()` heuristic (core HP / wave RNG / enemy count), live-boss and boss-spawn-group track selection, `WaveEvent` delayed (8–15 s) random/boss replay, `ResetEvent` teardown, `volumeMultiplier = musicvol/100 × clamp(Rules.musicVolume)`.
3. **Routing and mixing.** Godot buses `Master → Sound`, `Music`, `UI` replacing Arc `soundBus`/`musicBus`/`uiBus`; `sounds/ui/*` auto-routing + forced `coreLaunch` UI bus; `sfxvol` with upstream per-call semantics; `musicvol`/`ambientvol`; dialog-pause lowpass (Arc `BiquadFilter.set(0, 500, 1)` + `paramWet` 0→1 over 0.4 s) mapped to `AudioEffectLowPassFilter` on the Sound bus; `Core.audio.setPaused(soundBus, paused)` mirror; `soundBus.stop()/play()` reset semantics.
4. **One-shot playback engine (`Arc Sound` parity).** `calcFalloff` (`falloff = 16000`, per-sound `falloffOffset`), `calcPan` (camera-relative, clamp ±0.9), `calcVolume × sfxvol`, `.at`/`play`/`loop` variants with `checkFrame` min-interval (16 ms) voice intensification and `< 0.005` discard, volume/pitch/pan clamps, `defaultSoundMaxConcurrent = 6`.
5. **`SoundPriority` parity.** Priority table, per-sound `maxConcurrent`, concurrent groups, `minConcurrentInterruptFraction(min, fraction)`, `falloffOffset` overrides, exact port of `SoundPriority.init()` order.
6. **Positional loop aggregation.** `SoundControl.loop(...)` accumulation semantics (weighted volume/centroid/pitch, 0.11 lerp, `ambientvol`), `updateLoops()` lifecycle (menu clear, pause freeze, start/stop on `soundID`/`isPlaying`), `SoundLoop` fade in/out (`fadeSpeed = 0.05`) and per-instance voice updates, weather loops (`Weather.sound` volume formula incl. `Noise.rawNoise` fade), turret/unit/weapon/fire/miner/conveyor loop call sites.
7. **Ambience.** `AmbientSource` interface parity (`isValid`/`shouldAmbientSound`/`getAmbientVolume`/`getAmbientSound`), building registration equivalent, 20 FPS poll equivalent with a defined thread/ownership boundary, `BuildingComp.ambientVolume()` overrides (`PowerGenerator`, `ImpactReactor`, `Drill`, `BurstDrill`), weather.
8. **`MusicContainer` + MapAudioView audition.** Lazy name→track resolution; editor `data/MapAudioView` audition behavior (`Music.play` / `Sound.play(uiBus)`, play/pause icon state, position slider, `keepSilent()` while auditioning, delete/stop).
9. **Settings + persistence.** `musicvol`, `sfxvol`, `ambientvol`, `alwaysmusic` (plan 04 store; ABI keys unchanged).
10. **Headless no-op + boundary.** `AudioSink` trait in `mind-core` (Godot-free); sim emits events unconditionally; `NoopAudioSink` in headless/dedicated-server; `RecordingAudioSink` (test build) is the deterministic event oracle; `mind-gdext` owns all Godot audio objects.

### 2.2 Definition of done

- `cargo test -p mind-core` passes every §7a test with no Godot/network; `mind-headless audio events|music|loops|policy|bench` pass with committed goldens.
- Bus layout, volume formulas, lowpass behavior, pause mirror, voice policy and loop aggregation asserted in-engine through §7c using `godot_exec` state reads (screenshots are not meaningful for audio).
- All `SoundControl` selection branches covered by headless state-machine goldens.
- §7d budgets met; no per-frame allocation after warm-up in loop aggregation, event drain or ambient snapshot build.
- Every §7e checklist item ticked with evidence in the Changelog.

### 2.3 Explicit boundaries (who owns what)

| Area | This plan (18) owns | Deferred to |
|---|---|---|
| `Sounds`/`Musics` file discovery, ids, name mangling, registry JSON | — | `03_ASSETS_IMPLEMENTATION_PLAN.md` (M8) |
| `SoundId`/`MusicRef` types, `findMusic` chain, `AudioStream` loop variants | this plan | — |
| Content sound fields (`ambientSound`, `deathSound`, `shoot_sound`, …) | field **contract** (§3.12) + resolution | `02_CONTENT_IMPLEMENTATION_PLAN.md` stores them |
| `SoundControl`/`MusicContainer`/`SoundPriority`/`AmbientSource`/`SoundLoop` semantics | this plan (pure logic in `mind-core`, playback in `mind-gdext`) | — |
| Every sim-side call site (block place/break, weapons, units, fires, puddles, weather) | the `AudioSink` API + event taxonomy | plans 07/08/09/10/11/12/13/17 call it |
| Effect-driven sounds (`Fx` `SoundEffect`) | `SoundId` + sink | `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (forwards via its `FxSink`) |
| mlog `playsound`/`playmusic` | `@sfx-*` `SoundId` table + sinks | `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` (instruction semantics) |
| Map data audio (`dp-`) loading/registration | audition path + `findMusic` fallback | `20_MODS_IMPLEMENTATION_PLAN.md` (`DataAudioLoader`), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (dialog) |
| Settings keys/persistence | key names + defaults | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (store) |
| Settings UI rows, dialog visibility, HUD mute toggles | semantic APIs | `14_UI_IMPLEMENTATION_PLAN.md` |
| Camera listener, viewport width for pan, frame slot | consumes | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Mobile audio session, export bus-layout validation, no-audio server | — | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` |
| STDB tables/reducers/views | none (no audio over the wire) | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Goldens/bench registration | contributes scenarios | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |

### 2.4 Deliberate deviations (reason stated)

| # | Mindustry behavior | Port behavior | Reason |
|---|---|---|---|
| 1 | `Sound.at/play` execute inside sim code (client `Control`), guarded implicitly by `Vars.headless`. | Sim emits `AudioEvent`s into an `AudioSink`; `NoopAudioSink` headless, `GodotAudioSink` client. Event order is sim-deterministic and dumpable. | D1/HLP §2.2 (no Godot in sim); preserves the headless guard structurally instead of by branch. |
| 2 | Audio `AudioThread` daemon reads live ECS `AmbientSource`s concurrently; render thread merges under `synchronized`. | Main-thread 20 Hz `AmbientPoller` reading a pre-built `AmbientSnapshot`; no second thread touches the ECS. Optional worker mode consumes the same snapshot over a channel (OD-18-A). | HLP §2.2/D8; removes data races; identical 20 FPS cadence and output. |
| 3 | SoLoud `BiquadFilter` with `paramWet` crossfades dry→filtered. | `AudioEffectLowPassFilter` on the `Sound` bus; "wet" maps to a cutoff sweep `20500 Hz → 500 Hz` over 0.4 s (`FILTER_12DB`, resonance 1.0). `lowpass_wet()` reports the synthetic 0..1 value. | Godot bus filters have no wet/dry parameter; the sweep is the closest audible equivalent (OD-18-B). |
| 4 | `Core.audio.setPaused(soundBus.id, paused)` pauses one bus voice; `soundBus.stop()/musicBus.play()/soundBus.play()` Soloud bus juggling. | `VoiceManager::set_sound_paused(paused)` sets `stream_paused` on every active Sound-bus voice (UI/Music unaffected); leaving-game path stops Sound voices and restores default filter. | Godot buses have no pause state; per-voice pause is the exact observable behavior. |
| 5 | `Sound.calcPan` uses `(x - camera.x)/(camera.width/2)` with Arc camera. | Volume uses the exact `calcFalloff` port; pan is applied by placing the `AudioStreamPlayer2D` at `listener.x + pan × viewport_width/2` with `attenuation = 0` (`set_pan` API). Godot's 2D pan curve is used instead of Arc's linear pan. | Pan is render-only and not checksummed; falloff (the audible distance law) is exact. Reconcile in OD-18-C. |
| 6 | Music scheduling uses Arc `Time.millis`/`Time.delta` and `Time.run` from client `Control`; `Time.delta ≈ 1` at 60 fps. | Audio layer uses a `FrameClock { now_ms, delta_frames }` supplied by `mind-gdext` (`delta*60`); the WaveEvent 8–15 s delay is a frame-clock timer, never a sim `SimClock::run`. | Client-only state must never mutate the sim (`SimClock.runs` participates in the checksum, 05 §6.5). |
| 7 | `Music.setLooping` / `Sound.loop` toggle per-voice looping. | `StreamCache` keeps `normal` + `looping` (`AudioStream` duplicate with `loop = true`) variants per sound/music; voices pick the variant. | Godot's loop flag lives on the shared stream resource; no per-playback toggle. |
| 8 | `Sound`/`Music` are object references inside content records. | `SoundId` (name-serde, runtime id) and `MusicRef` (name-serde, lazily resolved). | 02 is Godot-free; content is data + IDs (HLP §2.4). |
| 9 | `Mathf.chance` / `Seq.random(exclude)` drive dark/boss selection from Arc global rand. | `AudioRng` trait; gdext uses a wall-clock-seeded `ArcRand` port; tests inject `SeededAudioRng`. Never touches `RngStream::Sim`. | Audio is render-only and must not affect determinism. |
| 10 | `MapAudioView` holds Arc `AudioSource`s and `Slider`/`Label` UI. | `AuditionModel` in `mind-gdext` + GDScript callbacks (`MindAudio.audition_*`); dialog layout stays in plan 19. | D1 (UI layout in Godot) / plan 19 ownership. |

## 3. Target design

Names below are final unless marked. `mind-core` is Godot-free and tokio-free. No `HashMap` iteration on any path that produces goldens; ordered `IndexMap`/`Vec` only. Capacities are preallocated; steady-state frames allocate nothing.

### 3.1 Module layout

```
client/rust/mind-core/src/audio/
  mod.rs            # AudioEvent, AudioSink, NoopAudioSink, RecordingAudioSink, AudioPlugin
  ids.rs            # SoundId, MusicRef, SoundTable/MusicTable views, priority/group newtypes
  math.rs           # calc_falloff, calc_pan, clamp01, lerp_delta, AudioRng trait + SeededAudioRng
  music.rs          # MusicPlayer (SoundControl state machine), MusicContext, MusicRules, playlists
  loops.rs          # LoopMixer (aggregation), LoopData, SoundLoopState, AmbientProvider/Snapshot
  priority.rs       # SoundPrioritySpec table + VoicePolicy (admission/eviction, pure)
  events.rs         # AudioEvent constructors used by sim call sites + JsonL dump
  tests/            # unit + integration tests (no Godot)

client/rust/mind-gdext/src/audio/
  mod.rs            # MindAudio Node, #[func] test API, frame driver, inspector feed
  streams.rs        # StreamCache: SoundId/MusicRef -> (normal, looping) AudioStream variants
  buses.rs          # bus layout creation/verification, volumes, lowpass, pause mirror
  sound_control.rs  # SoundControl frame driver: MusicPlayer + LoopMixer + VoiceManager glue
  voices.rs         # VoiceManager + Voice / LoopVoice / SoundLoopVoice (Arc Sound parity)
  ambient.rs        # AmbientPoller (20 Hz, main thread default) + provider registration
  audition.rs       # MapAudioView audition model
  debug.rs          # MindAudio stats, probe helpers, dev loop/one-shot injection

client/scenes/spine.tscn            # + MindAudio node (appended by plan 00/16 owner)
client/rust/mind-gdext/src/spine.rs # + register MindAudio + AudioListener2D on the camera
```

`AudioPlugin` registers the `AudioSink` resource and `AudioEvent` registration in `mind-core`; it never constructs a Godot type. `mind-gdext` installs `GodotAudioSink` on the client, `NoopAudioSink` on headless/dedicated; `RecordingAudioSink` is behind `#[cfg(any(test, feature = "audio-record"))]`.

### 3.2 Sound registry, `SoundId`, stream resolution

- Plan 03's `sounds.index.json` walks `assets/sounds/**` (categories `beams block charge environment explosions loops movement shoot ui`) and `assets/music/**`; ids dense from 0 in sorted order; `none`/`unset` virtual dummies (id `-1`/`-2`). Uniqueness: base name across all subfolders must be unique or pack fails (03 hard rule) — plan 18 never re-derives names from the filesystem.
- `SoundId(i32)`: `-1 = NONE`, `-2 = UNSET`, `>= 0` registry id. `SoundTable::name(id)`, `SoundTable::id(name)`; `SoundId` serializes **by name** (`"shootDuo"`, `"none"`, `"unset"`) so content JSON and mods stay name-stable (02/20). `UNSET` resolves to the same silent sentinel as `NONE` for playback but is serialized distinctly; `none` is the safe default.
- `MusicRef(String)`: music has no ids upstream; serde by name only. `MusicTable` maps name → file path.
- `findMusic(name)` port (`SoundControl.findMusic`): exact registry name → `name.ogg` → `name.mp3` → `music/name.ogg` → `music/name.mp3` → if any mods: `FileTree::resolve_sound("music/" + name)` (03; `dp-` overlay included by 20). Returns `None` on miss (Java returns `null`; callers no-op).
- `StreamCache` (gdext): `SoundId`/`MusicRef` → `StreamVariants { normal: Gd<AudioStreamOggVorbis/MP3>, looping: Gd<AudioStream> }`, created on first use; `.ogg` prefers `AudioStreamOggVorbis`, `.mp3` `AudioStreamMP3`; decode/import is Godot's (`audio/streams` importer). Loop variant = `normal.duplicate()` with `loop = true` (music normally, loop sounds always).
- Missing-file behavior: registry entry without a loadable stream logs `[E] missing audio <name>` **once** per name and the sound behaves as `NONE` (all play calls become no-ops, loops never start); a missing sound in `SoundPriority` is a startup warning, not a crash (contrast upstream: compile error — our registry is data, so we warn + fail the §7e checklist). A missing `MusicRef` resolves to `None`; playlists filter `None` entries exactly like Java `removeAll(m -> m == null)`.

### 3.3 `AudioSink` event boundary (`mind-core`)

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioEvent {
    /// Sound.at(x, y, pitch, volume) with checkFrame; pos in world px.
    At { sound: SoundId, x: f32, y: f32, pitch: f32, volume: f32, check_frame: bool },
    /// Sound.play(volume, pitch, pan, loop, checkFrame); volume already sfx-scaled per call site.
    Play { sound: SoundId, volume: f32, pitch: f32, pan: f32, loop: bool, check_frame: bool },
    /// SoundControl.loop(sound, pos, volume[, pitch]) aggregation input.
    LoopAdd { sound: SoundId, x: f32, y: f32, volume: f32, pitch: f32 },
    /// SoundLoop.update(...) — per-instance loop with fade and a stable voice key.
    LoopInstance { key: VoiceKey, sound: SoundId, x: f32, y: f32, play: bool, volume_scl: f32 },
    /// Weather/other camera-centered loops.
    LoopCamera { sound: SoundId, volume: f32 },
    /// Music control from mlog / map scripts (findMusic + playMusic semantics).
    MusicPlay { name: MusicRef, interrupt: bool },
    MusicStop,
    KeepSilent,
    StopLoops,
}

pub trait AudioSink: Send { fn emit(&mut self, e: AudioEvent); }
pub struct NoopAudioSink;
pub struct RecordingAudioSink { pub events: Vec<TickedAudioEvent>, /* tick stamped on emit */ }
```

- Emitting is unconditional in sim code (`Sim::tick` order preserved); `RecordingAudioSink` stamps `tick = SimClock.tick` and is the oracle for §7b. Event variants are **append-only** (like `SimEvent`).
- The client drains the sink once per rendered frame after view sync (plan 16 stage order post-`SimHost` apply) and applies events through the voice manager; headless drains nothing (Noop).
- `AudioPlugin` owns the active sink; plans must call `audio.emit(...)`, not construct Godot types. Headless scenarios may swap in `RecordingAudioSink` via the scenario harness.
- Event volume semantics: `At` carries the raw per-call `volume` and the client applies `calcFalloff × sfxvol`; `Play` carries the final volume as computed by its call site (matching Arc: `play(float)` ignores `sfxvol`, `LExecutor`/`NetClient` multiply `sfxvol` explicitly). `LoopAdd`/`LoopInstance`/`LoopCamera` never apply `sfxvol`.

### 3.4 `SoundControl` parity — music state machine (`mind-core::audio::music`)

```rust
pub struct MusicPlayer {
    pub fin_time: f32,          // 120
    pub fout_time: f32,         // 120
    pub music_interval: f64,    // 3.0 * Time::to_minutes  (= 10800)
    pub music_chance: f32,      // 0.8
    pub music_wave_chance: f32, // 0.46
    pub ambient_music: Vec<MusicRef>, dark_music: Vec<MusicRef>, boss_music: Vec<MusicRef>,
    pub fade: f32, pub silenced: bool, pub keep_silent: bool,
    pub current: Option<MusicRef>, pub last_random_played: Option<MusicRef>,
    pub last_played_ms: f64, pub was_playing: bool,
    pub wave_music_due_ms: Option<f64>,   // WaveEvent 8–15 s delay (frame clock)
    pub lowpass_target_wet: f32, pub lowpass_poll_ms: f64,
}
pub struct MusicRules { pub ambient: Option<Vec<MusicRef>>, pub dark: Option<Vec<MusicRef>>,
                        pub disable: bool, pub always: bool, pub volume: f32 }
pub struct PlanetMusic { pub launch: Option<MusicRef>, pub ambient: Option<Vec<MusicRef>>,
                         pub dark: Option<Vec<MusicRef>>, pub always: bool }
pub struct MusicContext {
    pub is_menu: bool, pub is_game: bool, pub paused: bool, pub editor_rules: bool,
    pub planet_ui_visible: bool, pub editor_ui_visible: bool,
    pub planet_music: PlanetMusic, pub rules_music: MusicRules,
    pub always_music_setting: bool, pub musicvol_setting: i32,
    pub core_hp_fraction: Option<f32>, pub wave: i32, pub enemies: i32,
    pub live_boss: bool, pub boss_spawn_group: bool,
    pub now_ms: f64, pub delta_frames: f32,
    pub tracks: &'static dyn MusicResolver,   // resolves MusicRef -> MusicHandle (all loaded)
}
pub trait MusicOutput { /* play/set_volume/stop/is_playing/fade_filter */ }
```

`MusicPlayer::update(ctx, out)` ports `SoundControl.update()` order exactly:

1. finished-track detection (`current` not playing → clear, `fade = 0`);
2. `lowpass_poll` every 30 delta-frames (`timer.get(1, 30f)`): `out.fade_filter(wet = paused ? 1 : 0, 0.4)`;
3. `was_playing` transition: leaving game → stop sound voices, reset filter; entering game → restore filter defaults (Godot glue, §3.5);
4. pause mirror: `out.set_paused(ctx.paused)`;
5. `keep_silent` → `stop()`; `is_menu` → launchMusic if planet UI, else editor if editor UI, else `menu`; `editor_rules` → `editor`; otherwise `silence()` then, unless `disable_music`: `always_play_music()` → `play_random()` when `current == None`, else if `now_ms - last_played_ms > 1000 * music_interval / 60` and `chance(music_chance)` → set `last_played_ms`, `play_random()`;
6. loop mixer update is called by the `SoundControl` frame driver after `update()` (upstream order: `updateLoops()` last).

`play_once(music)`: no-op if `current != null || music == None || musicvol == 0`; `last_random_played = music`; `fade = 1`; `current = music`; `volume = volume_multiplier()`; looping false; play.
`play(music)`: if `!should_play()` → set current volume 0, `fade = 0`, return; current volume = `fade × volume_multiplier()`; if `silenced` return; `current == None && music != None` → start, `fade = 0`; `current == music` → `fade = clamp(fade + delta/fin_time)`; else fade out (`fade - delta/fout_time`), and when `fade <= 0.01` stop, `silenced = true`, start new track at `fade = 0`.
`play_music(music, interrupt)`: if interrupt and current → stop/clear; `play_once`; if `current == music` → `silenced = true`.
`stop()`: `silenced = true`, stop/clear current if any, `fade = 0`. `keep_silent()` sets the flag.
Playlist getters port the exact override chain including upstream's quirk that **boss selection falls back to `planet.darkMusic`** (`getBossMusic`: `rules.darkMusic` override → `planet.darkMusic` → default `bossMusic`).
`is_dark(ctx, rng)`: `core_hp < 0.85` → true; `chance((((wave - 17)/19).log10() + 1)/4)` → true; `chance(enemies/70 + 0.1)` (negative/NaN probability ⇒ false, matching `rand < p` with NaN false).
`play_random(ctx, rng)`: `live_boss` → boss playlist; else `is_dark` → dark; else ambient; chosen element = Arc `Seq.random(exclude=last_random_played)` port (uniform over elements excluding the exclusion; `None` if empty/only excluded); advance `last_random_played`.
`volume_multiplier = musicvol_setting/100 × clamp(rules.volume, 0, 1)`; `should_play = musicvol_setting > 0`.
`always_play_music = rules.always || always_music_setting || planet.always`.
WaveEvent listener (registered client-side): schedule `wave_music_due_ms = now + rand(8..=15) * 1000`; on due, if `rules.disable → return`; `boss = boss_spawn_group` (computed from `Rules.spawns`: any group with `get_spawned(wave - 2) > 0 && group.effect == StatusEffects::boss`); boss → `play_once(boss playlist random)`; else `chance(music_wave_chance)` → `play_random()`.
`reload()`: `current = None; fade = 0`; restore default playlists (`ambient = [game1, game3, game6, game8, game9, fine]`, `dark = [game2, game5, game7, game4]`, `boss = [boss1, boss2, game2, game5]`); re-derive UI-bus routing; fire `MusicRegisterEvent` (05 `SimEvent`; client-side listeners may append to playlists; fired on `ClientLoadEvent`).
`ResetEvent` handler: `last_played_ms = now`; stop all Sound voices; reset ambient poller + loop mixer; re-fire nothing.

### 3.5 Mixing/routing — Godot buses, volumes, lowpass, pause

Bus layout (created by `buses.rs::ensure_buses()` at `MindAudio` ready; verified in §7c; a committed `client/audio/bus_layout.json` documents it for plan 22):

| Bus | Send | Effects | Volume source | Used by |
|---|---|---|---|---|
| `Master` | — | — | 1.0 (Godot project master) | — |
| `Music` | Master | — | 1.0 (per-track volume carries `fade × musicvol/100 × Rules.musicVolume`) | music voices, audition |
| `Sound` | Master | slot 0 `AudioEffectLowPassFilter` (cutoff 20500 → 500 Hz, `FILTER_12DB`, resonance 1.0) | 1.0 (`sfxvol` is applied per call, §2.4/§3.3) | all `sounds/*` voices, loops, ambient |
| `UI` | Master | — | 1.0 | every sound whose registry category/file is under `sounds/ui/`, plus forced `coreLaunch` |

- UI routing is derived once in `reload()`: registry entry `file` starts with `sounds/ui/` → `BusKind::Ui`; `coreLaunch` → `Ui` (SoundPriority line 16). The registry's `category` field (03 §6.5) is the fast path.
- `set_music_volume`/`set_sfx_volume`/`set_ambient_volume` settings handlers update the player fields only (no bus gain), preserving Arc's per-call math; `ambientvol` scales `LoopData.cur_volume` inside `LoopMixer::update` exactly as upstream.
- Lowpass: `fade_filter(wet, 0.4)` starts a 0.4 s tween on the `Sound` bus filter `cutoff_hz` between `20500` (wet 0) and `500` (wet 1); `lowpass_wet()` returns the current synthetic wet for tests/inspector. The poll interval is 30 delta-frames (0.5 s at 60 fps) and commits to the target (upstream fades a continuously-evaluated parameter every 30 frames).
- Pause mirror: `VoiceManager::set_sound_paused(paused)` sets `stream_paused` on all Sound-bus voices/loops/ambient; Music/UI continue. Resume restores. This replicates `Core.audio.setPaused(soundBus.id, state.isPaused())` and the fact that a paused game freezes conveyor/turret loops.
- Leaving-game reset: stop all Sound voices + ambient and restore wet 0 (port of `soundBus.stop()/play()` + `setupFilters()`).

### 3.6 One-shot voice engine + `SoundPriority`

`VoiceManager` (gdext) preallocates a fixed pool at `MindAudio._ready()`:

- `MAX_VOICES = 128` `AudioStreamPlayer2D` world voices (`attenuation = 0.0`, `max_distance = 1e9`, `panning_strength = 1.0`, `bus` per sound) + `MAX_UI_VOICES = 16` non-positional `AudioStreamPlayer` on the `UI` bus + 1 music player + 1 audition player. No node creation after ready (test assert in §7c).
- `Voice { state: Idle|Playing, stream_id: SoundId, started_ms, priority, group, min_interrupt_s, check_frame, last_volume, loop_variant }`.
- Admission (`VoicePolicy`, pure, `mind-core`): count active voices per `SoundId` and per group; if under `max_concurrent` → play; else if some active instance has `elapsed < min_interrupt` → deny (upstream: not interrupted); else if new priority ≥ lowest active priority → replace the lowest-priority oldest voice; else deny. `checkFrame` path first: if the same sound played within 16 ms **and** the new volume is greater → intensify the last voice to `min(last_volume + volume, volume × 1.25)` (Arc `Sound.play` behavior) and return `Boosted`.
- Volume application: `At` → `calc_falloff(x,y) × sfxvol × volume`, discard `< 0.005`; `calc_falloff = clamp(falloff / max(dst2 - falloff_offset², 0), 0, 1)` with `falloff = 16000`; `Play` → `volume`; pitch `clamp(pitch × global_pitch, 0.0001, 10)`; pan `clamp(pan, -1, 1)` then `set_pan`; NaN/Inf sanitized to 0/1 exactly as Arc.
- `set_pan(pan)` places the node at `listener.x + pan × viewport_width/2` with `y = listener.y`, `attenuation = 0`; falloff is applied via `volume_linear` (deviation 5).
- `SoundPriority` spec table (§6.3) is ported in `priority.rs::table()` and applied at `SoundPriority::init()` equivalent after `ClientLoadEvent`; default `max_concurrent = 6` for every sound (`Audio.defaultSoundMaxConcurrent`) and `min_interrupt = min(0.25, length×0.5)` for every sound before specific overrides.
- Length for `min_interrupt_fraction` comes from the loaded stream (`stream.get_length()`); headless goldens use the registry-provided length (03 may emit lengths; otherwise tests use fixed lengths from a fixture table).

### 3.7 Positional loop aggregation, `SoundLoop`, weather

`LoopMixer` (`mind-core`, pure) mirrors `SoundControl.loop`/`updateLoops`:

- `accumulate(sound, pos, volume, pitch)`: `base = calc_falloff(pos.x, pos.y)` (camera-relative; the camera view is passed into the mixer each frame, matching `Core.camera.position`); `vol = base × volume`; `data.volume = clamp(data.volume + vol, 0, 1)`; `data.pitch += pitch × vol`; `data.total += base`; `data.total_volume += vol`; `data.sum_x += x × base`; `sum_y += y × base`.
- `update(avol, in_game, paused)`:
  - not in game → `clear()` and return (upstream `sounds.clear()`);
  - paused → return without resetting (upstream freezes);
  - per sound: `cur_volume = lerp_delta(cur_volume, volume × avol, 0.11)` (Arc `lerpDelta` = `a + (b-a) × (1 - (1-α)^delta)` with `delta = 1` per frame); `pan = total == 0 ? 0 : calc_pan(sum_x/total, sum_y/total)`; `pitch = total_volume == 0 ? 1 : data.pitch/total_volume`; if no live voice and `cur_volume > 0.01` → start loop voice (`loop=true` variant, protected); if live voice and `cur_volume <= 0.001` → stop, clear; else update pan/volume/pitch; then zero the accumulators.
  - ambient-thread results merge into the same map before this pass.
- Voice protection: upstream `Core.audio.protect(id, true)` prevents SoLoud voice stealing; our voice manager marks loop voices as non-stealable (`VoiceClass::Loop`).
- `SoundLoopState` (`mind-core`): `base_volume`, `volume`, `voice_key`, `fade_speed = 0.05`; `update(x, y, play, volume_scl)`: if `base_volume <= 0` no-op; no voice and `play` → start at `calc_falloff(x,y) × volume × base_volume × volume_scl`; else fade `volume ± fade_speed × delta` (delta = 1 per frame), stop at `≤ 0.001`, else update volume/pan. Emits `AudioEvent::LoopInstance` so client and headless share the math; the gdext side maps `VoiceKey` to one protected voice per key (turret/unit keyed).
- Weather: `WeatherDef` volume fields; `WeatherState` update emits `LoopCamera { sound, volume = max((sound_vol + noise) × opacity, sound_vol_min) }` with `noise = |Noise.rawNoise(Time.time / sound_vol_osc_scl)| × sound_vol_osc_mag` (computed by the weather update system, plan 12/16 data; `Noise` port shared with 17); `sound == None` skips.

### 3.8 Ambience — `AmbientSource` + 20 FPS poll

- Trait parity: `AmbientProvider` implementations expose `{ position, is_valid, should_ambient_sound, ambient_volume, ambient_sound }`. Building registration equivalent: plan 07/09 building behaviors report `ambientSound != None` sources; the provider is queried on the main thread when building a snapshot (no registration lifetime bug possible for destroyed buildings — invalid entries just disappear, mirroring `isValid()` swap-removal).
- `AmbientSnapshot { entries: Vec<AmbientEntry> }` rebuilt (reused allocation) at the 20 Hz poll cadence; `AmbientPoller` (gdext) runs on the main thread in `MindAudio._process` with a 50 ms accumulator (default, OD-18-A): `poll(snapshot) -> Vec<AmbientSoundData>` where each entry is `loop(...)`-accumulated into a per-sound aggregate (`volume/pitch/total/totalVolume/sumX/sumY`), matching `AudioThread.doLoop()` including "append to output only when the sound was silent and became audible" semantics.
- Ordering: the ambient results merge into `LoopMixer` before `update()` each poll; `updateLoops` resets accumulators each frame; the poller only computes when `is_playing` (upstream thread breaks on menu and skips when not playing).
- Thread/ownership boundary (default): **no second thread**; the poller reads the snapshot built by sim/view code on the main thread and writes aggregates into the mixer's scratch. Optional worker mode (OD-18-A) sends `Arc<AmbientSnapshot>` over a channel and receives `Vec<AmbientSoundData>`; either way the ECS is never accessed off-thread. `Vars.headless` equivalent: the whole `MindAudio` node does not exist headless, so no snapshots are built.
- Volume overrides (`ambientVolume()`): `PowerGenerator` (production), `ImpactReactor` (warmup), `Drill` (`shouldConsume`-ish), `BurstDrill` (`× progress^4`) are read from plan 09/07 read models; default `should_ambient_sound()` = `shouldConsume()` equivalent exposed by the building.

### 3.9 `MusicContainer` + MapAudioView audition

- `MusicRef` is the serializable lazy handle (name only). Resolution is `MusicTable::find(name)` (cache of resolved `AudioStream` names) deferred until playback, matching `MusicContainer.accessed`.
- Audition (`MapAudioView`): `MindAudio.audition_play(name) -> bool` resolves `dp-<name>` first (20's DataAudioLoader overlay), else `findMusic`; plays through the audition player (`Music` on Music bus at full volume / `Sound` on UI bus), tracks `count_playing`; `audition_playing()`/`audition_position()`/`audition_length()` feed the dialog slider; `audition_stop()`; while playing, the model calls `keep_silent()` every frame (upstream `act()` calls it while `lastPlaying.countPlaying() > 0`), so the regular music stops on the next update. Delete/reload of data assets stops the audition (`MapAudioView.remove` behavior).
- `MindAudio.play_oneshot(sound, volume, pitch, pan)` is the GDScript/UI one-shot path (plan 14's `play_ui_sound` routes here) and always plays on the UI bus for `sounds/ui/*` names.

### 3.10 Godot / STDB surfaces touched

- **Node:** `/root/Spine/MindAudio` (`Node`; 2D voices parented under an internal `Node2D`). `AudioListener2D` is appended to `/root/Spine/World/Camera2D` and made current at ready (plan 16 camera).
- **`MindAudio` `#[func]` stable test API:** `is_ready()`, `bus_index(name) -> int`, `bus_volume_db(name) -> float`, `current_track() -> String`, `current_track_playing() -> bool`, `force_music(name, interrupt) -> bool`, `stop_music()`, `keep_silent()`, `lowpass_wet() -> float`, `lowpass_cutoff() -> float`, `sound_bus_paused() -> bool`, `voice_count() -> int`, `loop_volume(sound: String) -> float`, `loop_voice_count(sound: String) -> int`, `pending_events() -> int`, `drain_log() -> PackedStringArray`, `play_oneshot(sound, volume, pitch, pan) -> int`, `debug_emit_loop(sound, x, y, volume)`, `stats() -> Dictionary` (`{mix_us_p50, mix_us_p99, voices, loop_sounds, events, allocs, tracks_played}`), `set_setting(name, value)` (musicvol/sfxvol/ambientvol/alwaysmusic through plan 04's store).
- **Inspector:** `/root/Spine/Ui/StateInspector/Audio` row reads `stats()`/`current_track()`/`lowpass_wet()` every 250 ms (plan 00 protocol).
- **STDB:** no tables/reducers/views. Sound events are local-only; plan 21 must not relay them (relayed chat/menu results may trigger local UI sounds only).

### 3.11 Boundaries & invariants

1. `mind-core` never imports Godot or tokio; `cargo tree -p mind-core | rg "godot|tokio"` stays empty. All Godot audio objects live in `mind-gdext`.
2. Sim code emits `AudioEvent`s unconditionally; the sink performs headless/no-op gating. No `Vars.headless` branch inside sim systems.
3. Audio uses a frame clock (`delta × 60`, `now_ms`), never `SimClock` for scheduling, and never writes sim resources (`SimClock.runs` is checksummed).
4. No `HashMap` iteration anywhere that produces goldens; `LoopMixer` uses `IndexMap<SoundId, LoopData>` with `reserve(64)`; all per-frame buffers (event queue, snapshot, scratch) are reused; measured zero steady-state allocations (§7d).
5. Content names, registry ids, setting keys (`musicvol`, `sfxvol`, `ambientvol`, `alwaysmusic`), sound names and playlist members are ABI; nothing renames or re-cases them.
6. `SoundId::UNSET` and `SoundId::NONE` never play; `Sounds.none` is the safe default for every defaulted field.
7. UI and Music buses are never paused by the game-pause mirror; only Sound-bus voices pause.
8. A missing stream degrades to silence with one logged error; it never panics and never blocks the frame.
9. Every ported file carries the GPL header; Arc audio classes are fetched oracles, not vendored code.

### 3.12 Sibling reconciliation (by filename)

| Sibling | Interface consumed/provided | Reconcile action |
|---|---|---|
| `02_CONTENT` | **Consumes:** complete sound/music fields on content records: `BlockDef { ambient_sound: SoundId, ambient_sound_volume: f32, destroy_sound: SoundId, destroy_sound_volume: f32, destroy_pitch_min/max: f32, land_music: MusicRef (CoreBlock/Accelerator) }`; `UnitTypeDef { death_sound, death_sound_volume, wreck_sound, wreck_sound_volume, loop_sound, loop_sound_volume, move_sound, move_sound_volume, move_sound_pitch_min/max, step_sound, step_sound_volume, step_sound_pitch, step_sound_pitch_range, tank_move_sound, tank_move_volume, mine_sound, mine_sound_volume }`; `WeaponDef.shoot_sound`/`active_sound`/volumes; `BulletDef { hit_sound, hit_sound_volume, despawn_sound, heal_sound, heal_sound_volume }`; `WeatherDef { sound, sound_vol, sound_vol_min, sound_vol_osc_mag, sound_vol_osc_scl }`; `PlanetDef { launch_music, ambient_music, dark_music, always_play_music }`. **Provides:** `SoundId`/`MusicRef` definitions (02 imports them; 02 §6.1's sample field lists do not mention these fields — orchestrator must confirm they are added). | Orchestrator: add the field lists to 02's records; `SoundId` serde-by-name uses 02's name resolution. |
| `03_ASSETS` | `sounds.index.json` (`{name,file,id,category}`), `Musics` name list, `SoundRegistry` lookup APIs (`resolve_sound`, `id->name`), lazy `AudioStream` cache hook (03 M8 "plan-18 handshake test"), `AssetsReadyEvent`. **Provides:** UI-bus category from `file`; `findMusic` uses `resolve_sound`. | 03 M8's "lazy `AudioStreamOggVorbis` cache" stays in `mind-gdext`; 18 owns `StreamCache` and the two-variant loop duplication. |
| `04_IO` | `SettingsStore` for `musicvol/sfxvol/ambientvol/alwaysmusic`; `MusicContainer → name string` serialization (04 §3.4). | 04 already serializes `MusicContainer`; 18 defines `MusicRef` as the same name-string shape. |
| `05_SIM_CORE` | `EventBus` registration for `MusicRegisterEvent` (05 `SimEvent`; 18 needs the variant), `WaveEvent`, `ResetEvent`, `ClientLoadEvent`; `SimClock::tick` for event stamping. **Provides:** `AudioSink` (a `bevy_ecs` resource, not a sim event); no sim system order change. | Orchestrator: add `MusicRegisterEvent` to 05's `SimEvent` enum (18 is its first consumer); register `AudioPlugin` after plan 05's plugins. |
| `10_COMBAT` | All `CombatFx`/`FxSink` sound hooks (17 supersedes `CombatFx`, 10 §3.14): `shoot_sound`, `charge`/`active` sounds, turret `soundLoop`, bullet `hitSound`/`despawnSound`/`healSound`, projectile loop sound. **Provides:** `SoundId` type + `AudioEvent` constructors; `SoundLoopState` helper for `Turret.sound_loop`. | 10 keeps calling the sink; volume/pitch formulas stay in 10 (call-site parity). |
| `11_UNITS` | Unit dead/wreck/step/move/loop sounds and `tankMove`/`mechStep` calls, `Units::unit_death` wreck sound site. **Provides:** same helpers. | Reconcile: unit loop call sites emit `LoopAdd` (not `SoundLoopState`) unless upstream used `SoundLoop`. |
| `12_CAMPAIGN` | `Rules.{ambient_music, dark_music, always_play_music, disable_music, music_volume}` (already present in 12 §3.3), `RulesLoadEvent` (playlists re-read per frame from the current `Rules`), `state.enemies`, `boss()`, `GameState.is_campaign`, weather `WeatherDef` runtime values, `Sector/Planet` lighting for `state.opacity`. **Provides:** `MusicRef` for `Option<Vec<MusicRef>>` (`MusicContainerRef` in 12's code is 18's `MusicRef`). | Orchestrator: rename/alias 12's `MusicContainerRef` → `mind_core::audio::MusicRef`. |
| `13_LOGIC` | `@sfx-*` ids from 18's `SoundTable`; `PlaySoundI`/`PlayMusicI` emit `AudioEvent::{At, Play, MusicPlay, MusicStop}`; `playmusic` uses `findMusic`+`playMusic(interrupt)`. | 13 §3.10/§8 R5 already expects the table; freeze constructor signatures in 18 M0. |
| `14_UI` | `MindUi.has_dialog()` (pause lowpass + `wasPlaying`/pause state), menu/editor/planet visibility for menu music, `ui.planet.state.planet.launchMusic`, settings rows, `play_ui_sound(name)`. **Provides:** `MindAudio.play_oneshot`, `keep_silent`, `audition_*`, `stats()`. | Orchestrator: wire plan 14's `play_ui_sound` to `/root/Spine/MindAudio` (path per OD16-J). |
| `16_RENDER` | Camera position/width (falloff/pan), `AudioListener2D` placement, post-`SimHost` frame slot, `Trigger.draw*` irrelevant. `Shaders.java` was checked: it has **no** audio volume handling (only ambient-color uniforms), so no interface exists there. **Provides:** no render changes; audio is a sibling frame consumer. | 16 §3.13 lists 18 as not-on-disk with contracts frozen; this plan fixes them. |
| `17_FX` | `FxSink::sound(SoundParams)` implementation forwards to 18's `AudioEngine::play_at`; `SoundParams.sound: SoundId` (17 §6.1). **Provides:** `SoundId` + `AudioSink`. | Orchestrator: 17's gdext sink calls `MindAudio`; avoid a second playback path. |
| `20_MODS` | `OverlaySound`/`dp-` names, `DataAudioLoader`, `FileTree` mod roots; `findMusic` mod fallback. **Provides:** audition + `findMusic` consumer. | 20 supplies overlay streams; 18 treats them as registry-less names resolved through `FileTree`. |
| `19_MAPS_EDITOR` | `MapAssetsDialog`/`MapAudioView` UI calls `MindAudio.audition_*`/`keep_silent`. | Shell ownership in 19; no audio logic there. |
| `21` / `22` / `23` | 21: no audio over the wire. 22: bus layout export check + mobile audio session. 23: registers `audio_*` scenarios/goldens. | Notes only. |

## 4. Port map

| Mindustry / Arc source | Target | Notes on adaptation |
|---|---|---|
| `core/src/mindustry/audio/SoundControl.java` | `mind-core/src/audio/{music,loops,math}.rs` + `mind-gdext/src/audio/sound_control.rs` | Pure state machine/mixer in core (testable); Godot object plumbing in gdext. `Time.delta` → `delta × 60`. |
| `audio/SoundPriority.java` | `mind-core/src/audio/priority.rs` (`SoundPrioritySpec` table) + `mind-gdext/src/audio/voices.rs` (policy application) | Table data in core; admission/eviction in `VoicePolicy`; defaults `max_concurrent = 6`, `min_interrupt = min(0.25, len×0.5)`. |
| `audio/AmbientSource.java` | `mind-core/src/audio/loops.rs` (`AmbientProvider`, `AmbientSnapshot`, `AmbientSoundData`) + `mind-gdext/src/audio/ambient.rs` | Interface + snapshot in core; poller in gdext. |
| `audio/SoundLoop.java` | `mind-core/src/audio/loops.rs::SoundLoopState` + gdext `LoopVoice` | Fade 0.05/frame, calcVolume/calcPan, `stop()`. |
| `audio/MusicContainer.java` | `mind-core/src/audio/ids.rs::MusicRef` + gdext `StreamCache` | Lazy `findMusic` cache (`accessed` semantics). |
| generated `mindustry.gen.Sounds` / `Musics` | plan 03 `mind-core/src/assets/generated.rs` + `mind-core/src/assets/sounds.rs`; consumed via `SoundTable`/`MusicTable` | No codegen here; ids/names from `sounds.index.json`. |
| Arc `arc/audio/Sound.java` | `mind-core/src/audio/math.rs` (`calc_falloff`/`calc_pan`) + gdext `voices.rs` (`at`/`play`/`loop`, min-interval, clamps, discard) | Fetched oracle; behavioral port. |
| Arc `arc/audio/Music.java` | gdext `voices.rs::MusicVoice` + core `MusicPlayer` | `play/stop/setVolume/setLooping/isPlaying/setPosition/getLength` mapped to `AudioStreamPlayer`. |
| Arc `arc/audio/Audio.java` (`falloff=16000`, `defaultSoundMaxConcurrent=6`, `sfxVolume`, `setPaused`, `protect`, `countPlaying`) | `mind-core::audio::math` constants + gdext `voices.rs`/`buses.rs` | `sfxVolume` updated from settings in the frame driver. |
| Arc `arc/audio/{AudioBus,AudioSource,Filters}` | gdext `buses.rs` | `uiBus` → `UI` bus; `soundBus` → `Sound`; `musicBus` → `Music`; `BiquadFilter` → `AudioEffectLowPassFilter`. |
| `core/Control.java` (sound create/update, menu music, core `landMusic` play) | gdext `sound_control.rs` frame driver + `Client` boot | `Core.landMusic` play path is plan 12's `Trigger::new_game` listener calling `MindAudio.force_music(land_music, true)`; volume `musicvol/100`. |
| `game/Rules.java` music fields | `mind-core::audio::music::MusicRules` adapter over 12's `Rules` | No ownership change. |
| `type/Planet.java` music fields | `mind-core::audio::music::PlanetMusic` adapter over 02's `PlanetDef` | Launch/ambient/dark/always. |
| `type/Weather.java` sound block | sim-side `LoopCamera` emission (12/16) + `LoopMixer` | Noise/opacity computed by the weather system. |
| `world/Block.java` ambient fields | `02 BlockDef` + 07/09 provider hooks | `ambientVolume()` overrides read from building read models. |
| `entities/effect/SoundEffect.java` | 17's `EffectKind::Sound` → `FxSink::sound` → 18 `AudioEngine::play_at` | Deferred `startDelay` is 17's deferred spawn; 18 plays at delivery time. |
| `logic/LExecutor.java` `PlaySoundI`/`PlayMusicI` | `AudioEvent::{At,Play}` / `{MusicPlay,MusicStop}` | Positional path `Min(volume,2)`; `check_frame = limit`; non-positional `volume × sfxVolume`, clamp 2. |
| `editor/data/MapAudioView.java` | gdext `audition.rs` + `MindAudio.audition_*`; dialog in 19 | Play/pause icon, slider, `keepSilent` loop. |
| `ui/dialogs/SettingsMenuDialog.java` (sound table) | plan 14 settings rows → plan 04 store; `MindAudio.set_setting` for tests | Keys unchanged. |
| `ClientLauncher.java` (Musics/Sounds load, `SoundPriority.init()`) | 03 boot stages + `MindAudio` `ClientLoadEvent`/`AssetsReadyEvent` handling | `init()` runs once after assets are ready. |

## 5. Milestones & task breakdown

Each milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, the named harness command and a Changelog entry with evidence. Order is strict. Smallest vertical slice first (M0: one event → one audible UI voice).

**M0 — Boundary + registry + vertical slice.**
- `audio/{mod,ids,events,math,priority}.rs` types; `AudioSink`/`NoopAudioSink`/`RecordingAudioSink`; `AudioPlugin`.
- `mind-gdext/src/audio/{mod,streams,buses,voices}.rs` skeleton: buses, `StreamCache`, `VoiceManager` with 8 voices + UI bus; `MindAudio` node + `play_oneshot`.
- Wire the block-place call site (07) as the first emitter; UI bus routing for `sounds/ui/*`.
- *Verify:* `cargo test -p mind-core audio::tests::{sound_id_serde_by_name, falloff_and_pan_values, policy_defaults}`; `mind-headless audio events --scenario audio_events_blocks --dump out/audio_events.json` records `blockPlace*`; MCP: `MindAudio.play_oneshot("uiButton",1,1,0)` → `voice_count() > 0` and `bus_index("UI") != -1`.

**M1 — Buses, volumes, lowpass, pause.**
- `buses.rs` final layout + `bus_layout.json`; settings binding (`musicvol`/`sfxvol`/`ambientvol`/`alwaysmusic` via 04); lowpass tween + 30-frame poll; pause mirror; leaving-game reset; `ResetEvent` handler.
- *Verify:* `audio::music::tests::volume_multiplier_and_should_play`; MCP §7c steps 4–6 (bus names, lowpass sweep, pause flag).

**M2 — Music state machine.**
- `music.rs` full `MusicPlayer` + `MusicContext`; playlists/overrides/`reload()`; dark/boss heuristics; menu/planet/editor game branches; `play_once`/`play_music`/`keep_silent`; WaveEvent frame-clock delay; `MusicRegisterEvent` wiring in 05.
- *Verify:* `audio::music::tests::{menu_selection, editor_selection, game_random_chance, dark_heuristic_golden, boss_spawn_group_flag, play_once_never_interrupts, play_music_interrupt, fade_crossfade_math}`; `mind-headless audio music --scenario audio_music_select --frames 600 --seed 7`.

**M3 — Voice engine + `SoundPriority`.**
- `voices.rs` full pool (128+16), admission/eviction/`checkFrame` boost, clamps/discard, `SoundPriority::init` application; `priority.rs` golden.
- *Verify:* `audio::priority::tests::{table_matches_sound_priority_init, admission_policy, group_limits, interrupt_window, boost_existing}`; `mind-headless audio policy --requests tests/fixtures/audio/requests.json`.

**M4 — Loops, `SoundLoop`, weather, ambience.**
- `LoopMixer`, `SoundLoopState`, `AmbientProvider`/snapshot/poller, weather emission, call-site migrations (07/08/09/10/11/12/13/17 per §3.12).
- *Verify:* `audio::loops::tests::{aggregation_math, lerp_and_reset, menu_clear_pause_freeze, sound_loop_fade, ambient_merge_silent_gate}`; `mind-headless audio loops --scenario audio_loop_aggregate --ticks 120`.

**M5 — `MusicContainer`, mod audio, audition.**
- `findMusic` mod/`dp-` chain; `StreamCache` overlay registration for 20; `audition.rs` + `MindAudio.audition_*`; `keepSilent` interaction.
- *Verify:* `audio::tests::find_music_chain`; fixture mod audio (20's fixture) auditioned via MCP eval; missing-file degradation test.

**M6 — Perf, MCP sweep, docs, exit.**
- `audio bench`; alloc audit; `MindAudio.stats()` inspector row; §7c end-to-end; `parity/ledgers/audio.md`; exit checklist green; reconciliation notes to 02/05/12/13/17 owners.

## 6. Data & formats

### 6.1 `SoundId` / `MusicRef`

```rust
#[repr(i32)] pub enum SoundId { None = -1, Unset = -2, Id(u16) }   // serde = name string
pub struct MusicRef(pub String);                                    // serde = name string
```
- Content JSON/TypeIO use names (`"none"`, `"shootDuo"`); runtime lookup through the compact `SoundTable` built from `sounds.index.json` at asset boot. `MusicRef` has no id space.
- `SoundTable::get_or_none(id)` (Java `Sounds.getSound`), `SoundTable::id(sound)` (Java `getSoundId`), `SoundTable::all()` (Java `Core.assets.getAll(Sound.class)` for bus/min-interrupt loops). Logic `@sfx-<name>` uses `SoundTable::id("sfx-"+name)` (names are registered by plan 13 from the same table).

### 6.2 `AudioEvent` dump (`mind-headless audio events`)

```json
{"format":1,"tick":417,"events":[
  {"kind":"at","sound":"shootDuo","x":512.0,"y":288.0,"pitch":1.03,"volume":1.0,"check_frame":true},
  {"kind":"loop_add","sound":"loopConveyor","x":520.0,"y":300.0,"volume":1.0,"pitch":1.0},
  {"kind":"loop_instance","key":17,"sound":"loopMineBeam","x":520.0,"y":300.0,"play":true,"volume_scl":0.8},
  {"kind":"music_play","name":"game1","interrupt":true}]}
```
- Sorted by `(tick, emission order)`; written by `RecordingAudioSink::dump`. Golden files at `tests/golden/audio/`. Variants append-only.

### 6.3 `SoundPriority` table (exact port, applied in this order)

| Priority | Sounds |
|---|---|
| 3.0 | `acceleratorLaunch`, `acceleratorCharge`, `coreLand`, `coreLaunch` |
| 2.0 | `beamMeltdown`, `beamLustre`, `beamPlasma`, `explosionReactor`, `explosionReactor2`, `explosionReactorNeoplasm`, `explosionCore`, `blockExplodeElectricBig`, `blockExplodeExplosive`, `blockExplodeExplosiveAlt` |
| 1.5 | `shootMeltdown`, `shootSublimate`, `shootForeshadow`, `shootConquer`, `shootCorvus`, `chargeCorvus`, `chargeVela`, `chargeLancer`, `shootReign`, `shootEclipse`, `shootArtillerySapBig`, `shootToxopidShotgun`, `beamPlasmaSmall`, `shootNavanax`, `explosionNavanax` |
| 1.0 | `loopConveyor`, `loopSmelter`, `loopDrill`, `loopExtract`, `loopFlux`, `loopHum`, `loopBio`, `loopTech`, `loopUnitBuilding` |
| -1.0 | `blockHeal`, `healWave` |
| -2.0 | `mechStep`, `mechStepHeavy`, `walkerStep`, `walkerStepSmall`, `walkerStepTiny`, `mechStepSmall` |

| Setting | Sounds |
|---|---|
| `max_concurrent = 7` | `beamPlasma`, `shootMeltdown`, `beamMeltdown` |
| `max_concurrent = 5` | `shootLancer`, `mechStep`, `mechStepHeavy`, `walkerStep`, `walkerStepSmall`, `walkerStepTiny` |
| `max_concurrent = 4` | `shieldHit` |
| `default max_concurrent = 6` | every other sound (Arc `Audio.defaultSoundMaxConcurrent`, applied at load) |
| group 1 | `shootFlame`, `shootFlamePlasma` |
| group 2 | `shootMissile`, `shootMissileShort`, `shootMissilePlasmaShort` |
| group 3 | `shootArc`, `shootPulsar` |
| `min_interrupt_fraction(0.25, 0.5)` | every sound |
| `min_interrupt(0.5)` | `mechStepSmall`, `mechStep` |
| `min_interrupt(0.6)` | `walkerStep`, `mechStepHeavy` |
| `falloff_offset` | `explosionCore = 100`, `blockExplodeElectricBig = 70` |
| forced bus | `coreLaunch` → `UI` |

All other sounds: priority 0, unique group, no falloff offset.

### 6.4 Settings keys and bus layout

- `musicvol` int 0..100 (default 100) → music multiplier; `sfxvol` int 0..100 (default 100) → `sfxVolume`; `ambientvol` int 0..100 (default 100) → loop/ambient multiplier; `alwaysmusic` bool (default false). Stored/served by plan 04; plan 14 renders the rows (already in its settings catalogue). The frame driver re-reads `sfxvol` every frame (Arc `ApplicationListener.update` behavior).
- `client/audio/bus_layout.json` (tracked; verified at runtime): bus list, send targets, effect slot, initial cutoff. `client/default_bus_layout.tres` is intentionally not required (programmatic creation guarantees parity); plan 22 validates exports.

### 6.5 Music selection oracle (`mind-headless audio music`)

```json
{"format":1,"frames":600,"seed":7,"context":{
   "state":"playing","wave":20,"enemies":40,"core_hp":0.5,
   "rules":{"disable_music":false,"always_play_music":false,"music_volume":1.0,
            "ambient_music":null,"dark_music":null},
   "planet":{"ambient_music":null,"dark_music":null,"always_play_music":false},
   "musicvol_setting":100},
 "trace":[{"frame":0,"action":"play_random","playlist":"dark","track":"game2"},
          {"frame":120,"action":"fade","value":0.5}, ...]}
```

### 6.6 Loop aggregate oracle (`mind-headless audio loops`)

Per-frame rows: `{"tick":n,"sound":"loopConveyor","volume_raw":0.42,"cur_volume":0.31,"pan":0.12,"pitch":1.0,"voice":"live"}` sorted by sound id; goldens under `tests/golden/audio/loops_*.json`.

### 6.7 Constants & capacities

```rust
pub const FALLOFF: f32 = 16000.0;
pub const DEFAULT_SOUND_MAX_CONCURRENT: i32 = 6;
pub const MIN_INTERVAL_MS: u64 = 16;
pub const FIN_TIME: f32 = 120.0;  pub const FOUT_TIME: f32 = 120.0;   // delta-frames
pub const MUSIC_INTERVAL: f64 = 3.0 * TIME_TO_MINUTES as f64;         // 10800
pub const MUSIC_CHANCE: f32 = 0.8; pub const MUSIC_WAVE_CHANCE: f32 = 0.46;
pub const LOOP_LERP: f32 = 0.11;  pub const SOUND_LOOP_FADE: f32 = 0.05;
pub const LOWPASS_CUTOFF_WET: f32 = 500.0; pub const LOWPASS_CUTOFF_DRY: f32 = 20500.0;
pub const MAX_VOICES: usize = 128; pub const MAX_UI_VOICES: usize = 16;
pub const AMBIENT_POLL_MS: u64 = 50;  // 20 FPS
```

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests

Mindustry has **no audio JUnit coverage** (`tests/src/test/java/**` grepped: no `Sound`/`Music`/`audio` references). Substitutes are deterministic unit/golden oracles:

| Oracle | Rust test (`mind-core`) | Notes |
|---|---|---|
| Arc `Sound.calcFalloff`/`calcPan` values | `audio::math::tests::falloff_and_pan_values` | `dst=0→1`, `dst²=16000→1`, `dst²=32000→0.5`, offset `100` on `explosionCore`, pan clamp ±0.9, NaN→0. |
| `Sound.play` min-interval boost | `audio::priority::tests::check_frame_boost` | 16 ms window; `lastVolume = min(last+vol, vol×1.25)`. |
| `SoundPriority.init` table | `audio::priority::tests::table_matches_java_init` | Golden JSON of the §6.3 table parsed from a fixture transcribed from `SoundPriority.java`. |
| Admission/eviction policy | `audio::priority::tests::{admission_policy, group_limits, interrupt_window, priority_replacement}` | Pure `VoicePolicy`. |
| `isDark` heuristic | `audio::music::tests::dark_heuristic_golden` | Seeded `AudioRng`; HP/wave/enemy branch coverage incl. wave ≤ 17 NaN/negative → false. |
| Playlist override chain (incl. boss→dark quirk) | `audio::music::tests::playlist_overrides` | Rules > Planet > defaults. |
| Menu/editor/planet selection | `audio::music::tests::{menu_selection, editor_selection}` | `is_menu`/UI flags. |
| `play`/`playOnce`/`playMusic`/fades | `audio::music::tests::{play_once_never_interrupts, play_music_interrupt, fade_crossfade_math, should_play_zero_volume}` | Frame counts exact. |
| WaveEvent boss/random | `audio::music::tests::wave_event_delay_and_boss` | 8–15 s frame clock; `boss_spawn_group` computation. |
| Loop aggregation math | `audio::loops::tests::{aggregation_math, lerp_and_reset, weighted_centroid_pan}` | Exact float compare where upstream arithmetic is deterministic. |
| Menu/pause loop semantics | `audio::loops::tests::{menu_clear, pause_freeze}` | `clear()` vs early return. |
| `SoundLoop` fade | `audio::loops::tests::sound_loop_fade` | `0.05`/frame, stop ≤ 0.001. |
| Ambient silent gate | `audio::loops::tests::ambient_merge_silent_gate` | Output only when `silent && volume > 0`. |
| `findMusic` chain | `audio::tests::find_music_chain` | registry → `.ogg`/`.mp3` → `music/` → mod tree. |
| Missing stream degradation | `audio::tests::missing_sound_is_none_no_panic` | One log, no panic. |
| `SoundId` serde | `audio::tests::sound_id_serde_by_name` | `none`/`unset`/names round-trip. |

### 7b. Headless harness scenarios (`mind-headless`)

```text
mind-headless audio events --scenario audio_events_blocks --dump out/audio_events.json
  # place/break/repair blocks + fire a turret at a dummy + run a wave timer;
  # expected: ordered At/LoopAdd/LoopInstance events with names/volumes/pitches;
  # golden tests/golden/audio/events_blocks.json (deterministic pitch via scenario seed).

mind-headless audio music --scenario audio_music_select --frames 600 --seed 7
  # scripted context timeline: menu → game (wave 4) → core HP 0.5 → enemy flood → boss wave → pause → menu;
  # expected: selection trace (playlists/tracks/fades) equals tests/golden/audio/music_select.json.

mind-headless audio loops --scenario audio_loop_aggregate --ticks 120
  # N scripted loop_add sources with positions/volumes + one SoundLoop instance + one weather loop;
  # expected: per-tick aggregate dump equals tests/golden/audio/loops_aggregate.json.

mind-headless audio policy --requests tests/fixtures/audio/requests.json --json
  # 100 synthetic requests against a max-7 sound, a group, and a min-interrupt window;
  # expected: admission decisions equal the golden.

mind-headless audio bench --ticks 3600 --voices 128 --loops 256 --json out/audio_bench.json
  # p50/p99 mix-update µs, zero allocations after warmup (test allocator).
```

### 7c. MCP playtest scenario (concrete; screenshots are not meaningful — state assertions only)

Preconditions follow plan 00 §7c/the repo skill: `godot_health check`; if `BRIDGE_NOT_CONNECTED`, launch the editor per the skill, wait ~20 s, `godot_instance list`. Every eval returns `{"pid": OS.get_process_id(), ...}` compared with `godot_game instances`.

1. `godot_editor_edit open_scene res://scenes/spine.tscn`; `godot_game play` with the scene path; wait for `[audio] ready` in `godot_log get`.
2. Bus layout: `godot_exec eval`:
   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   return {"pid": OS.get_process_id(), "music": a.bus_index("Music"), "sound": a.bus_index("Sound"),
           "ui": a.bus_index("UI"), "ready": a.is_ready()}
   ```
   expect all indices `> 0` and `ready`.
3. Menu track: `force_music("menu", true)` → `current_track() == "menu"` and `current_track_playing() == true` after ≤ 1 s.
4. Start a game (`/root/Spine/MindCampaign.start_sector("serpulo", 0)` per plan 12, or the plan-05 play command), then `set_setting("musicvol", 0)`; wait a frame; eval `current_track_playing()` volume path via `stats().current_volume == 0` (or `current_track() == ""` after fade). Restore `musicvol=100`; `force_music("game1", true)` → `current_track()=="game1"`.
5. Pause lowpass: eval initial `{"cutoff": a.lowpass_cutoff(), "wet": a.lowpass_wet()}` (wet 0, cutoff 20500). `godot_exec call /root/MindUi open_dialog ["settings"]`; wait 0.6 s; eval `{"cutoff": a.lowpass_cutoff(), "wet": a.lowpass_wet(), "paused": get_node("/root/Spine/SimHost").is_paused()}` → paused true, wet ≥ 0.9, cutoff ≤ 700. Close dialog; wait 0.6 s → wet ≤ 0.1, cutoff ≥ 19000.
6. UI bus one-shot: `a.play_oneshot("uiButton", 1.0, 1.0, 0.0)` → returns index ≥ 0; `stats().last_bus == "UI"`.
7. Loop path: `a.debug_emit_loop("loopConveyor", 512, 512, 1.0)`; step 2 frames; eval `{"vol": a.loop_volume("loopConveyor"), "voices": a.loop_voice_count("loopConveyor")}` → `vol > 0`, `voices == 1`; then `a.debug_emit_loop("loopConveyor", 512, 512, 0.0)` twice → voice stops.
8. Music override: `a.force_music("boss1", true)` → `current_track()=="boss1"`; `a.stop_music()` → not playing.
9. Event drain: `pending_events() == 0` and `drain_log()` contains no `missing audio` errors; `godot_log errors` empty.
10. Teardown: `godot_game stop`.

`tools/mcp-smoke.sh` adds steps 1–3 and 9 to the smoke path.

### 7d. Performance budget + measurement

Baseline: HLP §7.4 (16.6 ms/frame; sim ≤ 4 ms; plan-16 render build ≤ 5 ms p99). Audio (dev host, release, measured by `MindAudio.stats()` + `mind-headless audio bench`):

| Metric | Budget | Measurement |
|---|---|---|
| Rust mix update (`MusicPlayer.update` + `LoopMixer.update` + event drain, 256 loop sounds, 32 live voices) | p50 ≤ 0.10 ms, p99 ≤ 0.25 ms | `audio bench --loops 256 --voices 32`; in-engine `stats().mix_us_p99` |
| Event drain (200 events in one frame burst) | ≤ 0.05 ms | `audio bench --burst 200` |
| Ambient snapshot build + 20 Hz poll (2 000 candidate buildings) | ≤ 0.10 ms per poll | `audio bench --ambient 2000` |
| Voice nodes | ≤ 128 world + 16 UI + 2 music/audition, allocated before gameplay, no runtime creation | MCP eval `get_tree().get_node_count()` stability + `stats().voices` high-water |
| Steady-state allocations | 0 after warm-up | test allocator in `mind-core` benches; in-engine debug counter in `stats().allocs` |
| Audio CPU (Godot mix thread) | ≤ 1.0 % of a core at 32 voices | `godot_profiler series` + `Performance` monitor |

Regressions block the P6 gate; numbers recorded in `bench/baselines.json` (plan 23).

### 7e. Exit criteria checklist

- [x] `cargo test -p mind-core` green with every §7a row implemented; no Godot/network needed. (2026-10-03: 817 lib + 3+5+2+2+1 integration = 830 passed / 2 ignored; 43 audio tests.)
- [x] `cargo fmt --check` + `cargo clippy -p mind-core -p mind-gdext -- -D warnings` clean; `mind-core` boundary grep empty.
- [x] `audio events|music|loops|policy` scenarios pass with committed goldens; `audio bench` meets §7d. *Release (2026-10-03): `audio bench --ticks 3600 --loops 256 --voices 32 --ambient 2000` → p50 **11.3 µs** / p99 **46.6 µs** (budget 250 µs); `events_sim` golden added; `mind-headless/tests/audio_golden.rs` runs all five.*
- [x] Sim call-site emitters routed to `AudioSink`: block place/break (`BuildHarness`), turret shoot (`turrets::shoot`), bullet hit/despawn (`combat::bullet`), unit death/wreck (`unit::kill_unit`); `audio events --scenario audio_events_sim` dumps real-harness events.
- [ ] Bus layout verified in-engine (Master→Sound/Music/UI + Sound lowpass slot 0). *(MCP deferred; code + `bus_layout.json` landed.)*
- [x] Every `SoundControl` branch asserted: menu/planet/editor/game, dark/boss, disable/always, keepSilent/stop, fades.
- [x] Volume formulas asserted: `musicvol × Rules.musicVolume` per track; `sfxvol` per-call semantics; `ambientvol` in loops.
- [ ] Dialog pause: lowpass target reached in ≤ 0.6 s, pause mirror set, resume restores; UI/Music unaffected. *(Core sweep math + pause fields landed; in-engine MCP deferred.)*
- [x] `SoundPriority` table golden matches the Java transcription; admission/eviction tests pass.
- [x] Loop aggregation matches upstream math; menu clear + pause freeze + voice start/stop asserted.
- [x] `SoundLoop` fade and stop asserted; weather loop emission asserted headless. *(`LoopCamera` event + `AmbientPoller`; sim weather call site is plan 12/16.)*
- [x] Ambient poll runs at 20 Hz, no ECS access off-thread, `isValid` disappearance handled. *(Core `AmbientPoller`; gdext poller wiring pending sim/view snapshot.)*
- [x] `MusicContainer` lazy resolve + `findMusic` mod/`dp-` chain + audition `keepSilent` asserted. *(`findMusic` + `audition_name` `dp-` overlay chain; gdext `audition_*`/`keep_silent`; in-engine audition remains in the §7c sweep.)*
- [x] Missing/`none`/`unset` degrade to silence with one logged error; no panic.
- [ ] MCP §7c executed; evidence (eval outputs, log excerpts) in the Changelog; `godot_log errors` clean. *(**DEFERRED** to the single-editor mutex — recorded in §7c and the ledger; code paths + `bus_layout.json` landed, `tools/mcp-smoke` steps queued with the orchestrator.)*
- [x] No per-frame allocation after warm-up; voice cap enforced. *(`audio::tests::steady_state_audio_allocates_nothing` with `--features alloc-audit` reports **0** allocs across 1000 steady-state mix/poll frames; 128+16 voice cap preallocated.)*
- [x] Reconciliation notes delivered to 02/05/12/13/17 owners; §3.12 rows resolved or explicitly deferred. *(HLP §13 + `parity/ledgers/audio.md`.)*
- [x] GPL headers on every new file; `parity/ledgers/audio.md` complete (0 unported upstream behaviors).

## 8. Risks & open decisions

Each item has the default this plan proceeds with. Items marked **NEEDS USER DECISION** are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`; execution continues on the stated default unless overridden.

| # | Risk / decision | Default taken | Status |
|---|---|---|---|
| OD-18-A | Ambient polling model: real `std::thread` + channel (upstream `AudioThread`) vs main-thread 20 Hz accumulator. | **Main-thread 20 Hz poller** over a pre-built `AmbientSnapshot`; worker mode behind the same trait if measured too expensive. No ECS access off-thread either way. | **locked 2026-10-01 (NUD-41=A)** |
| OD-18-B | Godot has no filter wet/dry parameter. | Lowpass "wet" = cutoff sweep 20500→500 Hz (0.4 s, `FILTER_12DB`, resonance 1.0) on the Sound bus; `lowpass_wet()` reports the synthetic value. | **locked 2026-10-01 (NUD-42=A)** |
| OD-18-C | Arc linear pan vs Godot `AudioStreamPlayer2D` panning. | Falloff exact in Rust; pan via `set_pan` (node offset from listener, `attenuation = 0`); Godot 2D pan curve accepted. Not checksummed. | Default (reconcile with 16) |
| OD-18-D | Mod data audio (`dp-`) as streams: 03 `OverlaySound` vs 20 `DataAudioLoader`. | `findMusic("dp-"+name)` first for audition; overlay registration owned by 20 using 03's hook; 18 only resolves by name. | Reconcile with 03/20 |
| OD-18-E | `sfxvol` as bus gain vs per-call. | Per-call (event carries whether the call site applied it), matching Arc's `play(float)` bypass; Sound bus gain stays 1.0. | Reconcile with 14 (settings row semantics unchanged) |
| OD-18-F | 12's `MusicContainerRef` vs 18's `MusicRef`. | Alias to `mind_core::audio::MusicRef`; same name-string serde. | Reconcile with 12 |
| OD-18-G | Voice cap. | 128 world + 16 UI; loops are protected from stealing; over-cap one-shots denied/replace by policy. | Default (perf gate will validate) |
| OD-18-H | Bus pause mirror | Per-voice `stream_paused`; Music/UI exempt. | Default |
| OD-18-I | SoLoud voice-quality details (resampler, limiter, `globalPitch`). | `globalPitch` ported (default 1.0, exposed on `MindAudio`); resampling/limiting delegated to Godot's mixer. | Accepted deviation |
| OD-18-J | Music OGG import settings (`loop` metadata). | Runtime loop variants are duplicated with `loop = true`; importer settings ignored. | Accepted |
| OD-18-K | Asset import: Godot imports `.ogg`; upstream also accepts `.mp3` (`findMusic`/`resolve_sound`). | Both are imported; registry prefers `.ogg`, falls back to `.mp3`. | Accepted (03 parity) |
| OD-18-L | `MusicRegisterEvent` placement in 05's `SimEvent` (05 not written against it). | Add the variant in 05 (append-only); 18 registers its client listener on `ClientLoadEvent`. | Reconcile with 05 |
| OD-18-M | Headless `AudioEvent` recording sink for oracle only vs a shipped feature. | `RecordingAudioSink` behind `cfg(test)`/`--features audio-record`; `mind-headless` enables it for `audio *` scenarios only. | Default |

## 9. References

Read in full or targeted for this plan:

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1–2.4, §3 plan table row 18, §4 template, §5 P6 gate, §6 conventions, §7 verification tooling, §8 addons, §9 parity ledger, §10 OD1–OD9, §11 execution).
- `mindustry-godot/PRELIMINARY_PLAN.md`.
- Sibling plans: `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.4–3.5 scene/node paths, §3.6 event bus, §3.10 extension contract, §6.3 dump/§7c MCP recipe/§7d budgets), `02_CONTENT_IMPLEMENTATION_PLAN.md` (§2.3 boundaries, §3.2–3.6, §6.1 records), `03_ASSETS_IMPLEMENTATION_PLAN.md` (§3.2 boot order, §3.3 runtime types, §3.8–3.10 overlay/invariants, §6.5 `sounds.index.json`, §7.1 handshake, M8), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (§3.4 content serializers incl. `MusicContainer`, settings store), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.4 schedule, §3.7 events, §3.8 Time, §3.13 Godot surfaces, §6.5 checksum), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (§2.3 boundaries, §3.4 weapon/turret updates, §3.14 ledger), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (sound call-site ownership), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (§3.3 `Rules` music fields, §3.4 teams/bosses, §3.9 `MindCampaign`), `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` (§3.10 `@sfx-*` table, `PlaySoundI`/`PlayMusicI`), `14_UI_IMPLEMENTATION_PLAN.md` (§3.4 dialogs/pause, §3.5 HUD, §3.9 `MindUi` API + settings rows), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (§3.3 frame stages, §3.13 reconciliation/OD16-J), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (§3.2 sink rule, §3.16 reconciliation, §6.1 `SoundParams`, `SoundEffect`), plus notes on 19/20/21/22/23.
- Mindustry AGENTS docs: `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `core/src/mindustry/audio/AGENTS.md`, `core/assets/AGENTS.md`, `core/src/mindustry/game/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/ui/AGENTS.md`, `core/src/mindustry/entities/AGENTS.md`, `core/src/mindustry/logic/AGENTS.md`, `core/src/mindustry/editor/AGENTS.md`.
- Mindustry sources: `audio/{SoundControl,SoundPriority,AmbientSource,SoundLoop,MusicContainer}.java`; `core/Control.java`; `core/GameState.java`; `core/Logic.java`; `game/{Rules,EventType}.java`; `type/{Planet,Weather}.java`; `world/Block.java`; `world/blocks/defense/turrets/Turret.java`; `entities/comp/{BuildingComp,UnitComp,FireComp,MinerComp,TankComp,BuilderComp,WeatherStateComp,PowerGenerator,ImpactReactor,Drill,BurstDrill}.java`; `entities/effect/SoundEffect.java`; `logic/LExecutor.java`; `editor/data/MapAudioView.java`; `ui/dialogs/SettingsMenuDialog.java`; `ClientLauncher.java`; `annotations/.../impl/AssetsProcess.java`; `tests/src/test/java/**` (inventory: no audio tests).
- Arc oracle sources (fetched from `https://github.com/Anuken/Arc` `master`, not present on disk): `arc-core/src/arc/audio/{Sound,Music,Audio,AudioSource,AudioBus,Filters}.java`.
- Godot 4.7 docs: `AudioStreamPlayer`, `AudioStreamPlayer2D`, `AudioEffectLowPassFilter`/`AudioEffectFilter`, `AudioServer`, `AudioListener2D`, `AudioStreamOggVorbis`/`AudioStreamMP3`.
- Skills: `C:\Users\Clinton\g\.opencode\skills\playtest\SKILL.md`, `C:\Users\Clinton\g\.opencode\skills\godot-compositor-testing\SKILL.md`.

## Changelog

- 2026-10-01 — Plan written (P6). No implementation started.

- 2026-10-02 — **M0–M4 core complete + M5 partial on `lane/18-audio` (commits `a741dfd`, `7c4a837`).**
  - **M0 boundary + registry + vertical slice.** `mind-core/src/audio/{mod,events,ids,math}.rs`: `AudioEvent` (append-only, serde `kind`), `AudioSink`/`NoopAudioSink`/`RecordingAudioSink`/`AudioSinkRes`/`AudioPlugin`; `SoundId` (re-export of plan-02 seed id + name serde + `none`/`unset` sentinels), `MusicRef` (name-string serde), `find_music_chain`, `bus_for_sound`, setting keys; `calc_falloff`/`calc_pan`/`lerp_delta`/`clamp01`/`sanitize`/`AudioRng`/`SeededAudioRng`/`random_index_excluding`. `mind-gdext/src/audio/{mod,streams,buses,voices}.rs`: `MindAudio` node + `#[func]` API, `Master→Sound/Music/UI` buses + Sound lowpass slot 0, normal/looping `StreamCache`, 128+16 pooled voices, `AudioListener2D` on the camera; node declared in `res://scenes/spine.tscn`. `mind-headless audio events` + `events_blocks.json` golden.
  - **M1 buses/volumes/lowpass/pause.** Bus layout programmatic + `client/audio/bus_layout.json`; lowpass `20500→500 Hz` 0.4 s sweep; pause mirror on the voice pool; settings binding (`musicvol`/`sfxvol`/`ambientvol`/`alwaysmusic`) via `MindAudio.set_setting`.
  - **M2 music state machine.** `music::MusicPlayer` full `SoundControl.update` order, playlists/override chain incl. boss→dark quirk, `is_dark`, `play/play_once/play_music/silence/stop/keep_silent`, `WaveEvent` 8–15 s frame-clock delay; `MusicRegisterEvent` already present in plan 05 (HLP C7 closed). Or: `audio music` + `music_select.json`.
  - **M3 priority.** `priority::SoundPriorityTable::build` exact `SoundPriority.init` order; pure `admit` admission/eviction + `SoundPlayState` `checkFrame` boost. Or: `audio policy` + `policy.json`.
  - **M4 loops/ambient.** `loops::{LoopMixer,LoopData,SoundLoopState,AmbientProvider,AmbientSnapshot,AmbientPoller}` port of `updateLoops`/`SoundLoop`/`AudioThread.doLoop` (20 Hz, silent gate, menu clear/pause freeze, voice lifecycle). Or: `audio loops` + `loops_aggregate.json`.
  - **M5 partial.** `findMusic` registry/`music/`/mod chain landed; `MindAudio.audition_*` + `dp-` data-audio overlay deferred (plan 20).
  - **M6 partial.** `audio bench` (+ `--voices/--loops/--ambient`) landed; release budget p99 ≤ 250 µs pending a release run; alloc-audit + inspector `Audio` row + `bench/baselines.json` deferred (plan 23).
  - **Reconciliation decisions:** `SoundId` reuses the plan-02 seed table (adds name-serde) rather than forking a second id space; `MusicRef` is byte-compatible with plan 04's `MusicContainer`; `MusicRegisterEvent` was already added by plan 05. Recorded in `parity/ledgers/audio.md` and HLP §13.
  - **Evidence:** `cargo test -p mind-core` → 298 lib + 2 + 1 = 301 passed / 1 ignored (35 audio); workspace `clippy -D warnings` + `fmt --check` clean; `cargo check -p mind-gdext` clean; `audio events|music|loops|policy` all PASS against committed goldens; `audio bench --ticks 3600 --loops 256 --ambient 2000` → debug p50 122.9 µs / p99 391.3 µs (release target, debug reports only).
  - **Deferred:** §7c in-engine MCP sweep (single-editor mutex), `tools/mcp-smoke` audio steps, M5 audition/`dp-`, M6 alloc-audit/inspector/baselines.

- 2026-10-03 — **M0–M6 COMPLETE on `lane/18-audio` (base `57b3c27`); §7c in-engine MCP sweep remains DEFERRED to the single-editor mutex.**
  - **M5 sim call-site emitters (plans 07/10 now merged).** New `mind-core::audio::sim` (`block_place_sound`/`block_break_sound`/`block_destroy_sound` port `Block.init` size defaults; `emit_block_*`/`emit_turret_shoot`/`emit_bullet_hit`/`emit_bullet_despawn`/`emit_unit_death`/`emit_unit_wreck`/`emit_unit_step`/`emit_loop_add`/`emit_loop_instance`; `none`/`unset` never emit). Wired to the `AudioSink` boundary: `world::BuildHarness::{place,finish}` (with the `ConstructBlock.shouldPlay` 32 ms gate), `turrets::shoot` (per-bullet `shootSound`, fixed pitch — never consumes the sim RNG), `combat::bullet::{hit_bullet,despawn_bullet}` (`hitSound`/`despawnSound`), and `entities::comp::unit::lifecycle::kill_unit` (`deathSound`/`wreckSound` via the new `UnitAudioComp` copied from `UnitTypeDef`). `CombatCtx` gained `audio: &AudioSinkRes`. New `SharedAudioLog` (Arc/Mutex `RecordingAudioSink`) + `AudioSinkRes` made `Arc`-backed/`Clone` so harnesses share one log. Fixture `duo` ammo gained `shoot_sound = shootDuo`; `fuse` gained `hit_sound`/`despawn_sound`. New headless scenario **`audio events --scenario audio_events_sim`** drives the real build/combat harness and dumps 6 events → committed `events_sim.json`; `mind-headless/tests/audio_golden.rs` gates all five oracles.
  - **M5 data-audio overlay + audition.** `audio::ids::audition_name` consumes the plan-20 `dp-<name>` `AudioApplier` overlay seam (test `audition_prefers_dp_overlay`); `MindAudio.audition_play/stop/playing/position/length` + `keep_silent` already landed.
  - **M6 perf/docs.** `audio::tests::steady_state_audio_allocates_nothing` (`alloc-audit`, **0** allocs / 1000 frames); release `audio bench --ticks 3600 --loops 256 --voices 32 --ambient 2000` → p50 **11.3 µs** / p99 **46.6 µs**; `bench/baselines.json` gains the plan-18 audio budgets/voice counts; state-inspector gains the `Audio` row (`/root/Spine/MindAudio.stats()`).
  - **Golden staleness fix.** `serde_json` `preserve_order` (enabled by plan 20) changed object key order, so the committed `music_select`/`loops_aggregate`/`policy` goldens were stale (values unchanged — verified by normalized diff) and were regenerated; `tools/ci.sh` had no `audio` step, hence the drift was invisible. Now enforced by `audio_golden.rs`.
  - **Evidence:** `cargo test -p mind-core` = **817 lib + 3 `blocks_golden` + 5 `combat_golden` + 2 `sim_core_determinism` + 2 `sim_core_meta` + 1 `sim_core_schedule` = 830 passed / 2 ignored** (43 audio tests, +8); `cargo test -p mind-headless` = **36 lib + 1 `audio_golden` = 37 passed**; `cargo fmt --all -- --check` + workspace `clippy -D warnings` + `cargo check -p mind-gdext` clean; all five audio goldens PASS.
  - **Deferred (unchanged):** §7c in-engine MCP sweep + `tools/mcp-smoke` audio steps (single-editor mutex); `BulletType.despawned` `despawnSound` under the plan-10 HIT gate; `RandomSound` alternate variants (sim-RNG free).
