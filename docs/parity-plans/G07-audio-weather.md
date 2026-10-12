<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G07 — Audio + weather in game (execution plan)

Date: 2026-10-13. Self-contained plan for the in-game audio (sounds, loops,
ambience, music, settings) and weather (simulation verification + rendering)
of the Mindustry-Godot port. Read `docs/parity-plans/00-environment.md` first
(paths `core/`, `gdext/`, `ui/`, `scenes/`, `J/`, `ARC/`, the `$G $H $I $C $MC`
shorthands, build/test/commit rules). Work the tasks in the §3 order. If the
code you find differs from what a task says (a symbol is missing, a line moved
by more than a search can fix, a behaviour is already fixed), stop that task,
write down what you saw, and go to the next one. Never invent a design that
is not written here.

Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line).

Audio cannot be heard by the executor. Every audio acceptance check below is
**structural**: a `#[func]` readback that proves a voice was started / a loop
volume is non-zero / a track is playing, plus a log check. State this in each
commit body ("audio verified structurally: ...").

---

## §0 Key facts found by the audit (read before any task)

1. **`MindAudio` may never run its per-frame pump.** Branch
   `agent/par0072-menu-music` (commit `5bf5061`, NOT merged into `main`)
   recorded that `MindAudio::process` was never dispatched in the live game,
   that `resolve_assets_dir()` in `gdext/audio/mod.rs:448` returns
   `res://../assets`, which misses every sound/music file, and that
   `StreamCache::music` (`gdext/audio/streams.rs`) passes the un-normalized
   name to `decode_bytes`, so no music track ever decodes. All three are still
   in `main`. T1 fixes them first; every other audio task depends on T1.
2. The sim→client audio path exists: `MindSimHost::install_sim_seams`
   (`gdext/sim_host.rs:367-389`) inserts `AudioSinkRes::new(self.audio_log)`;
   `MindAudio::process` drains it (`gdext/audio/mod.rs:179-187`). Emitters
   that are live today: turret shoot (`core/world/blocks/defense/turrets/mod.rs:1650`,
   only duo bullets carry a sound), bullet hit/despawn
   (`core/combat/bullet/mod.rs:569,628`), unit death/wreck
   (`core/entities/comp/unit/lifecycle.rs:319-333`).
3. `LoopMixer::accumulate` is called but `LoopMixer::update` is never called
   anywhere in `gdext/`, and there is no `LoopOutput` implementation, so no loop
   voice can ever start (PAR-0071). `AmbientPoller` is never constructed
   outside tests. `AmbientPoller::do_loop` (`core/audio/loops.rs:489-513`)
   pushes a **clone taken at the first contribution**, so the output misses
   every later source of the same sound (Java pushes the live accumulator and
   clones it after the loop, `J/audio/SoundControl.java:526-549`), and
   `AmbientPoller::update` clears its output on non-poll frames whereas Java
   merges the last output **every frame** (`SoundControl.java:452-468`).
4. Unit weapon sounds go to `FxSink::sound` (`core/weapons/mod.rs:620,749,752`)
   which has no position and is dropped by `MindFx::tick_view`
   (`gdext/fx.rs` `FxEvent::Sound { .. } => {}`): every unit gun is silent.
5. No block carries place/break/destroy/ambient sound data; place/break
   sounds are emitted only by the test `BuildHarness` (`core/world/harness.rs:405-407`) (PAR-0073).
6. Settings: `ui/dialogs/settings_menu_dialog.gd:458` looks up
   `/root/MindAudio` (the node is `/root/Spine/MindAudio`); `MindAudio` never
   reads the settings store, so it always plays at 100 % (PAR-0074).
7. Weather sim (`core/game/runtime.rs:825 weather_system`, `core/entities/comp/weather.rs`)
   is ported incl. the PlayEvent re-arm (`PlaySession.weather_reset_pending`,
   `core/game/play.rs:502`). Missing: `ParticleWeather.update` wind force
   (sandstorm/sporestorm push units), weather sound loops, `state.envAttrs`
   (weather `light`/`water`/`spores` attributes) feeding generators, and all
   live weather rendering (only the dev probe `MindFx.set_weather` draws).
8. **Ground Zero's save names a weather Java cannot resolve.** The imported
   profile's `saves/sector-serpulo-15.msav` (Ground Zero, port sector 170)
   carries `weather:[{weather:snow,minFrequency:72000,maxFrequency:216000,minDuration:18000,maxDuration:54000,cooldown:173819.92}]`
   (read with zlib from `../mindustry.save.full.tech.tree.zip`). The weather
   content name is `snowing` (`J/content/Weathers.java:20`), `JsonIO` reads
   weathers with `content.getByName(ContentType.weather, name)`
   (`J/io/JsonIO.java:224-233`, no fallback map for weathers,
   `J/core/ContentLoader.java:228-239`), and `Logic.updateWeather` drops null
   entries (`Logic.java:377`). So in the reference Java client Ground Zero has
   **no** weather; the port's `retain(weather_by_name)` (`core/game/runtime.rs:840-845`)
   matches. No other serpulo sector save exists in the profile. In-engine
   verification therefore uses the forced-weather seam (T9) and a fresh
   generated/preset sector after T11 (Frozen Forest, sector 64, is unlocked
   without a base: its fresh generation gets `snowing` from `addWeather`).
9. Java `World.setSectorRules` (`J/core/World.java:300-303`) calls
   `planet.generator.addWeather(sector, rules)` for every freshly generated
   sector (presets included); the port's `PlanetGenerator::add_weather`
   (`core/maps/generators/planet.rs:345-348`) is a no-op (T11).

---

## §1 Inventory

### 1.1 Java → port table

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `J/audio/SoundControl.java` `update` (pump) | `gdext/audio/mod.rs` `process` | ported, liveness unverified | §0.1 | T1 |
| `SoundControl` music (`play`/`playOnce`/`playRandom`/`silence`/`isDark`/boss/dark lists/`WaveEvent` delay) | `core/audio/music.rs` `MusicPlayer` | ported, context mostly empty in game (no rules/wave/core/boss/WaveEvent/ResetEvent); streams never decode | `gdext/audio/mod.rs:243-263` fills only is_menu/paused | T1, T7 |
| `SoundControl.loop` + `updateLoops` | `core/audio/loops.rs` `LoopMixer` | accumulate only; `update` never called; no `LoopOutput` | §0.3 | T4 |
| `SoundControl.AudioThread` (20 Hz ambient) | `core/audio/loops.rs` `AmbientPoller`/`AmbientSnapshot` | ported with 2 bugs, unwired | §0.3 | T4, T5 |
| `J/audio/AmbientSource` + `BuildingComp.shouldAmbientSound/ambientVolume/getAmbientSound`, `Block.ambientSound/ambientSoundVolume`, per-class overrides | — | missing (no block data, no provider) | grep `ambient_sound` only in `core/audio` | T5 |
| `J/audio/SoundLoop` (turret `loopSound`, weapon continuous `shootSound`) | `core/audio/loops.rs` `SoundLoopState` | ported, no emitter, no voice | `emit_loop_instance` never called | T6 |
| `J/audio/SoundPriority` | `core/audio/priority.rs` `SoundPriorityTable`, `admit`, `SoundPlayState` | ported, unwired (pool round-robins 128 voices, no checkFrame, no falloffOffset) | `gdext/audio/voices.rs` | T8 |
| `J/audio/MusicContainer`, `findMusic` | `core/audio/ids.rs` `MusicRef`, `StreamCache::music` | decode bug | §0.1 | T1 |
| Arc `Sound.at/calcFalloff/calcPan`, `Audio.sfxVolume`, `Audio.falloff=16000` | `core/audio/math.rs` | ported; listener width uses screen px not world px | `gdext/audio/mod.rs:222-237` | T1 |
| Settings `alwaysmusic/musicvol/sfxvol/ambientvol` (`SettingsMenuDialog.java:368-371`) | `ui/dialogs/settings_menu_dialog.gd:451-460`, `MindAudio.set_setting` | broken path, no store load | §0.6 | T1 (PAR-0074) |
| `mutemusic`/`mutesound` | — | do not exist in v8 Java (no task) | grep in `J/` returns nothing | — |
| `Turret.shoot/bullet` `shootSound`/`chargeSound`/`soundPitchMin/Max`/`shootSoundVolume` (`Turret.java:127-138,757,804`) | `core/world/blocks/defense/turrets/mod.rs:1589,1650` | partial: bullet sound only (duo), pitch fixed 1, volume 1, no turret-level sound, no charge sound | | T2 |
| `PointDefenseTurret.shootSound` (`PointDefenseTurret.java:30,98`), `TractorBeamTurret` loop (`:35-36,117`) | — | missing | | T2, T6 |
| `Turret.loopSound`/`SoundLoop` (`Turret.java:134-136,304,504`; Liquid/Continuous/Laser `shouldActiveSound`) | — | missing | | T6 |
| `Weapon.shootSound/chargeSound/initialShootSound` (`Weapon.java:470,515-517`) | `core/weapons/mod.rs:620,749,752` via `FxSink::sound` | wired to a dropping sink | §0.4 | T2 |
| `Weapon.activeSound`, continuous `SoundLoop` (`Weapon.java:391-420,431`) | — | missing | | deferred (§6) |
| `ConstructBlock.constructFinish/deconstructFinish` place/break sound + `shouldPlay`/`calcPitch` (`ConstructBlock.java:58-145`) | `core/world/harness.rs:405` only | missing in live sim | §0.5 | T3 (PAR-0073) |
| `BuildingComp.onDestroyed` destroy sounds (`BuildingComp.java:1524-1543`), `Block.init` defaults (`Block.java:1360-1379`), `Wall` size-2, `CoreBlock.playDestroySound` | `core/audio/sim.rs` helpers exist | not called in `resolve_deaths` | `core/sim/unit_runtime.rs:585-608` | T3 |
| `Build.java:90`/`InputHandler.java:704` `blockRotate` | — | missing | `core/sim/mod.rs:830` Rotate | T3 |
| `BuilderComp` `loopBuild` (`BuilderComp.java:158`), `MinerComp` `mineSound` (`:119`), `FireComp` `loopFire` (`:46`), `UnitComp` `loopSound/moveSound` (`:762-766`), `TankComp` `tankMoveSound` (`:54`) | — | missing | | T6 |
| `MechComp`/`LegsComp` step sounds | `core/audio/sim.rs emit_unit_step` (unused) | missing | | T12 |
| `Control.java:129` `Sounds.waveSpawn.play()` on `WaveEvent` | — | missing | | T7 |
| `Control.java:198-222` land music + `CoreBlock.beginLaunch` `coreLand` sound (`CoreBlock.java:376`) | — | missing | | T7 |
| `UI.java:146` uiButton click, `BaseDialog` uiBack, `HudFragment` uiNotify/uiFavorite, `ResearchDialog` uiUnlock | `MindUi.play_ui_sound` emits a signal nobody connects | missing | `gdext/ui/ui_host.rs:367` | T13 |
| `LExecutor` `playsound`/`playMusic` | `AudioEvent::Play/MusicPlay` handled in `apply_events` | ported (G06 owns the instruction) | | — |
| `InputHandler` payloadPickup/payloadDrop (`InputHandler.java:597-614`, `PayloadComp.java:107,117,145,203,223,230`), `configureSound` (`InputHandler.java:1992`) | — | missing | `core/sim/unit_runtime.rs:1916` | T14 |
| `J/type/Weather.java` `WeatherEntry`, `Logic.updateWeather`, `PlayEvent` re-arm | `core/game/runtime.rs:825-946`, `core/game/play.rs:502` | ported (T3 of combat plan landed) | | T9 verify |
| `WeatherStateComp.update` (opacity/life) + `Weather.updateEffect` status | `core/entities/comp/weather.rs:118-163` | ported; `checkTarget` approximated by `PhysicsComp.flying` | | T9 (note) |
| `ParticleWeather.update` wind `force` impulse | — | missing (comment at `weather.rs:138` wrongly says no-op) | | T9 |
| `Weather.updateEffect` sound loop (`soundVol`, `soundVolMin`, `soundVolOscMag/Scl`) | `AudioEvent::LoopCamera` exists, never emitted | missing | | T6 |
| `Logic.java:603-606` `state.envAttrs` = rules.attributes + weather attrs × opacity | `weather_water_attr` (water only, fires) | partial; `GeneratorEnv` resource never inserted | `core/world/blocks/power/generator.rs:39` | T9 |
| `J/content/Weathers.java` (6 weathers) | `core/content/registries/weathers.rs` | ported | | — |
| `Weather.drawParticles/drawRain/drawSplashes`, `ParticleWeather/RainWeather.drawOver/drawUnder`, `NoiseEffect.drawNoise` | `core/fx/weather_fx.rs` builders | ported with RNG-order bugs; only a dev probe draws them | §2 T10 | T10, T10b |
| `Renderer.weatherAlpha`, `showweather` setting | — | missing | | T10 |
| `PlanetGenerator.addWeather` (`PlanetGenerator.java:102-160`) | `core/maps/generators/planet.rs:345` no-op | missing | §0.9 | T11 |
| `MagneticStorm`, `SolarFlare` | stubs upstream | n/a | | — |

### 1.2 Already done / verified (do not redo)

* `weather_reset_pending` PlayEvent re-arm (`core/game/play.rs:502`, `core/game/runtime.rs:849`).
* `weather_water_attr` feeds fire extinguish (`core/sim/unit_runtime.rs:762`).
* `Block.init` default place/break/destroy sound helpers
  `block_place_sound/block_break_sound/block_destroy_sound` (`core/audio/sim.rs:37-82`) with tests.
* Bus layout `Master→Sound/Music/UI` + Sound lowpass (`gdext/audio/buses.rs`).
* Music state machine unit tests (`core/audio/music.rs` tests).
* Switch click sound (building-production plan T9, `0dbd8a0`).

---

## §2 Tasks

Tasks are ordered by player impact; §3 gives the execution order.

### T1. Audio pump liveness, asset root, music decode, settings (PAR-0074)

**Symptom / acceptance.** Today the client is silent: sound files are looked
up under `res://../assets` (miss), music never decodes, the volume sliders do
nothing, and the per-frame pump may never run. After: (a) the run log contains
`[audio] ready (buses ..., N sounds)` with `N > 150`; (b)
`$G --eval 'return get_node("/root/Spine/MindAudio").stats()'` twice, 1 s
apart, shows `frames` increasing; (c) with the Settings > Sound
`sfxvol` slider at 30, `stats()["sfxvol"] == 30` and the value survives a
`--play` restart; (d) after entering Ground Zero with `alwaysmusic` on,
`current_track_playing()` is `true` within 2 s and `current_track()` is one of
`game1 game3 game6 game8 game9 fine` (ambient list) or
`game2 game5 game7 game4` (dark list).

**Java.**
* `SoundControl.update` runs every frame from `Control.update`
  (`J/core/Control.java`, `sound.update()`); `Audio` listener sets
  `sfxVolume = settings.getInt("sfxvol") / 100f` every frame
  (`ARC/arc-core/src/arc/audio/Audio.java:53-57`).
* `volumeMultiplier = musicvol/100 * clamp(rules.musicVolume)`;
  `shouldPlay = musicvol > 0` (`SoundControl.java:259-262,331-333`).
* `updateLoops`: `avol = getInt("ambientvol", 100)/100` (`SoundControl.java:419`).
* `Sound.calcPan(x, y) = clamp((x - camera.x) / (camera.width / 2), -0.9, 0.9)`
  where `camera.width` is in **world units** (`ARC/.../Sound.java:166-170`).
* `Core.audio.setPaused(soundBus.id, state.isPaused())` (`SoundControl.java:151`);
  lowpass wet = `state.isGame() && Core.scene.hasDialog()` (`:120,130-132`).
* Sounds in `sounds/ui/` use `uiBus` (`SoundControl.java:98-104`): not paused,
  not low-passed.
* `playOnce` plays **non-looping** (`setLooping(false)`, `:327`) so a random
  track ends and `current` clears (`:124-127`); `play()` (menu/editor fade)
  plays looping (`:288`).

**Port today.**
* `gdext/audio/mod.rs:117-118` `let assets_dir = resolve_assets_dir();`
  (local fn at `:448-456`, probes `res://assets` then `res://../assets`).
  The atlas uses `crate::assets::loader::resolve_assets_dir()`
  (`gdext/assets/loader.rs:32`, `pub(crate) mod loader`), which globalizes the
  dev path and works.
* `gdext/audio/streams.rs` `StreamCache::music`: `decode_bytes(&candidate, ...)`
  with `candidate = "music/game1"` (no extension) → `None`.
* `GodotMusicOutput::play` ignores `_looping`; `StreamCache::music` decodes with
  `looping = true` (`streams.rs` `decode_bytes(.., true)`).
* No `set_process(true)` in `MindAudio::ready` (`MindCampaign::ready` does it,
  `gdext/campaign.rs:113`).
* `Settings` struct (`gdext/audio/mod.rs:61-78`) is only changed by
  `set_setting`; the GDScript caller uses the wrong path
  (`ui/dialogs/settings_menu_dialog.gd:458`).
* `listener()` (`:222-237`) uses `viewport.get_visible_rect().size.x` (screen px).
* `play_sound` (`:356-378`) calls `bus_for_sound(name, None)` → only
  `coreLaunch` reaches the UI bus.
* `sound_paused` is only set by `set_sound_paused` (#[func]); `music_context`
  sets `dialog: paused` (`:256`).

**Changes.**
1. `gdext/audio/mod.rs` `ready()`: replace `resolve_assets_dir()` with
   `crate::assets::loader::resolve_assets_dir()`; delete the local
   `fn resolve_assets_dir` (`:447-456`). Keep the `[audio] ready` log line and
   add the dir: `log::info!("[audio] ready (buses {:?}, {} sounds, dir {})", self.bus, self.streams.sound_count(), assets_dir)`.
2. `gdext/audio/streams.rs` `music()`: compute `let file = normalize(&candidate);`
   once, use it for both the path and `decode_bytes(&file, &bytes, true)`.
   Add `pub fn file_of(&self, name: &str) -> Option<&str>` returning
   `self.sounds.get(name)?.file.as_deref()` (check the `file` field type in
   `mind_core::assets::sounds::Sounds`; it is `Option<String>` per
   `streams.rs:52`).
3. Music looping: in `GodotMusicOutput::play(&mut self, track, volume, looping)`
   after `set_stream`, set the loop flag on the stream:
   ```rust
   if let Some(stream) = self.player.get_stream() {
       if let Ok(mut ogg) = stream.clone().try_cast::<godot::classes::AudioStreamOggVorbis>() { ogg.set_loop(looping); }
       else if let Ok(mut mp3) = stream.try_cast::<godot::classes::AudioStreamMp3>() { mp3.set_loop(looping); }
   }
   ```
   (`MusicPlayer::play_once_ctx` passes `false`, `play` passes `true`; verified
   in `core/audio/music.rs:395,415,425`.)
4. Liveness: add field `frames: u64` (init 0); first statement of `process`:
   `self.frames = self.frames.wrapping_add(1);`; add `frames` to `stats()`.
   At the end of `ready()` add
   `self.base_mut().call_deferred("set_process", &[true.to_variant()]);`.
   **Decision gate (do it right after the first build):** run
   `$G --eval 'return get_node("/root/Spine/MindAudio").stats()["frames"]'`
   twice 1 s apart. If it increases, skip the rest of this step. If it stays 0,
   apply the fallback: move the body of `process` into
   `#[func] pub fn drive_frame(&mut self, delta: f64)` (keep `process` calling
   it), and in `gdext/campaign.rs` `fn process(&mut self, delta: f64)` (rename
   `_delta`) add **after** `self.pump_runtime();`:
   ```rust
   // PAR-0072/T1: MindAudio's own `_process` is not dispatched; pump it here.
   if let Some(mut audio) = self.base().try_get_node_as::<crate::audio::MindAudio>("../MindAudio") {
       audio.bind_mut().drive_frame(delta);
   }
   ```
   plus a guard so it does not double-pump when `process` does run: store
   `last_frame_ms: f64` and return early from `drive_frame` when
   `now_ms - last_frame_ms < 1.0`. Record which branch you took in the commit body.
5. Settings load: add `fn refresh_settings(&mut self)` that reads
   `/root/MindUi` dynamically (no typed bind):
   ```rust
   let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else { return };
   let int = |ui: &mut Gd<GdNode>, key: &str| ui.call("settings_get_i32", &[GString::from(key).to_variant()]).try_to::<i64>().unwrap_or(100) as i32;
   self.settings.musicvol = int(&mut ui, "musicvol").clamp(0, 100);
   self.settings.sfxvol = int(&mut ui, "sfxvol").clamp(0, 100);
   self.settings.ambientvol = int(&mut ui, "ambientvol").clamp(0, 100);
   self.settings.alwaysmusic = ui.call("settings_get_bool", &[GString::from("alwaysmusic").to_variant()]).try_to::<bool>().unwrap_or(false);
   ```
   Call it at the end of `ready()` and every 30th frame in `process`
   (`if self.frames % 30 == 0`). Both `settings_get_i32`/`settings_get_bool`
   are `#[func]` on `MindUi` (`gdext/ui/ui_host.rs:661,673`).
6. `ui/dialogs/settings_menu_dialog.gd:458`: `get_node_or_null("/root/Spine/MindAudio")`.
7. Listener width in world units: in `listener()` read the camera zoom:
   `let zoom = camera.get_zoom().x.abs().max(0.01);` and use
   `width / zoom` (camera zoom is `(camerascale, -camerascale)`,
   `gdext/camera.rs:203`).
   The voice pool's node-offset panning (`VoicePool::play_world`, plan-18
   deviation 5) positions the player at `listener.x + pan * viewport_width / 2`
   in **world** units relative to the `AudioListener2D` on the camera; keep that
   consistent by passing `listener.width` (now world units) as
   `viewport_width`. UNVERIFIED: whether Godot's 2D panning then yields full
   left/right at `pan = ±0.9`; executor checks once with
   `$G --eval 'return get_node("/root/Spine/MindAudio").play_oneshot("uiButton", 1.0, 1.0, 0.9)'`
   (must return ≥ 0, no error) and does not tune further.
8. UI bus routing: in `play_sound`, resolve the file first:
   `let file = self.streams.file_of(name).map(str::to_owned);`
   `let ui = bus_for_sound(name, file.as_deref()) == BusKind::Ui;`. Same in
   `play_oneshot`.
9. Pause mirror + dialog lowpass: in `process` before the drain, compute
   `paused = host.is_paused()` (already read in `music_context`) and set
   `self.sound_paused = paused`. In `music_context` set
   `dialog: !is_menu && ui_has_dialog` where `ui_has_dialog` is
   `MindUi.dialog_stack()` non-empty, read exactly like
   `MindInput::dialog_open` (`gdext/input/mod.rs:428-441`, dynamic
   `call("dialog_stack")` → `PackedStringArray`).
10. `is_menu`: `state == "menu" || menu background visible`, where the latter is
    `get_node_or_null("/root/Spine/Ui/MenuBackground")` → `get("visible")`
    → bool (the menu shows it, `ui/ui_root.gd:206-211`). The sim boots in
    `State::Playing` (`core/sim/mod.rs:201`), so the state alone reports "game"
    at the main menu.

**Tests.** `cargo test -p mind-core --lib audio` must stay green (no core
change). GDScript parse gate (§2 of the env doc).

**Verify in-engine.**
```bash
tools/session-play.sh N --import
grep "\[audio\] ready" $(ls -t $U/logs/* | head -1)        # N sounds > 150, dir ends in /assets
$G --eval 'return get_node("/root/Spine/MindAudio").stats()'   # frames > 0
sleep 1; $G --eval 'return get_node("/root/Spine/MindAudio").stats()["frames"]'   # larger
$G --eval 'get_node("/root/MindUi").settings_set("sfxvol", 30); return true'
sleep 1; $G --eval 'return get_node("/root/Spine/MindAudio").stats()["sfxvol"]'  # 30
$G --eval 'get_node("/root/MindUi").settings_set("alwaysmusic", true); return true'
# enter Ground Zero (env §4.1), then:
sleep 3; $G --eval 'var a = get_node("/root/Spine/MindAudio"); return [a.current_track(), a.current_track_playing()]'
$G --eval 'return get_node("/root/Spine/MindAudio").drain_log()'   # no "[E] missing audio" lines for music
$G --eval 'get_node("/root/MindUi").settings_set("sfxvol", 100); get_node("/root/MindUi").settings_set("alwaysmusic", false); return true'
```
Expected: track in the lists above, `true`.

**Pitfalls.** `MindAudio::ready` adds children while bound; do not add an
`on_notification` arm (re-entrancy, `d70faab`). Never hold `host.bind()` across
a call into another node. The settings poll uses dynamic `call` on purpose.

**Commit.** `Fixes PAR-0074: MindAudio loads assets/settings and pumps every frame`

### T2. Unit weapon and turret shoot/charge sounds

**Symptom / acceptance.** Today only `duo` makes a shot sound (its bullets
carry `SHOOT_DUO`); every other turret (scatter, hail, lancer, arc, salvo,
ripple, ...) and every unit weapon (player alpha, wave daggers/flares) is
silent. After: the sim emits a positioned `AudioEvent::At` for every unit
weapon shot and every turret shot with the Java sound, volume and a pitch in
`[soundPitchMin, soundPitchMax]`. Acceptance: test assertions below + in-engine
`MindAudio.stats()["events_applied"]` grows while the player alpha shoots.

**Java.**
* `Weapon.shoot` (`J/type/Weapon.java:460-520`):
  `chargeSound.at(shootX, shootY, random(soundPitchMin, soundPitchMax))` when
  `shoot.firstShotDelay > 0`; per bullet
  `shootSound.at(bulletX, bulletY, random(soundPitchMin, soundPitchMax), shootSoundVolume)`
  (`initialShootSound` instead for `continuous`). Defaults: `shootSound = Sounds.shoot`,
  `shootSoundVolume = 1`, `soundPitchMin = 0.8`, `soundPitchMax = 1`
  (`Weapon.java:118-145`).
* `Turret.bullet` (`Turret.java:804`):
  `(type.shootSound != none ? type.shootSound : shootSound).at(bulletX, bulletY, random(soundPitchMin, soundPitchMax), shootSoundVolume)`;
  `Turret.shoot` (`:756-758`): `chargeSound.at(bulletX, bulletY, random(soundPitchMin, soundPitchMax))`
  when `shoot.firstShotDelay > 0`. Turret defaults (`Turret.java:127-138`):
  `shootSound = shootDuo`, `shootSoundVolume = 1`, `chargeSound = none`,
  `loopSound = none`, `loopSoundVolume = 0.5`, `soundPitchMin = 0.9`, `soundPitchMax = 1.1`.
  Class defaults: `LiquidTurret` loop `loopSpray`, shoot `none`
  (`LiquidTurret.java:25-26`); `ContinuousLiquidTurret` loop `loopMineBeam`,
  shoot `none` (`ContinuousLiquidTurret.java:20-21`).
* `PointDefenseTurret` (`PointDefenseTurret.java:30,98`): on each intercept
  `shootSound(=shootSegment).at(x + trns(rotation, shootLength).x, y + ..., random(0.9, 1.1))`, volume 1.
* Per-block values (`J/content/Blocks.java`, line of the `shootSound` assignment):

  | block | shootSound | vol | chargeSound | loopSound | loopVol |
  |---|---|---|---|---|---|
  | duo (3333) | shootDuo | 1 | – | – | – |
  | scatter (3429) | shootScatter | 1 | – | – | – |
  | scorch (3476) | shootFlame | 1 | – | – | – |
  | hail (3540) | shootArtillerySmall | 1 | – | – | – |
  | wave, tsunami | none (LiquidTurret) | – | – | loopSpray | 0.5 |
  | lancer (3596-3598) | shootLancer | 1 | chargeLancer | – | – |
  | arc (3655) | shootArc | 1 | – | – | – |
  | swarmer (3744) | shootMissile | 1 | – | – | – |
  | salvo (3851) | shootSalvo | 1 | – | – | – |
  | fuse (3952-3953) | shootFuse | 0.9 | – | – | – |
  | ripple (4105) | shootRipple | 1 | – | – | – |
  | cyclone (4220) | shootCyclone | 1 | – | – | – |
  | foreshadow (4260) | shootForeshadow | 1 | – | – | – |
  | spectre (4331) | shootSpectre | 1 | – | – | – |
  | meltdown (4352-4354) | shootMeltdown | 1 | – | beamMeltdown | 2.0 |
  | breach (4467) | shootBreach | 1 | – | – | – |
  | diffuse (4554) | shootDiffuse | 1 | – | – | – |
  | sublimate (4628-4630) | none | – | – | shootSublimate | 1.0 |
  | titan (4824) | shootTank | 1 | – | – | – |
  | disperse (5007) | shootDisperse | 1 | – | – | – |
  | afflict | shootDuo (default; bullet overrides shootAfflict) | 1 | – | – | – |
  | lustre (5222-5224) | none | – | – | beamLustre | 1.0 |
  | scathe (5663) | shootScathe | 1 | – | – | – |
  | smite (5762, 5955-5956) | shootSmite | 1 | – | loopGlow | 0.8 |
  | malign (5970-5972) | shootMalign | 1 | – | loopMalign | 1.3 |
  | segment | shootSegment (PointDefense, pitch 0.9..1.1) | 1 | – | – | – |
  | parallax | beamParallax loop (TractorBeam, `control.sound.loop`, vol 0.9) — T6 | | | | |

  No vanilla turret overrides `soundPitchMin/Max`. Loop columns are consumed by T6.

**Port today.**
* `core/weapons/mod.rs:620` `ctx.fx.sound(weapon.charge_sound, 1.0, pitch)`
  and `:749/:752` `ctx.fx.sound(weapon.(initial_)shoot_sound, weapon.shoot_sound_volume, pitch)`;
  `bullet_x/bullet_y` are in scope at `:749` (used for `BulletSpawn`), the
  charge site has `_shoot_x/_shoot_y` params (`:603-604`). `CombatCtx` carries
  `audio: &AudioSinkRes` (`core/combat/bullet/mod.rs:186`).
* `core/world/blocks/defense/turrets/mod.rs:1589` reads only the bullet sound;
  `:1651` `emit_turret_shoot(ctx.audio, sound, bx, by, 1.0, 1.0)`.
  `TurretConfig` (`:153-264`) has no sound fields. Literal constructions:
  `mod.rs` `config_for_inner` 4 sites (`Some(TurretConfig {` at ~647/706/765/824)
  and `advanced.rs:1030` `base_config`. `config_for` (`mod.rs:517`) post-processes
  with `apply_block_coolant`.
* Point defense: `advanced.rs:1713 update_point_defense` (no sound).

**Changes.**
1. `core/audio/sim.rs`: add
   ```rust
   /// `Weapon.shoot`/`chargeSound` `Sound.at(x, y, pitch, volume)`.
   pub fn emit_weapon_sound(audio: &AudioSinkRes, sound: SoundId, x: f32, y: f32, volume: f32, pitch: f32) {
       emit_at(audio, sound, x, y, volume, pitch, true);
   }
   ```
2. `core/weapons/mod.rs`: replace the three `ctx.fx.sound(...)` calls with
   `crate::audio::sim::emit_weapon_sound(ctx.audio, <sound>, <x>, <y>, <volume>, pitch)`:
   charge → `(weapon.charge_sound, _shoot_x, _shoot_y, 1.0)` (rename the params
   to `shoot_x/shoot_y`); shoot → `(weapon.shoot_sound or initial_shoot_sound, bullet_x, bullet_y, weapon.shoot_sound_volume)`.
   Keep the existing `pitch` draw (it already consumes `RngStream::Sim`; do not
   change RNG consumption — checksums).
3. `TurretConfig`: add fields (doc-comment each with the Java field):
   `shoot_sound: SoundId` (default `SoundId::SHOOT_DUO`),
   `shoot_sound_volume: f32` (1.0), `charge_sound: SoundId` (`NONE`),
   `loop_sound: SoundId` (`NONE`), `loop_sound_volume: f32` (0.5),
   `sound_pitch_min: f32` (0.9), `sound_pitch_max: f32` (1.1). Add the defaults
   to all 5 literal sites. Add a table + applier next to `COOLANT_MULTIPLIERS`:
   ```rust
   /// `Blocks.java` turret sound fields (name, shootSound, vol, chargeSound, loopSound, loopVol).
   const TURRET_SOUNDS: &[(&str, &str, f32, &str, &str, f32)] = &[
       ("duo","shootDuo",1.0,"none","none",0.5), ("scatter","shootScatter",1.0,"none","none",0.5),
       ("scorch","shootFlame",1.0,"none","none",0.5), ("hail","shootArtillerySmall",1.0,"none","none",0.5),
       ("wave","none",1.0,"none","loopSpray",0.5), ("tsunami","none",1.0,"none","loopSpray",0.5),
       ("lancer","shootLancer",1.0,"chargeLancer","none",0.5), ("arc","shootArc",1.0,"none","none",0.5),
       ("swarmer","shootMissile",1.0,"none","none",0.5), ("salvo","shootSalvo",1.0,"none","none",0.5),
       ("fuse","shootFuse",0.9,"none","none",0.5), ("ripple","shootRipple",1.0,"none","none",0.5),
       ("cyclone","shootCyclone",1.0,"none","none",0.5), ("foreshadow","shootForeshadow",1.0,"none","none",0.5),
       ("spectre","shootSpectre",1.0,"none","none",0.5), ("meltdown","shootMeltdown",1.0,"none","beamMeltdown",2.0),
       ("breach","shootBreach",1.0,"none","none",0.5), ("diffuse","shootDiffuse",1.0,"none","none",0.5),
       ("sublimate","none",1.0,"none","shootSublimate",1.0), ("titan","shootTank",1.0,"none","none",0.5),
       ("disperse","shootDisperse",1.0,"none","none",0.5), ("lustre","none",1.0,"none","beamLustre",1.0),
       ("scathe","shootScathe",1.0,"none","none",0.5), ("smite","shootSmite",1.0,"none","loopGlow",0.8),
       ("malign","shootMalign",1.0,"none","loopMalign",1.3), ("segment","shootSegment",1.0,"none","none",0.5),
   ];
   fn apply_turret_sounds(name: &str, config: &mut TurretConfig) {
       let Some(row) = TURRET_SOUNDS.iter().find(|row| row.0 == name) else { return };
       let id = |n: &str| SoundId::by_name(n).unwrap_or(SoundId::NONE);
       config.shoot_sound = id(row.1); config.shoot_sound_volume = row.2;
       config.charge_sound = id(row.3); config.loop_sound = id(row.4); config.loop_sound_volume = row.5;
   }
   ```
   Call `apply_turret_sounds(name, &mut config);` in `config_for` after
   `apply_block_coolant`. (`SoundId::by_name` is `core/content/registries/sound_meta.rs:664`.)
4. `shoot()` (`mod.rs:1554`): replace `let shoot_sound = ...map(|def| def.shoot_sound)`
   with the Java rule
   ```rust
   let bullet_sound = ctx.content.bullet(bullet).map(|def| def.shoot_sound).unwrap_or(SoundId::NONE);
   let shoot_sound = if !bullet_sound.is_none_or_unset() { bullet_sound } else { config.shoot_sound };
   ```
   and at `:1650` emit with
   `pitch = {let mut r = crate::audio::sim::FxAudioRng { rng: ctx.rng }; config.sound_pitch_min + r.next_f32() * (config.sound_pitch_max - config.sound_pitch_min)}`
   (`AudioRng::next_f32`, `core/audio/math.rs`; the Fx stream does not affect
   gameplay RNG) and volume `config.shoot_sound_volume`. Charge: at the top of
   `shoot()` if `config.shoot.first_shot_delay > 0.0` emit `config.charge_sound`
   at the first barrel position (use `x + trnsx(rotation - 90, shoot_x, shoot_y)`
   exactly as `bx` is computed at `:1600`, with zero offsets) with an Fx-stream
   pitch in the same range, volume 1.
5. `advanced.rs update_point_defense`: where the bullet is damaged/despawned
   (the `lethal` branch block), emit `config.shoot_sound` (from step 3, `segment`)
   at `(x + cos(rot)*config.shoot_length, y + sin(rot)*config.shoot_length)`
   with an Fx pitch in `0.9..1.1`, volume 1. Rotation = `state.rotation` in degrees.

**Tests.** In `core/world/blocks/defense/turrets/mod.rs` tests (or the file the
existing turret tests live in; `grep -n "fn .*duo" core/world/blocks/defense/turrets/*.rs`):
* `turret_sound_table_matches_blocks_java`: `config_for(content, "hail", names)`
  → `shoot_sound == SoundId::SHOOT_ARTILLERY_SMALL`; `"fuse"` → volume `0.9`;
  `"lancer"` → `charge_sound == SoundId::CHARGE_LANCER`; `"wave"` →
  `shoot_sound == NONE`, `loop_sound == LOOP_SPRAY`; `"meltdown"` →
  `loop_sound_volume == 2.0`. Build `content`/`names` like the existing
  `config_for` tests do.
* `hail_shot_emits_its_own_sound`: copy the setup of the existing duo shoot
  test that uses `SharedAudioLog` (`grep -rn "SharedAudioLog" core/world/blocks/defense/turrets`;
  if none exists, use `CombatHarness` + `harness.build.set_audio_sink(log.clone())`
  exactly as `mind-headless/src/audio_scenarios.rs:216-280` does) with `hail`
  + graphite; assert one `At` event with `sound == SHOOT_ARTILLERY_SMALL`,
  `0.9 <= pitch <= 1.1`.
* In `core/weapons/mod.rs` tests: a dagger shooting with a `SharedAudioLog`
  installed on `CombatCtx.audio` records an `At` with `sound == SoundId::SHOOT`
  (dagger weapon "large-weapon" uses the default `Sounds.shoot`) and
  `0.8 <= pitch <= 1.0`.
* Golden: `cargo test -p mind-headless` `audio_events_sim` will change (duo
  pitch no longer 1.0). Regenerate
  `client/rust/mind-headless/tests/golden/audio/events_sim.json` with
  `cargo run -p mind-headless -- audio events --scenario audio_events_sim --dump client/rust/mind-headless/tests/golden/audio/events_sim.json`
  (confirm the flag with `--help`), update its sha256 in
  `parity/golden_manifest.json`, name it in the commit body
  ("Turret.java:804 random(soundPitchMin, soundPitchMax)").

**Verify in-engine.** Ground Zero (env §4.1), then
```bash
$G --eval 'return get_node("/root/Spine/MindAudio").stats()["events_applied"]'
python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":700,"y":300},"coords":"viewport","pressed":true}}'
sleep 1
python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":700,"y":300},"coords":"viewport","pressed":false}}'
$G --eval 'return get_node("/root/Spine/MindAudio").stats()["events_applied"]'   # larger
$G --eval 'return get_node("/root/Spine/MindAudio").drain_log()'                  # no "missing audio shoot"
```
Holding LMB shoots with the player alpha (G08 owns the input; if the count does
not grow, place a duo + copper per env §4.1 facts and wait for a wave instead).

**Pitfalls.** `TurretConfig` is `Arc`-shared; never mutate a live config.
Do not draw the pitch from `RngStream::Sim` in the turret path (the checksum
goldens would change). `SoundId` import in turrets: `crate::content::registries::sound_meta::SoundId`.

**Commit.** `Gameplay G07 T2: unit weapon and turret shoot/charge sounds reach the client`

### T3. Block place / break / destroy / rotate sounds in live play (PAR-0073)

**Symptom / acceptance.** Finishing a build, finishing a deconstruct, a block
being destroyed and rotating a block are silent in live play. After: each
emits the Java sound through the sim audio sink; acceptance = tests below +
in-engine `MindAudio.stats()["events_applied"]` increases by ≥1 when a
`copper-wall` placed through `apply_sim_command_json` (`"player":0`) finishes.

**Java.**
* `ConstructBlock.constructFinish` (`J/world/blocks/ConstructBlock.java:71-120`):
  `if(fogControl.isVisibleTile(team, x, y)){ placeEffect...; if(shouldPlay()) block.placeSound.at(tile, block.placePitchChange ? calcPitch(true) : 1f); }`.
  `deconstructFinish` (`:58-69`): same with `breakSound`, `calcPitch(false)`.
  ```java
  static boolean shouldPlay(){ if(Time.timeSinceMillis(lastPlayed) >= 32){ lastPlayed = Time.millis(); return true; } return false; }
  static float calcPitch(boolean up){
      if(Time.timeSinceMillis(lastTime) < 16 * 30){ lastTime = Time.millis(); pitchSeq++; if(pitchSeq > 30) pitchSeq = 0;
          return 1f + Mathf.clamp(pitchSeq / 30f) * (up ? 1.9f : -0.4f); }
      else{ pitchSeq = 0; lastTime = Time.millis(); return Mathf.random(0.7f, 1.3f); }
  }
  ```
  `tile` position = `tile.drawx()/drawy()` = tile world + `block.offset`.
  `placePitchChange = breakPitchChange = true` by default (`Block.java:328-330`).
* Defaults (`Block.java:1360-1379`): place `blockPlace1/2/3`, break
  `blockBreak1/2/3` by size (≥3, ≥2, else); destroy size≥3 `blockExplode3`,
  size 2 `RandomSound(blockExplode2, blockExplode2Alt)`, size 1
  `RandomSound(blockExplode1, blockExplode1Alt)`. Overrides: `Wall` size 2
  destroy `blockExplodeWall` (`Wall.java:74`); `CoreBlock` destroy
  `explosionCore`, `destroySoundVolume = 1.6` (`CoreBlock.java:90-91`);
  `Prop` break `rockBreak` (`Prop.java:21`); `spore-cluster` break `plantBreak`
  (`Blocks.java:699`); `ForceProjector.breakSound = shieldBreak` is the shield
  break, not the block (ignore). `destroySoundVolume = 1`, `destroyPitchMin = destroyPitchMax = 1`.
* `BuildingComp.onDestroyed` (`BuildingComp.java:1524-1538`), after
  `playDestroySound()`:
  ```java
  if(explosiveness > 40f) (chance(0.5) ? blockExplodeExplosive : blockExplodeExplosiveAlt).at(tile, random(pmin,pmax), destroySoundVolume);
  else if(flammability > 5f) blockExplodeFlammable.at(tile, ..., destroySoundVolume);
  if(power > 30000f) blockExplodeElectricBig.at(...); else if(power > 2000f) blockExplodeElectric.at(...);
  ```
  `CoreBlock.playDestroySound` (`CoreBlock.java:708-716`): when it is the
  player team's last core and `rules.canGameOver`, play non-positional
  `destroySound.play(1.6 * sfxVolume, 1, 0)`.
* Rotate (`Build.java:90`, `InputHandler.java:704`):
  `Sounds.blockRotate.at(build, 1f + Mathf.range(0.1f), 1f)`.

**Port today.**
* Live finish events: `SimEvent::BlockPlacedEvent`/`BlockBrokenEvent` pushed at
  `core/sim/mod.rs:288,312,334,345,530,989`, `core/sim/builder.rs:715,747`,
  and for destroyed buildings at `core/sim/unit_runtime.rs:606-608` (same
  `BlockBrokenEvent`, indistinguishable from a deconstruct). All are dispatched
  in `Sim::flush_events` (`core/sim/mod.rs:1279-1302`).
* `resolve_deaths` (`core/sim/unit_runtime.rs:~480-660`) has `center`, `size`,
  `explosion_terms = (flammability, explosiveness, power)` per dead building.
* Rotate: `SimCommand::Rotate` in `core/sim/mod.rs:830-870`.
* Audio access inside `Sim`: `self.ecs.0.get_resource::<AudioSinkRes>()`;
  content: `self.block_runtime.as_ref().map(|rt| rt.content())` →
  `&ContentRegistry` (`core/sim/runtime.rs:159`); `BlockDef` has `size`,
  `offset`, `kind: BlockKind`, `name`.

**Changes.**
1. `core/event.rs` `BlockBrokenEvent`: add `pub destroyed: bool` (doc:
  "`true` when the building died (`BlockDestroyEvent`), `false` for
  deconstruct/API break"). Set `false` at `sim/mod.rs:334,345`,
  `sim/builder.rs:715`; `true` at `sim/unit_runtime.rs:607`. Fix `sim/dump.rs`
  if it destructures the struct.
2. `core/audio/events.rs`: append variant (append-only rule)
   ```rust
   /// `ConstructBlock.constructFinish/deconstructFinish` sound; the client applies
   /// `shouldPlay` (32 ms gate) and `calcPitch` on the wall clock.
   Construct { sound: SoundId, x: f32, y: f32, place: bool, pitch_change: bool },
   ```
   and add it to `AudioEvent::sound()`.
3. New file `core/audio/blocks.rs` (license header; `pub mod blocks;` in
   `core/audio/mod.rs`):
   ```rust
   pub fn place_sound(def: &BlockDef) -> SoundId { super::sim::block_place_sound(def.size) }
   pub fn break_sound(def: &BlockDef) -> SoundId {
       match def.kind { BlockKind::Prop if def.name == "spore-cluster" => SoundId::PLANT_BREAK,
                        BlockKind::Prop => SoundId::ROCK_BREAK, _ => super::sim::block_break_sound(def.size) }
   }
   /// (candidates, volume) for `destroySound`.
   pub fn destroy_sound(def: &BlockDef) -> (&'static [SoundId], f32) {
       match def.kind {
           BlockKind::CoreBlock => (&[SoundId::EXPLOSION_CORE], 1.6),
           BlockKind::Wall if def.size == 2 => (&[SoundId::BLOCK_EXPLODE_WALL], 1.0),
           _ => (super::sim::block_destroy_alternates(def.size), 1.0),
       }
   }
   pub fn world_center(def: &BlockDef, x: i16, y: i16) -> (f32, f32) {
       (f32::from(x) * 8.0 + def.offset, f32::from(y) * 8.0 + def.offset)
   }
   ```
   Check `BlockKind::CoreBlock`, `Wall`, `Prop` exist (`core/content/registries/blocks/mod.rs:64+`, verified).
   Note: `Wall.init` only swaps the size-2 default; walls of other sizes keep the default.
4. `Sim::flush_events` (`core/sim/mod.rs`): before `self.events.dispatch(event)`,
   ```rust
   if let Some(audio) = self.ecs.0.get_resource::<crate::audio::AudioSinkRes>().cloned()
       && let Some(content) = self.block_runtime.as_ref().map(|rt| rt.content())
   {
       match &event {
           SimEvent::BlockPlacedEvent(e) => if let Some(def) = content.block(e.block) {
               let (x, y) = crate::audio::blocks::world_center(def, e.x, e.y);
               audio.emit(AudioEvent::Construct { sound: crate::audio::blocks::place_sound(def), x, y, place: true, pitch_change: true });
           },
           SimEvent::BlockBrokenEvent(e) if !e.destroyed => if let Some(def) = content.block(e.block) { /* same with break_sound, place: false */ },
           _ => {}
       }
   }
   ```
   (`AudioSinkRes` is `Clone`; clone before borrowing `self.block_runtime`.)
   Skip `BlockId::AIR` and blocks whose `kind` is `Floor`/`OverlayFloor`/`OreBlock` (no sound upstream for those? — upstream plays for floors too; keep it simple: emit for everything except `AIR`).
5. `resolve_deaths`: right after `destroyed += 1;` emit, using
   `self.ecs.0.get_resource::<AudioSinkRes>().cloned()` and the def
   (`self.unit_runtime().map(|rt| &rt.content)` → `content.block(block)`):
   * destroy sound: `(cands, vol) = destroy_sound(def)`; pick with
     `crate::audio::sim::pick_random_sound(&mut FxAudioRng { rng: &mut <SimRng> }, cands)`
     — use the unit runtime's `rng` (remove/insert the non-send `UnitRuntime`
     as the spill code below does) or, simpler, `self.rng_streams`
     (`Sim.rng_streams: SimRng`, Fx stream only). Use `self.rng_streams`.
     Emit `AudioEvent::at(sound, cx, cy, 1.0, vol, true)` where `(cx, cy) =
     world_center(def, x, y)`.
     Core special case: if `def.kind == CoreBlock` and the dead core's team
     (`team` var already computed) is the campaign `rules.default_team` and no
     other core of that team remains (count entities with `Building` whose
     block kind is `CoreBlock` and `TeamComp.team == team`, excluding this one
     — it is already removed at this point, so count == 0) and
     `rules.canGameOver` (Java default `true`; the port's `core/io/json/rules.rs` has no such field as of the audit — `grep -n can_game_over`; if absent treat it as `true`), emit
     `AudioEvent::play(SoundId::EXPLOSION_CORE, 1.6, 1.0, 0.0, false, false)`
     instead of the positional `At`. The sim has no settings, so in
     `gdext/audio/mod.rs` `apply_events` multiply `AudioEvent::Play` volumes by
     `self.sfx_scale()` (Java's `Sound.play` callers — `CoreBlock.java:712`,
     `LExecutor.java:2256`, `NetClient.java:277` — all pass `volume * Core.audio.sfxVolume`).
     Update the `Play.volume` doc comment in `core/audio/events.rs` to
     "raw volume; the client multiplies by `sfxvol`".
   * extra explosion sounds from `explosion_terms` exactly as the Java block
     above (Fx-stream `chance(0.5)` → `next_f32() < 0.5`).
6. `SimCommand::Rotate` success path: emit
   `AudioEvent::at(SoundId::BLOCK_ROTATE, cx, cy, 1.0 + (fx * 0.2 - 0.1), 1.0, true)`
   with `fx = self.rng_streams.next_float(RngStream::Fx)`; centre from the
   building's def/tile.
7. `gdext/audio/mod.rs` `apply_events`: handle `Construct`:
   fields `construct_last_played_ms: f64`, `construct_last_time_ms: f64`,
   `construct_pitch_seq: i32` on `MindAudio`;
   ```rust
   AudioEvent::Construct { sound, x, y, place, pitch_change } => {
       if now_ms - self.construct_last_played_ms < 32.0 { continue; }
       self.construct_last_played_ms = now_ms;
       let pitch = if !pitch_change { 1.0 } else if now_ms - self.construct_last_time_ms < 480.0 {
           self.construct_last_time_ms = now_ms; self.construct_pitch_seq += 1;
           if self.construct_pitch_seq > 30 { self.construct_pitch_seq = 0; }
           1.0 + (self.construct_pitch_seq as f32 / 30.0).clamp(0.0, 1.0) * if *place { 1.9 } else { -0.4 }
       } else { self.construct_pitch_seq = 0; self.construct_last_time_ms = now_ms; 0.7 + self.rng.next_f32() * 0.6 };
       let scaled = calc_falloff(*x, *y, listener, 0.0) * self.sfx_scale();
       if scaled >= MIN_PLAY_VOLUME { self.play_sound(sound.name(), scaled, pitch, calc_pan(*x, listener)); }
   }
   ```
   (`SeededAudioRng` implements `AudioRng::next_f32`; import the trait.)
   Note: `shouldPlay` gates on the call, before `calcPitch` — same order as Java.
8. `core/world/harness.rs:405-407` (BuildHarness) keeps its own emission; do
   not double-emit: the harness does not go through `Sim::flush_events`.

**Tests.** `core/sim/production_tests.rs` (helpers `runtime_sim`, `place` are
private to that file; add the tests there):
* `placing_emits_construct_place_sound`: `runtime_sim()`, install
  `AudioSinkRes::new(log.clone())` with `sim.ecs.0.insert_resource(...)`,
  `place(&mut sim, "copper-wall", 5, 5)`, `sim.tick()` (flush runs at tick end —
  confirm with `grep -n "fn tick" core/sim/mod.rs`; if `place` already flushes,
  no tick needed). Assert one `AudioEvent::Construct { sound: BLOCK_PLACE1, x: 40.0, y: 40.0, place: true, .. }`
  (size 1 → offset 0 → 5*8).
* `breaking_emits_break_not_destroy`: then `SimCommand::Break { x: 5, y: 5 }`,
  tick, assert a `Construct { sound: BLOCK_BREAK1, place: false }` and no `At`.
* `large_wall_place_sound_is_size_2`: `copper-wall-large` at (8,8) → `BLOCK_PLACE2`,
  `x == 8*8 + offset` (read `offset` from the def; expected 4.0 for size 2 if
  `offset = (size + 1) % 2 * 4`; assert against `def.offset`).
* In `core/sim/combat_tests.rs` (has unit-runtime sims; reuse a test that kills
  a building, `grep -n "destroyed\|resolve_deaths" core/sim/combat_tests.rs`):
  `destroyed_wall_emits_destroy_sound`: a killed `copper-wall` emits an `At`
  with sound ∈ {`BLOCK_EXPLODE1`, `BLOCK_EXPLODE1_ALT`} and no `Construct`.
* `core/audio/blocks.rs` tests: `destroy_sound` for `core-shard` →
  `([EXPLOSION_CORE], 1.6)`; for `copper-wall-large` → `BLOCK_EXPLODE_WALL`;
  `break_sound` for `boulder` (a `Prop`, check with `content.block_id("boulder")`) → `ROCK_BREAK`.

**Verify in-engine.** Ground Zero (env §4.1), seed copper:
```bash
$G --eval "$H.debug_add_core_items(\"copper\", 300); return true"
$G --eval "return get_node(\"/root/Spine/MindAudio\").stats()[\"events_applied\"]"
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":127,\"y\":52,\"block\":\"copper-wall\",\"rotation\":0,\"team\":1,\"player\":0}')"
sleep 4
$G --eval "return [$H.building_debug(127,52), get_node(\"/root/Spine/MindAudio\").stats()[\"events_applied\"]]"
```
(Check the JSON keys of the `place` op with `grep -n '"place"' gdext/sim_host.rs` first.)
Expected: the wall exists and `events_applied` grew.

**Pitfalls.** `flush_events` takes `&mut self`; clone the `AudioSinkRes`
first, then borrow `block_runtime` immutably. Fog gating (`isVisibleTile`) is
skipped (fog is G03); note it in the commit body. `BlockBrokenEvent` is
`Copy + Eq`; the new bool keeps that.

**Commit.** `Fixes PAR-0073: live place/break/destroy/rotate block sounds`

### T4. Loop voices: `LoopOutput` in the client, `LoopMixer::update` every frame, ambient poller fixes (PAR-0071 part 1)

**Symptom / acceptance.** Loop aggregation never reaches a voice. After:
`$G --eval 'var a = get_node("/root/Spine/MindAudio"); a.debug_emit_loop("loopConveyor", 0, 0, 1.0); return true'`
repeated for ~10 frames (or a new `debug_hold_loop` seam below) makes
`loop_voice_count("loopConveyor") == 1` and `loop_volume("loopConveyor") > 0.01`;
after the hold stops, within 2 s the count returns to 0.

**Java.** `SoundControl.updateLoops` (`J/audio/SoundControl.java:410-469`,
transcribed in `core/audio/loops.rs::LoopMixer::update`, which is correct):
not in game → clear; paused → return; `avol = ambientvol/100`; per sound
`curVolume = lerpDelta(curVolume, volume * avol, 0.11)`; start a looping voice
when `curVolume > 0.01`; stop when `curVolume <= 0.001`; else update pan/volume
and pitch (`pitch = Σ(pitch·vol)/Σvol`); reset accumulators. Then merge the
ambient thread's **last** output (`localData.set(ambientThread.outputData)`)
into `sounds` **every frame**. `AudioThread.doLoop` (`:505-561`): for each
source accumulate into a per-sound `SoundData`; the first time a sound becomes
non-silent in this pass, `localData.add(data)` (the live object); after the
loop `outputData = clones of localData` (so each output row holds the **full**
sum of all sources of that sound); then reset the accumulators.

**Port today.**
* `core/audio/loops.rs:489-513 AmbientPoller::do_loop` clones at first touch
  (bug 1). `AmbientPoller::update` (`:465-487`) clears `output` on non-due frames
  (bug 2: Java's merge source persists between polls).
* `gdext/audio/mod.rs` never calls `self.mixer.update(...)`; there is no
  `LoopOutput` impl; `StopLoops` replaces the mixer without stopping voices.

**Changes.**
1. `core/audio/loops.rs` `do_loop`: collect the sound ids that became
   non-silent into `self.output_order: Vec<SoundId>` (new field, capacity 16)
   during the pass; after the loop, `self.output.clear()` and push
   `self.scratch[&id].clone()` for each id in order. Then (Java's reset) the
   scratch is cleared at the start of the next pass (already). Keep zero
   steady-state allocation (reuse vectors).
2. `AmbientPoller::update`: on not-due frames return `&self.output` unchanged
   (the last poll's rows); on `is_menu || !is_playing` keep clearing. Add
   `pub fn last_output(&self) -> &[AmbientSoundData]`.
3. `LoopMixer::stop_all` already exists; use it for `AudioEvent::StopLoops`
   and when leaving the game.
4. `gdext/audio/voices.rs`: add
   ```rust
   pub const MAX_LOOP_VOICES: usize = 32;
   pub struct LoopVoicePool { players: Vec<Gd<AudioStreamPlayer2D>>, generation: Vec<u32> }
   impl LoopVoicePool {
       pub fn new(base: &mut Gd<Node>, sound_bus: &str) -> Self   // like VoicePool::new, parent "LoopVoices"
       /// Voice id = (generation << 8) | slot; returns 0 when no slot is free.
       pub fn start(&mut self, stream: Gd<AudioStream>, volume: f32, pitch: f32, pan: f32, listener: (f32, f32), width: f32) -> u64
       pub fn update(&mut self, voice: u64, volume: f32, pitch: f32, pan: f32, listener: (f32, f32), width: f32)
       pub fn stop(&mut self, voice: u64)        // stop + bump generation
       pub fn is_playing(&self, voice: u64) -> bool  // generation matches && player.is_playing()
       pub fn set_paused(&self, paused: bool); pub fn stop_all(&mut self)
   }
   ```
   Free slot = first player with `!is_playing()`. Position/pan exactly like
   `VoicePool::play_world`. Volume `set_volume_linear(clamp01)`, pitch
   `set_pitch_scale(clamp(0.0001, 10))`.
5. `gdext/audio/mod.rs`: field `loop_pool: Option<LoopVoicePool>` created in
   `ready()` after `VoicePool::new` (same `self_node`; `# code-instantiated`
   comment as for the voice pool). A private adapter:
   ```rust
   struct GodotLoopOutput<'a> { pool: &'a mut LoopVoicePool, streams: &'a mut StreamCache, listener: Listener }
   impl LoopOutput for GodotLoopOutput<'_> {
       fn start_loop(&mut self, sound: SoundId, v: f32, p: f32, pan: f32) -> u64 {
           match self.streams.sound(sound.name(), true) { Some(s) => self.pool.start(s, v, p, pan, (self.listener.x, self.listener.y), self.listener.width), None => 0 }
       }
       fn update_loop(&mut self, voice: u64, v: f32, p: f32, pan: f32) { self.pool.update(voice, v, p, pan, (self.listener.x, self.listener.y), self.listener.width) }
       fn stop_loop(&mut self, voice: u64) { self.pool.stop(voice) }
       fn is_playing(&self, voice: u64) -> bool { voice != 0 && self.pool.is_playing(voice) }
   }
   ```
   Java skips `setPitch` when pitch ≈ 1 — irrelevant here.
6. In `process`, after `apply_events` and before music: 
   ```rust
   let in_game = !context_is_menu;      // the same is_menu as T1 step 10
   let avol = self.settings.ambientvol as f32 / 100.0;
   if let (Some(pool), listener) = (self.loop_pool.as_mut(), self.listener()) {
       let mut out = GodotLoopOutput { pool, streams: &mut self.streams, listener };
       self.mixer.update(avol, in_game, self.sound_paused, listener, &mut out);
   }
   ```
   then (T5 fills the snapshot) `self.mixer.merge_ambient(self.poller.last_output())`
   where `poller: AmbientPoller` is a new field (`AmbientPoller::new(50.0)`).
   `LoopMixer::update` with `in_game == false` clears data without stopping
   voices: call `self.mixer.stop_all(&mut out)` instead when `!in_game`, then
   skip `update`. Apply `loop_pool.set_paused(self.sound_paused)` next to the
   voice-pool pause mirror.
7. `debug_emit_loop` uses `Listener::default()`; switch to `self.listener()`.
   Add `#[func] debug_hold_loop(sound, x, y, volume, frames: i64)` that stores a
   `(SoundId, x, y, vol, remaining)` and re-accumulates it each frame until
   `remaining` hits 0 (test seam; doc it as such).

**Tests.** `core/audio/loops.rs`:
* `do_loop_output_holds_full_sum`: snapshot with two `LOOP_HUM` entries at
  `(0,0)` and `(10,0)`, volume 0.5 each, listener `(0,0,100)`. Falloff:
  `calc_falloff = clamp(1/(dst²/16000))` → `(0,0)`: dst² 0 → `1/0 = inf` →
  clamp 1.0; `(10,0)`: 16000/100 = 160 → 1.0. Expected output row
  `volume = clamp(0.5 + 0.5) = 1.0`, `total = 2.0`, `sum_x = 10.0`. (Today it
  reports 0.5/1.0/0.0.)
* `poller_keeps_output_between_polls`: after a due poll, `update(10.0, ...)`
  (not due) returns the same rows.
* Update the existing `ambient_merge_silent_gate` test only if it asserted the
  old clearing behaviour; record that in the commit body.
* Keep `steady_state_audio_allocates_nothing` compiling (feature-gated).

**Verify in-engine.** Ground Zero:
```bash
$G --eval 'get_node("/root/Spine/MindAudio").debug_hold_loop("loopConveyor", 1032, 440, 1.0, 120); return true'
sleep 0.5; $G --eval 'var a = get_node("/root/Spine/MindAudio"); return [a.loop_voice_count("loopConveyor"), a.loop_volume("loopConveyor")]'   # [1, >0.01]
sleep 3;  $G --eval 'return get_node("/root/Spine/MindAudio").loop_voice_count("loopConveyor")'   # 0
```

**Pitfalls.** Borrow split: `self.mixer`, `self.loop_pool`, `self.streams` are
separate fields — destructure or borrow them individually (no `&mut self`
method calls while `out` is alive). `LoopMixer::update` freezes when paused —
expected (Java freezes too).

**Commit.** `PAR-0071: loop voices play (LoopOutput + per-frame LoopMixer::update, ambient poller fixes)`

### T5. Building ambient sounds: drills, conveyors, crafters, generators, projectors (PAR-0071 part 2)

**Symptom / acceptance.** No building hums. After: on Ground Zero with a
working `mechanical-drill` at (130,48) feeding a `duo` and a `conveyor` line,
`loop_voice_count("loopDrill") == 1` and `loop_volume("loopDrill") > 0` with
the camera on the drill; moving the camera 400 px away drops the volume
(falloff).

**Java.**
* `BuildingComp.add` → `control.sound.addAmbientSource(this)` when
  `block.ambientSound != Sounds.none` (`BuildingComp.java:131-133`).
  `getAmbientVolume() = block.ambientSoundVolume * ambientVolume()`,
  `getAmbientSound() = block.ambientSound` (`:2296-2303`).
  Defaults: `shouldAmbientSound() = shouldConsume()` (`:1217`),
  `ambientVolume() = efficiency` (`:2022`), `Block.ambientSoundVolume = 0.05`
  (`Block.java:358`).
* Overrides:
  | class (`BlockKind`) | shouldAmbientSound | ambientVolume |
  |---|---|---|
  | `GenericCrafter`, and subclasses `AttributeCrafter`, `HeatCrafter`, `HeatProducer` | `efficiency > 0` | default |
  | `Separator` | `efficiency > 0` | default |
  | `Conveyor`, `ArmoredConveyor` | `clogHeat <= 0.5` | default |
  | `StackConveyor` | `false` | – |
  | `Drill` (`BurstDrill` inherits) | `efficiency > 0.01 && items.total() < itemCapacity` | `efficiency * size² / 4` |
  | `BurstDrill` | as Drill | `Drill volume * progress()^4` (deferred: see below) |
  | power generators (`ConsumeGenerator`, `ThermalGenerator`, `NuclearReactor`, `SolarGenerator`, `VariableReactor`, `HeaterGenerator`) | default | `clamp(productionEfficiency)` |
  | `ImpactReactor` | default | `warmup` |
  | `RepairTurret` | `target != null && efficiency > 0` | default |
  | `ForceProjector` | `!broken && realRadius() > 1` | default |
  (`Separator.java:87`, `GenericCrafter.java:373`, `Conveyor.java:207`,
  `StackConveyor.java:329`, `Drill.java:252-259`, `BurstDrill.java:123`,
  `PowerGenerator.java:203`, `ImpactReactor.java:123`, `RepairTurret.java:182`,
  `ForceProjector.java:184`.)
* Class default sounds (constructor): `Conveyor` loopConveyor 0.0022;
  `StackConveyor` loopConveyor 0.004; `Drill` loopDrill 0.019; `BurstDrill`
  drillCharge 0.18; `BeamDrill` loopMineBeam 0.05; `Fracker` loopDrill 0.03;
  `GenericCrafter` (+ subclasses) loopMachine 0.03; `RepairTurret` beamHeal 1.0;
  `ForceProjector` loopShield 0.1; `Reconstructor` loopUnitBuilding (0.05);
  `UnitFactory` loopUnitBuilding 0.09; `UnitAssembler` loopUnitBuilding 0.13;
  `UnitCargoLoader` loopUnitBuilding (0.05); `OverdriveProjector` loopCircuit
  0.13; `RegenProjector` loopRegen 0.45.
* Per-block overrides (`Blocks.java` line): silicon-smelter loopSmelter 0.07
  (1079); silicon-crucible loopSmelter 0.07 (1098); kiln loopSmelter 0.07 (1113);
  phase-weaver loopTech 0.02 (1148); pyratite-mixer loopMachineSpin 0.1 (1199);
  blast-mixer loopMachineSpin 0.12 (1213); pulverizer loopGrind 0.025 (1306);
  silicon-arc-furnace loopSmelter 0.12 (1349); electrolyzer loopElectricHum 0.08
  (1392); atmospheric-concentrator loopExtract 0.06 (1419); oxidation-chamber
  loopExtract 0.08 (1443); electric-heater / slag-heater / phase-heater loopHum
  (volume inherited 0.03; 1462/1478/1492); carbide-crucible loopSmelter 0.09
  (1546); surge-crucible loopSmelter 0.9 (1598); cyanogen-synthesizer
  loopExtract 0.08 (1643); phase-synthesizer loopTech 0.04 (1666);
  overdrive-projector 0.08 (1951); overdrive-dome 0.12 (1961);
  combustion-generator loopSmelter 0.03 (2528); thermal-generator loopHum 0.06
  (2550); steam-generator loopSmelter 0.06 (2563); differential-generator
  loopDifferential 0.12 (2596); thorium-reactor loopThoriumReactor 0.11 (2632);
  impact-reactor loopPulse 0.08 (2656); turbine-condenser loopHum 0.06 (2715);
  chemical-combustion-chamber loopSmelter 0.06 (2747); pyrolysis-generator
  loopSmelter 0.06 (2775); flux-reactor loopFlux 0.15 (2793);
  neoplasia-reactor loopBio 0.2 (2841); cultivator loopCultivator 0.075 (2966);
  vent-condenser loopHum 0.06 (3011); cliff-crusher loopDrill 0.04 (3031);
  large-cliff-crusher loopDrill 0.08 (3044); repair-point 0.9 (6427);
  repair-turret 1.25 (6442); heat-source none (6802).
* 20 Hz poll + merge: T4.

**Port today.** No ambient data on `BlockDef`. Readable state per entity
(all `bevy` components, verified): `crate::entities::comp::Building`
(`efficiency`, `should_consume`, `enabled`, `tile`, `block`;
`core/entities/comp/building.rs:27-60`),
`crate::world::blocks::distribution::conveyor::ConveyorBuild.clog_heat`
(`conveyor.rs:35,57`), `crate::world::modules::ItemModule::total()`
(`world/modules/mod.rs:120`), `crate::world::blocks::power::generator::GeneratorState.production_efficiency`
(`generator.rs:137-149`), `crate::world::blocks::power::behavior::ImpactReactorState.warmup`
(`behavior.rs:528-532`), `crate::world::blocks::defense::shields::ForceProjectorState`
(`broken`, `real_radius()`; `shields.rs:69,91,129`). No ECS field stores a
repair turret target (`RepairTurret` component holds config only,
`world/blocks/units/mod.rs:534`).

**Changes.**
1. New `core/audio/ambient.rs` (`pub mod ambient;` in `core/audio/mod.rs`):
   ```rust
   /// `Block.ambientSound` / `ambientSoundVolume` resolved from class defaults + Blocks.java overrides.
   pub fn block_ambient(def: &BlockDef) -> Option<(SoundId, f32)>
   /// Fills `out` with one entry per building whose sound is set and which should sound now.
   pub fn collect_ambient(world: &World, content: &ContentRegistry, out: &mut AmbientSnapshot)
   ```
   `block_ambient`: first a `match def.name.as_str()` over the per-block list
   above (return `Some((SoundId::by_name("<name>")?, vol))`; for the three
   heaters use volume 0.03; `heat-source` → `None`), then
   `match def.kind` for the class defaults; `_ => None`. For per-block
   entries that only override the volume (overdrive-projector/dome,
   repair-point/turret) combine with the class sound.
   `collect_ambient`: `out.entries.clear()`; iterate
   `world.iter_entities()` filtering `Building`, collect
   `(EntitySeq, Entity)`, sort by `(seq, entity.index())` (determinism rule —
   order affects only float summation, but keep the rule), then for each:
   `def = content.block(building.block)`, `(sound, base_vol) = block_ambient(def)?`,
   `should` and `volume_scale` per the override table (read components with
   `world.get::<T>(e)`; missing component → default rule), skip when `!should`,
   `vol = base_vol * volume_scale`, skip when `vol <= MIN_LOOP_VOLUME`, push
   `AmbientEntry { sound, x: tile.x*8 + def.offset, y: tile.y*8 + def.offset, volume: vol }`.
   `RepairTurret`: use `efficiency > 0` only (no target field; deviation noted
   in the doc comment). `BurstDrill`: Drill formula without `progress^4`
   (deviation; Erekir only).
2. `gdext/sim_host.rs`: Rust-only (no `#[func]`) seam
   ```rust
   /// T5: live ambient sources for `MindAudio`'s 20 Hz poll.
   pub fn ambient_snapshot(&self, out: &mut mind_core::audio::AmbientSnapshot) {
       let Some(content) = self.sim.block_runtime().map(|rt| rt.content()) else { out.entries.clear(); return };
       mind_core::audio::ambient::collect_ambient(&self.sim.ecs.0, content, out);
   }
   ```
   (`Sim::block_runtime()` is at `core/sim/runtime.rs:959`; confirm it is `pub`.)
3. `gdext/audio/mod.rs`: field `ambient: AmbientSnapshot`; in `process`,
   before the T4 mixer block: compute `due = self.poller.accum_ms + delta_ms >= self.poller.interval_ms`;
   when due and in game, refresh `self.ambient` via
   `self.sim_host().map(|h| h.bind().ambient_snapshot(&mut self.ambient))`
   (bind only for this statement), then
   `self.poller.update(delta_ms, is_menu, in_game && !paused, &self.ambient, listener)`.
   `delta_ms = delta * 1000.0`. Then `mixer.merge_ambient(poller.last_output())`
   **after** `mixer.update` (Java order). Add `ambient_sources` (entry count)
   to `stats()`.

**Tests.** `core/audio/ambient.rs` tests (build content with
`crate::content::load::...` like other registry tests; simplest: use a
`Sim` from `core/sim/production_tests.rs` patterns — `Sim::new(3, 32, 32, ...)`,
`install_block_runtime()`, place via `SimCommand::Place`):
* `block_ambient_table`: `mechanical-drill` → `(LOOP_DRILL, 0.019)`;
  `conveyor` → `(LOOP_CONVEYOR, 0.0022)`; `silicon-smelter` → `(LOOP_SMELTER, 0.07)`;
  `graphite-press` (plain GenericCrafter) → `(LOOP_MACHINE, 0.03)`;
  `electric-heater` → `(LOOP_HUM, 0.03)`; `overdrive-projector` →
  `(LOOP_CIRCUIT, 0.08)`; `copper-wall` → `None`; `plastanium-conveyor` → `(LOOP_CONVEYOR, 0.004)`.
* `drill_volume_formula`: place a `mechanical-drill` (size 2), set
  `Building.efficiency = 1.0`, empty `ItemModule` → one entry with
  `volume = 0.019 * 1.0 * 4 / 4 = 0.019`; set efficiency 0.0 → no entry; fill the
  `ItemModule` to `item_capacity` (10 for mechanical-drill — read it from the
  def) → no entry.
* `conveyor_clog_gate`: `clog_heat = 0.6` → no entry; `0.2` + `should_consume`
  irrelevant → entry with `0.0022 * efficiency` (set efficiency 1.0).

**Verify in-engine.** Ground Zero, build the drill→duo setup (env §4.1 facts:
`place_block(130,48,"mechanical-drill")`, `place_block(132,48,"duo")`), center
the camera `$C.center_on_tile(130,48)`, step 120 ticks, then
```bash
$G --eval 'var a = get_node("/root/Spine/MindAudio"); return [a.stats()["ambient_sources"], a.loop_voice_count("loopDrill"), a.loop_volume("loopDrill")]'
```
Expected `[≥1, 1, >0]`. Then `$C.center_on_tile(180,100)` and re-read
`loop_volume` → smaller (or 0 after the lerp).

**Pitfalls.** Do not call `ambient_snapshot` every frame (20 Hz only).
`BlockKind` variants used must exist (verified list in §1: Conveyor,
ArmoredConveyor, StackConveyor, Drill, BurstDrill, BeamDrill, Fracker,
GenericCrafter, AttributeCrafter, HeatCrafter, HeatProducer, Separator,
RepairTurret, ForceProjector, Reconstructor, UnitFactory, UnitAssembler,
UnitCargoLoader, OverdriveProjector, RegenProjector, ConsumeGenerator,
ThermalGenerator, NuclearReactor, ImpactReactor, VariableReactor,
HeaterGenerator, WallCrafter, SolarGenerator). `WallCrafter` has no class
default (only the two cliff crushers set it).

**Commit.** `Fixes PAR-0071: building ambient loops (drills, conveyors, crafters, generators)`

### T6. Per-frame loop sources: build beam, mining, fire, unit loops, weather wind/rain, turret loops

**Symptom / acceptance.** Building with the player (build beam), mining,
fires, hover units, tanks, rain/snow/sandstorm and turret loops (wave spray,
meltdown beam, parallax) are silent. After: while the player builds a
`copper-wall` within range, `loop_voice_count("loopBuild") == 1`; with a forced
snow weather (T9 seam) `loop_voice_count("windHowl") == 1`.

**Java (all `control.sound.loop(sound, pos, volume[, pitch])` each update):**
* `BuilderComp.update` (`J/entities/comp/BuilderComp.java:150-158`): for the
  current plan within `finalPlaceDst` (= `state.rules.infiniteResources ? MAX : type.buildRange`):
  `loop(Sounds.loopBuild, tile, 1.3f)` (position = plan tile world centre).
* `MinerComp.update` (`MinerComp.java:85-119`): when `mining() && item != null`
  (`mining() = mineTile != null && !activelyBuilding()`):
  `loop(type.mineSound, this, type.mineSoundVolume)` (defaults `loopMineBeam`, 0.6, `UnitType.java:394-396`).
* `FireComp.update` (`FireComp.java:46`): `loop(Sounds.loopFire, this, 0.07f)`.
* `UnitComp.update` (`UnitComp.java:762-767`): `loop(type.loopSound, this, type.loopSoundVolume)`;
  if `moveSound != none`: `progress = clamp(vel.len() / type.speed)`,
  `loop(moveSound, this, moveSoundVolume * progress, lerp(moveSoundPitchMin, moveSoundPitchMax, progress))`.
* `TankComp.update` (`TankComp.java:35,54`): while moving (`walked`):
  `loop(type.tankMoveSound, this, type.tankMoveVolume)` (defaults `tankMove`, 0.5).
* `Weather.updateEffect` (`J/type/Weather.java:103-106`):
  `noise = soundVolOscMag > 0 ? |Noise.rawNoise(Time.time / soundVolOscScl)| * soundVolOscMag : 0;`
  `loop(sound, Core.camera.position, max((soundVol + noise) * state.opacity, soundVolMin))`.
  Values (`Weathers.java`): snowing windHowl soundVol 0, osc mag 1.5, osc scl
  1100, min 0.02; rain `rain` 0.25; sandstorm `wind` 0.8; sporestorm `wind` 0.7;
  fog/suspend-particles none.
* `TractorBeamTurret.updateTile` (`TractorBeamTurret.java:117`): while it has a
  target and is shooting: `loop(shootSound=beamParallax, this, 0.9)`.
* Turret `SoundLoop` (`Turret.java:304,504`): `soundLoop.update(x, y, shouldActiveSound(), activeSoundVolume())`
  each `updateTile`; `stop()` on remove/destroy. `shouldActiveSound`:
  `Turret`: `shootWarmup > 0.01 && loopSound != none`; `LiquidTurret` /
  `ContinuousLiquidTurret`: `wasShooting && enabled`; `ContinuousTurret` /
  `LaserTurret`: `bullets.any()`. `activeSoundVolume`: `Turret` `shootWarmup`,
  Continuous/Laser `1`. `SoundLoop.update` (`J/audio/SoundLoop.java:25-46`):
  first call with `play` starts the voice at `calcVolume * volume(=0) * base * scl`;
  then `volume = clamp(volume ± 0.05 * delta)`; stop when `volume <= 0.001`;
  `Core.audio.set(id, calcPan, calcVolume(x,y) * volume * baseVolume * volumeScl)`
  — note `calcVolume` = falloff × **sfxVolume**.

**Port today.** None of these sources exist. `SoundLoopState`
(`core/audio/loops.rs:202-318`) mirrors `SoundLoop` (without sfx scaling).
Components: `crate::sim::builder::BuilderPlans { plans: VecDeque<UnitBuildPlan>, update_building, .. }`
(`core/sim/builder.rs:97`), `MinerComp { mine_tile, mining }`,
`TankComp`, `UnitTypeComp { type_id }` (`core/entities/comp/unit/comp.rs`),
`Pos`/`Vel` (`core/entities/comp/base.rs`), `crate::combat::fires::FireState { tile }`
(`core/combat/fires.rs:37`), `WeatherRuntime` (`core/entities/comp/weather.rs:36`),
`TurretState { config, shoot_warmup, was_shooting, bullets, target, .. }`
(`core/world/blocks/defense/turrets/mod.rs:282+`). `UnitTypeDef` fields
`loop_sound`, `loop_sound_volume`, `move_sound`, `move_sound_volume`,
`move_sound_pitch_min/max`, `tank_move_sound`, `tank_move_volume`,
`mine_sound`, `mine_sound_volume`, `speed`, `build_range`
(`core/content/registries/units/mod.rs` resolved def ~1011-1320).
`WeatherDef.sound: Option<String>`, `sound_vol`, `sound_vol_min`,
`sound_vol_osc_mag`, `sound_vol_osc_scl` (`core/content/registries/weathers.rs:244-253`).

**Changes.**
1. New `core/audio/frame.rs` (`pub mod frame;`):
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub enum LoopPos { World(f32, f32), Camera }
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub struct LoopSource { pub sound: SoundId, pub pos: LoopPos, pub volume: f32, pub pitch: f32 }
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub struct TurretLoopSource { pub key: u64, pub sound: SoundId, pub base_volume: f32, pub x: f32, pub y: f32, pub play: bool, pub volume_scl: f32 }
   /// Every `control.sound.loop` source live this frame (see T6 table).
   pub fn collect_frame_loops(world: &World, content: &ContentRegistry, view_tick: f32, infinite_build: bool, out: &mut Vec<LoopSource>)
   /// Turret `SoundLoop` inputs (`Turret.updateTile`).
   pub fn collect_turret_loops(world: &World, content: &ContentRegistry, out: &mut Vec<TurretLoopSource>)
   ```
   Iterate entities sorted by `(EntitySeq, index)`. Units: skip `UnitCore.dead`.
   * builder: `BuilderPlans` with `update_building == true` and a front plan;
     `range = if infinite_build { f32::MAX } else { def.build_range }`;
     plan centre `(px, py) = (x*8 + off, y*8 + off)` where `off` = the planned
     block's `def.offset` (`content.block(plan.block)`; breaking plans: the
     current tile block — use offset 0 if unknown); if
     `(px-ux)² + (py-uy)² <= range²` push `(LOOP_BUILD, World(px,py), 1.3, 1.0)`.
   * miner: `MinerComp { mining: true, mine_tile: Some(_) }` → `(def.mine_sound, World(ux,uy), def.mine_sound_volume, 1.0)`.
     (`mining` is set by the miner system only while actually mining — it is
     the port's `mining() && item != null`; verify with
     `grep -n "mining = " core/entities core/ai -r` and note what you found.)
   * unit loop: `def.loop_sound` (skip none) volume `def.loop_sound_volume`;
     move sound: `progress = clamp(|vel| / def.speed)`, volume
     `def.move_sound_volume * progress`, pitch
     `lerp(def.move_sound_pitch_min, def.move_sound_pitch_max, progress)`.
   * tank: entity has `TankComp` and `|vel| >= 0.01` → `(def.tank_move_sound, World(ux,uy), def.tank_move_volume, 1.0)`.
   * fire: `FireState` → `(LOOP_FIRE, World(tile.0*8, tile.1*8), 0.07, 1.0)`.
   * weather: each `WeatherRuntime`, `wdef = content.weather(rt.weather)`,
     `sound = SoundId::by_name(wdef.sound?)`, `noise = if osc_mag > 0 { crate::math::noise::raw2d(0, (view_tick / osc_scl) as f64, 0.0).abs() as f32 * osc_mag } else 0`,
     volume `((sound_vol + noise) * rt.opacity).max(sound_vol_min)`, `Camera`.
     Deviation (document it): Arc `Noise.rawNoise` 1-D Perlin with a seeded
     permutation table is replaced by the port's simplex `raw2d` (audio only).
   * tractor beam: `TurretState` with `config.kind == TurretKind::TractorBeam`
     and `target.is_some()` (check the port's "shooting" flag name in
     `advanced.rs` tractor update; if there is a `any`/`is_shooting` field use
     it) → `(BEAM_PARALLAX, World(bx,by), 0.9, 1.0)`.
   `collect_turret_loops`: every `TurretState` whose `config.loop_sound` is not
   none (needs T2): `play` and `volume_scl` per the Java table above
   (`TurretKind::Liquid` → `was_shooting && Building.enabled`, scl
   `shoot_warmup`; `TurretKind::Continuous | Laser` → `!bullets.is_empty()`,
   scl 1.0; else `shoot_warmup > 0.01`, scl `shoot_warmup`), `key = entity.to_bits()`,
   `base_volume = config.loop_sound_volume`, position = building centre.
2. `gdext/sim_host.rs` Rust-only seams:
   `pub fn audio_frame_loops(&self, out: &mut Vec<LoopSource>)` and
   `pub fn audio_turret_loops(&self, out: &mut Vec<TurretLoopSource>)` using
   `self.sim.block_runtime().map(|rt| rt.content())` (or `content_snapshot`
   if the block runtime is absent), `view_tick = self.sim.tick_count() as f32`,
   `infinite_build` = campaign `rules.infinite_resources` (field name: grep
   `infinite_resources` in `core/io/json/rules.rs`).
3. `gdext/audio/mod.rs`, each frame in game and not paused, before the T4
   `mixer.update`: refill `self.frame_loops` (reused `Vec`) from the host and
   `mixer.accumulate(sound, x, y, volume, pitch, listener)` with
   `(x, y) = listener.(x, y)` for `Camera`. Turret loops: field
   `turret_loops: IndexMap<u64, SoundLoopState>`; for each source, get or
   insert `SoundLoopState::new(sound, base_volume, key)`, call
   `update(x, y, play, volume_scl, listener)` and apply the action on the
   `LoopVoicePool` (Start → `pool.start(stream_looping, volume * sfx_scale(), 1.0, pan, ..)`
   and store the returned voice in `state.voice`; Update → `pool.update(voice, volume * sfx_scale(), 1.0, pan, ..)`;
   Stop → `pool.stop(voice)`). Keys not reported this frame → `state.stop()` +
   `pool.stop` + remove (Java `remove()`/`onDestroyed` → `soundLoop.stop()`).
   Leaving the game → stop all turret loops.
   Note the port's `SoundLoopState::update` start volume uses `self.volume`
   (0 on start) — same as Java.

**Tests.** `core/audio/frame.rs` tests using a unit-runtime sim (copy the
`sim_with_player()` body from `core/sim/builder.rs:988-...` — it is private):
* `builder_in_range_emits_loop_build`: queue a player place plan next to the
  player (`sim.queue_player_place(x, y, block, 0)`, `core/sim/builder.rs:777`),
  run one tick so `update_building` is set, collect → exactly one
  `LOOP_BUILD` with volume 1.3 at the plan centre.
* `fire_emits_loop_fire`: spawn a fire with the fires API used by
  `core/sim/combat_tests.rs` (`grep -n "fires::create\|FireState" core/sim/combat_tests.rs`)
  → one `LOOP_FIRE`, volume 0.07.
* `rain_weather_emits_camera_loop`: spawn `WeatherRuntime::new(rain_id, 1.0, 600.0)`
  with `opacity = 1.0` → `LoopSource { sound: RAIN, pos: Camera, volume: 0.25 }`;
  snow with opacity 0 → volume `0.02` (soundVolMin).
* `liquid_turret_loop_plays_when_shooting`: a `wave` `TurretState` with
  `was_shooting = true`, enabled → `TurretLoopSource { sound: LOOP_SPRAY, play: true, base_volume: 0.5 }`.

**Verify in-engine.** Ground Zero:
```bash
$G --eval "$H.debug_add_core_items(\"copper\", 300); return true"
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":127,\"y\":52,\"block\":\"copper-wall-large\",\"rotation\":0,\"team\":1,\"player\":0}')"
sleep 0.5; $G --eval 'var a = get_node("/root/Spine/MindAudio"); return [a.loop_voice_count("loopBuild"), a.loop_volume("loopBuild")]'   # [1, >0]
```
Weather half: after T9 (`debug_create_weather("snowing", 1.0, 3600)`, wait 5 s):
`loop_voice_count("windHowl") == 1`.

**Pitfalls.** Collecting runs every frame: reuse the `Vec`s; bind the host only
for the collect call. `SoundLoop` volumes include `sfxvol` (Java `calcVolume`),
`SoundControl.loop` volumes do not (they use `ambientvol` in `updateLoops`).

**Commit.** `Gameplay G07 T6: build/mine/fire/unit/weather/turret loop sounds`

### T7. In-game music context, wave sound/music, world reset, landing (PAR-0072 in-game half)

**Symptom / acceptance.** In game the music state machine sees default rules,
wave 0, no core, no boss: `isDark` never triggers from core damage, wave music
never schedules, `rules.disableMusic`/`musicVolume`/`ambientMusic` are
ignored, the 3-minute interval starts at the first menu frame (so a track can
start the instant a sector loads), no `waveSpawn` sound, no landing music. After:
`$G --eval 'return get_node("/root/Spine/MindAudio").music_debug()'` (new
seam) reports `wave`, `core_hp`, `last_played_ms` reset on sector load, and
`$MC.run_wave()` makes `wave_music_due_ms` non-null and the voice count jump
(waveSpawn).

**Java.**
* `SoundControl` ctor (`J/audio/SoundControl.java:56-83`): on `WaveEvent`,
  after `random(8, 15) * 60` ticks: `if(rules.disableMusic) return;`
  `boss = rules.spawns.contains(g -> g.getSpawned(state.wave - 2) > 0 && g.effect == StatusEffects.boss)`;
  `boss ? playOnce(bossMusic.random(lastRandomPlayed)) : chance(0.46) ? playRandom()`.
  On `ResetEvent`: `lastPlayed = Time.millis()`, stop+restart the sound bus
  (all in-game voices stop).
* `Control.java:123-130`: on `WaveEvent` `Sounds.waveSpawn.play()` (UI bus,
  volume `sfxVolume`).
* `isDark` (`:244-257`): core `healthf() < 0.85` → dark; else
  `chance((log10((wave - 17)/19) + 1) / 4)`; else `chance(enemies / 70 + 0.1)`.
  `playRandom` (`:214-222`): `state.boss() != null` → boss list.
* `volumeMultiplier = musicvol/100 * clamp(rules.musicVolume)`;
  `alwaysPlayMusic = rules.alwaysPlayMusic || setting || planet.alwaysPlayMusic`;
  `getAmbientMusic/getDarkMusic/getBossMusic` use `rules.ambientMusic` /
  `rules.darkMusic` when non-null (boss uses **darkMusic** override — quirk kept
  in `MusicPlayer::boss_playlist`).
* `Control.java:198-222` (`Trigger.newGame`, not skipped, not pvp): if
  `musicvol > 0`: `music = core.landMusic()` (`Musics.land`,
  `CoreBlock.java:58`) → `stop(); play(); setVolume(musicvol/100)` (a direct
  Music play, outside `SoundControl.current`). `CoreBlock.beginLaunch(false)`
  (`CoreBlock.java:375-376`): `landSound(=coreLand).at(camera.position, 1, landSoundVolume=1)`.

**Port today.** `MindAudio::music_context` (`gdext/audio/mod.rs:243-263`)
fills only `is_menu/is_game/paused/dialog/musicvol/alwaysmusic/now/delta`.
`MusicPlayer::on_wave_event` / `update_wave` exist (`core/audio/music.rs:436-475`).
`MusicPlayer.last_played_ms` starts at 0 (`music.rs:199`). The live session:
`CampaignRuntime.session` (`core/game/runtime.rs:178-180`) → `rules`, `wave`,
`enemies` (`core/game/play.rs:101-125`). Rules music fields:
`ambient_music/dark_music: Option<Vec<MusicContainer>>`, `always_play_music`,
`disable_music`, `music_volume`, `editor`, `spawns: Vec<SpawnGroup>`
(`core/io/json/rules.rs:500,617-628`; the json `SpawnGroup` has `begin, end,
spacing, max, unit_scaling, unit_amount, effect: Option<String>`). Landing is
triggered from `gdext/campaign.rs:830-853` (`camera.call("show_landing", ...)`).

**Changes.**
1. `core/audio/frame.rs` (T6 file; create it here if T6 is not done):
   ```rust
   #[derive(Debug, Clone, Default, PartialEq)]
   pub struct AudioGameState { pub in_campaign: bool, pub editor: bool, pub rules: MusicRules,
       pub wave: i32, pub enemies: i32, pub core_hp_fraction: Option<f32>, pub live_boss: bool, pub boss_spawn_group: bool }
   pub fn read_game_state(world: &World, content: &ContentRegistry) -> AudioGameState
   ```
   from `world.get_resource::<CampaignRuntime>()` (absent → default):
   `rules` = `MusicRules { ambient: rules.ambient_music.map(|v| v.iter().map(|m| MusicRef::new(m.name())).collect()), dark: same, disable: rules.disable_music, always: rules.always_play_music, volume: rules.music_volume }`;
   `wave`, `enemies` from the session; `core_hp_fraction`: first (lowest
   `EntitySeq`) entity with `Building` whose block `kind == BlockKind::CoreBlock`
   and `TeamComp.team == rules.default_team`, `Health.health / Health.max_health`;
   `live_boss`: any unit with `TeamComp.team == rules.wave_team` whose
   `StatusComp` has the `boss` status (`content.status_id("boss")`,
   `StatusComp::has_effect_of`); `boss_spawn_group`: any json spawn group with
   `effect.as_deref() == Some("boss")` and `spawned(group, wave - 2) > 0`,
   where `spawned` is `SpawnGroup.getSpawned` transcribed:
   ```rust
   let spacing = g.spacing.max(1);
   if w < g.begin || w > g.end || (w - g.begin) % spacing != 0 { 0 }
   else { (g.unit_amount + ((w - g.begin) / spacing) as f32 / g.unit_scaling) as i32).min(g.max) }
   ```
   (same as `core/game/spawn_group.rs:114-122`; check `unit_scaling` default
   ≠ 0 in the json struct). `Health` is `crate::entities::comp::Health`.
2. `gdext/sim_host.rs`: `pub fn audio_game_state(&self) -> AudioGameState`.
3. `gdext/audio/mod.rs`:
   * `music_context` fills `rules_music`, `editor_rules: state.editor`,
     `core_hp_fraction`, `wave`, `enemies`, `live_boss`, `boss_spawn_group` from
     one `audio_game_state()` read per frame (bind the host for that call only).
   * ResetEvent mirror: field `last_in_game: Option<bool>`, `last_wave: Option<i32>`.
     When `in_game` changes (either direction) or the sim tick goes backwards
     (track `last_tick: i64`, `host.get_tick()`): `self.music.last_played_ms = now_ms`;
     `pool.stop_all()`; `loop_pool.stop_all()`; `mixer = LoopMixer::new()`;
     `turret_loops.clear()`; `last_wave = None`.
   * Wave: when in game and `last_wave == Some(w0)` and `wave > w0`:
     `self.music.on_wave_event(now_ms, &mut self.rng)` and
     `self.play_sound("waveSpawn", self.sfx_scale(), 1.0, 0.0)` (UI bus after
     T1 step 8). Then `last_wave = Some(wave)`.
   * `MusicPlayer::update` already calls `update_wave` each frame.
   * Remove the PAR-0072 WIP idea of seeding `last_played_ms` at the first
     frame: the reset mirror covers it (first observation counts as a change).
4. Landing: `#[func] pub fn play_landing(&mut self)` on `MindAudio`:
   play `coreLand` via `play_sound("coreLand", 1.0 * self.sfx_scale(), 1.0, 0.0)`
   (`at(camera.position)` → falloff 1, pan 0); if `musicvol > 0` play
   `streams.music("land")` on a second `AudioStreamPlayer` `land_player`
   (Music bus, created in `ready()` like `music_player`), non-looping, volume
   `musicvol / 100`. In `gdext/campaign.rs` right after the
   `camera.call("show_landing", ...)` block, call it dynamically:
   `if let Some(mut audio) = self.base().get_node_or_null("/root/Spine/MindAudio") { audio.call("play_landing", &[]); }`.
5. `#[func] music_debug() -> VarDictionary` with keys `wave`, `core_hp`,
   `live_boss`, `last_played_ms`, `wave_music_due_ms` (or -1), `current`,
   `in_game`.

**Tests.** `core/audio/frame.rs`: `read_game_state_defaults_without_campaign`
(empty `World` → `AudioGameState::default()`); `boss_spawn_group_uses_wave_minus_two`:
rules with one group `{begin: 10, end: 10, spacing: 1, unit_amount: 1, max: 100, unit_scaling: 1, effect: "boss"}`,
`wave = 12` → true (`12 - 2 = 10`), `wave = 11` → false. Build a `World`, insert
a `CampaignRuntime` (see how `core/game/runtime.rs` tests construct one:
`grep -n "CampaignRuntime::new\|fn campaign_runtime" core/game/runtime.rs`).
`core/audio/music.rs` already tests `is_dark`/`update_wave`.

**Verify in-engine.** Ground Zero:
```bash
$G --eval 'return get_node("/root/Spine/MindAudio").music_debug()'      # wave = save wave, core_hp ~1, last_played_ms ≈ now
$G --eval 'return get_node("/root/Spine/MindAudio").voice_count()'
$G --eval "return $MC.run_wave()"
$G --eval 'return [get_node("/root/Spine/MindAudio").voice_count(), get_node("/root/Spine/MindAudio").music_debug()["wave_music_due_ms"]]'   # voices +1, due > 0
```
Fresh sector (env §4.1, sector 98) → `drain_log()` has no `missing audio coreLand`
and `land_player` played (add `land_playing` to `music_debug`).

**Pitfalls.** `get_tick()` is a `#[func]` on the host but call the Rust method
inside one bind. `on_wave_event` uses wall-clock ms (Java `Time.run` is ticks;
8–15 s at 60 fps; accepted deviation already documented in `music.rs`).

**Commit.** `PAR-0072: in-game music context (rules, wave, boss, reset) + waveSpawn/landing audio`

### T8. One-shot voice policy: `checkFrame` intensify, max-concurrent/priority, falloff offset

**Symptom / acceptance.** Ten duos firing in the same frame start ten
overlapping voices (Java: one voice, intensified); 128 round-robin voices
evict arbitrary sounds; `explosionCore` falls off from its centre instead of
100 px out. After: tests below; in-engine `voice_count()` while 10 duos fire
stays ≤ 6 for `shootDuo`.

**Java.** Arc `Sound.play` (`ARC/arc-core/src/arc/audio/Sound.java:124-155`):
`if(checkFrame && Time.timeSinceMillis(lastTimePlayed) <= minInterval){ if(volume > lastVolume) Core.audio.set(lastVoice, pan, lastVolume = max(lastVolume, min(lastVolume + volume, volume * 1.25f))); return -1; }`
then `lastVolume = volume; lastTimePlayed = now; sourcePlayBus(...)`. SoLoud
voice limits per sound: `maxConcurrent` (default 6, `Audio.defaultSoundMaxConcurrent`),
priority + concurrent groups + min-interrupt from `SoundPriority.init`
(`J/audio/SoundPriority.java:13-104`). `calcFalloff` subtracts
`falloffOffset²` (`explosionCore` 100, `blockExplodeElectricBig` 70).

**Port today.** All policy is ported and unit-tested in
`core/audio/priority.rs` (`SoundPriorityTable::build/spec`, `admit`,
`SoundPlayState::{within_min_interval, intensify, record}`) but
`gdext/audio/mod.rs::play_sound` ignores it; `VoicePool::play_world`
round-robins (`gdext/audio/voices.rs`).

**Changes.**
1. `VoicePool`: track per slot `(sound: SoundId, started_ms: f64, priority: f32, group: i32, min_interrupt: f32, generation: u32)`;
   `pub fn active_voices(&self, out: &mut Vec<ActiveVoice>)` lists playing world slots
   (`ActiveVoice.voice = (generation << 8) | slot`);
   `pub fn play_world_at(&mut self, slot_or_new: Option<u64>, ...) -> u64`
   (Replace → reuse that slot; Play → first idle slot, else Deny);
   `pub fn set_voice_volume(&mut self, voice: u64, volume: f32, pan: f32)`.
2. `MindAudio`: fields `play_state: HashMap<SoundId, SoundPlayState>`,
   `active_scratch: Vec<ActiveVoice>`. `play_sound(name, volume, pitch, pan)`
   for world sounds when `check_frame`:
   ```rust
   let st = self.play_state.entry(id).or_default();
   if st.within_min_interval(now_ms, MIN_INTERVAL_MS) {
       if volume > st.last_volume { let v = st.intensify(volume); pool.set_voice_volume(st.last_voice, v, pan); }
       return;
   }
   let spec = self.table.spec(id);
   pool.active_voices(&mut self.active_scratch);
   let req = PlayRequest { sound: id, priority: spec.priority, group: spec.group, max_concurrent: spec.max_concurrent, min_interrupt: spec.min_interrupt(length_s) };
   match admit(&self.active_scratch, &req, now_ms) { Admission::Deny => return, a => { let voice = pool.play_world_at(...); st.record(voice, volume, now_ms); } }
   ```
   (`length_s` = `stream.get_length()`; check the `PlayRequest` fields at
   `core/audio/priority.rs:286-297`.) Pass `check_frame` from `AudioEvent::At`
   through `play_sound` (add a parameter; `Play` events use their flag).
3. `AudioEvent::At` falloff: `calc_falloff(x, y, listener, self.table.spec(*sound).falloff_offset)`.

**Tests.** None new in core (policy tested). Parse/clippy gates.

**Verify in-engine.** Place 10 duos with copper around (127..136, 50) with
`place_block` + `debug_add_building_items(x,50,"copper",20)`, spawn a dagger
in range (`apply_sim_command_json` `spawn_unit`, team 2), step 60, then
`$G --eval 'return get_node("/root/Spine/MindAudio").voice_count()'` → ≤ 7
(6 shootDuo + others).

**Pitfalls.** Keep UI sounds outside the policy (Java UI bus sounds still obey
`checkFrame` only when called with it; `Sound.play()` uses `checkFrame = true`
— apply only the intensify rule to UI).

**Commit.** `Gameplay G07 T8: wire SoundPriority admission and checkFrame intensify`

### T9. Weather sim completion + in-engine verification (PAR-0127)

**Symptom / acceptance.** Weather has never been seen in-engine. Gaps: no
wind push from sandstorm/sporestorm, weather `light`/`spores`/`water`
attributes never reach `envAttrs` consumers other than fires (solar output is
not dimmed by rain), and no way to force a weather for verification.
Acceptance: on Ground Zero (no resolvable weather entry, §0.8),
`$H.weather_json()` shows `entries: []`; after
`$H.debug_create_weather("rain", 1.0, 3600)`, `get_group_count("weather") == 1`,
a ground unit gains `wet` within 2 s, and the weather despawns after 3600
ticks; a forced `sandstorm` moves an idle dagger.

**Java.**
* `Logic.updateWeather` / PlayEvent re-arm: already ported (§1.2).
* `ParticleWeather.update` (`J/type/weather/ParticleWeather.java:44-54`):
  ```java
  float speed = force * state.intensity * Time.delta;
  if(speed > 0.001f){ float windx = state.windVector.x * speed, windy = state.windVector.y * speed;
      for(Unit unit : Groups.unit) unit.impulse(windx, windy); }
  ```
  `PhysicsComp.impulse(x, y)`: `vel.add(x / mass, y / mass)`, `mass = hitSize² · π`
  (`PhysicsComp.java:22-25`). `force`: sandstorm 0.1, sporestorm 0.1, others 0.
  `Weather.update` runs **before** `updateEffect` in `WeatherStateComp.update`
  (`Weather.java:313-325`).
* `Weather.updateEffect` status: `u.checkTarget(statusAir, statusGround)` =
  `(isGrounded() && statusGround) || (isFlying() && statusAir)`
  (`isGrounded: elevation < 0.001`, `isFlying: elevation >= 0.09`).
* `Logic.update` (`J/core/Logic.java:603-606`):
  `envAttrs.clear(); envAttrs.add(rules.attributes); Groups.weather.each(w -> envAttrs.add(w.weather.attrs, w.opacity));`
  consumers: `Attribute.env()` (solar `SolarGenerator.java:29-33`:
  `solarMultiplier * maxZero(light.env() + (rules.lighting ? 1 - ambientLight.a : 1))`,
  thermal/heat `heat.env()`, fires `water.env()`).
* `Call.createWeather(weather, intensity, duration, windX, windY)` →
  `weather.create(intensity, duration).windVector.set(windX, windY)`
  (`Weather.java:254-257`).

**Port today.** `update_weather_state` (`core/entities/comp/weather.rs:118-163`)
has no `force` step; the comment at `:138` ("`Weather.update` is a no-op for
every vanilla weather") is wrong. `apply_weather_status` uses
`PhysicsComp.flying` (`:96-103`). `GeneratorEnv` (`core/world/blocks/power/behavior.rs:57-85`,
fields `solar_multiplier, lighting, ambient_light_alpha, light_env, heat_env,
steam_env, reactor_explosions`) is read via `get_resource` at
`core/world/blocks/power/generator.rs:39` but **never inserted** in live play.
`weather_water_attr` (`core/game/runtime.rs:950-968`) is the only envAttrs use.
`ParticleWeatherFields.force` exists (`core/content/registries/weathers.rs`).

**Changes.**
1. `update_weather_state`: before the status block, add
   ```rust
   // `ParticleWeather.update` (ParticleWeather.java:44-54).
   if let Some(def) = content.weather(runtime.weather)
       && let Some(particle) = def.particle.as_ref()
   {
       let speed = particle.force * runtime.intensity;
       if speed > 0.001 {
           let (wx, wy) = (runtime.wind_x * speed, runtime.wind_y * speed);
           // collect (seq, entity) of every `Unit` sorted by (seq, index) like apply_weather_status
           for each unit: let hit = content.unit(world.get::<UnitTypeComp>(u)?.type_id)?.hit_size;
               let mass = (hit * hit * PI).max(0.0001);
               if let Some(mut vel) = world.get_mut::<Vel>(u) { vel.x += wx / mass; vel.y += wy / mass; }
       }
   }
   ```
   Fix the stale comment. Keep `checkTarget` as is but document the
   flying/grounded approximation (hovering units between elevation 0.001 and
   0.09 get both in the port, neither in Java) — only change it if
   `UnitCore.elevation` is reliable: then use `elevation < 0.001` (ground) /
   `elevation >= 0.09` (air).
2. `weather_system` end (before re-inserting resources): build the env and
   insert it:
   ```rust
   let rules = &campaign.session.rules;
   let mut env = |attr: Attribute| rules.attributes.get(attr) + Σ over WeatherRuntime (def.attr(attr) * rt.opacity);
   world.insert_resource(crate::world::blocks::power::behavior::GeneratorEnv {
       solar_multiplier: rules.solar_multiplier, lighting: rules.lighting,
       ambient_light_alpha: rules.ambient_light.0.a,
       light_env: env(Attribute::Light), heat_env: env(Attribute::Heat), steam_env: env(Attribute::Steam),
       reactor_explosions: rules.reactor_explosions,
   });
   ```
   (`WeatherDef::attr(attribute)` exists, `weathers.rs:323`; `Attribute` is
   `crate::content::Attribute`.) Note Java computes envAttrs **after**
   `updateWeather` and before `updateEntities` — the generator update runs in
   the next tick's `update_buildings`, a one-tick lag; accept and document.
   Also replace the body of `weather_water_attr` to add
   `rules.attributes.get(Water)` when a `CampaignRuntime` is present (Java adds
   the rules attributes too).
3. `gdext/sim_host.rs` `#[func]`s:
   * `weather_json() -> GString`: `{"entries":[{weather,cooldown,minFrequency,maxFrequency,minDuration,maxDuration,always,intensity}],"active":[{weather,intensity,opacity,life,windX,windY}]}`
     from `CampaignRuntime.session.rules.weather` and `WeatherRuntime` entities.
   * `debug_create_weather(name: GString, intensity: f32, duration: f32) -> bool`:
     resolve `content.weather_by_name(name)`, spawn exactly like
     `weather_system` does (`WeatherRuntime::new(id, intensity, duration)`,
     wind from an Fx-stream angle, `crate::entities::comp::WeatherState`,
     `EntitySeq` from the unit runtime's `seq` — take/put the non-send
     `UnitRuntime` as `weather_system` does). Returns false if a weather of that
     type is already active or the name is unknown. Doc it as a test seam
     (`Call.createWeather`).

**Tests.** `core/entities/comp/weather.rs` tests (harness pattern at `:165-200`):
* `sandstorm_pushes_units`: spawn a dagger with `Vel {0,0}`, a sandstorm
  `WeatherRuntime` intensity 1, `wind_x = 1, wind_y = 0`; one update →
  `vel.x == 0.1 / (8² · π)` — dagger `hitSize = 8` (`UnitTypes.java` dagger;
  verify the port def) → `0.1 / 201.0619 = 4.9736e-4`; `vel.y == 0`.
* `rain_has_no_force`: same with rain → vel unchanged.
* `core/game/runtime.rs` test `weather_env_feeds_generator_env`: campaign
  runtime with rules `solar_multiplier 1`, one active rain at opacity 1 →
  `GeneratorEnv.light_env == -0.2` (`Weathers.java` rain `light -0.2`).
* Re-run `sim::combat_tests::weather_rain_spawns_and_wets_units` (must pass).

**Verify in-engine.** Ground Zero (env §4.1 Go):
```bash
$G --eval "return $H.weather_json()"          # {"entries":[],"active":[]}
$G --eval "return $H.debug_create_weather(\"rain\", 1.0, 3600)"   # true
$G --eval "return $H.get_group_count(\"weather\")"                # 1
$G --eval "return $H.apply_sim_command_json('{\"op\":\"spawn_unit\",\"unit\":'+str($H.content_list(\"unit\").find(\"dagger\"))+',\"team\":1,\"px\":1040,\"py\":440}')"
sleep 2; $G --eval "return $H.unit_debug_json()"                  # the dagger lists status "wet" (check the JSON key)
$G --eval "$H.set_paused(true); return $H.step(3700)"             # chunk if >15 s: step(600) ×7
$G --eval "return $H.get_group_count(\"weather\")"                # 0
```
Save JSON evidence to `runs/<date>-sN-G07-T9/`. The first `weather_json()` must show `entries: []` (the save's `snow` entry is unresolvable, §0.8); if it shows a `snow` entry, the reader keeps unresolved names — report it (do not add an alias: Java has none).

**Pitfalls.** `weather_system` removes `CampaignRuntime` and `UnitRuntime`
from the world while running; insert `GeneratorEnv` while they are out (no
conflict). The impulse pass needs `content.unit(...)` — `content` is the unit
runtime's registry inside `weather_system`; `update_weather_state` already
receives it.

**Commit.** `Fixes PAR-0127: weather wind force, envAttrs, weather seams + in-engine check`

### T10. Weather rendering: particles (snow/sandstorm/sporestorm), rain streaks, splashes

**Symptom / acceptance.** An active weather draws nothing (only the dev probe
`MindFx.set_weather` draws a fixed default field). After: with
`debug_create_weather("rain", 1, 3600)` a screenshot shows blue rain streaks
over the whole view and small splash sparks on dry ground / splash frames on
water; with `"snowing"` white flakes drift down. Compare against Java visually
only if a reference frame exists (`runs/*ref*`); none is required.

**Java.**
* Draw hook: `WeatherStateComp.draw` (`J/type/Weather.java:326-340`): if
  `renderer.weatherAlpha > 0.0001 && renderer.drawWeather && settings.showweather`:
  `Draw.z(Layer.weather=130)`: `Draw.alpha(weatherAlpha * opacity * opacityMultiplier); weather.drawOver(state)`;
  `Draw.z(Layer.debris=20)`: same alpha; `weather.drawUnder(state)`.
  `Renderer.weatherAlpha` = 0 during the landing cutscene, else
  `lerpDelta(weatherAlpha, 1, 0.08)` per frame (`Renderer.java:180-191`).
* `ParticleWeather.drawOver` (`ParticleWeather.java:57-88`):
  `windx, windy = useWindVector ? windVector * (baseSpeed * intensity) : (xspeed, yspeed)`;
  noise layers (T10b) then
  `drawParticles(region, color, sizeMin, sizeMax, density, intensity, opacity, windx, windy, minAlpha, maxAlpha, sinSclMin, sinSclMax, sinMagMin, sinMagMax, randomParticleRotation)`.
* `Weather.drawParticles` (`Weather.java:120-154`), `rand.setSeed(0)`:
  ```java
  r1.setCentered(cam.x, cam.y, graphics.width / renderer.minScale(), graphics.height / renderer.minScale()); r1.grow(sizeMax * 1.5f);
  camera.bounds(r2); int total = (int)(r1.area() / density * intensity); Draw.color(color, opacity);
  for i < total:
    scl = rand.random(0.5f, 1f); scl2 = rand.random(0.5f, 1f); size = rand.random(sizeMin, sizeMax);
    x = rand.random(0f, boundMax) + Time.time * windx * scl2; y = rand.random(0f, boundMax) + Time.time * windy * scl;
    alpha = rand.random(minAlpha, maxAlpha); rotation = randomParticleRotation ? rand.random(0f, 360f) : 0f;
    x += Mathf.sin(y, rand.random(sinSclMin, sinSclMax), rand.random(sinMagMin, sinMagMax));   // sin(y / scl) * mag
    x = mod(x - r1.x, r1.width) + r1.x; y = mod(y - r1.y, r1.height) + r1.y;
    if(r3.setCentered(x, y, size).overlaps(r2)){ Draw.alpha(alpha * opacity); Draw.rect(region, x, y, size, size, rotation); }
  ```
  `boundMax = 80000`; `renderer.minScale() = Scl.scl(minZoomInGame = 1.5)` (desktop: 1.5).
* `Weather.drawRain` (`:156-189`): same frame with `r1.grow(sizeMax * 0.9)`,
  `alpha = Draw.getColorAlpha()` (= weatherAlpha·opacity·opacityMultiplier), `Draw.color(color)`;
  per drop: `scl, scl2, size, x = r(0,B) + t*xspeed*scl2, y = r(0,B) - t*yspeed*scl, tint = rand.random(1f) * alpha`;
  wrap; if overlapping: `Draw.alpha(tint); Lines.lineAngle(x, y, angle(xspeed*scl2, -yspeed*scl), size/2)`, stroke 0.75.
  Rain: `yspeed 5, xspeed 1.5, density 1200, stroke 0.75, sizeMin 8, sizeMax 40, splashTimeScale 22, color 7a95eaff, liquid water`.
* `Weather.drawSplashes` (`:191-236`), drawn by `RainWeather.drawUnder` at
  `Layer.debris` with `padding = sizeMax`, `total = (int)(area/density*intensity)/2`,
  `rand.setSeed(0)`, `t = Time.time / timeScale`; per i: `offset = rand.random(0f,1f); time = t + offset; pos = (int)time; life = time % 1;
  x = rand.random(0,B) + pos*953; y = rand.random(0,B) - pos*453;` wrap; if
  `r3.setCentered(x, y, life*4).overlaps(r2)`: tile = `world.tileWorld(x, y)`;
  floor `liquidDrop == water` → `Draw.color(floor.mapColor * 1.5, a = opacity)`,
  `Draw.rect(splashes[(int)(life * 11)], x, y)`; else if `liquidDrop == null && !floor.solid`
  → `Draw.color(color); Draw.alpha(slope(life) * opacity)`; for `j in {-1, 1}`:
  `v = trns(90 + j*45, 1 + 5*life)`; `Lines.lineAngle(x + v.x, y + v.y, 90 + j*45, 3*(1 - life))`.
  `Mathf.slope(f) = 1 - |f - 0.5| * 2`.
* `Weathers.java` values: snowing particle `particle` sizes 2.6–13 density 1200
  (xspeed 0.25, yspeed -2 defaults, alpha 1–1, sin defaults 30–80 / 1–7);
  sandstorm `particle` color f7cba4 sizes 70–140 alpha 0–0.2 density 1500
  baseSpeed 5.4 useWindVector, opacityMultiplier 0.35, drawNoise; sporestorm
  `circle-small` color 7457ce sizes 2.5–5 alpha 0.1–0.8 density 2000 baseSpeed 4.3
  useWindVector, opacityMultiplier 0.5, drawNoise; fog no particles, noise only;
  suspend-particles hidden (Erekir).

**Port today.** Builders in `core/fx/weather_fx.rs`:
* `build_particles` (`:153-207`) has the wrong RNG sequence: an extra
  `_scl3` draw, `y` uses `scl2` instead of `scl`, the sine draws happen before
  the conditional rotation draw (Java: rotation before sine), and rotation is
  drawn only inside the overlap branch. All of these shift the field.
* `build_rain` (`:209-246`) misses the `tint = rand.random(1f)` draw (field and
  per-drop alpha wrong) and tests overlap with `cam.x` as a centre while
  `build_particles` treats `cam.x` as the left edge.
* `build_splashes` (`:283-376`) emits at `Layer::Weather` (Java: debris) and
  treats `cam.x` as centre.
* Live drawing: none. `MindFx` (a single `Node2D` parked on the `Layer.effect`
  band, `gdext/render.rs:419-424`) only draws the probe. The band pattern for
  passes is `gdext/render.rs:516-531` (`StatusRenderer`, `ShieldRenderer`
  added as children of `band_node_at(BandKey::base(Layer::X))`); bands for
  `Layer::Weather` and `Layer::Debris` exist (`core/render/layer.rs:89-125`).
  `ShieldRenderer` (`gdext/render/shields.rs`) is the model: `host: Option<Gd<MindSimHost>>`
  resolved in `bootstrap`, `process` → `queue_redraw`, `draw` binds the host.

**Changes.**
1. `core/fx/weather_fx.rs`: define the rect convention once — `WeatherView`
   `x, y` are the **bottom-left** corner (already true for `centered`). Rewrite
   the three builders to the Java sequences above exactly (draw order of `rand`
   calls is the contract). Overlap test helper:
   ```rust
   fn overlaps(x: f32, y: f32, size: f32, cam: WeatherView) -> bool {
       let h = size / 2.0;
       x + h > cam.x && x - h < cam.x + cam.w && y + h > cam.y && y - h < cam.y + cam.h
   }
   ```
   (Arc `Rectangle.overlaps`: `x < r.x + r.width && x + width > r.x && y < r.y + r.height && y + height > r.y`.)
   `build_rain` gets `alpha: f32` (the layer alpha) and draws
   `color.with_alpha(rand(1) * alpha)`; `build_particles` colours
   `color.with_alpha(alpha * state.opacity)`; `build_splashes` emits at
   `Layer::Debris.z()`. Builders take `wind: (f32, f32)` from the caller (the
   particle wind rule is applied by the caller).
2. `core/fx/weather_fx.rs`: add
   ```rust
   /// Resolves a `WeatherDef` into its draw parameters (`ParticleWeather`/`RainWeather` fields).
   pub fn weather_fx_for(def: &WeatherDef) -> Option<WeatherFx>
   ```
   `RegionKey` needs `&'static str`: map `particle_region` with
   `match s { "particle" => "particle", "circle-small" => "circle-small", "circle-shadow" => "circle-shadow", _ => "particle" }`.
   Rain → `kind: Rain`, `xspeed/yspeed/stroke/size_min/size_max/density/color` from `RainWeatherFields`.
3. `gdext/sim_host.rs` Rust-only:
   `pub fn weather_views(&self, out: &mut Vec<(WeatherId, WeatherStateView)>)` (from
   `WeatherRuntime`: intensity, opacity, wind (x,y), life; sorted by `EntitySeq`)
   and `pub fn splash_ground(&self, x: f32, y: f32) -> SplashGround`: tile =
   `(x / 8).round()` per `World.tileWorld` (`Math.round(x / tilesize)`);
   `self.sim.grid.floor_at(TilePos::new(tx, ty))` (`None` → `SplashGround::None`);
   floor def: `liquid_drop == content.liquid_id("water")` →
   `Liquid(map_color * 1.5 clamped)`; `liquid_drop.is_none() && !solid` → `Clear`; else `Solid`.
4. New `gdext/render/weather.rs` (`mod weather;` in `gdext/render.rs`):
   `#[derive(GodotClass)] #[class(base=Node2D)] pub struct WeatherRenderer { base, host, under: bool, assets: Option<Gd<MindAssets>>, regions: HashMap<&'static str, Option<Gd<Texture2D>>>, prims: Vec<DrawPrim>, weather_alpha: f32 }`.
   * `process(delta)`: `weather_alpha` = 0 while
     `/root/Spine/World/Camera2D.is_cutscene()` (dynamic `call`), else
     `weather_alpha += (1 - weather_alpha) * (1 - (1 - 0.08f32).powf(delta * 60))`;
     `queue_redraw()`.
   * `draw()`: return if `weather_alpha <= 0.0001` or setting `showweather`
     false (`/root/MindUi` `settings_get_bool("showweather")`, cache every 30
     frames); bind host once: `views`, `content = host.content_registry()`,
     `t = host.get_tick() as f32` (Java `Time.time` stops while paused — the
     tick does too). Camera: `cam_pos` = Camera2D global position,
     `zoom = get_zoom().x.abs()`, `vp` = viewport size; camera bounds
     `cam = WeatherView { x: cx - vp.x/zoom/2, y: cy - vp.y/zoom/2, w: vp.x/zoom, h: vp.y/zoom }`;
     particle frame `view = WeatherView::centered(cx, cy, vp.x/1.5, vp.y/1.5, grow)`.
     For each view: `def = content.weather(id)`, `fx = weather_fx_for(def)`;
     `under == false`: Particle → wind rule, `build_particles(..., state with opacity)`;
     Rain → `build_rain(..., alpha = weather_alpha * opacity * def.opacity_multiplier)`;
     `under == true`: Rain → `build_splashes(..., |x, y| host.splash_ground(x, y))`
     with `splash_regions` = `["splash-0", …, "splash-11"]` (static array) and
     `splash_size` = region width × `Draw.scl` (UNVERIFIED: `Draw.scl = 1 /
     scale_marker.width` (`J/core/Control.java:330`); executor reads
     `get_node("/root/MindAssets").find_region("scale_marker").get_width()`
     (expected 4 → 0.25) and hard-codes the factor with a comment).
     Execute prims: `PrimKind::Region` → like `MindFx::draw_prim`'s region arm
     (`gdext/fx.rs` ~`:276-320`: `draw_set_transform_ex` + `draw_texture_rect_ex`
     + reset); `PrimKind::Line` → `draw_line_ex(..).width(stroke.max(1.0))`.
     Alpha rules (Java sets `Draw.alpha(weatherAlpha·…)` before `drawOver`,
     but `drawParticles` and `drawSplashes` overwrite the alpha): particles use
     `alpha * opacity` only; rain uses the `alpha` parameter
     (`weather_alpha * opacity * opacity_multiplier`) times the per-drop tint;
     splashes use `opacity` only. Do not multiply anything else by `weather_alpha`.
5. `gdext/render.rs build_floor`: after the shields block add two passes:
   `Layer::Weather` band → `WeatherRenderer` with `under = false`;
   `Layer::Debris` band → `under = true` (`# code-instantiated` comments like
   the shield pass). Add fields `weather: Option<Gd<WeatherRenderer>>`,
   `weather_under: ...` and init them to `None`.
6. Remove nothing from `MindFx` (the probe stays for tests).

**Tests.** `core/fx/weather_fx.rs`:
* `particles_rng_sequence_matches_java`: hand-run the Java loop for `total = 1`
  with `ArcRand::new(0)`: compute expected `x, y, size, alpha` in the test by
  calling `ArcRand` in the Java order (scl, scl2, size, x, y, alpha, [rot], sinScl, sinMag)
  and compare with the single emitted prim (choose `view`/`cam` = one huge rect
  so wrapping is identity and overlap always passes: `WeatherView{x:0,y:0,w:80000,h:80000}`,
  density = area, intensity 1 → total 1).
* `rain_tint_draw_consumed`: two drops; the second drop's `x` equals the value
  computed with the tint draw included.
* `splashes_on_debris_layer`: emitted prims have `z == Layer::Debris.z()`.
* Update existing tests in the file that asserted the old sequences; list them in the commit body.

**Verify in-engine.** Ground Zero:
```bash
$G --eval "return $H.debug_create_weather(\"rain\", 1.0, 3600)"
sleep 6   # opacity fades in: lerp 0.004/tick from 0 → ~0.9 after 600 ticks; wait 10 s total
$G --screenshot runs/<date>-sN-G07-T10/rain.png
$G --eval "$H.set_paused(true); $H.step(4000); $H.set_paused(false); return $H.debug_create_weather(\"snowing\", 1.0, 3600)"
sleep 10; $G --screenshot runs/<date>-sN-G07-T10/snow.png
```
Open both PNGs with the Read tool: streaks/flakes cover the view.
`grep -c panic` on the log must be 0.

**Pitfalls.** World is y-up through the camera (`zoom (z, -z)`); rain angle
`angle(xspeed, -yspeed)` points down-right in world space — check visually
that streaks fall diagonally down. `host.splash_ground` is called per splash
inside `draw` while the host is bound immutably — fine; never `bind_mut`.
Particle counts: ~350 per weather at 1152×648 — no batching needed.

**Commit.** `PAR-0127: draw live weather (particles, rain, splashes) on the weather/debris bands`

### T10b. Weather noise layers (sandstorm, sporestorm, fog)

**Symptom / acceptance.** Fog draws nothing; sandstorm/sporestorm lack their
moving haze. After: forced `"fog"` tints the view grey with slow drifting
noise; screenshot evidence.

**Java.** `ParticleWeather.drawOver` noise loop (`ParticleWeather.java:70-85`):
```java
float sspeed = 1f, sscl = 1f, salpha = 1f, offset = 0f;
for(int i = 0; i < noiseLayers; i++){
    drawNoise(noise, noiseColor, noiseScale * sscl, state.opacity * salpha * opacityMultiplier,
              sspeed * (useWindVector ? 1f : baseSpeed), state.intensity, windx, windy, offset);
    sspeed *= noiseLayerSpeedM; salpha *= noiseLayerAlphaM; sscl *= noiseLayerSclM; offset += 0.29f;
}
```
`NoiseEffect.drawNoise` (`J/entities/effect/NoiseEffect.java:61-74`):
`Draw.alpha(opacity); Draw.tint(color); speed = baseSpeed * intensity; windx = vwindx*speed; windy = vwindy*speed;
scale = 1/noisescl; scroll = Time.time * scale + offset; camera.bounds(r1);
uv = (r1.x*scale, r1.y*scale, (r1.x+r1.w)*scale, (r1.y+r1.h)*scale) scrolled by (-windx*scroll, -windy*scroll);
Draw.rect(uv-region, camera.x, camera.y, camera.width, -camera.height)`. Texture
`sprites/<noisePath>.png` (`noiseAlpha` default, `fog` for fog), wrap repeat,
linear filter. Fog: layers 3, layerSclM 0.6 (last assignment wins),
layerAlphaM 0.7, layerSpeedM 2, baseSpeed 0.05, color grays(0.4), noiseScale
1100, xspeed 1, yspeed 0.01, opacityMultiplier 0.47. Sandstorm/sporestorm:
noiseLayers 1, noiseScale 2000.

**Port today.** `build_noise_layers` (`weather_fx.rs:378-...`) emits
`PrimKind::NoiseLayer` with an empty rect and no scale; no executor
(`gdext/fx.rs` returns false for `NoiseLayer`). Textures exist:
`REPO/assets/sprites/noiseAlpha.png`, `fog.png`.

**Changes.** Draw directly in `WeatherRenderer` (`under == false`, before the
particles): load once `Image::load_from_file(assets_dir + "/sprites/" + noise_path + ".png")`
→ `ImageTexture::create_from_image` (assets dir from `/root/MindAssets.assets_dir()`),
set the node's `texture_repeat = ENABLED` and `texture_filter = LINEAR` on the
`WeatherRenderer` node itself (the parent renderer sets NEAREST). Per layer:
`src = Rect2(u1*tw, v1*th, (u2-u1)*tw, (v2-v1)*th)` with the uv math above,
`draw_texture_rect_region(tex, Rect2(cam.x, cam.y + cam.h, cam.w, -cam.h), src)`
modulated by `noise_color` with alpha `opacity`. Remove the unused
`build_noise_layers` or leave it (do not wire it).

**Tests.** `uv_scroll_math`: a pure helper `noise_uv(cam, t, scale, wind, offset) -> [f32; 4]`
in `core/fx/weather_fx.rs`, asserting for `cam (0,0,100,100)`, `noisescl 1000`,
`t 0`, `offset 0` → `[0, 0, 0.1, 0.1]`; with `t = 1000`, wind `(1, 0)`, speed 1 →
u shifted by `-1 * (1000/1000 + 0)` = `-1`.

**Verify in-engine.** `debug_create_weather("fog", 1, 3600)`, wait 10 s,
screenshot `runs/<date>-sN-G07-T10b/fog.png`.

**Commit.** `PAR-0127: weather noise layers (fog/sandstorm/sporestorm)`

### T11. `PlanetGenerator.addWeather` for freshly generated sectors

**Symptom / acceptance.** A fresh sector never gets weather. Java gives every
freshly generated sector terrain-based weather. After: launching to the fresh
preset Frozen Forest (sector 64, unlocked without a base in the imported
profile, env §4.1 list) → `$H.weather_json()` lists one `snowing` entry
(Frozen Forest's dominant floor is snow/ice — **UNVERIFIED**, the executor
reports the computed dominant floor).

**Java.** `World.setSectorRules` (`J/core/World.java:300-303`) calls
`sector.planet.generator.addWeather(sector, state.rules)` for every
`loadSector` generation (presets too, after `preset.rules.get(state.rules)`).
`PlanetGenerator.addWeather` (`J/maps/generators/PlanetGenerator.java:102-160`):
```java
rules.weather.clear();
for(Tile tile : world.tiles){ if(world.getDarkness(tile.x, tile.y) >= 3) continue;
    Liquid liquid = tile.floor().liquidDrop;
    if(tile.floor().itemDrop != null) content.add(tile.floor().itemDrop);
    if(tile.overlay().itemDrop != null) content.add(tile.overlay().itemDrop);
    if(liquid != null) content.add(liquid);
    if(!tile.block().isStatic()){ floorc.increment(tile.floor()); if(tile.overlay() != Blocks.air) floorc.increment(tile.overlay()); } }
entries sorted by count desc; remove entries with count < 30; floors = keys;
hasSnow = floors.length > 0 && (floors[0].name.contains("ice") || floors[0].name.contains("snow"));
hasRain = floors.length > 0 && !hasSnow && content.contains(Liquids.water) && !floors[0].name.contains("sand");
hasDesert = floors.length > 0 && !hasSnow && !hasRain && floors[0] == Blocks.sand;
hasSpores = floors.length > 0 && (floors[0].name.contains("spore") || floors[0].name.contains("moss") || floors[0].name.contains("tainted"));
if(hasSnow) add(new WeatherEntry(snow)); if(hasRain){ add(rain); add(fog); } if(hasDesert) add(sandstorm); if(hasSpores) add(sporestorm);
```
`new WeatherEntry(w)` = `(w, minFrequency = duration*2, maxFrequency = duration*6, minDuration = duration/2, maxDuration = duration*1.5)`,
`cooldown = random(minFreq, maxFreq)` (re-rolled anyway by PlayEvent),
`intensity 1`, `always false`. Durations: default `10 min = 36000` ticks;
sandstorm/sporestorm `7 min = 25200`; fog `15 min = 54000`.
`BlankPlanetGenerator.addWeather` and `TantrosPlanetGenerator.addWeather` are no-ops.
Sort tie order: `ObjectIntMap` iteration order is hash-based — ties at the top
are rare; break ties by floor id ascending (document the deviation).

**Port today.** `core/maps/generators/planet.rs:345-348` no-op; the fresh-world
path is `core/game/play.rs:367 play_new_sector` (no grid access) called from
`gdext/campaign.rs:725`. Darkness: `core/world/darkness.rs:119 get_static_darkness(grid, content, x, y) -> f32`.
`Tile` static check: `BlockDef` field for `isStatic` (grep `is_static` in
`core/content/registries/blocks/mod.rs`; Java `isStatic()` = `cacheLayer == CacheLayer.walls`-style
static walls — use the port's existing helper if present, else `kind == StaticWall`).

**Changes.**
1. Replace `add_weather` with a free function in `core/maps/generators/planet.rs`:
   `pub fn add_weather(grid: &WorldGrid, content: &ContentRegistry) -> Vec<crate::io::json::rules::WeatherEntry>`
   implementing the Java body (counts in a `BTreeMap<BlockId, i32>`; sort by
   `(-count, id)`). Entry constructor helper
   `fn weather_entry(content, name) -> WeatherEntry` from `def.duration`
   (cooldown = `min_frequency`; PlayEvent re-rolls it).
2. Call site: in `gdext/campaign.rs` right after `play_new_sector(...)` returns
   for a **fresh** generation (the branch at `:725`), once the world grid is
   loaded: bind the host, compute `add_weather(&host.sim.grid, content)` and
   assign `runtime.session.rules.weather = entries` (via
   `take_campaign_runtime`/`put_campaign_runtime`, `gdext/sim_host.rs:1589-1601`).
   Only for planets whose generator is the Serpulo/Erekir `PlanetGenerator`
   (not blank/Tantros): check `planet_name == "serpulo" || "erekir"`.
   **Stop rule:** if the grid is not yet loaded at that point (the world load
   happens later in the frame), report and leave the call site for G04.

**Tests.** `add_weather_snow_floor`: a 40×40 grid filled with `snow` floor →
one entry `snowing`, `min_frequency 72000, max_frequency 216000,
min_duration 18000, max_duration 54000` (36000 × 2/6/0.5/1.5).
`add_weather_rain_and_fog`: `stone` floor + 1 `shallow-water` tile → `rain`, `fog`.
`add_weather_sand_desert`: all `sand-floor` (Java `Blocks.sand` name: check the
port name; the `"sand"` substring blocks rain) → `sandstorm`.

**Verify in-engine.** Fresh sector 64 per env §4.1 (`load_sector` +
`start_sector`), then `$G --eval "return $H.weather_json()"`.

**Commit.** `Gameplay G07 T11: PlanetGenerator.addWeather for fresh sectors`

### T12. Mech step sounds (optional, lowest priority)

**Java.** `MechComp.update` (`J/entities/comp/MechComp.java:33-56`):
`extend = walkExtend(false); base = walkExtend(true); extendScl = base % 1;
if(extendScl < lastExtend(=walkExtension) && base % 2 > 1 && !isFlying()) { side = -sign(extend);
width = hitSize/2 * side; length = mechStride * 1.35; cx = x + trnsx(baseRotation, length, width); cy = ...;
stepSound.at(cx, cy, stepSoundPitch + range(0.1), stepSoundVolume); } walkExtension = extendScl;`
Defaults `stepSound = mechStepSmall`, volume 0.5, pitch 1 (`UnitType.java:313-317`).

**Port.** `MechComp { base_rotation, walk_time, walked }`
(`core/entities/comp/unit/comp.rs:120`), `mech_walk_extend(walk_time, stride, scaled)`
(`core/render/unit_draw.rs:32`), `emit_unit_step` unused (`core/audio/sim.rs:167`).

**Changes.** Add `walk_extension: f32` to `MechComp` (default 0, not
serialized — check `MechComp` is not in a save codec: `grep -rn MechComp core/io`;
if it is, stop and report). In the mech update (`core/entities/comp/unit/mech.rs:19`)
after `walk_time` advances, apply the Java test and emit through the
`AudioSinkRes` resource if present (`world.get_resource::<AudioSinkRes>()`),
pitch `step_sound_pitch + (fx - 0.5) * 0.2` with `fx` from an Fx-stream draw
(requires the unit runtime rng; if not reachable from `mech.rs`, emit with pitch
`step_sound_pitch` and note the deviation).

**Tests.** A dagger walking 200 px emits ≥ 1 `At { sound: MECH_STEP_SMALL? }`
(check the dagger's resolved `step_sound`).

**Commit.** `Gameplay G07 T12: mech step sounds`

### T13. UI sounds: `play_ui_sound` listener, button clicks, dialog back, notify, unlock

**Symptom / acceptance.** No UI sound ever plays: `MindUi.play_ui_sound`
emits the `play_ui_sound` signal (`gdext/ui/ui_host.rs:126-129,365-371`) and
nothing is connected to it. After: clicking any button, closing a dialog, a
toast/unlock notification and a research purchase each start one UI voice
(`MindAudio.voice_count()` +1 right after the action; `drain_log()` has no
`missing audio ui*`).

**Java.**
* `UI.java:146`: `ClickListener.clicked = () -> Sounds.uiButton.play();` —
  Arc's `ClickListener` calls this static hook on **every** click of any
  element with a click listener (buttons, image buttons, check boxes).
* `BaseDialog.java:27-32`: `hidden(() -> { ...; Sounds.uiBack.play(); })` —
  every dialog hide.
* `HudFragment.java:752-757` `showToast`: `if(state.isMenu()) return;` then
  (scheduled) `Sounds.uiNotify.play()`; `HudFragment.java:785-791`
  `showUnlock`: same gate, `Sounds.uiNotify.play()`.
* `ResearchDialog.java:618-624`: after a successful unlock,
  `Sounds.uiUnlock.play()`.
* `HudFragment.java:126,130` `uiFavorite.play(0.35, 0.67, 0)` /
  `play(0.6, 2.0, 0)` on block favourite toggle (only if the port has
  favourites: `grep -rn favorite ui/fragments/placement_fragment.gd`; else skip).
* `Sound.play()` = `play(sfxVolume)`, pitch 1, pan 0, UI bus for `sounds/ui/*`.

**Port today.** Signal declared, no listener. `MindAudio.play_oneshot(name,
volume, pitch, pan)` exists (`gdext/audio/mod.rs:597-624`). Buttons are created
both in code (`ui/mind_widgets.gd:38-66` `button`/`image_button`/…, and many
`Button.new()` sites) and in `.tscn` files, so a per-factory hook would miss
most of them. Dialog base `ui/mind_dialog.gd:134 hide_dialog()` → `hidden()`.
Research purchase: `ui/dialogs/research_dialog.gd:161-169
_on_research_requested`. Toast/unlock display: `ui/ui_root.gd` `_on_toast`
(~389) and the unlock handler just above it (`grep -n "func _on_" ui/ui_root.gd`).
The switch tap already calls `play_ui_sound("click")` (`gdext/input/desktop.rs:1121-1126`).

**Changes.**
1. `gdext/audio/mod.rs` `ready()`: connect the signal (deferred, MindUi is an
   autoload so it exists):
   ```rust
   if let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") {
       let callable = self.base().callable("on_ui_sound");
       let _ = ui.connect("play_ui_sound", &callable);
   }
   ```
   plus `#[func] fn on_ui_sound(&mut self, name: GString)` →
   `self.play_oneshot(name, self.sfx_scale() as f64, 1.0, 0.0)`. Also add
   `#[func] fn on_ui_sound_ex(&mut self, name: GString, volume: f64, pitch: f64)`
   for `uiFavorite` (volume × sfx). `play_oneshot` must route by file (T1 step 8).
2. Global click hook in `ui/ui_root.gd` `_ready()`:
   ```gdscript
   get_tree().node_added.connect(_on_node_added)
   for node in get_tree().root.find_children("*", "BaseButton", true, false):
       _hook_button(node)
   ...
   func _on_node_added(node: Node) -> void:
       if node is BaseButton:
           _hook_button(node)

   func _hook_button(button: BaseButton) -> void:
       if not button.pressed.is_connected(_on_any_button_pressed):
           button.pressed.connect(_on_any_button_pressed)

   ## `UI.java:146` `ClickListener.clicked = () -> Sounds.uiButton.play()`.
   func _on_any_button_pressed() -> void:
       var ui := get_node_or_null("/root/MindUi")
       if ui != null:
           ui.call("play_ui_sound", "uiButton")
   ```
   (`BaseButton` covers `Button`, `CheckBox`, `TextureButton`.)
3. `ui/mind_dialog.gd hide_dialog()`: after `hidden()`, call
   `MindUi.play_ui_sound("uiBack")` (Java plays it on every hide).
4. `ui/ui_root.gd` toast and unlock handlers: before showing, if
   `not menu_group.visible` call `play_ui_sound("uiNotify")`.
5. `ui/dialogs/research_dialog.gd _on_research_requested`: after the
   successful `research` call, `play_ui_sound("uiUnlock")`.
6. Sound names: `uiButton`, `uiBack`, `uiNotify`, `uiUnlock`, `uiFavorite`
   (`SoundId::UI_BUTTON` etc. exist, `core/content/registries/sound_meta.rs:601-616`).

**Tests.** GDScript parse gate; `cargo clippy`. No core change.

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/Spine/MindAudio").voice_count()'
$G --eval 'return get_node("/root/MindUi").open_dialog("settings", "")'; sleep 1
$G --eval 'get_node("/root/MindUi").close_dialog("settings"); return get_node("/root/Spine/MindAudio").voice_count()'   # ≥ 1 (uiBack)
# real click on a HUD button (mouse via the bridge, env §4) then voice_count again
$G --eval 'return get_node("/root/Spine/MindAudio").drain_log()'
```

**Pitfalls.** `node_added` fires for every node: keep `_on_node_added` cheap
(type check only). Do not double-play: the switch tap's `"click"` stays (it is
a block `clickSound`, not a button). Use dynamic `call` on `MindUi` from
GDScript (the `play_ui_sound` `#[func]` exists, `ui_host.rs:367`).

**Commit.** `Gameplay G07 T13: UI sounds (button click, dialog back, notify, unlock)`

### T14. Input-path sounds: payload pickup/drop, block configure

**Symptom / acceptance.** Picking up / dropping payloads and opening a block's
config panel are silent. After: tests below; in-engine a pickup + drop with
the player unit increments `events_applied`.

**Java.**
* Pickup: `PayloadComp.pickup(Unit)` → `payloadPickup.at(self, random(0.9, 1.1))`
  (`PayloadComp.java:107`); `PayloadComp.pickup(Building)` →
  `payloadPickup.at(self)` (pitch 1, `:117`); `InputHandler.pickedBuildPayload`
  taking a block's payload (`InputHandler.java:597,606,614`) →
  `payloadPickup.at(unit, random(0.9, 1.1))`.
* Drop: unit/payload drop → `playPayloadDropSound(payload)`
  (`PayloadComp.java:145,203,230-235`): `size <= 12 → payloadDrop1`,
  `<= 20 → payloadDrop2`, else `payloadDrop3`; `.at(self, random(0.9, 1.1))`.
  `Payload.size()`: `UnitPayload` = `unit.hitSize`, `BuildPayload` =
  `block.size * tilesize`. A **block** dropped on the ground plays
  `on.block().placeSound.at(tile)` instead (`PayloadComp.java:223`, pitch 1).
* Configure: `InputHandler.java:1985-1993` when tapping a configurable block
  opens its config: `build.block.configureSound.at(build.x, build.y, 1, 1, control.sound.uiBus)`
  (default `configureSound = Sounds.click`, `Block.java:263`; positional
  falloff/pan, UI bus).

**Port today.** `Sim::apply_payload_command(kind, x, y, target) -> bool`
(`core/sim/unit_runtime.rs:1916-1927`; kind 0 = pickup unit, 1 = pickup
building/building payload, 2 = drop) is the single live entry
(`SimCommand::Payload`, `core/sim/mod.rs:875-880`). Carried stack:
`UnitPayloads.stack: Vec<CarriedPayload>` (`CarriedPayload::Unit { type_id, .. }`
/ block variant; `core/entities/comp/unit/comp.rs:256+`). Config panel open:
`Effect::OpenBlockConfig` pushed in `gdext/input/desktop.rs` (`:887`, `:1158`),
executed in `gdext/input/mod.rs:527`.

**Changes.**
1. `apply_payload_command`: before dispatching kind 2, read the carrier's top
   `CarriedPayload` (as `payload_drop` does at `:2190-2196`) and the carrier
   `Pos`; after a `true` result emit through
   `self.ecs.0.get_resource::<AudioSinkRes>().cloned()`:
   kind 0 → `PAYLOAD_PICKUP`, pitch `0.9 + fx*0.2`; kind 1 → `PAYLOAD_PICKUP`
   pitch 1.0 (ground block) — if the port distinguishes "took a block's
   payload" vs "picked the block", use the random pitch for the former;
   kind 2 → unit payload: size = unit type `hit_size` → drop1/2/3 by the
   thresholds, pitch `0.9 + fx*0.2`; block payload: `audio::blocks::place_sound(def)`
   (T3) at pitch 1 if it was placed on the ground, else (handed to an
   acceptor) the drop sound by `def.size * 8`. If the port cannot tell
   ground vs acceptor from the return value, use the drop-size rule and note
   the deviation. `fx` = `self.rng_streams.next_float(RngStream::Fx)`.
   Position = carrier position (`.at(self)`), volume 1.
2. Configure sound: in `gdext/input/mod.rs` `Effect::OpenBlockConfig { x, y, .. }`
   arm, after the panel `configure` call, play `click` at the building
   centre: `MindAudio` `#[func] play_at_ui(name, x, y, volume, pitch)` that
   applies `calc_falloff × sfx` and `calc_pan` like `AudioEvent::At` but plays
   on the UI pool. World position = `x*8 + offset` (use
   `host.bind().block_offset(x, y)` if a helper exists, else tile centre
   `x*8 + 4` for size-1; note it). Per-block `configureSound` overrides: grep
   `configureSound` in `J/` — none set in vanilla content besides the default
   (verify; if any exist, add a name table).

**Tests.** `core/sim/unit_runtime.rs` tests or the existing payload tests
(`grep -rn "apply_payload_command" core/sim/*tests*.rs core/sim/unit_runtime.rs`):
with a `SharedAudioLog` installed, a mono/flare pickup by a payload-capable
player unit records one `At { sound: PAYLOAD_PICKUP, 0.9 <= pitch <= 1.1 }`;
dropping a dagger (hit size 8 ≤ 12) records `PAYLOAD_DROP1`.

**Verify in-engine.** Only if a payload unit is available in the session
(Ground Zero's alpha cannot carry payloads); otherwise "headless only — no
payload-capable player unit in the imported Ground Zero". Configure: tap a
`sorter` with a real mouse click (env §4) and check `voice_count()` grew.

**Commit.** `Gameplay G07 T14: payload pickup/drop and block configure sounds`

---

## §3 Execution order, dependencies, shared files

1. **T1** (everything audible depends on it; includes the process-dispatch decision gate).
2. **T2**, **T3** (independent sim emitters; T3 adds an `AudioEvent` variant — rebuild before T4).
3. **T4** → **T5** → **T6** (loop chain; T6 needs T2's `TurretConfig.loop_sound`).
4. **T7** (needs T1; T6's `frame.rs` file if present).
5. **T9** (weather sim + seams; T6 weather-sound verify and T10 need its `debug_create_weather`).
6. **T10** → **T10b**.
7. **T13** (after T1), **T14** (after T3's `audio::blocks`).
8. **T8**, **T11**, **T12** (polish; T11 may hit its stop rule).

Rebuild + `tools/session-play.sh N` after every Rust task. Run
`cargo test -p mind-core --lib audio`, `... weather`, the touched modules, then
the full suites per env §2 before each commit.

Files shared with other areas (coordinate / expect merge conflicts):
* `core/world/blocks/defense/turrets/mod.rs`, `advanced.rs` (G01/G02 turrets) — T2.
* `core/weapons/mod.rs` (G01) — T2.
* `core/event.rs`, `core/sim/mod.rs` (`flush_events`, Rotate), `core/sim/unit_runtime.rs` (`resolve_deaths`) (G02/G04/G08) — T3.
* `core/entities/comp/weather.rs`, `core/game/runtime.rs` (`weather_system`) (G01) — T9.
* `gdext/sim_host.rs` (everyone) — T5/T6/T7/T9/T10 add Rust-only seams; keep them grouped under a `// ---- G07 audio/weather seams ----` comment.
* `gdext/render.rs` `build_floor` (G03) — T10.
* `gdext/campaign.rs` (G04/G05) — T1 fallback, T7 landing, T11.
* `ui/dialogs/settings_menu_dialog.gd` (G04 settings hub PAR-0164) — T1 one-line path fix.
* `ui/ui_root.gd`, `ui/mind_dialog.gd`, `ui/dialogs/research_dialog.gd` (G03/G05) — T13; G05 only adds `play_ui_sound(name)` calls at its own sites.
* `core/sim/unit_runtime.rs` payload commands (G08) — T14.
* `gdext/input/mod.rs` `OpenBlockConfig` arm (G08) — T14.
* Goldens: `mind-headless/tests/golden/audio/events_sim.json` (T2, maybe T3).

## §4 Ledger changes to report

* PAR-0074 → verified after T1 (structural evidence: `stats()` settings mirror + persisted slider).
* PAR-0073 → verified after T3 (tests + `events_applied` growth).
* PAR-0071 → verified after T4+T5 (+T6 for unit/build loops) with `loop_voice_count` evidence.
* PAR-0072 → note: "in-game half fixed by G07 T1/T7 (assets dir, music decode, looping flag, is_menu from MenuBackground, rules/wave/boss context, ResetEvent mirror); menu half (planet launchMusic, editor) still needs its own pass; WIP branch `agent/par0072-menu-music` superseded for the parts listed".
* PAR-0127 → verified after T9+T10 (forced weather in-engine: status applied, entity despawns, screenshots); add note: "Ground Zero's imported save names `snow` (unresolvable content name; Java drops it too) — no natural weather on GZ in either client".
* New items to file:
  * "Unit weapons are silent (FxSink::sound dropped)" — fixed by T2.
  * "Most turrets have no shoot sound (only duo bullets carry one)" — fixed by T2.
  * "Weather attributes never reach generators (GeneratorEnv never inserted)" — fixed by T9.
  * "Sandstorm/sporestorm do not push units" — fixed by T9.
  * "Fresh sectors never get terrain weather (addWeather no-op)" — T11.
  * "UI click/back/notify/unlock sounds never play (play_ui_sound signal unconnected)" — fixed by T13.
  * "Payload pickup/drop and block configure sounds missing" — fixed by T14.

## §5 Done criteria

* T1–T7, T9, T10 committed with their acceptance checks recorded; T8/T10b/T11/T12 committed or listed as not reached with the reason.
* `cargo test -p mind-core --lib --features msav-import` green; `mind-headless` failures only from the G00 known list; clippy clean; GDScript parse gate clean.
* In-engine evidence in `runs/<date>-sN-G07-*/` (stats JSON, weather screenshots), 0 panics in the run log.
* `docs/parity-plans/G07-audio-weather_summary.md` written per env §6.

## §6 Cross-area findings

1. **G03:** `ConstructBlock` place/break **effects** (`placeEffect`/`breakEffect`,
   `ConstructBlock.java:64,114`) and `Fx.rotateBlock` (`Build.java:88`) — check
   they are emitted alongside T3's sounds.
2. **G02:** `GeneratorEnv` (`core/world/blocks/power/behavior.rs:57`) is read
   but never inserted; T9 inserts it from `weather_system` (campaign only).
   Non-campaign sims (custom games) still get the default env — G02 should
   insert it for custom rules too, reusing T9's computation.
3. **G01:** `Weather.updateEffect` `checkTarget` uses elevation thresholds
   (grounded `< 0.001`, flying `>= 0.09`); the port uses `PhysicsComp.flying`.
4. **G04:** `PlanetGenerator.addWeather` call site needs the fresh-world grid
   (T11 stop rule).
5. **G05:** UI call sites G05 owns call `MindUi.play_ui_sound(name)` only;
   T13 provides the listener and the global button/dialog hooks.

### Deferred (why)

* `Weapon.activeSound` (`Weapon.java:431-433`) and the continuous-weapon
  `SoundLoop` (`Weapon.java:391-394,419-420`): needs per-mount loop state
  (`WeaponMount.sound`) and mount warmup/`shoot` flags exposed to the client;
  no Ground Zero / early-Serpulo unit uses either (only Erekir/late units).
  Revisit after T6 by adding mount entries to `collect_turret_loops`' pattern.
* `BurstDrill.ambientVolume` `progress^4` factor (Erekir only; T5 note).
* Repair point `shouldAmbientSound` target check (no target field in ECS; T5 note).
* Leg-unit step sounds (`LegsComp.java:187`; Erekir/late units).
* Fog gating of construct sounds (`fogControl.isVisibleTile`; fog is G03).
