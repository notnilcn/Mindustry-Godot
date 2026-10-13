<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H07 — Audio + weather (execution plan, round H)

Date: 2026-10-14. Area: in-game audio (one-shot voice policy, block/unit/ability
sounds, step/walk sounds, weather sound, menu music) and weather (fresh-sector
weather entries, weather rendering, env attributes, wind displacement check).

**Read `docs/H-environment.md` first, fully** (paths `core/ gdext/ headless/ ui/
scenes/ J/ ARC/`, the `$G $H $I $C $MC` shorthands, session/build/test/gmcp/
commit rules, §4.2 seam table). Commit subjects: `Gameplay H07 T<n>: ...` or
`Fixes PAR-XXXX: ...` / `PAR-XXXX: ...` exactly as each task gives them.

Start state: `git log main --oneline | grep -m1 'H00'` must print a line (H00
fixed Serpulo `rules.env` 31 -> 233, so the player unit spawns, and `player:0`
placements go through the build queue). If it prints nothing, stop.

Drift rule: Java line numbers were checked on 2026-10-14. Port line numbers
drift: search the quoted function name with `grep -n`. If the code you find
differs from what a task says (a symbol is missing, a behaviour already
exists, a helper has another name and no obvious equivalent), stop that task,
write down what you saw, and continue with the next task. Never invent a design
that is not written here.

Audio cannot be heard by the executor. Every audio acceptance is
**structural**: a `#[func]` readback (voice count, loop volume, event counter),
a unit test on the emitted `AudioEvent`, and `drain_log()` empty. Say
"audio verified structurally: ..." in each audio commit body.

---

## §1 Inventory

### 1.1 Spot-check of the G07 summary claims (verified 2026-10-14 by reading code)

| G07 claim | Verdict | Evidence |
|---|---|---|
| T1 pump/assets/settings (PAR-0074) | TRUE | `gdext/audio/mod.rs` `ready()` defers `set_process(true)`, `process()` calls `refresh_settings()` every 30 frames, settings read via `/root/MindUi settings_get_i32` |
| T2 weapon + turret sounds | TRUE | `core/weapons/mod.rs:620` (charge) and `:757-767` (`continuous ? initial_shoot_sound : shoot_sound`, `Weapon.java:514-518`); `TURRET_SOUNDS` table (`core/world/blocks/defense/turrets/mod.rs:588`) matches `Blocks.java` for duo/hail/lancer/fuse/meltdown |
| T3 place/break/destroy/rotate | TRUE | `core/sim/mod.rs` `BLOCK_ROTATE` (~1332), `AudioEvent::Construct` (~1821/1834), `unit_runtime.rs` `emit_building_destroyed` (~730) |
| T4 loop voices | TRUE | `LoopMixer::update` + `GodotLoopOutput` every frame; update-then-`merge_ambient` order matches `SoundControl.updateLoops` (`SoundControl.java:410-469`) |
| T5 building ambient | TRUE | `core/audio/ambient.rs` drill 0.019, conveyor 0.0022, crafter 0.03 = `Drill.java:84`, `Conveyor.java:50`, `GenericCrafter.java:60` |
| T6 per-frame loops | TRUE in code, two gaps: weather loop uses simplex `raw2d` instead of Arc `Noise.rawNoise` (-> **T8**); build-beam half never verified in-engine (-> **T12**) |
| T7 music context (PAR-0072) | PARTIAL / **false for the ledger item**: PAR-0072's title is the MENU ("Main menu never plays menu.ogg"); `music_context()` never sets `planet_ui_visible`, `editor_ui_visible`, `planet_music.launch` (`gdext/audio/mod.rs` `music_context`), so the planet dialog never plays `launch.ogg` and the editor never plays `editor.ogg`; the menu track itself was never read back in-engine (-> **T10**) |
| T9 weather sim / envAttrs (PAR-0127, PAR-0201) | PARTIAL: wind force + `GeneratorEnv` landed, but `SolidPump` still ignores `attribute.env()` (`core/world/behavior/production.rs` ~1476 comment "which is 0 on Serpulo" — false while it rains: rain `water +0.2`) and the env carries no water/spores (-> **T9**) |
| T13 UI sounds | TRUE for button/back/toast/research; note `MindHud.push_unlock` has no producer (H05 seam, §6) |
| T14 payload/configure | TRUE headless; in-engine payload half never run (-> **T12**) |

### 1.2 Done — do not redo

* Audio pump, settings mirror, music decode, bus layout, UI bus routing (G07 T1).
* Weapon/turret shoot + charge sounds, `TURRET_SOUNDS` (G07 T2).
* Place/break/destroy/rotate sounds, `ConstructBlock` pitch run (G07 T3).
* `LoopMixer` voices, `AmbientPoller` 20 Hz, building ambients (G07 T4/T5).
* Per-frame loop sources incl. turret `SoundLoop`s (`core/audio/frame.rs`) (G07 T6).
* In-game music context, `waveSpawn`, landing music (G07 T7).
* Weather wind impulse, `GeneratorEnv` insertion, `weather_json`, `debug_create_weather` (G07 T9).
* UI sounds via `MindUi.play_ui_sound` (G07 T13); payload/configure sounds (G07 T14).
* `fx_golden` 1-ULP failure: owned by **H03** (H-environment §2). Ignore it.
* `SoundPriorityTable`, `admit`, `SoundPlayState` pure policy + tests (`core/audio/priority.rs`) — T1 wires them, does not rewrite them.

### 1.3 Gaps found by this audit (Java -> port)

| Java | Port today | Task |
|---|---|---|
| Arc `Sound.play` `checkFrame` intensify + SoLoud max-concurrent/priority + `falloffOffset` (`ARC/arc-core/src/arc/audio/Sound.java` `play`/`calcFalloff`) | `VoicePool::play_world` round-robins 128 voices; `calc_falloff(..., 0.0)` everywhere | T1 |
| `UnitFactory.java:440`, `Reconstructor.java:354`, `UnitAssembler.java:568` `createSound`; `MendProjector.java:120` `mendSound`; `Door.java:40`/`AutoDoor.java:82` `doorSound`; `MassDriver.java:322,342`, `PayloadMassDriver.java:186,305`; `ForceProjector.java:65,246`; `PayloadVoid.java:62` | none emitted | T2 |
| `MechComp.java:33-56` step sound; `UnitComp.java:736-745` floor walk sound | `emit_unit_step` never called; no splash timer | T3 |
| `BulletType.java:564` hit pitch range, `:658` despawn pitch range, `:473` heal sound; abilities `RepairFieldAbility.java:143`, `ShieldRegenFieldAbility.java:64`, `ForceFieldAbility.java:49,91,124`, `EnergyFieldAbility.java:188`, `ShieldArcAbility.java:64,73` | hit/despawn pitch fixed, no heal sound, no ability sounds | T4 |
| `World.java:292,303` `setSectorRules` -> `PlanetGenerator.addWeather` (`PlanetGenerator.java:102-160`) | `core/maps/generators/planet.rs:346` no-op (PAR-0203) | T5 |
| `Weather.drawParticles/drawRain/drawSplashes` (`Weather.java:116-236`), `WeatherStateComp.draw` (`:326-341`), `Renderer.weatherAlpha` (`Renderer.java:180-191`) | builders with RNG-order bugs, no live renderer (PAR-0127 visual) | T6 |
| `ParticleWeather.drawOver` noise loop + `NoiseEffect.drawNoise` (`ParticleWeather.java:73-83`, `NoiseEffect.java:57-71`) | `build_noise_layers` emits empty `NoiseLayer` prims nobody executes | T7 |
| `Noise.rawNoise(double)` (Arc, seed 100) in `Weather.updateEffect` (`Weather.java:103`) | simplex `raw2d` | T8 |
| `Attribute.env()` in `SolidPump.java:124`, `AttributeCrafter.java:96,100` | ignored | T9 |
| `SoundControl.update` menu branch (`SoundControl.java:156-164`) | planet/editor flags never set (PAR-0072) | T10 |
| `Weapon.java:381-421` continuous-weapon `SoundLoop` (vela, navanax) | missing | T11 |
| build beam loop / payload sounds in-engine (blocked by no player unit before H00) | unverified | T12 |
| sandstorm displaces units (PAR-0202) | blocked by H01's kinematic ground mover | T13 |

---

## §2 Tasks

Ordered by what a player notices first after entering a campaign sector:
loudness/overlap of every gun (T1), constant base sounds (T2), marching enemy
mechs (T3), combat sounds (T4), then weather (T5-T8), then smaller gaps.

---

### T1. One-shot voice policy: `checkFrame` intensify, max-concurrent/priority admission, falloff offset

> **In-engine verification (2026-10-14, session 2):** PARTLY: CONFIRMED in code (`gdext/audio/mod.rs:731` `play_sound(name, volume, pitch, pan)` has no policy; `calc_falloff(.., 0.0)` at `mod.rs:624`, `:721`, `:1125`; `active_voice_counts` does not exist yet, as the task expects); NOT reproduced in-engine: ten duos at (133..142, 74) given copper via `debug_add_building_items` (item module shows copper 10, no ammo bar) fired only occasionally (bullets 1-4) and `MindAudio.voice_count()` peaked at 3 (`H07-T1-ten-duos.png`). The verify recipe needs an enemy target inside every duo's range: place team-2 walls with `apply_sim_command_json('{"op":"place","x":..,"y":..,"block":<numeric id>,"rotation":0,"team":2}')` (a team-2 dagger near the core shoots the core and destroys it in ~30 s; stop it with `debug_apply_status(id, "disarmed", 5000)`).

**Symptom / acceptance.** Ten duos firing in the same frame start ten
overlapping voices at full volume (Java: the second play within 16 ms raises
the first voice's volume instead; at most 6 concurrent voices per sound); 128
round-robin voices evict arbitrary sounds; `explosionCore` falls off from its
centre instead of 100 px out. After: unit tests on a new pure planner;
in-engine, while 10 duos fire, `active_voice_counts()["shootDuo"] <= 6`.

**Java.** `ARC/arc-core/src/arc/audio/Sound.java` `play(volume, pitch, pan, loop, checkFrame, bus)`:
```java
if((checkFrame && Time.timeSinceMillis(lastTimePlayed) <= minInterval)){   // minInterval = 16 ms
    if(volume > lastVolume){
        Core.audio.set(lastVoice, pan, lastVolume = Math.max(lastVolume, Math.min(lastVolume + volume, volume * 1.25f)));
    }
    return -1;
}
...
lastVolume = volume;
lastTimePlayed = Time.millis();
return lastVoice = sourcePlayBus(handle, bus.handle, volume, clamp(pitch * globalPitch, 0.0001f, 10f), clamp(pan, -1f, 1f), loop);
```
`calcFalloff(x, y)`: `dst2 = max(dst2(x, y, cam) - falloffOffset*falloffOffset, 0); return clamp(1 / (dst2 / Core.audio.falloff))`.
`at(x, y, pitch, volume)`: `vol = calcVolume(x, y) * volume; if(vol < 0.005f) return -1; play(vol, pitch, calcPan(x, y), false, checkFrame=true)`.
Per-sound concurrency/priority comes from `J/audio/SoundPriority.java:13-104`
(already ported as `SoundPriorityTable`; `explosionCore` falloffOffset 100,
`blockExplodeElectricBig` 70).

**Port today.** `gdext/audio/mod.rs` `play_sound(name, volume, pitch, pan)`
ignores policy; `apply_events` passes `calc_falloff(*x, *y, listener, 0.0)` for
`AudioEvent::At` and `Construct`; `play_at_ui` too. `gdext/audio/voices.rs`
`VoicePool::play_world` round-robins; returns the slot index. Pure policy:
`core/audio/priority.rs` (`SoundPrioritySpec::min_interrupt(length_s)`,
`ActiveVoice`, `PlayRequest`, `Admission::{Play, Deny, Replace{voice}}`,
`admit(active, request, now_ms)`, `SoundPlayState::{within_min_interval, intensify, record}`),
`core/audio/math.rs` `MIN_INTERVAL_MS = 16.0`, `MIN_PLAY_VOLUME`.

**Changes.**
1. `core/audio/priority.rs`, after `SoundPlayState` (before `mod tests`), add a pure planner:
   ```rust
   /// What the client does with one world one-shot (Arc `Sound.play` + SoLoud admission).
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub enum OneShotPlan {
       /// Inside the 16 ms window and not louder: drop.
       Skip,
       /// Inside the window and louder: set `voice` to `volume` (`Core.audio.set`).
       Intensify { voice: u64, volume: f32 },
       /// Start a new voice in an idle slot.
       Play,
       /// Start the new voice in the slot of `voice` (evict it).
       Replace { voice: u64 },
       /// SoLoud refused (concurrency/priority).
       Deny,
   }

   /// Arc `Sound.play` (`checkFrame` branch) followed by [`admit`].
   #[allow(clippy::too_many_arguments)]
   pub fn plan_one_shot(
       state: &mut SoundPlayState,
       active: &[ActiveVoice],
       spec: &SoundPrioritySpec,
       sound: SoundId,
       volume: f32,
       length_s: f32,
       now_ms: f64,
       check_frame: bool,
   ) -> OneShotPlan {
       if check_frame && state.within_min_interval(now_ms, MIN_INTERVAL_MS) {
           if volume > state.last_volume {
               let v = state.intensify(volume);
               return OneShotPlan::Intensify { voice: state.last_voice, volume: v };
           }
           return OneShotPlan::Skip;
       }
       let request = PlayRequest {
           sound,
           priority: spec.priority,
           group: spec.group,
           max_concurrent: spec.max_concurrent,
           min_interrupt: spec.min_interrupt(length_s),
       };
       match admit(active, &request, now_ms) {
           Admission::Play => OneShotPlan::Play,
           Admission::Replace { voice } => OneShotPlan::Replace { voice },
           Admission::Deny => OneShotPlan::Deny,
       }
   }
   ```
   The caller calls `state.record(voice, volume, now_ms)` after a successful
   `Play`/`Replace` (Java records `lastVolume`/`lastTimePlayed` before
   `sourcePlayBus`; a denied play in SoLoud still returned a handle of 0 — keep
   the record only on success and note this in the doc comment).
   Check the exact `PlayRequest` field names at `priority.rs:286-297` before
   writing; they are `sound, priority, group, max_concurrent, min_interrupt`.
2. `gdext/audio/voices.rs` `VoicePool`: add per-world-slot metadata and stable ids.
   ```rust
   #[derive(Debug, Clone, Copy, Default)]
   struct SlotMeta {
       sound: Option<SoundId>,
       started_ms: f64,
       priority: f32,
       group: i32,
       min_interrupt: f32,
       volume: f32,
   }
   ```
   fields `meta: Vec<SlotMeta>` (len `MAX_VOICES`), `generation: Vec<u32>` (all `1`).
   Voice id = `(generation << 8) | slot` exactly like `LoopVoicePool`
   (`MAX_VOICES = 128 < 256`). New methods:
   * `pub fn active_voices(&self, out: &mut Vec<ActiveVoice>)`: `out.clear()`;
     for each world slot whose player `is_playing()` and `meta.sound` is `Some`,
     push `ActiveVoice { voice: id, sound, group, priority, started_ms, min_interrupt, volume }`.
   * `pub fn play_world_meta(&mut self, replace: Option<u64>, stream, volume, pitch, pan, listener, width, meta: (SoundId, f64 /*now_ms*/, f32 /*priority*/, i32 /*group*/, f32 /*min_interrupt*/)) -> Option<u64>`:
     slot = the `replace` slot when its generation matches (stop that player
     first), else the first world slot with `!is_playing()`; `None` when no
     slot -> caller treats as Deny. Bump `generation[slot]`, fill `meta[slot]`,
     configure the player exactly like `play_world` does today, return the id.
   * `pub fn set_voice_volume(&mut self, voice: u64, volume: f32, pan: f32, listener: (f32, f32), width: f32)`:
     ignore stale ids; set `set_volume_linear(volume.clamp(0,1))`, the pan
     position (`listener.0 + pan * width / 2`), and `meta.volume`.
   Keep `play_world` (used by `play_oneshot`) but make it use the same id/meta
   bookkeeping with `sound: None` (not counted by the policy).
3. `gdext/audio/mod.rs` `MindAudio`: fields
   `play_state: std::collections::HashMap<SoundId, SoundPlayState>` (lookups
   only, never iterated — fine for determinism; audio is view-only),
   `active_scratch: Vec<ActiveVoice>`, `now_ms: f64` (set at the top of
   `process()` from the existing `now_ms`). Change
   `fn play_sound(&mut self, name: &str, volume: f32, pitch: f32, pan: f32, check_frame: bool)`:
   * UI bus (unchanged routing): if `check_frame` and the per-sound state is
     inside 16 ms, return (no intensify on the UI pool); else play and record.
   * world bus: `let id = SoundId::by_name(name)` (missing -> `log_missing`,
     return); `let spec = self.table.spec(id)`;
     `pool.active_voices(&mut self.active_scratch)`;
     `let length = stream.get_length() as f32`;
     `match plan_one_shot(state, &self.active_scratch, &spec, id, volume, length, self.now_ms, check_frame)`:
     `Skip | Deny` -> return; `Intensify{voice, volume}` -> `pool.set_voice_volume(voice, volume, pan, ..)`;
     `Play` -> `play_world_meta(None, ..)`; `Replace{voice}` -> `play_world_meta(Some(voice), ..)`;
     on `Some(new_id)` -> `state.record(new_id, volume, self.now_ms)`.
   * Callers: `AudioEvent::At` passes its `check_frame`; `AudioEvent::Play`
     passes its `check_frame`; `Construct` passes `true`; `waveSpawn` and
     `coreLand` pass `true` (`Sound.play()` = `play(sfxVolume)` -> checkFrame true).
4. Falloff offset: in `apply_events` `AudioEvent::At` and `Construct`, and in
   `play_at_ui`, replace `calc_falloff(x, y, listener, 0.0)` with
   `calc_falloff(x, y, listener, self.table.spec(sound_id).falloff_offset)`.
5. New `#[func] pub fn active_voice_counts(&self) -> VarDictionary`: sound name
   -> number of playing world voices (from `active_voices`). Test seam.

**Tests.** `core/audio/priority.rs`:
* `plan_skips_quieter_replay_within_16ms`: `state.record(5, 0.5, 1000.0)`; plan
  at `now 1010`, volume 0.4, check_frame -> `Skip`.
* `plan_intensifies_louder_replay`: record `(5, 0.5, 1000)`; plan volume 0.8 at
  1010 -> `Intensify { voice: 5, volume: max(0.5, min(1.3, 1.0)) = 1.0 }`.
* `plan_ignores_window_without_check_frame`: same as above with
  `check_frame = false` -> `Play` (empty `active`).
* `plan_denies_seventh_concurrent`: six `ActiveVoice` of `SHOOT_DUO` started at
  `now - 10_000` ms with equal priority -> request -> `Replace{..}` or `Deny`
  exactly as `admit` decides (assert it is not `Play`).
Then `cargo clippy -p mind-gdext`, `cargo test -p mind-gdext --lib`.

**Verify in-engine.** Ground Zero (env §4.1). Check the seams first:
`grep -rn -B1 "fn active_voice_counts" client/rust/mind-gdext/src` shows `#[func]`.
```bash
$G --eval "for i in range(10): $H.place_block(127+i, 50, \"duo\")
return true"
$G --eval "for i in range(10): $H.debug_add_building_items(127+i, 50, \"copper\", 20)
return true"
$G --eval "return $H.apply_sim_command_json('{\"op\":\"spawn_unit\",\"unit\":'+str($H.content_list(\"unit\").find(\"dagger\"))+',\"team\":2,\"px\":1060,\"py\":480}')"
sleep 1; $G --eval 'return get_node("/root/Spine/MindAudio").active_voice_counts()'   # shootDuo <= 6
$G --eval 'return get_node("/root/Spine/MindAudio").drain_log()'                      # []
```
(If `debug_add_building_items` does not load turret ammo — env §4.2 says the
item module is not ammo — feed the duos with the drill+duo fixture from env
§4.1 repeated, or use `graphite` via the same seam and record what worked.)
Save the dictionary to `runs/<date>-s<N>-H07-T1/evidence.json`.

**Pitfalls.** Never hold `self.pool.as_mut()` while calling `self.table.spec` —
copy the spec first. `stream.get_length()` can be 0 for a bad stream; then
`min_interrupt` uses `min(0.25, 0)` = 0 (fine). Do not apply the policy to
`LoopVoicePool`. The UI pool keeps its 16 voices round-robin.

**Commit.** `Gameplay H07 T1: one-shot voice policy (checkFrame intensify, SoundPriority admission, falloff offset)`

---

### T2. Block action sounds: unit factory/reconstructor/assembler, mender, doors, mass drivers, force projector, payload void

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code + in-engine): none of `UNIT_CREATE`, `HEAL_WAVE`, `DOOR`, `MASSDRIVER*`, `SHIELD_HIT`, `SHIELD_BREAK`, `UNIT_EXPLODE1` is emitted by a block/behaviour site (only `sound_meta`, the ability registry default and tests). In-engine a `ground-factory` at (142,70) + `power-source` (144,70), `configure_building(142,70,"int","0")` (readback `"0.0"`), 10 silicon + 10 lead (`debug_add_building_items` caps at capacity 10) produced a team-1 dagger after ~1000 ticks; `stats().events_applied` cannot attribute sounds (no per-sound readback: +22 over the window from other sources). Recipe coordinates corrected in place (the plan's (120..122, 52..53) is not on the verified free ground (132..147, 57..72)).

**Symptom / acceptance.** A ground factory finishing a dagger, a mender
pulse, opening a door, a mass driver shot, bullets hitting a force-projector
shield are all silent. After: a headless test per site sees exactly one
`AudioEvent::At` with the Java sound/volume/pitch window; in-engine a ground
factory finishing a unit increments `MindAudio.stats()["events_applied"]` and
logs no missing sound.

**Java (all positions are the building centre unless stated; `Mathf.range(r)` = uniform `[-r, r]`).**

| Site | Sound | Volume | Pitch | Condition |
|---|---|---|---|---|
| `UnitFactory.java:440` `createSound.at(this, 1f + Mathf.range(0.06f), createSoundVolume)` | `unitCreate` (`:40`) | 1 (`:41`) | `1 ± 0.06` | unit produced (`progress >= plan.time`) |
| `Reconstructor.java:354` | `unitCreate` (`:40`) | 1 | `1 ± 0.06` | upgrade finished |
| `UnitAssembler.java:568` `createSound.at(spawn.x, spawn.y, ...)` | `unitCreateBig` (`:54`; `Blocks.java:6383,6407` keep it) | 1 | `1 ± 0.06` | unit assembled; position = the spawn point |
| `MendProjector.java:120` `mendSound.at(this, 1f + Mathf.range(0.1f), mendSoundVolume)` | `healWave` (`:32`) | 0.5 (`:33`) | `1 ± 0.1` | `any` building healed in this pulse |
| `Door.java:40` `doorSound.at(base)` | `door` (`:28`) | 1 | 1 | door config applied and `!world.isGenerating()` |
| `AutoDoor.java:82` `doorSound.at(this)` | `door` (`:29`) | 1 | 1 | auto door toggles |
| `MassDriver.java:322` `shootSound.at(x, y, 1f + Mathf.range(0.2f), shootSoundVolume)` | `massdriver` (`:39`) | 0.5 (`:41`) | `1 ± 0.2` | fires |
| `MassDriver.java:342` `receiveSound.at(...)` | `massdriverReceive` (`:40`) | 0.5 | `1 ± 0.2` | receives a bullet |
| `PayloadMassDriver.java:305` `shootSound.at(x, y, Mathf.random(0.9f, 1.1f), shootSoundVolume)` | `massdriver` (`:35`) | 0.7 (`:37`) | `[0.9, 1.1]` | fires |
| `PayloadMassDriver.java:186` | `massdriverReceive` | 0.7 | `1 ± 0.2` | receives |
| `ForceProjector.java:65` `paramBlock.hitSound.at(bullet.x, bullet.y, 1f + Mathf.range(0.1f), paramBlock.hitSoundVolume)` | `shieldHit` (`:47`) | 0.12 (`:48`) | `1 ± 0.1` | each absorbed bullet, at the bullet |
| `ForceProjector.java:246` `breakSound.at(x, y)` | `shieldBreak` (`:46`) | 1 | 1 | shield breaks |
| `PayloadVoid.java:62` `incinerateSound.at(this)` | `unitExplode1` (`:12`) | 1 | 1 | payload incinerated |

Out of scope (not ported in the sim yet; listed in §6): `Wall.java:173`
lightning (surge wall, H02), `LaunchPad.java:149`/`LandingPad.java:153` (no
port launch pad behaviour), `ShockwaveTower`, `BurstDrill`, `Accelerator`,
`PowerGenerator.explodeSound` (Erekir-only).

**Port today.** Block behaviours run as `fn update_tile(&self, world: &mut World, e: Entity)`
with no RNG access. The sink is the world resource `crate::audio::AudioSinkRes`
(`core/audio/mod.rs:55`); the isolated view stream is `RngStream::Fx`
(`core/determinism/rng.rs:23`, "never in checksum"). Sites:
* UnitFactory: `core/world/blocks/units/behavior.rs` `update_tile` (~271-319,
  the `produced` branch after the `consume()` comment).
* Reconstructor: same file `update_tile` (~450-470, after `consume_items`).
* UnitAssembler: same file (~710-727, the `spawn_unit_def(world, seq, def, team, x, y, 90.0)` call).
* Mender: `core/world/blocks/defense/shields.rs` `update_mend_projector_world`
  (the `for target in targets` loop at the end; add an `any` flag).
* Doors: `core/world/behavior/defense.rs` (`Door` ~161, the set-open helper
  ~238, `AutoDoor` ~280).
* Mass driver: `core/world/blocks/distribution/mass_driver.rs` (`fn fire` ~112
  and ~532; `handle_payload` ~262 is the receive path — read the file to find
  where an arriving bullet is accepted).
* Payload mass driver / void: `core/world/blocks/payloads/payload_mass_driver.rs`, `payload_void.rs`.
* Force projector: `core/world/blocks/defense/shields.rs` (`absorb` ~155-169
  and the broken transition; grep `broken = true`).

**Changes.**
1. `core/audio/sim.rs`: add a view-only pitch RNG resource and one helper.
   ```rust
   /// View-only RNG for block/ability one-shot pitches. Java draws these from
   /// the unseeded global `Mathf.rand`; the port keeps them off the gameplay
   /// stream (`RngStream::Fx`, never in the checksum).
   #[derive(bevy_ecs::resource::Resource)]
   pub struct AudioViewRng(pub SimRng);

   /// `sound.at(x, y, base + Mathf.range(range), volume)` from a `&mut World`
   /// site (blocks, abilities). No-op without an installed [`AudioSinkRes`].
   pub fn emit_world_sound(
       world: &mut World,
       sound: SoundId,
       x: f32,
       y: f32,
       volume: f32,
       pitch_base: f32,
       pitch_range: f32,
   ) {
       let Some(sink) = world.get_resource::<AudioSinkRes>().cloned() else {
           return;
       };
       if sound.is_none_or_unset() {
           return;
       }
       let pitch = if pitch_range > 0.0 {
           let mut rng = world.get_resource_or_insert_with(|| AudioViewRng(SimRng::new(0)));
           pitch_base + (rng.0.next_float(RngStream::Fx) * 2.0 - 1.0) * pitch_range
       } else {
           pitch_base
       };
       emit_at(&sink, sound, x, y, volume, pitch, true);
   }
   ```
   (`emit_at` is the existing private helper in this file; check its
   signature `emit_at(audio, sound, x, y, volume, pitch, check_frame)`. Check
   `SimRng::new(u64)` and `next_float(RngStream)` exist with
   `grep -n "pub fn new\|pub fn next_float" core/determinism/rng.rs`.)
   For `Mathf.random(0.9, 1.1)` use `pitch_base = 1.0, pitch_range = 0.1`.
2. Building centre helper: reuse `building_centre` style from
   `core/audio/frame.rs` (`tile * 8 + def.offset`); add
   `pub fn building_centre_of(world: &World, content: &ContentRegistry, e: Entity) -> Option<(f32, f32)>`
   in `core/audio/sim.rs` if the behaviour has no position at hand (most have
   `Building` + a content handle; read each site).
3. Call `emit_world_sound` at every row of the Java table, right at the line
   that matches the Java statement (after the unit spawn / after the heal loop
   when `any` / when the door state changes / when the driver fires or accepts /
   per absorbed bullet / on the broken transition / on incineration). Use the
   `SoundId` constants (`grep -n "UNIT_CREATE\|HEAL_WAVE\|DOOR\b\|MASSDRIVER\|SHIELD_HIT\|SHIELD_BREAK\b\|UNIT_EXPLODE1" core/content/registries/sound_meta.rs`).
   Doors: do not play while a world is being generated/loaded (Java
   `!world.isGenerating()`): only play from the config path that a live
   `configure` reaches, never from the building read/load path.
4. Tests must install a recording sink: copy the `SharedAudioLog` pattern
   used by `core/sim/production_tests.rs` (`grep -n "SharedAudioLog\|AudioEvent::Construct" core/sim/production_tests.rs`).

**Tests.** In `core/sim/production_tests.rs` (helpers listed in H-environment §2):
* `ground_factory_plays_unit_create`: power-source + `ground-factory`
  configured to dagger, add silicon/lead, tick until a dagger spawns -> exactly
  one `At { sound: UNIT_CREATE, volume: 1.0, 0.94 <= pitch <= 1.06 }` at the
  factory centre.
* `mender_pulse_plays_heal_wave`: damaged `copper-wall` next to a powered
  `mender`; tick past `reload` -> one `HEAL_WAVE`, volume 0.5, pitch in `[0.9, 1.1]`;
  with no damaged block -> none.
* `door_toggle_plays_door`: `configure(door, true)` -> one `DOOR` at volume 1, pitch 1.
* `mass_driver_shot_and_receive`: two linked `mass-driver`s with items and
  power -> one `MASSDRIVER` at the shooter, one `MASSDRIVER_RECEIVE` at the receiver.
* `force_projector_hit_plays_shield_hit`: use the absorb test already in
  `shields.rs` tests (grep `fn .*absorb`) with a sink installed -> one
  `SHIELD_HIT` volume 0.12 per absorbed bullet.
Run `cargo test -p mind-core --lib audio` and the touched modules; then the
full suite. `mind-headless` `audio/events_sim.json` golden may change only if a
golden scenario contains one of these blocks — if it changes, regenerate and
name the Java line in the commit body.

**Verify in-engine.** Ground Zero:
```bash
# verified 2026-10-14 on free ground: factory (142,70), source (144,70); plan index 0 = dagger
$G --eval "$H.place_block(144, 70, \"power-source\"); $H.place_block(142, 70, \"ground-factory\"); return $H.building_debug(142,70)"
$G --eval "return $I.configure_building(142, 70, \"int\", \"0\")"
$G --eval "$H.debug_add_building_items(142,70,\"silicon\",30); $H.debug_add_building_items(142,70,\"lead\",30); return true"   # caps at 10 each
$G --eval 'return get_node("/root/Spine/MindAudio").stats()["events_applied"]'
# wait ~12 s (dagger build time 60*15 ticks / unitBuildSpeed), read again: +1 or more, and a new dagger in unit_debug_json()
$G --eval 'return get_node("/root/Spine/MindAudio").drain_log()'   # []
```
(The power-source/factory size and origin are content facts — read
`building_debug` instead of trusting the coordinates above.)

**Pitfalls.** Never call `emit_world_sound` while holding a component borrow
from `world` (take positions into locals first). `get_resource_or_insert_with`
needs `&mut World`. The mender may heal every 200 ticks — test with
`state.reload` from the def, not a constant.

**Commit.** `Gameplay H07 T2: block action sounds (unit create, mender, doors, mass drivers, force projector, payload void)`

---

### T3. Mech step sounds and floor walk sounds

> **In-engine verification (2026-10-14, session 2):** CONFIRMED in-engine: a team-1 dagger spawned at (1120,520) and ordered with `apply_sim_command_json('{"op":"unit_command","units":[<id>],"command":0,"x":1040.0,"y":560.0}')` (command 0 = `move`, `content_list("unitCommand")`) walked ~45 px in 120 ticks while `stats().events_applied` stayed 149 -> 149; `emit_unit_step` has no caller (`core/audio/sim.rs:181` definition only). Recipe corrected in place: the plan's team-2 dagger at (1060,480) shoots the core (its shot events mask steps) and kills it.

**Symptom / acceptance.** Daggers, maces and fortresses walk silently;
units wading through shallow water or mud make no splash sound. After: a
dagger walking 200 px emits >= 1 `At { sound: MECH_STEP_SMALL }` with
volume 0.4 (dagger `stepSoundVolume`, `UnitTypes.java:105`) and pitch in
`[0.9, 1.1]`; a dagger walking through `shallow-water` emits `STEP_WATER`.

**Java.**
* `MechComp.update` (`J/entities/comp/MechComp.java:33-56`):
  ```java
  float extend = walkExtend(false);
  float base = walkExtend(true);
  float extendScl = base % 1f;
  float lastExtend = walkExtension;
  if(!headless && extendScl < lastExtend && base % 2f > 1f && !isFlying() && !inFogTo(player.team())){
      int side = -Mathf.sign(extend);
      float width = hitSize / 2f * side, length = type.mechStride * 1.35f;
      float cx = x + Angles.trnsx(baseRotation, length, width),
            cy = y + Angles.trnsy(baseRotation, length, width);
      ... shake / floorDust ...
      type.stepSound.at(cx, cy, type.stepSoundPitch + Mathf.range(0.1f), type.stepSoundVolume);
  }
  walkExtension = extendScl;
  ```
  `walkExtend(scaled)` (`:76-86`) = port `mech_walk_extend(walk_time, stride, scaled)`
  (`core/render/unit_draw.rs:34`). `Mathf.sign(x)` = `x < 0 ? -1 : 1`.
  Defaults `stepSound = mechStepSmall`, `stepSoundVolume = 0.5`,
  `stepSoundPitch = 1` (`UnitType.java:313-317`); dagger volume 0.4; mace
  `mechStepSmall` 0.8/0.65; fortress `mechStep` 0.9/0.35; scepter
  `mechStepHeavy` 0.9/0.45 (port: `core/content/registries/units/standard.rs:75,157-159,212-214,333-334`).
  `mechStride < 0` -> `4 + (hitSize - 8) / 2.1` is already resolved
  (`core/content/registries/units/mod.rs:2112`).
  `isFlying()` = `elevation >= 0.09`.
* `UnitComp.update` (`J/entities/comp/UnitComp.java:736-745`):
  ```java
  if(!type.hovering && isGrounded() && type.emitWalkEffect){
      if((splashTimer += Mathf.dst(deltaX(), deltaY())) >= (7f + hitSize()/8f)){
          floor.walkEffect.at(...);
          splashTimer = 0f;
          if(type.emitWalkSound){
              floor.walkSound.at(x, y, Mathf.random(floor.walkSoundPitchMin, floor.walkSoundPitchMax), floor.walkSoundVolume);
          }
      }
  }
  ```
  `isGrounded()` = `elevation < 0.001`. Floor walk sounds (`Floor.java:38-40,209-211`;
  `Blocks.java:405-408`): `mud` -> `stepMud`, volume 0.08, pitch `[0.4, 0.5]`;
  any floor with `isLiquid` -> `stepWater`, volume 0.1, pitch `[0.8, 1.2]`;
  every other floor -> `none` (no sound).

**Port today.** `core/entities/comp/unit/mech.rs` `update_mech` advances
`walk_time`; `MechComp { base_rotation, walk_time, walked }`
(`core/entities/comp/unit/comp.rs:119`); `UnitTypeDef.step_sound`,
`step_sound_volume`, `step_sound_pitch`, `mech_stride`, `hovering`,
`emit_walk_sound`, `emit_walk_effect`, `hit_size` exist (resolved def,
`core/content/registries/units/mod.rs`; check exact resolved field names with
`grep -n "pub step_sound\|pub emit_walk\|pub hovering\|pub mech_stride" core/content/registries/units/mod.rs`).
`UnitCore.elevation` (`comp.rs:26`). The per-unit kinematics hook is
`core/sim/unit_runtime.rs` `unit_update_system` (`pub fn unit_update_system`
~1910): `movement::update_kinematics(world, &runtime.grid, &runtime.content, entity, delta)`
(~2439) with `runtime: UnitRuntime` owned (`runtime.rng` is a `SimRng`).
`emit_unit_step` exists but is unused (`core/audio/sim.rs:181`).

**Changes.**
1. New file `core/audio/steps.rs` (`pub mod steps;` in `core/audio/mod.rs`;
   license header per H-environment §3):
   ```rust
   /// View-only step/walk audio state (not saved, not checksummed).
   #[derive(Debug, Clone, Copy, Default, PartialEq, bevy_ecs::component::Component)]
   pub struct UnitStepAudio {
       /// `MechComp.walkExtension`.
       pub walk_extension: f32,
       /// `UnitComp.splashTimer`.
       pub splash_timer: f32,
   }

   /// `MechComp.update` step + `UnitComp.update` walk sound for one unit.
   pub fn update_unit_step_audio(
       world: &mut World,
       grid: &WorldGrid,
       content: &ContentRegistry,
       rng: &mut SimRng,
       entity: Entity,
       delta: (f32, f32),
   )
   ```
   Body, in this order:
   * Return early when `world.get_resource::<AudioSinkRes>()` is `None`
     (headless: no RNG draws, no component insert).
   * `def` = `content.unit(world.get::<UnitTypeComp>(entity)?.type_id)`; `pos`;
     `elevation` from `UnitCore`; insert `UnitStepAudio::default()` if absent.
   * Mech half — only if the entity has `MechComp`:
     `stride = def.mech_stride`; `extend = mech_walk_extend(walk_time, stride, false)`;
     `base = mech_walk_extend(walk_time, stride, true)`; `extend_scl = base % 1.0`;
     `if extend_scl < state.walk_extension && base % 2.0 > 1.0 && elevation < 0.09`:
     `side = if extend < 0.0 { 1.0 } else { -1.0 }` (`-Mathf.sign(extend)`);
     `width = hit_size / 2 * side`; `length = stride * 1.35`;
     `cx = x + trnsx(base_rotation, length, width)`, `cy = y + trnsy(base_rotation, length, width)`
     (use `crate::fx::angles::{trnsx_vec, trnsy_vec}` — `(angle, x, y)` forms,
     `core/fx/angles.rs:24,31`; Arc `Angles.trnsx(angle, x, y)` = `x*cos - y*sin`;
     read the two bodies to confirm before use);
     pitch `def.step_sound_pitch + (rng.next_float(RngStream::Fx) * 2 - 1) * 0.1`;
     `emit_unit_step(&sink, def.step_sound, cx, cy, def.step_sound_volume, pitch)`.
     Then `state.walk_extension = extend_scl` (always, Java assigns after the if).
     Fog (`inFogTo`) is not applied — document as a deviation.
   * Walk half: if `!def.hovering && elevation < 0.001 && def.emit_walk_effect`:
     `state.splash_timer += (delta.0² + delta.1²).sqrt()`; if
     `>= 7 + hit_size / 8`: `state.splash_timer = 0`; if `def.emit_walk_sound`
     and `floor_walk_sound(floor_def)` is `Some((sound, vol, pmin, pmax))`:
     pitch `pmin + rng.next_float(RngStream::Fx) * (pmax - pmin)`, emit at `(x, y)`.
     Floor = `grid.floor_at(TilePos::new((x/8).round() as i16..., ...))` — use the
     same world->tile rounding the port uses elsewhere (`grep -n "fn tile_world\|fn world_to_tile" core/world`).
     Leave a comment `// H03: floor.walkEffect fires here (UnitComp.java:739)`.
   * `pub fn floor_walk_sound(def: &BlockDef) -> Option<(SoundId, f32, f32, f32)>`:
     `"mud"` -> `(STEP_MUD, 0.08, 0.4, 0.5)`; else `def.is_liquid` ->
     `(STEP_WATER, 0.1, 0.8, 1.2)`; else `None`. Doc-cite `Floor.java:38-40,209-211`
     and `Blocks.java:405-408`.
   * Also add `pub fn emit_leg_step(world, def, x, y, rng)` =
     `stepSound.at(x, y, stepSoundPitch + Mathf.range(stepSoundPitchRange), stepSoundVolume)`
     (`LegsComp.java:187`) for H01 to call when it ports the leg group stage
     (§6). Do not call it yourself (the port's legs are an approximation).
2. `core/sim/unit_runtime.rs` right after the `update_kinematics` call in
   `unit_update_system`: `crate::audio::steps::update_unit_step_audio(world, &runtime.grid, &runtime.content, &mut runtime.rng, entity, delta);`
   (check `runtime` is mutable there; `delta` is the local computed just above).

**Tests.** `core/audio/steps.rs` tests (use `core/sim/combat_tests.rs`
`live_sim(size)` + `sim.spawn_live_unit("dagger", 1, px, py, 0.0)` per
H-environment §2; install a `SharedAudioLog` sink resource):
* `dagger_walk_emits_mech_steps`: move the dagger 200 px by calling
  `update_unit_step_audio` with `delta = (2, 0)` for 100 calls after manually
  advancing `MechComp.walk_time += 2` each call -> `>= 1` `MECH_STEP_SMALL`,
  every one with volume 0.4 and pitch in `[0.9, 1.1]`.
* `flying_unit_no_steps`: same with elevation 1.0 -> none.
* `shallow_water_walk_sound`: floor `shallow-water` under the dagger, 100 px
  of movement -> >= 1 `STEP_WATER` with pitch in `[0.8, 1.2]`; on `stone` -> none.
* `headless_no_sink_no_component`: without `AudioSinkRes` the function inserts
  nothing (`world.get::<UnitStepAudio>(e).is_none()`).
Re-run the determinism/checksum goldens (`cargo run -q -p mind-headless -- parity goldens`):
they must not change (Fx stream only, new component not checksummed). If a
checksum changes, the component is being walked by the checksummer: stop and report.

**Verify in-engine.** Ground Zero (corrected 2026-10-14: a team-2 dagger shoots the core, so its shot
events mask the steps): spawn a TEAM-1 dagger at world (1120, 520) (`spawn_unit` with `"team":1`),
order it with `apply_sim_command_json('{"op":"unit_command","units":[<id>],"command":0,"x":1040.0,"y":560.0}')`
(id from `unit_debug_json`, command 0 = `move`), pause, `step(40)` three times and read
`MindAudio.stats()["events_applied"]` each time: it grows while `unit_debug_positions()` shows the
dagger moving (today it stays constant).
`drain_log()` empty.

**Pitfalls.** `base % 1.0` with negative values: `walk_time` is never
negative, keep `%`. Do not touch `MechComp` (H01 edits it). The Fx stream is
shared with turrets; draw count changes are fine (view-only).

**Commit.** `Gameplay H07 T3: mech step and floor walk sounds`

---

### T4. Bullet hit/despawn pitch variance, bullet heal sound, unit ability sounds

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code): `core/combat/bullet/mod.rs:659` passes `def.hit_sound_pitch` unchanged, `:718` `emit_bullet_despawn(.., 1.0, 1.0)`; no `BLOCK_HEAL` emit in `hit_tile`; `combat/abilities.rs` emits no ability sounds. Not checked in-engine.

**Symptom / acceptance.** Every bullet impact plays at exactly the same
pitch (Java varies ±0.1); healing bullets (nova/pulsar/poly) on blocks are
silent; Oct/Quasar/Mega abilities (repair field, shield regen, force field,
energy field) are silent. After: unit tests per site.

**Java.**
* `BulletType.hit` (`BulletType.java:564`): `hitSound.at(x, y, hitSoundPitch + Mathf.range(hitSoundPitchRange), hitSoundVolume)`;
  defaults `hitSoundPitch 1`, `hitSoundPitchRange 0.1`, `hitSoundVolume 1` (`:93-95`).
* `BulletType.despawned` (`:658`): `despawnSound.at(b, 1f + Mathf.range(hitSoundPitchRange))`.
* `BulletType.hitTile` heal branch (`:470-475`): `healSound.at(build, 1f + Mathf.range(0.1f), healSoundVolume)`;
  `healSound = Sounds.blockHeal`, `healSoundVolume = 0.9` (`:177,179`); no
  vanilla content overrides it (`grep -rn healSound J/content` is empty).
  Position = building centre.
* `RepairFieldAbility.java:139-144`: `if(hasHealed){ ...; sound.at(unit, 1f + Mathf.range(0.1f), soundVolume); }` — `healWave`, 0.5 (`:20-21`).
* `ShieldRegenFieldAbility.java:61-65`: `if(applied){ ...; sound.at(unit, 1f + Mathf.range(0.1f), soundVolume); }` — `shieldWave`, 0.7 (`:18-19`).
* `ForceFieldAbility.java:49`: absorbed bullet -> `hitSound.at(b.x, b.y, 1f + Mathf.range(0.1f), hitSoundVolume)` — `shieldHit`, 0.12 (`:35-36`);
  `:88-91` broken transition and `:121-125` `death` with shield -> `breakSound.at(unit.x, unit.y)` — `shieldBreakSmall` (`:34`).
* `EnergyFieldAbility.java:187-189`: `if(anyNearby) shootSound.at(unit, 1f + Mathf.range(0.1f), 1f)` — `shootEnergyField` (`:27`).
* `ShieldArcAbility.java:64,73`: hit `shieldHit` 0.12 `1 ± 0.1` at the bullet; break `shieldBreakSmall` at `paramPos` (`:145-147`).
* Per-unit overrides: `grep -n "sound = Sounds\|soundVolume =" J/content/UnitTypes.java` inside ability
  blocks; apply any you find as a name table (none expected besides defaults — record what you found).

**Port today.** `core/combat/bullet/mod.rs` `hit_bullet` (~653) emits
`def.hit_sound_pitch` fixed; `despawn` (~718) pitch 1.0; `hit_tile` (~572-628)
heals without sound. `CombatCtx` has `rng: &mut SimRng` and `audio: &AudioSinkRes`
(`bullet/mod.rs:184-188`). Abilities: `core/combat/abilities.rs`
`update_ability` (`AbilityKind::ShieldRegenField` ~255, `RepairField` ~269,
`ForceField` ~284, `EnergyField` ~318, `ShieldArc` ~346, plus the bullet
absorb arms ~108-118).

**Changes.**
1. `hit_bullet`: pitch = `def.hit_sound_pitch + (ctx.rng.next_float(RngStream::Fx) * 2.0 - 1.0) * def.hit_sound_pitch_range`.
   Draw the Fx float **only if** `!def.hit_sound.is_none_or_unset()` (Java
   `Mathf.range` runs anyway, but its global RNG is unseeded; skipping the draw
   for silent bullets keeps Fx draws minimal — document).
2. despawn: pitch = `1.0 + (fx * 2 - 1) * def.hit_sound_pitch_range`, same guard on `despawn_sound`.
3. `hit_tile` heal branch: after `heal_health`, emit
   `audio_sim::emit_at`-style `At { sound: SoundId::BLOCK_HEAL, x: centre, y: centre, volume: 0.9, pitch: 1 ± 0.1 }`
   via a new `audio_sim::emit_bullet_heal(ctx.audio, x, y, pitch)`; constants
   `BULLET_HEAL_SOUND = SoundId::BLOCK_HEAL`, `BULLET_HEAL_VOLUME = 0.9` in
   `core/audio/sim.rs` with the Java cite (no registry field: no override exists).
   Centre = the target `Building` tile * 8 + block offset.
4. Abilities: at each Java site call `crate::audio::sim::emit_world_sound`
   (from T2) with the table values. Take positions into locals before the call.

**Tests.** `core/combat/bullet` tests (grep an existing hit test that installs
a sink; else copy the `dagger_shot_emits_default_shoot_sound` setup from
`core/weapons/mod.rs` tests): 50 hits of a bullet with `hit_sound` set ->
pitches all in `[0.9, 1.1]` and not all equal. `heal_bullet_plays_block_heal`:
a heal bullet (`healPercent > 0`, e.g. the nova laser bolt def — find it with
`grep -n "heal_percent" core/content/registries/units/standard.rs`) hitting a
damaged own-team wall -> one `BLOCK_HEAL` volume 0.9. Abilities: one test for
`RepairField` (damaged ally in range -> one `HEAL_WAVE` 0.5) and one for
`ForceField` break (-> `SHIELD_BREAK_SMALL`). Regenerate
`mind-headless/tests/golden/audio/events_sim.json` if it changes (bullet
pitches are now random): name `BulletType.java:564,658` in the body and update
its sha256 in `parity/golden_manifest.json`.

**Verify in-engine.** Ground Zero duo vs dagger fixture (T1): `events_applied`
grows; `drain_log()` empty. Abilities are headless only unless you spawn a
`quasar`/`oct` with `debug_spawn_player` — optional.

**Commit.** `Gameplay H07 T4: bullet hit/despawn pitch range, bullet heal sound, ability sounds`

---

### T5. `PlanetGenerator.addWeather` for freshly generated sectors (PAR-0203) + shared `create_weather`

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: fresh Frozen Forest (`load_sector("serpulo", 64)` + `start_sector`) -> `weather_json()` `{"entries":[],"active":[]}`, `rules.weather` `[]`; `core/maps/generators/planet.rs:346` is the no-op. Dominant floor (sampled every 4th tile with one `tile_debug` loop, `StaticWall` tiles skipped, darkness ignored): snow 334, moss 188, dacite 177, ice 148, spore-moss 6 -> Java would add `snowing`. The run mutated session 2's profile (sector 64 save). Note: the seeded Java profile's Ground Zero also shows no weather (`java-08-gz-in2.png`, `java-09-gz-10s.png`); the older `runs/20261009-065416-ref-ground-zero/java/r03-02-mine.png` shows snow from a different save.

**Symptom / acceptance.** A freshly generated sector never gets weather:
Java gives every fresh sector terrain weather (snow on ice/snow maps, rain+fog
on maps with water, sandstorm on sand, sporestorm on moss/spore maps). After:
launching to the fresh preset Frozen Forest (sector 64, unlocked without a
base, H-environment §4.1) -> `$H.weather_json()` lists a `snowing` entry with
`minFrequency 72000, maxFrequency 216000, minDuration 18000, maxDuration 54000`
(only if the dominant floor is ice/snow — **UNVERIFIED**, report the computed
dominant floor name).

**Java.** `World.loadSector` (`J/core/World.java:265-297`) calls
`setSectorRules` before and after generation; `setSectorRules` (`:299-303`)
calls `sector.planet.generator.addWeather(sector, state.rules)`. The second
call runs **after** `sector.preset.rules.get(state.rules)` (`:272`), so for a
fresh load the terrain weather replaces whatever weather the preset map
carried. A resumed save does not go through `loadSector` (its saved weather
stays). `PlanetGenerator.addWeather` (`J/maps/generators/PlanetGenerator.java:102-160`):
```java
rules.weather.clear();
ObjectIntMap<Block> floorc = new ObjectIntMap<>();
ObjectSet<UnlockableContent> content = new ObjectSet<>();
for(Tile tile : world.tiles){
    if(world.getDarkness(tile.x, tile.y) >= 3) continue;
    Liquid liquid = tile.floor().liquidDrop;
    if(tile.floor().itemDrop != null) content.add(tile.floor().itemDrop);
    if(tile.overlay().itemDrop != null) content.add(tile.overlay().itemDrop);
    if(liquid != null) content.add(liquid);
    if(!tile.block().isStatic()){
        floorc.increment(tile.floor());
        if(tile.overlay() != Blocks.air) floorc.increment(tile.overlay());
    }
}
Seq<Entry<Block>> entries = floorc.entries().toArray();
entries.sort(e -> -e.value);
entries.removeAll(e -> e.value < 30);
Block[] floors = keys of entries;
boolean hasSnow = floors.length > 0 && (floors[0].name.contains("ice") || floors[0].name.contains("snow"));
boolean hasRain = floors.length > 0 && !hasSnow && content.contains(Liquids.water) && !floors[0].name.contains("sand");
boolean hasDesert = floors.length > 0 && !hasSnow && !hasRain && floors[0] == Blocks.sand;   // name "sand-floor"
boolean hasSpores = floors.length > 0 && (floors[0].name.contains("spore") || floors[0].name.contains("moss") || floors[0].name.contains("tainted"));
if(hasSnow) rules.weather.add(new WeatherEntry(Weathers.snow));
if(hasRain){ rules.weather.add(new WeatherEntry(Weathers.rain)); rules.weather.add(new WeatherEntry(Weathers.fog)); }
if(hasDesert) rules.weather.add(new WeatherEntry(Weathers.sandstorm));
if(hasSpores) rules.weather.add(new WeatherEntry(Weathers.sporestorm));
```
`new WeatherEntry(w)` (`J/type/Weather.java:275-287`) = `(w, minFrequency = duration*2, maxFrequency = duration*6, minDuration = duration/2, maxDuration = duration*1.5)`,
`cooldown = Mathf.random(minFrequency, maxFrequency)`, `intensity 1`, `always false`.
Durations (`J/content/Weathers.java`): default `10 min = 36000`; sandstorm and
sporestorm `7 min = 25200`; fog `15 min = 54000`.
`World.getDarkness` (`World.java:484-534`): max of the border term (port
`core/world/darkness.rs` `border_darkness`), the non-preset sector circle term
(`:505-525`) and `tile.data` for darkened walls (port `get_static_darkness`).
`Block.isStatic()` = `cacheLayer == CacheLayer.walls` (`Block.java:1121`);
port approximation `crate::maps::filters::is_static(def)` (`core/maps/filters/mod.rs:111`).
`BlankPlanetGenerator.addWeather` and `TantrosPlanetGenerator.addWeather` are
empty overrides (map weather kept).

**Port today.** `core/maps/generators/planet.rs:346` `add_weather(&self, _content)` logs and does nothing.
`gdext/campaign.rs` `start_sector_with_loadout`: the fresh path is the
`else { play_new_sector(...) }` branch of `let events = if has_save && !lost_replay`;
afterwards the loaded map rules are adopted (`adopt_map_rules(...)` +
`host.bind_mut().apply_world_rules(&self.session.rules)`). The grid is already
loaded by `MindSimHost.load_sector` (the caller runs it first, env §4.1).
`MindSimHost::grid()` (`gdext/sim_host.rs:3391`) and `content_registry()`
(~3726) are Rust-only accessors. Rules fields: `border_darkness`,
`limit_map_area`, `limit_x/limit_y/limit_width/limit_height`
(`core/io/json/rules.rs` ~674-680). `WeatherEntry { weather: String, min_frequency, max_frequency, min_duration, max_duration, cooldown, intensity, always }`
(`core/io/json/rules.rs:429`).

**Changes.**
1. `core/maps/generators/planet.rs`: delete the no-op method; add
   ```rust
   /// `PlanetGenerator.addWeather` (`PlanetGenerator.java:102-160`): terrain weather
   /// for a freshly generated sector. Ties at equal counts break by block id
   /// ascending (Java `ObjectIntMap` order is hash-based; deviation).
   pub fn add_weather(grid: &WorldGrid, content: &ContentRegistry, rules: &Rules) -> Vec<WeatherEntry>
   pub fn weather_entry(def: &WeatherDef) -> WeatherEntry   // duration*2/*6//2/*1.5, cooldown = min_frequency, intensity 1, always false
   pub fn planet_adds_weather(planet_name: &str) -> bool      // "serpulo" | "erekir" -> true; else false
   ```
   Body: iterate `grid.tiles.iter()`; darkness = `max(border_darkness(grid, rules.border_darkness, limit, x, y), get_static_darkness(grid, content, x, y))`
   where `limit = rules.limit_map_area.then_some((limit_x, limit_y, limit_width, limit_height))`;
   skip when `>= 3.0`. The sector-circle darkness term applies only to
   non-preset (procedural) sectors; it needs Arc 2-D `Noise.noise` — not
   ported; document "circle term omitted (affects only edge tiles of numbered
   sectors)". Counts in `BTreeMap<BlockId, i32>`; a `content.has_water`
   flag set when any floor's `liquid_drop` is the `water` liquid
   (`content.liquid_id("water")` — check the helper name). Sort
   `Vec<(BlockId, i32)>` by `(-count, id)`; drop `< 30`; name tests on
   `content.block(floors[0]).name`.
2. `core/entities/comp/weather.rs`: extract the spawn used twice today
   (`game/runtime.rs` `weather_system` and `gdext/sim_host.rs` `debug_create_weather`):
   ```rust
   /// `Weather.create(intensity, duration)` + `windVector.set(wind)` (`Weather.java:56-67,258-260`).
   pub fn create_weather(world: &mut World, seq: u64, weather: WeatherId, intensity: f32, duration: f32, wind: (f32, f32)) -> Entity
   ```
   and use it from both places (no behaviour change; same component tuple
   `(WeatherRuntime, WeatherState, EntitySeq)`). H06's `weatherset`
   instruction will call it (§6).
3. `gdext/campaign.rs` `start_sector_with_loadout`: keep
   `let fresh = !(has_save && !lost_replay);` before the `events` branch; right
   after the `adopt_map_rules` block (and before
   `self.session.objectives = ...`), add:
   ```rust
   // `World.loadSector` -> `setSectorRules` -> `planet.generator.addWeather`
   // (World.java:292,303): fresh generations get terrain weather, replacing the
   // preset map's entries; resumed saves keep theirs.
   if fresh && mind_core::maps::generators::planet::planet_adds_weather(&planet_name)
       && let Some(host) = self.host()
   {
       let host = host.bind();
       if let Some(content) = host.content_registry() {
           self.session.rules.weather =
               mind_core::maps::generators::planet::add_weather(host.grid(), content, &self.session.rules);
       }
   }
   ```
   It must run before the live `CampaignRuntime` snapshot is taken from
   `self.session` (read the rest of the function; if the snapshot is taken
   earlier, stop and report — H04 owns that flow).

**Tests.** `core/maps/generators/planet.rs` tests (content from
`create_base_content` — copy the construction from an existing test in the
same file or `core/maps`; grid `WorldGrid::new(40, 40)` and set
`grid.tiles.get_mut(x, y).floor = id`):
* `add_weather_snow_floor`: all `snow` -> one entry `snowing`,
  `(72000, 216000, 18000, 54000)`.
* `add_weather_rain_and_fog`: all `stone` + 1 `shallow-water` tile ->
  `rain (72000, 216000, 18000, 54000)` then `fog (108000, 324000, 27000, 81000)`.
* `add_weather_sand_desert`: all `sand-floor` -> `sandstorm (50400, 151200, 12600, 37800)`.
* `add_weather_darksand_no_rain`: `darksand` + water -> no rain ("sand"
  substring), no desert (not `sand-floor`) -> empty.
* `add_weather_moss_spores`: all `moss` -> `sporestorm`.
* `add_weather_skips_dark_and_static`: a 40x40 snow map whose interior 30x30
  is `stone-wall` blocks -> the count excludes wall tiles (assert the stone
  floor under the walls does not win).
* `add_weather_needs_30`: 20 tiles of snow, rest air/space floor -> empty.

**Verify in-engine.** Fresh Frozen Forest per H-environment §4.1 ("A fresh,
never-played preset sector", sector 64 instead of 98):
```bash
$G --eval "$H.load_sector(\"serpulo\", 64); return true" \
   --eval "return $MC.start_sector(\"serpulo\", 64)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval "return $H.weather_json()"
```
Record the entries. Then Ground Zero resume (sector 170 "go" path): entries
must still be `[]` (resumed save, unresolvable `snow` entry dropped, G07 §0.8).
This mutates the profile (sector 64 gets a save on exit): afterwards do the
clean re-import (H-environment §4.3).

**Pitfalls.** Do not hold `host.bind()` while calling into another node.
`self.session.rules` is the live session rules in `campaign.rs`; check the
field path compiles. The preset-map weather is intentionally discarded for
fresh loads (Java does the same).

**Commit.** `Fixes PAR-0203: PlanetGenerator.addWeather for fresh sectors`

---

### T6. Weather rendering: particles (snow/sandstorm/sporestorm), rain streaks, splashes (PAR-0127 visual half)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: `debug_create_weather("rain", 1.0, 3600)`, then `snowing`, `fog`, `sandstorm` (all active, opacity ~0.93 in `weather_json`) draw nothing (`H07-T6-rain.png`, `H07-T6-T7-snow-fog-sandstorm.png`); rain does apply `wet` to the alpha (status icon). `loop_voice_count("rain") == 1`, `loop_voice_count("windHowl") == 1`. Setter for the off-check is `settings_set(key, value)`.

**Symptom / acceptance.** An active weather draws nothing (only the MCP
probe `MindFx.set_weather` draws a fixed field). After: with
`debug_create_weather("rain", 1, 3600)` a screenshot shows thin blue diagonal
streaks over the whole view and small V-shaped sparks on dry ground (splash
frames on water); with `"snowing"` white flakes drift down; with `"sandstorm"`
large faint tan blobs move along the wind.

**Java.**
* Draw hook `WeatherStateComp.draw` (`J/type/Weather.java:326-341`): if
  `renderer.weatherAlpha > 0.0001 && renderer.drawWeather && settings.getBool("showweather")`:
  at `Layer.weather (130)`: `Draw.alpha(weatherAlpha * opacity * opacityMultiplier); drawOver(state)`;
  at `Layer.debris (20)`: same alpha; `drawUnder(state)`.
  `Renderer.weatherAlpha` (`Renderer.java:180-191`): `0` while `landTime > 0`
  (landing cutscene), else `lerpDelta(weatherAlpha, 1, 0.08)` per frame.
  `showweather` default true (`SettingsMenuDialog.java:528`).
* `ParticleWeather.drawOver` (`J/type/weather/ParticleWeather.java:53-88`):
  `useWindVector ? (windx, windy) = windVector * (baseSpeed * intensity) : (xspeed, yspeed)`;
  noise layers (T7); then if `drawParticles`:
  `drawParticles(region, color, sizeMin, sizeMax, density, intensity, opacity, windx, windy, minAlpha, maxAlpha, sinSclMin, sinSclMax, sinMagMin, sinMagMax, randomParticleRotation)`.
* `Weather.drawParticles` (`Weather.java:116-155`) — the RNG order is the contract:
  ```java
  rand.setSeed(0);
  Tmp.r1.setCentered(cam.x, cam.y, graphics.getWidth() / renderer.minScale(), graphics.getHeight() / renderer.minScale());
  Tmp.r1.grow(sizeMax * 1.5f);
  Core.camera.bounds(Tmp.r2);
  int total = (int)(Tmp.r1.area() / density * intensity);
  Draw.color(color, opacity);
  for(int i = 0; i < total; i++){
      float scl = rand.random(0.5f, 1f);
      float scl2 = rand.random(0.5f, 1f);
      float size = rand.random(sizeMin, sizeMax);
      float x = (rand.random(0f, boundMax) + Time.time * windx * scl2);
      float y = (rand.random(0f, boundMax) + Time.time * windy * scl);
      float alpha = rand.random(minAlpha, maxAlpha);
      float rotation = randomParticleRotation ? rand.random(0f, 360f) : 0f;
      x += Mathf.sin(y, rand.random(sinSclMin, sinSclMax), rand.random(sinMagMin, sinMagMax));   // sin(y / scl) * mag
      x -= r1.x; y -= r1.y; x = Mathf.mod(x, r1.width); y = Mathf.mod(y, r1.height); x += r1.x; y += r1.y;
      if(Tmp.r3.setCentered(x, y, size).overlaps(Tmp.r2)){
          Draw.alpha(alpha * opacity);
          Draw.rect(region, x, y, size, size, rotation);
      }
  }
  ```
  `boundMax = 80000`; `renderer.minScale()` = `Scl.scl(minZoomInGame)` = 1.5 on
  desktop (`Renderer.java:48,174,535-537`). `Mathf.mod(f, n) = ((f % n) + n) % n`.
  Arc `Rectangle.overlaps(r)`: `x < r.x + r.width && x + width > r.x && y < r.y + r.height && y + height > r.y`.
* `Weather.drawRain` (`:157-189`): `rand.setSeed(0)`; `r1` grown by `sizeMax * 0.9`;
  `alpha = Draw.getColorAlpha()` (= `weatherAlpha * opacity * opacityMultiplier`); `Lines.stroke(stroke)`; `Draw.color(color)`;
  per drop: `scl, scl2, size, x = r(0,B) + t*xspeed*scl2, y = r(0,B) - t*yspeed*scl, tint = rand.random(1f) * alpha`;
  wrap; if `r3.setCentered(x, y, size).overlaps(r2)`: `Draw.alpha(tint); Lines.lineAngle(x, y, Angles.angle(xspeed*scl2, -yspeed*scl), size/2f)`.
* `Weather.drawSplashes` (`:191-236`), called by `RainWeather.drawUnder` with
  `(splashes, padding = sizeMax, density, intensity, opacity, splashTimeScale, stroke, color, liquid = water)`:
  `r1` grown by `padding`; `total = (int)(r1.area()/density*intensity) / 2`; `Lines.stroke(stroke)`; `rand.setSeed(0)`; `t = Time.time / timeScale`;
  per i: `offset = rand.random(0f, 1f); time = t + offset; pos = (int)time; life = time % 1f;
  x = rand.random(0f, B) + pos*953; y = rand.random(0f, B) - pos*453;` wrap;
  if `r3.setCentered(x, y, life * 4f).overlaps(r2)`: `tile = world.tileWorld(x, y)` (`Math.round(x / 8)`);
  `floor.liquidDrop == water` -> `Draw.color(Tmp.c1.set(floor.mapColor).mul(1.5f).a(opacity)); Draw.rect(splashes[(int)(life * (splashes.length - 1))], x, y)`
  (native region size x `Draw.scl`; atlas `splash-0..11` are 32 px, `scale_marker` is 4 px -> `Draw.scl = 0.25` -> 8x8 world units);
  else if `liquidDrop == null && !floor.solid`: `Draw.color(color); Draw.alpha(Mathf.slope(life) * opacity)`;
  `for j in {-1, 1}: v = trns(90 + j*45, 1 + 5*life); Lines.lineAngle(x + v.x, y + v.y, 90 + j*45, 3 * (1 - life))`.
  `Mathf.slope(f) = 1 - |f - 0.5| * 2`. `Color.mul(float)` multiplies r,g,b,a and clamps; `.a(opacity)` then sets alpha.
* Values (`J/content/Weathers.java`, already in `core/content/registries/weathers.rs`):
  snowing `particle` sizes 2.6–13, density 1200, xspeed 0.25, yspeed -2, alpha 1–1, sin 30–80 / 1–7, color white;
  rain stroke 0.75, sizes 8–40, xspeed 1.5, yspeed 5, density 1200, splashTimeScale 22, color `7a95eaff`;
  sandstorm `particle` color `f7cba4` sizes 70–140 alpha 0–0.2 density 1500 baseSpeed 5.4 useWindVector, opacityMultiplier 0.35;
  sporestorm `circle-small` color `7457ce` sizes 2.5–5 alpha 0.1–0.8 density 2000 baseSpeed 4.3 useWindVector, opacityMultiplier 0.5;
  fog: no particles (`drawParticles = false`).
  Particle counts at 1152x648: snow `(768+39)*(432+39)/1200 ≈ 316`; sandstorm `(768+420)*(432+420)/1500 ≈ 674`; rain `(768+72)*(432+72)/1200 ≈ 352`.

**Port today.** `core/fx/weather_fx.rs`:
* `build_particles` (~153-212): extra `_scl3` draw; `y` uses `scl2`; sine draws
  before the conditional rotation draw; rotation drawn only inside the overlap
  branch; overlap test treats `cam.x` as a centre — five RNG/geometry bugs.
* `build_rain` (~215-253): no `tint` draw; overlap uses `cam` as centre; alpha = opacity.
* `build_splashes` (~282-368): emits at `Layer::Weather`; overlap uses `cam` as centre.
* `WeatherView { x, y, w, h }` with `centered(cam_x, cam_y, w, h, grow)` -> bottom-left corner.
* Live drawing: none. `gdext/render.rs` `build_floor` adds passes as children
  of band nodes (`self.band_node_at(BandKey::base(Layer::Shields))` ->
  `shields::ShieldRenderer`, ~566-572); bands `Layer::Weather` (130) and
  `Layer::Debris` (20) exist (`core/render/layer.rs` `Layer::ALL`).
  `ShieldRenderer` (`gdext/render/shields.rs`) is the node template
  (`bootstrap` in `ready` + `EXTENSION_RELOADED`, `process` -> `queue_redraw`).
  Region drawing reference: `gdext/fx.rs` `draw_prim` `PrimKind::Region` arm
  (~286-330): `draw_set_transform_ex` + `draw_texture_rect_ex` with a
  **negative height** (world is y-up through the camera) + reset transform.
  Region lookup: `/root/MindAssets` `find_region(name) -> Option<Gd<AtlasTexture>>`.
  Floor map colours: `gdext/render/minimap.rs` `fn load_block_colors() -> Vec<u32>`
  (row 0 of `sprites/block_colors.png`, packed `rgba8888` by block id).
  Cutscene: `/root/Spine/World/Camera2D` `#[func] is_cutscene()`.
  `showweather`: `/root/MindUi` `#[func] settings_get_bool(key)`.

**Changes.**
1. `core/fx/weather_fx.rs` — rewrite the three builders to the Java sequences above:
   * One overlap helper (Arc semantics, `cam` = bottom-left rect):
     ```rust
     fn overlaps_centered(x: f32, y: f32, size: f32, cam: WeatherView) -> bool {
         let h = size / 2.0;
         x - h < cam.x + cam.w && x + h > cam.x && y - h < cam.y + cam.h && y + h > cam.y
     }
     fn arc_mod(f: f32, n: f32) -> f32 { ((f % n) + n) % n }
     ```
   * `build_particles(&self, state, view, cam, view_tick, wind: (f32, f32), out)`:
     wind passed by the caller (the `useWindVector` rule lives in the caller);
     draws in this exact order: `scl, scl2, size, x(r(0,B)), y(r(0,B)), alpha, [rotation], sin_scl, sin_mag`;
     `x += (y / sin_scl).sin() * sin_mag`; wrap with `arc_mod`; colour
     `self.color.with_alpha(alpha * state.opacity)`; emit `PrimKind::Region`
     with `z = Layer::Weather.z()`.
   * `build_rain(&self, state, view, cam, view_tick, alpha: f32, out)`: order
     `scl, scl2, size, x, y, tint`; `tint = rand.random_float(1.0) * alpha`;
     line from `(x, y)` to `(x + trnsx(ang, size/2), y + trnsy(ang, size/2))`,
     `ang = angle(xspeed*scl2, -yspeed*scl)`, stroke `self.stroke`, colour
     `self.color.with_alpha(tint)`.
   * `build_splashes(...)`: z = `Layer::Debris.z()`; overlap with `overlaps_centered(x, y, life * 4, cam)`;
     index `(life * (n - 1) as f32) as usize`; Liquid colour = the passed floor
     colour (caller applies `*1.5` clamp) with alpha `state.opacity`; Clear
     lines colour `self.color.with_alpha(slope(life) * state.opacity)`,
     stroke = rain stroke (0.75).
   * Add `pub fn weather_fx_for(def: &WeatherDef) -> Option<WeatherFx>`:
     particle -> `kind Particle`, region from `particle_region` mapped to
     `&'static str` (`"particle" | "circle-small" | "circle-shadow"`, anything
     else -> `"particle"` and a `log::warn!` once), colour/sizes/density/alphas/sin
     fields; returns `None` when `!draw_particles`. Rain -> `kind Rain` with
     `xspeed, yspeed, stroke, size_min, size_max, density, color`.
   * Keep `MindFx`'s probe compiling (it calls the old signatures: pass
     `state.wind_vector` as `wind` and `state.opacity` as `alpha`).
2. `gdext/sim_host.rs` (Rust-only, next to `weather_json`):
   `pub fn weather_views(&self, out: &mut Vec<WeatherStateView>)` — `out.clear()`,
   one view per `WeatherRuntime` (weather id, intensity, opacity, `(wind_x, wind_y)`, life),
   sorted by `EntitySeq`.
3. Make `load_block_colors` `pub(crate)` in `gdext/render/minimap.rs`.
4. New `gdext/render/weather.rs` (`pub mod weather;` next to `pub mod shields;`):
   ```rust
   #[derive(Clone, Copy, PartialEq, Eq, Debug)]
   pub enum WeatherPass { Noise, Over, Under }

   #[derive(GodotClass)]
   #[class(base=Node2D)]
   pub struct WeatherRenderer {
       base: Base<Node2D>,
       host: Option<Gd<MindSimHost>>,
       pass: WeatherPass,
       regions: HashMap<&'static str, Option<Gd<Texture2D>>>,
       noise_textures: HashMap<String, Option<Gd<ImageTexture>>>,   // T7
       block_colors: Vec<u32>,
       views: Vec<WeatherStateView>,
       prims: Vec<DrawPrim>,
       weather_alpha: f32,
       show_weather: bool,
       frames: u64,
   }
   ```
   * `pub fn set_pass(&mut self, pass: WeatherPass)` (Rust-only, called by the renderer after `new_alloc`).
   * `bootstrap`: resolve `host` at `/root/Spine/SimHost`; `block_colors = load_block_colors()`.
   * `process(delta)`: `weather_alpha = 0` if `Camera2D.is_cutscene()` (dynamic
     `call` on the node), else `weather_alpha += (1 - weather_alpha) * (1 - (1 - 0.08f32).powf(delta as f32 * 60.0))`
     (Arc `lerpDelta`); every 30 frames refresh `show_weather` from
     `MindUi.settings_get_bool("showweather")`; `queue_redraw()`.
   * `draw()`: return if `weather_alpha <= 0.0001 || !show_weather`. Bind the host
     once: `views` via `weather_views`, `content = host.content_registry()`,
     `t = host.get_tick() as f32` (Java `Time.time` stops while paused; so does the tick).
     Camera: `Camera2D` global position `(cx, cy)` (world units, y-up),
     `zoom = get_zoom().x.abs()`, viewport `vp = get_viewport_rect().size`;
     camera bounds `cam = WeatherView { x: cx - vp.x/zoom/2, y: cy - vp.y/zoom/2, w: vp.x/zoom, h: vp.y/zoom }`.
     For each view (`def = content.weather(view.weather)`):
     * `Over`: particle weathers: `wind = if p.use_wind_vector { (wv.0 * p.base_speed * intensity, wv.1 * ...) } else { (p.xspeed, p.yspeed) }`;
       frame `WeatherView::centered(cx, cy, vp.x / 1.5, vp.y / 1.5, p.size_max * 1.5)`;
       `build_particles(.., wind, ..)`. Rain: frame grown by `size_max * 0.9`,
       `alpha = weather_alpha * opacity * def.opacity_multiplier`, `build_rain`.
     * `Under`: rain only: frame grown by `size_max`, `build_splashes(..)` with
       `splash_regions = ["splash-0", .., "splash-11"]` (`RegionKey` static
       array), `splash_size = 32.0 * 0.25` (comment: `Draw.scl = 1/scale_marker.width = 1/4`, region 32 px — both read from `assets/sprites/sprites.atlas.json` on 2026-10-14),
       ground closure: tile `((x / 8).round() as i32, (y / 8).round() as i32)`
       (`World.tileWorld` = `Math.round(x / tilesize)`), out of bounds -> `None`;
       floor def: `liquid_drop == water` -> `Liquid(rgb * 1.5 clamped from block_colors[floor id], a 1)`;
       `liquid_drop.is_none() && !solid` -> `Clear`; else `Solid`.
     * `Noise`: T7.
     Execute prims: `Region` -> exactly the `gdext/fx.rs` region arm
     (transform + negative-height `draw_texture_rect_ex` + reset); `Line` ->
     `draw_line_ex(a, b, colour).width(stroke)` (no `max(1.0)` clamp: rain
     stroke is 0.75 world units). Alpha: particles use `alpha * opacity` only;
     rain uses the passed `alpha` times per-drop tint; splashes use `opacity`
     only. Do not multiply anything else by `weather_alpha`.
5. `gdext/render.rs` `build_floor`, after the shields block:
   ```rust
   // Weather (`WeatherStateComp.draw`, Weather.java:326-341): drawOver at
   // Layer.weather (noise pass first, then particles/rain), drawUnder at Layer.debris.
   if let Some(band) = self.band_node_at(BandKey::base(Layer::Weather)) {
       for pass in [WeatherPass::Noise, WeatherPass::Over] {
           let mut node = weather::WeatherRenderer::new_alloc();
           // code-instantiated: fixed weather passes; the count is fixed by the pipeline, not by content.
           node.bind_mut().set_pass(pass);
           band.clone().add_child(&node);
           self.weather_nodes.push(node);
       }
   }
   if let Some(band) = self.band_node_at(BandKey::base(Layer::Debris)) { /* one Under node, same pattern */ }
   ```
   Add field `weather_nodes: Vec<Gd<weather::WeatherRenderer>>` (init empty).
   Respect the "adopt persisted band children" rule from `mind-gdext/AGENTS.md`
   (read how `shields` avoids duplicates on `EXTENSION_RELOADED`; copy that).

**Tests.** `core/fx/weather_fx.rs`:
* `particles_rng_sequence_matches_java`: `view = cam = WeatherView { x: 0, y: 0, w: 80000, h: 80000 }`,
  `density = 80000*80000` so `total = 1`, `view_tick = 0`; compute the expected
  `(x, y, size, alpha)` in the test with a fresh `ArcRand::new(0)` drawing in
  the Java order (`scl, scl2, size, x, y, alpha, sinScl, sinMag` with
  `random_particle_rotation = false`; a second case with it `true` inserts the
  rotation draw after `alpha`); assert the single prim equals it.
* `rain_tint_is_drawn`: `total = 2`; expected second drop `x` computed with
  six draws per drop (incl. tint) — assert equality and that the first drop's
  colour alpha = `tint * alpha`.
* `splashes_on_debris_layer`: prims have `z == Layer::Debris.z()`.
* `overlap_uses_bottom_left_rect`: a particle at `(cam.x + 1, cam.y + 1)` with
  size 1 is emitted; one at `(cam.x - 5, ...)` is not.
* `weather_fx_for_matches_weathers_java`: snowing/rain/sandstorm/sporestorm
  fields equal the Java values listed above; fog -> `None`.
* Update the existing tests that asserted the old sequences
  (`mod_wrap_keeps_particles_in_view`, `splashes_branch_on_ground`); list them in the commit body.
Gates: `cargo clippy -p mind-core -p mind-gdext`, `cargo test -p mind-gdext --lib`,
shader/parse gate (no shader added).

**Verify in-engine.** Ground Zero (env §4.1 "Go" path), camera on the core:
```bash
$G --eval "return $H.debug_create_weather(\"rain\", 1.0, 3600)"
sleep 10    # opacity lerps 0.004/tick: ~0.95 after 750 ticks
$G --screenshot runs/<date>-s<N>-H07-T6/rain.png
$G --eval "$H.set_paused(true); $H.step(600); return true"   # repeat until get_group_count("weather") == 0 (chunks of 600)
$G --eval "$H.set_paused(false); return $H.debug_create_weather(\"snowing\", 1.0, 3600)"
sleep 10; $G --screenshot runs/<date>-s<N>-H07-T6/snow.png
# same for "sandstorm"
$G --eval 'return get_node("/root/Spine/MindAudio").loop_voice_count("windHowl")'   # 1 while snowing (G07 T6 weather half)
```
Open each PNG with Read: rain = thin diagonal streaks falling down-right
across the whole view; snow = white dots of mixed sizes; sandstorm = big
faint tan blobs. Compare the rain screenshot's density with the count above
(~350 streaks). `grep -c panic` on the newest log = 0. Toggle
`MindUi.settings_set("showweather", false)` (check the setter name with grep)
-> next screenshot has no weather.

**Pitfalls.** Never `bind_mut` the host in `draw`. `ground` closure is called
per splash while the host is bound immutably — read-only grid access only.
If the port lets the camera zoom out further than 1.5x, the particle frame
(sized from 1.5) will not cover the view — that is Java behaviour too
(`minZoomInGame = 1.5`); do not "fix" it. Do not clamp line width to 1.

**Commit.** `PAR-0127: draw live weather (particles, rain, splashes) on the weather/debris bands`

---

### T7. Weather noise layers (fog, sandstorm, sporestorm haze)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED with T6: active `fog` and `sandstorm` show no haze (`H07-T6-T7-snow-fog-sandstorm.png`).

**Symptom / acceptance.** Fog draws nothing; sandstorm/sporestorm lack their
moving haze. After: forced `"fog"` greys the view with slowly drifting
cloudy noise that moves to the right (fog `xspeed = 1`); screenshot evidence.
With T6 this closes PAR-0127's visual half.

**Java.** `ParticleWeather.drawOver` noise loop (`ParticleWeather.java:66-83`):
```java
float sspeed = 1f, sscl = 1f, salpha = 1f, offset = 0f;
for(int i = 0; i < noiseLayers; i++){
    drawNoise(noise, noiseColor, noiseScale * sscl, state.opacity * salpha * opacityMultiplier,
              sspeed * (useWindVector ? 1f : baseSpeed), state.intensity, windx, windy, offset);
    sspeed *= noiseLayerSpeedM; salpha *= noiseLayerAlphaM; sscl *= noiseLayerSclM; offset += 0.29f;
    col.mul(noiseLayerColorM);   // col is never passed: colour stays noiseColor for every layer
}
```
`NoiseEffect.drawNoise` (`J/entities/effect/NoiseEffect.java:57-71`):
```java
Draw.alpha(opacity); Draw.tint(color);              // tint keeps the alpha: colour = (color.rgb, opacity)
float speed = baseSpeed * intensity;
float windx = vwindx * speed, windy = vwindy * speed;
float scale = 1f / noisescl;
float scroll = Time.time * scale + offset;
Core.camera.bounds(Tmp.r1);
Tmp.tr1.set(r1.x * scale, r1.y * scale, (r1.x + r1.width) * scale, (r1.y + r1.height) * scale);
Tmp.tr1.scroll(-windx * scroll, -windy * scroll);
Draw.rect(Tmp.tr1, camera.position.x, camera.position.y, camera.width, -camera.height);
```
Arc `TextureRegion.scroll(xAmount, yAmount)` (`ARC/.../TextureRegion.java:267-278`):
`if(xAmount != 0){ w = (u2-u)*texW; u = (u + xAmount) % 1; u2 = u + w/texW; }` same for v.
Texture `sprites/<noisePath>.png`, wrap repeat, linear filter
(`ParticleWeather.java:67-71`). Values: fog layers 3, `noiseLayerSclM 0.6`
(the later assignment wins over 0.8), `noiseLayerAlphaM 0.7`,
`noiseLayerSpeedM 2`, baseSpeed 0.05, colour `grays(0.4)` = (0.4, 0.4, 0.4, 1),
noiseScale 1100, noisePath `fog` (256x256), `useWindVector false`, xspeed 1,
yspeed 0.01, opacityMultiplier 0.47. Sandstorm/sporestorm: layers 1,
noiseScale 2000, path `noiseAlpha` (128x128), `useWindVector true` (so the
per-layer base speed factor is 1 and the wind is `windVector * baseSpeed * intensity`).

**Port today.** `build_noise_layers` (`weather_fx.rs` ~374-419) emits
`PrimKind::NoiseLayer` with an empty rect; nothing executes it. Textures exist:
`REPO/assets/sprites/noiseAlpha.png`, `fog.png`; assets dir via
`/root/MindAssets` `#[func] assets_dir()`.

**Changes.**
1. `core/fx/weather_fx.rs`: pure helper (unit-testable) and remove nothing:
   ```rust
   /// `NoiseEffect.drawNoise` UV rect after `TextureRegion.scroll` (u, v, u2, v2).
   pub fn noise_uv(cam: WeatherView, time: f32, noisescl: f32, base_speed: f32, intensity: f32, vwind: (f32, f32), offset: f32) -> [f32; 4] {
       let speed = base_speed * intensity;
       let (wx, wy) = (vwind.0 * speed, vwind.1 * speed);
       let scale = 1.0 / noisescl;
       let scroll = time * scale + offset;
       let (mut u, mut v) = (cam.x * scale, cam.y * scale);
       let (mut u2, mut v2) = ((cam.x + cam.w) * scale, (cam.y + cam.h) * scale);
       let (xa, ya) = (-wx * scroll, -wy * scroll);
       if xa != 0.0 { let w = u2 - u; u = (u + xa) % 1.0; u2 = u + w; }
       if ya != 0.0 { let h = v2 - v; v = (v + ya) % 1.0; v2 = v + h; }
       [u, v, u2, v2]
   }
   /// One noise layer to draw: uv rect, colour alpha, texture path.
   pub struct NoiseLayerDraw { pub uv: [f32; 4], pub color: Rgba, pub texture: &'static str }
   /// `ParticleWeather.drawOver` noise loop for one weather state.
   pub fn particle_noise_layers(def: &WeatherDef, state: &WeatherStateView, cam: WeatherView, time: f32, out: &mut Vec<NoiseLayerDraw>)
   ```
   `particle_noise_layers`: return if `!p.draw_noise`; wind as in T6
   (`use_wind_vector` rule); per layer `noise_uv(cam, time, p.noise_scale * sscl, sspeed * (if p.use_wind_vector { 1.0 } else { p.base_speed }), state.intensity, wind, offset)`,
   colour `p.noise_color.with_alpha(state.opacity * salpha * def.opacity_multiplier)`,
   texture `"fog"` or `"noiseAlpha"` (map `noise_path`; unknown -> `"noiseAlpha"`); multipliers as in Java.
2. `gdext/render/weather.rs` `WeatherPass::Noise` node: in `bootstrap` set
   `self.base_mut().set_texture_repeat(TextureRepeat::ENABLED)` and
   `set_texture_filter(TextureFilter::LINEAR)` on this node only (the parent
   renderer forces NEAREST). Load each texture once:
   `Image::load_from_file(&format!("{assets_dir}/sprites/{name}.png"))` ->
   `ImageTexture::create_from_image(&image)`; cache `None` on failure with one `log::warn!`.
   In `draw()`: for every weather view, `particle_noise_layers(...)`; per layer:
   `src = Rect2(u*tw, v*th, (u2-u)*tw, (v2-v)*th)` (pixels; repeat wraps),
   destination `Rect2(Vector2(cam.x, cam.y), Vector2(cam.w, cam.h))` — a
   positive height in the y-up world equals Java's `-camera.height` (regions
   need a negative height to be upright, `gdext/fx.rs` region arm, so Java's
   flipped draw is the port's positive one); call
   `draw_texture_rect_region_ex(&tex, dest, src).modulate(colour).clip_uv(false).done()`
   (check the builder method names in the `godot` crate: `grep -rn "fn draw_texture_rect_region_ex" ~/.cargo/registry/src/*/godot-core-*/src` or the generated API; `clip_uv` must be false or the region is clamped instead of tiled).
3. Leave `build_noise_layers` in place (still used by `render/env.rs` docs) — do not wire it.

**Tests.** `uv_scroll_math`: `cam (50, 0, 100, 100)`, noisescl 1000, wind
`(1, 0)`, base_speed 1, intensity 1, offset 0: `time 0` -> `[0.05, 0.0, 0.15, 0.1]`
(x amount is `-0.0`, which is `== 0.0`, so no scroll);
`time 1000` -> scroll 1 -> `u = (0.05 - 1) % 1 = -0.95`, `u2 = -0.85`, v unchanged:
`[-0.95, 0.0, -0.85, 0.1]` (tolerance 1e-6). `fog_layers_match_java`: 3 layers
with alpha `opacity*0.47*{1, 0.7, 0.49}`, noisescl `{1100, 660, 396}`,
base speed factor `{0.05, 0.1, 0.2}`, colour rgb 0.4.

**Verify in-engine.** `debug_create_weather("fog", 1, 3600)`, wait 10 s,
`runs/<date>-s<N>-H07-T7/fog-1.png`; wait 10 s more, `fog-2.png`: the grey
cloud pattern shifted right between the two (fog `xspeed 1`). Sandstorm: tan
haze plus T6 blobs (`sandstorm.png`). Log `grep -c panic` = 0.

**Commit.** `Fixes PAR-0127: weather noise layers (fog, sandstorm, sporestorm) complete the weather draw`

---

### T8. Arc `Noise.rawNoise` (1-D Perlin, seed 100) for the weather sound oscillator

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code): `core/audio/frame.rs:218-219` uses `crate::math::noise::raw2d(0, view_tick / osc_scl, 0.0)` with the deviation comment.

**Symptom / acceptance.** Snow's `windHowl` volume oscillates from the port's
simplex `raw2d`, not Arc's 1-D Perlin, so its loudness curve differs from
Java. After: `raw_noise_1d(x)` matches the values below to 1e-12 and
`collect_frame_loops` uses it.

**Java.** `Weather.updateEffect` (`Weather.java:102-105`):
`noise = soundVolOscMag > 0 ? abs(Noise.rawNoise(Time.time / soundVolOscScl)) * soundVolOscMag : 0;`
`control.sound.loop(sound, max((soundVol + noise) * state.opacity, soundVolMin))`.
Arc `arc/util/noise/Noise.java`: `seed = 100`, `B = 256`, `M = 255`, `N = 256`,
static `init()` (lines 222-262) with `java.util.Random(seed)`:
```java
for(i = 0; i < B; i++){
    p[i] = i;
    g1[i] = 2 * r.nextDouble() - 1;
    do{ u = 2*r.nextDouble()-1; v = 2*r.nextDouble()-1; }
    while(u*u + v*v > 1 || abs(u) > 2.5*abs(v) || abs(v) > 2.5*abs(u) || abs(abs(u) - abs(v)) < .4);
    ... g2 (unused here) ...
    do{ u = 2*r.nextDouble()-1; v = 2*r.nextDouble()-1; w = 2*r.nextDouble()-1;
        U = abs(u); V = abs(v); W = abs(w); Lo = min(U, min(V, W)); Hi = max(U, max(V, W)); }
    while(u*u + v*v + w*w > 1 || Hi > 4*Lo || min(abs(U-V), min(abs(U-W), abs(V-W))) < .2);
}
while(--i > 0){ k = p[i]; j = (int)(r.nextLong() & M); p[i] = p[j]; p[j] = k; }
for(i = 0; i < B + 2; i++){ p[B + i] = p[i]; g1[B + i] = g1[i]; ... }
```
```java
public static double rawNoise(double x){
    t = x + N; bx0 = ((int)t) & M; bx1 = (bx0 + 1) & M; rx0 = t - (int)t; rx1 = rx0 - 1;
    sx = rx0 * rx0 * (3 - rx0 - rx0); u = rx0 * g1[p[bx0]]; v = rx1 * g1[p[bx1]];
    return u + sx * (v - u);
}
```
`Noise.setSeed` is never called in Mindustry (grep), so seed 100 is fixed.

**Port today.** `core/audio/frame.rs` (~216-223) uses
`crate::math::noise::raw2d(0, view_tick / osc_scl, 0.0)` with a "deviation" comment.
`crate::random::JavaRandom` (`core/random.rs:15`) is an exact `java.util.Random`
(`new(seed)`, `next_double()`, `next_long()`).

**Changes.**
1. `core/math/noise.rs`: add
   ```rust
   /// Arc `Noise` 1-D Perlin tables (`arc/util/noise/Noise.java:222-262`, seed 100).
   struct ArcPerlin1 { p: [usize; 514], g1: [f64; 514] }
   static ARC_PERLIN1: std::sync::OnceLock<ArcPerlin1> = std::sync::OnceLock::new();
   /// Arc `Noise.rawNoise(double)`.
   pub fn raw_noise_1d(x: f64) -> f64
   ```
   Build the tables exactly as the Java above, consuming the same
   `next_double` draws for the 2-D and 3-D rejection loops (the values are
   discarded, but the draws shift the stream). Truncation `(int)t` = `t as i32`
   (x is never negative here). Note `p` indices from `& M` are `0..=255`.
2. `core/audio/frame.rs`: replace the `raw2d` call with
   `crate::math::noise::raw_noise_1d(f64::from(view_tick / def.sound_vol_osc_scl)).abs() as f32 * def.sound_vol_osc_mag`;
   delete the deviation comment.

**Tests.** `core/math/noise.rs`:
`arc_raw_noise_1d_matches_reference` — values produced by an independent
Python port of the Java code above (`java.util.Random` LCG re-implemented;
**not** a JVM run — say so in the test doc):
```
g1[0] = 0.4440193097192868, p[0..4] = [66, 74, 50, 45]
raw_noise_1d(0.25)               = 0.12894795904560868
raw_noise_1d(0.5)                = 0.24520408728404314
raw_noise_1d(1.7)                = 0.10345653783232334
raw_noise_1d(1000.0 / 1100.0)    = 0.07698456651943808
raw_noise_1d(12.345)             = -0.028115906860696198
```
(tolerance 1e-12; expose the table getters only under `#[cfg(test)]`).
Also `raw_noise_1d(k as f64) == 0.0` for integers `k` in `0..10`.
Update `core/audio/frame.rs` test `rain_weather_emits_camera_loop` only if it
asserted the snow volume value.

**Verify in-engine.** Not needed beyond T6's `loop_voice_count("windHowl") == 1`.

**Commit.** `Gameplay H07 T8: Arc 1-D Perlin rawNoise for the weather sound oscillator`

---

### T9. Weather env attributes reach every `Attribute.env()` consumer (water extractor, attribute crafters)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code): `core/world/behavior/production.rs:1477` still says the port ignores `attribute.env()`. Not checked in-engine.

**Symptom / acceptance.** Rain should raise a water extractor's output by
`+0.2` of its base rate and a sporestorm should boost cultivators by `+1`
efficiency; the port ignores `Attribute.env()` outside generators/fires.
After: test — a water extractor on `stone` pumps `(1 + 0.2)` times its no-rain
rate while a rain `WeatherRuntime` at opacity 1 is active.

**Java.** `Logic.update` (`J/core/Logic.java:603-606`):
`state.envAttrs.clear(); envAttrs.add(rules.attributes); Groups.weather.each(w -> envAttrs.add(w.weather.attrs, w.opacity));`
`Attribute.env()` = `state.envAttrs.get(this)` (`J/world/meta/Attribute.java:33`). Consumers:
`SolidPump.updateTile` (`SolidPump.java:124`): `fraction = max(validTiles + boost + (attribute == null ? 0 : attribute.env()), 0)`;
`AttributeCrafter` (`AttributeCrafter.java:96,100`): `baseEfficiency + min(maxBoost, boostScale * attrsum) + attribute.env()`;
`ThermalGenerator` (`:88`), `SolarGenerator` (`:29`), `Liquid.willBoil` (`Liquid.java:112`), `FireComp` (`:50`).
Weather attrs: snowing light -0.15; rain light -0.2, water 0.2; sandstorm light -0.1, water -0.1;
sporestorm spores 1, light -0.15; fog light -0.3, water 0.05.

**Port today.** `game/runtime.rs` `weather_system` (end) inserts
`GeneratorEnv { solar_multiplier, lighting, ambient_light_alpha, light_env, heat_env, steam_env, reactor_explosions }`
(`core/world/blocks/power/behavior.rs:57-85`, `fn attribute_env` ~90 returns
steam or heat only). `core/world/behavior/production.rs` SolidPump (~1476-1483)
computes `fraction = (valid_tiles + boost).max(0.0)` with the comment "the port
ignores `attribute.env()`, which is 0 on Serpulo" — false during rain/fog/sandstorm.
The SolidPump attribute name is resolved in `on_proximity_update` (~800-849,
`BlockKindData::SolidPump`/`Fracker` def `attribute: String`).
`AttributeCrafter` has no attrsum at all (`efficiency_scale` returns
`base_efficiency`, production.rs ~198-206) — **H02 owns that block** (§6).

**Changes.**
1. `core/world/blocks/power/behavior.rs` `GeneratorEnv`: add `water_env: f32`,
   `spores_env: f32` (defaults 0); `attribute_env` becomes a full match
   (`Heat, Steam, Light, Water, Spores`). Add
   ```rust
   /// `Attribute.env()` (`Attribute.java:33`): `state.envAttrs.get(attribute)`.
   pub fn env_attr(world: &World, attribute: Attribute) -> f32 { attribute_env(&generator_env(world), attribute) }
   ```
2. `weather_system`: fill `water_env: env(Attribute::Water)`, `spores_env: env(Attribute::Spores)`.
3. SolidPump: `fraction = (valid_tiles + boost + env).max(0.0)` where `env` =
   `env_attr(world, attr)` when the def's attribute name resolves via
   `crate::world::attributes::attribute_from_name`, else 0. Remove the false comment.
4. Do **not** touch AttributeCrafter; §6 gives H02 the `env_attr` seam.

**Tests.** `core/sim/production_tests.rs`: `water_extractor_rain_bonus` — copy
`water_extractor_scales_with_floor`; insert
`GeneratorEnv { water_env: 0.2, ..Default::default() }` as a resource and
compare to the no-env run: ratio `1.2 ± 0.05`. `core/game/runtime.rs`:
extend `weather_env_feeds_generator_env` (exists, ~1673) with
`water_env == 0.2` for rain at opacity 1.

**Verify in-engine.** Headless only is acceptable (no visible readback of the
pump rate besides `liquid_debug`). Optional: place a powered water extractor,
read `liquid_debug(x,y)` over 600 ticks with and without
`debug_create_weather("rain", 1, 3600)` (wait for opacity ~1 first).

**Commit.** `Gameplay H07 T9: weather water/spores env attributes reach SolidPump (Attribute.env)`

---

### T10. Menu music: `menu.ogg` at the main menu, `launch.ogg` in the planet dialog, `editor.ogg` in the editor (PAR-0072)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED, cause narrowed: main menu `current_track() == "menu"` and `current_track_playing() == true` (G07 fix holds). Planet dialog opened from the menu -> `current_track() == ""` (silence), `MenuBackground.visible == false`, and `SimHost.get_state()` returns `"playing"` even at the main menu, so `is_menu` is false and the in-game branch picks nothing; Java plays `launch`. Map editor open (`editor_maps` -> `_open_map(row)`; `EditorDialog.visible == true`, `MenuBackground.visible` stays true) -> `"menu"`; Java plays `editor` (`H07-T10-editor-open.png`). So step 3 is needed for the planet case: `is_menu` must also hold while the planet dialog is open with no sector loaded (use `dialog_stack()` containing `"planet"` while `HudGroup` is hidden). After an `--import` boot the stack also holds `"full_text"` (Campaign Changes Notice).

**Symptom / acceptance.** PAR-0072 ("Main menu never plays menu.ogg"). Java's
menu branch picks the planet's `launchMusic` while the planet dialog is shown
from the menu and `Musics.editor` while the map editor is shown. After: at the
main menu `current_track() == "menu"`; with the planet dialog open from the
menu `current_track() == "launch"` (after the 2 s crossfade); with the editor
open `"editor"`.

**Java.** `SoundControl.update` (`J/audio/SoundControl.java:153-185`):
```java
}else if(state.isMenu()){
    silenced = false;
    if(ui.planet.isShown()){ play(ui.planet.state.planet.launchMusic); }
    else if(ui.editor.isShown()){ play(Musics.editor); }
    else{ play(Musics.menu); }
}else if(state.rules.editor){ silenced = false; play(Musics.editor); }
```
`Planet.launchMusic = Musics.launch` (`J/type/Planet.java:143`); no vanilla
planet overrides it. `play()` crossfades with `finTime = foutTime = 120` frames.

**Port today.** `core/audio/music.rs` already implements the branch
(`MusicContext.planet_ui_visible`, `editor_ui_visible`,
`planet_music.launch`; tested by `menu_selection`). `gdext/audio/mod.rs`
`music_context()` never sets those three fields (they stay default false/None).
`is_menu = state == "menu" || /root/Spine/Ui/MenuBackground.visible`.
`/root/MindUi` `#[func] dialog_stack() -> PackedStringArray` (dialog names,
e.g. `"planet"`, `ui/dialogs_manifest.json`). Editor node:
`/root/Spine/Ui/EditorDialog` (`scenes/game.tscn`). Music files exist:
`REPO/assets/music/{menu,launch,editor}.ogg`.

**Changes.**
1. Probe first (record in the summary): boot to the main menu, then
   ```bash
   $G --eval 'return [get_node("/root/Spine/MindAudio").current_track(), get_node("/root/Spine/Ui/MenuBackground").visible, get_node("/root/Spine/SimHost").get_state()]'
   $G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
   $G --eval 'return [get_node("/root/Spine/Ui/MenuBackground").visible, get_node("/root/MindUi").dialog_stack()]'
   ```
   If `MenuBackground.visible` is false while the planet dialog is open from
   the menu, `is_menu` must also accept "no sector loaded"
   (`audio_game_state().in_campaign == false && state == "menu"` is not
   reliable — use `/root/Spine/Ui/UiRoot/MenuGroup` visibility or
   `dialog_stack` containing `"planet"` while `HudGroup` is hidden). Write down
   which signal you used and why.
2. `music_context()`: set
   `planet_ui_visible = is_menu && dialog stack contains "planet"`,
   `editor_ui_visible = is_menu && EditorDialog.visible` (dynamic `get("visible")`),
   `planet_music.launch = Some(MusicRef::new("launch"))` (comment: `Planet.launchMusic = Musics.launch`, `Planet.java:143`).
   Reuse the existing `ui_has_dialog` helper pattern; read the stack once per frame.
3. If step 1 shows `current_track()` is not `"menu"` at the main menu, fix the
   `is_menu` derivation with the signal you chose (that is the ledger item);
   otherwise record "menu track already correct (G07 T1 MenuBackground fix)".

**Tests.** `core/audio/music.rs` already covers the selection; add
`launch_in_planet_dialog_crossfades_from_menu`: menu playing `menu`, then
`planet_ui_visible = true` with `launch` set -> after 130 `update` frames
(`delta_frames 1`) the output plays `launch`.

**Verify in-engine.** Boot to menu (no sector): `current_track()` -> `"menu"`,
`current_track_playing()` -> true. Open the planet dialog from the menu,
`sleep 5` -> `"launch"`. Close it, `sleep 5` -> `"menu"`. Open the editor (grep
how the menu opens it: `grep -n "editor" ui/fragments/menu_fragment.gd`) ->
`"editor"`. Enter Ground Zero -> `alwaysmusic` off: the menu track fades
out (`current_track()` becomes `""` within 3 s). Evidence JSON in `runs/`.

**Commit.** `Fixes PAR-0072: menu/planet-launch/editor music selection`

---

### T11. Continuous-weapon `SoundLoop` (vela, navanax)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code): no `collect_weapon_loops`; `core/audio/frame.rs` has no continuous-weapon loop. Not checked in-engine.

**Symptom / acceptance.** A vela's plasma beam is silent while firing (only
the initial shot plays). After: test — a unit mount with a live continuous
bullet reports one `TurretLoopSource`-style input with `play = true`, and
`play = false` once the bullet ends.

**Java.** `Weapon.update` (`J/type/Weapon.java:381-421`):
```java
if(continuous && mount.bullet != null){
    if(!mount.bullet.isAdded() || mount.bullet.time >= mount.bullet.lifetime || mount.bullet.type != bullet){ mount.bullet = null; }
    else{ ...; if(shootSound != Sounds.none && !headless){
            if(mount.sound == null) mount.sound = new SoundLoop(shootSound, 1f);
            mount.sound.update(bulletX, bulletY, true); } ... }
}else{ ...; if(mount.sound != null){ mount.sound.update(bulletX, bulletY, false); } }
```
`SoundLoop(sound, baseVolume = 1)`; fade 0.05/frame (`SoundLoop.java`).
vela: `shootSound = beamPlasma`, `initialShootSound = shootBeamPlasma`,
`continuous = true` (`UnitTypes.java:536-542`); navanax (`~2451`). On unit
removal the loop stops.

**Port today.** `core/audio/frame.rs` `collect_turret_loops` +
`gdext/audio/mod.rs` `turret_loops: IndexMap<u64, SoundLoopState>` handle
keyed fade loops. `mind_core::weapons::UnitWeapons { weapons: Vec<WeaponDef>, mounts: Vec<WeaponMount>, .. }`
(`core/weapons/mod.rs:68`); `WeaponMount.bullet: Option<Entity>`
(`core/weapons/mount.rs:56`); `WeaponDef.continuous`, `.shoot_sound`.

**Changes.**
1. `core/audio/frame.rs`: `pub fn collect_weapon_loops(world: &World, out: &mut Vec<TurretLoopSource>)`
   that **appends** (no clear): for each unit (sorted like `collect_turret_loops`,
   skip `UnitCore.dead`) with `UnitWeapons`, for each `(index, weapon)` with
   `weapon.continuous && !weapon.shoot_sound.is_none_or_unset()`:
   `play = mount.bullet.is_some_and(|b| world.get_entity(b).is_ok())`;
   position = the bullet's `Pos` when playing, else the unit's `Pos`;
   `key = entity.to_bits() ^ (1u64 << 63) ^ ((index as u64 + 1) << 56)`
   (disjoint from turret keys: building entity bits never set bit 63 —
   document the assumption); `base_volume 1.0`, `volume_scl 1.0`.
2. `gdext/sim_host.rs` `audio_turret_loops`: call `collect_weapon_loops` after
   `collect_turret_loops`. The existing per-key sweep in `MindAudio` stops
   keys that disappear (unit death/removal) — matches Java.

**Tests.** `core/audio/frame.rs`: spawn a vela via the unit harness, set
`mounts[i].bullet = Some(<spawned dummy entity with Pos>)` -> one source with
`play = true` at the bullet position; despawn the dummy -> `play = false`.

**Verify in-engine.** Optional (`debug_spawn_player("vela", 1, x, y)` and
shoot) — headless only is acceptable; say so.

**Commit.** `Gameplay H07 T11: continuous-weapon SoundLoop (vela, navanax)`

---

### T12. In-engine re-verification of the player-unit halves (build beam, payload, weather loops)

> **In-engine verification (2026-10-14, session 2):** PASS (no commit needed): with the alpha (H00 env fix applied locally) a queued `copper-wall-large` gives `loop_voice_count("loopBuild") == 1`, `loop_volume` 0.098, plan progress 0.47 (`runs/20261014-s2-verify2/H03-T2d-build-beam.png`); `debug_spawn_player("mega", 1, 1044.0, 560.0)` next to a team-1 dagger: `pickup_payload()` removed the dagger and `events_applied` +1, `drop_payload()` restored it and +1, `drain_log()` `[]`; rain loop 1, windHowl loop 1 (T6). Correction in place: `apply_sim_command_json` `block` must be the NUMERIC id (`content_list("block").find("copper-wall-large")`); a string parses as 0 (air).

**Symptom / acceptance.** G07 could not verify the build-beam loop (T6) and
payload sounds (T14) because no player unit existed before H00. After H00
(player alpha at the core): `player:0` placements build through the queue.
Acceptance: `loop_voice_count("loopBuild") == 1` while the player builds;
a payload pickup/drop with a payload-capable player unit grows `events_applied`.
If everything passes, this task has **no commit** — record the evidence in the
summary and the ledger notes (PAR-0071, PAR-0205). If a check fails, fix it in
this task and commit `Gameplay H07 T12: <what was fixed>`.

**Steps.** Ground Zero:
```bash
$G --eval "return $H.has_player_unit()"                                       # true (H00)
$G --eval "$H.debug_add_core_items(\"copper\", 300); return true"
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":134,\"y\":58,\"block\":'+str($H.content_list(\"block\").find(\"copper-wall-large\"))+',\"rotation\":0,\"team\":1,\"player\":0}')"
sleep 0.5; $G --eval 'var a = get_node("/root/Spine/MindAudio"); return [a.loop_voice_count("loopBuild"), a.loop_volume("loopBuild")]'   # [1, >0]
$G --eval "return $H.player_build_plans_json()"                               # plan present
```
Payload: `debug_spawn_player("mega", 1, 1032, 440)` (check the signature with
`grep -n -B1 "fn debug_spawn_player" client/rust/mind-gdext/src/sim_host.rs`),
spawn a team-1 dagger next to it, `$I.pickup_payload()`, read
`stats()["events_applied"]` before/after, then `$I.drop_payload()` and read
again. Weather loops: covered by T6's `windHowl` check; also
`debug_create_weather("rain", 1, 3600)` -> `loop_voice_count("rain") == 1`
after ~5 s.

**Commit.** none if all pass (see above).

---

### T13. Sandstorm wind displacement in the live sim (PAR-0202) — depends on H01

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: during an active sandstorm (`windX` 0.998) an idle team-1 dagger at (1044.48, 558.65) and the alpha moved 0.0 px over 1260 ticks (Java predicts ~1.5 px). Waits for H01's mover.

**Dependency.** H01 owns the ground-movement fix that keeps external `Vel`
(G07 finding: `core/ai/types/ground.rs` sets `vel` and integrates `pos` in one
kinematic step, discarding the weather impulse). Check: write the test below
first; if it fails on your branch because the dagger does not move at all,
run `git log main --oneline | grep -i "H01"` and look for the movement task;
if it has not landed, skip this task ("skipped: waiting for H01 kinematic
mover") and do not commit the failing test.

**Java.** `ParticleWeather.update` (`ParticleWeather.java:42-51`):
`speed = force * intensity * Time.delta; if(speed > 0.001) for(unit) unit.impulse(windVector * speed)`;
`impulse(x, y)`: `vel.add(x / mass, y / mass)`, `mass = hitSize² · π`. Dagger
`hitSize 8` -> impulse `0.1 / 201.06 = 4.97e-4` px/tick² per tick; ground
drag `UnitType.drag = 0.3` (`vel.scl(1 - drag)` per tick) gives a steady
drift of roughly `4.97e-4 * 0.7 / 0.3 ≈ 1.2e-3` px/tick, i.e. about 0.7 px
over 600 ticks — Java's push on a dagger is subtle.

**Test.** `core/sim/combat_tests.rs` `sandstorm_drifts_idle_dagger`:
`live_sim(64)`, `sim.spawn_live_unit("dagger", 1, 256.0, 256.0, 0.0)` (no
enemy core, so GroundAI idles — first assert the dagger does not move over 600
ticks without weather), then spawn a sandstorm via T5's `create_weather` with
wind `(1, 0)`, intensity 1, opacity forced to 1; tick 600 -> displacement
along +x between 0.3 and 1.5 px and `|dy| < 0.1`. Record the measured value in
the commit body.

**Verify in-engine.** Ground Zero: spawn a team-1 dagger at (1040, 440), read
`unit_debug_positions()`, `debug_create_weather("sandstorm", 1, 3600)`, wait
20 s, read again; `weather_json().active[0].windX/windY` gives the direction.
Expect sub-tile drift along the wind (record numbers; it is small in Java too).

**Commit.** `Fixes PAR-0202: sandstorm wind impulse moves ground units (verified after H01)`

---

## §3 Order, dependencies, hot files

1. T1 (voice policy) — independent; touches `gdext/audio/{mod,voices}.rs`, `core/audio/priority.rs`.
2. T2 (block action sounds) -> T4 reuses its `emit_world_sound`/`AudioViewRng`.
3. T3 (steps), T4 (bullets/abilities).
4. T5 (addWeather + `create_weather`) -> T13 uses `create_weather`.
5. T6 -> T7 (weather draw; T7 extends T6's node). T8 after T6 (snow sound check).
6. T9, T10, T11 (independent).
7. T12 after T6 (weather loop check) — verification only.
8. T13 last (H01 dependency).

Rebuild + `tools/session-play.sh N` after every Rust task. Run
`cargo test -p mind-core --lib audio`, `... weather`, the touched modules,
then the full suites per H-environment §2 before each commit.

Hot files shared with other plans:
* `gdext/render.rs` `build_floor` (H03) — T6 adds the weather nodes after the shields block.
* `gdext/campaign.rs` `start_sector_with_loadout` (H04/H05) — T5 adds one block after `adopt_map_rules`.
* `core/sim/unit_runtime.rs` `unit_update_system` (H01) — T3 adds one call after `update_kinematics`.
* `core/combat/bullet/mod.rs`, `core/combat/abilities.rs` (H01) — T4 adds emit calls only.
* `core/world/blocks/units/behavior.rs`, `defense/shields.rs`, `behavior/defense.rs`, `distribution/mass_driver.rs`, `payloads/*` (H02) — T2 adds emit calls only.
* `core/world/behavior/production.rs` SolidPump, `power/behavior.rs` `GeneratorEnv` (H02) — T9.
* `core/entities/comp/weather.rs`, `core/game/runtime.rs` `weather_system` (H06 weather instruction) — T5 extracts `create_weather`.
* `gdext/sim_host.rs` (everyone) — keep new Rust-only seams under a `// ---- H07 audio/weather seams ----` comment.
* Goldens: `mind-headless/tests/golden/audio/events_sim.json` (T4, maybe T2).

## §4 Ledger changes to report (coordinator applies; never edit `chains.json`)

* PAR-0203 -> verified after T5 (tests + Frozen Forest `weather_json` entries; note the circle-darkness omission).
* PAR-0127 -> verified after T6+T7 (screenshots rain/snow/sandstorm/fog; `windHowl` loop live). Note: natural weather now also needs a fresh sector (T5); Ground Zero resume still has none in both clients.
* PAR-0072 -> verified after T10 (menu/launch/editor `current_track` readbacks).
* PAR-0202 -> verified after T13 (only once H01's mover landed; record measured drift).
* PAR-0071 / PAR-0205 -> add T12 in-engine notes (build beam loop, payload events).
* New items to file (status verified when their task lands):
  * "Every gun overlaps: no checkFrame intensify / no per-sound voice limit / no falloff offset" — T1.
  * "Unit factory, reconstructor, mender, door, mass driver, force-projector and payload-void sounds missing" — T2.
  * "Mech step and floor walk (water/mud) sounds missing" — T3.
  * "Bullet hit/despawn pitch never varies; heal bullets and unit abilities silent" — T4.
  * "Snow wind-howl volume uses the wrong noise function" — T8.
  * "Rain/sporestorm env attributes ignored by the water extractor (Attribute.env)" — T9 (AttributeCrafter half -> H02).
  * "Continuous-weapon beam loop sound (vela/navanax) missing" — T11.

## §5 Done criteria

* T1-T11 committed with their acceptance checks recorded (T11 may be headless
  only), T12 evidence recorded, T13 committed or "skipped: waiting for H01".
* `cargo test -p mind-core --lib --features msav-import`, `mind-headless --lib`,
  `mind-gdext --lib` at or above the post-H00 baseline (`docs/H00-blockers_summary.md`),
  clippy clean, `parity goldens` all PASS (any regenerated golden named in its
  commit), GDScript parse gate clean.
* In-engine evidence in `runs/<date>-s<N>-H07-*/` (JSON + weather PNGs), 0 panics in the run log.
* `REPO/docs/H07-audio-weather_summary.md` written per H-environment §6.

## §6 Cross-area seams

Needed from other plans:
* **H01**: (a) ground mover must keep external `Vel` contributions (wind impulse) — T13 waits on it (PAR-0202). (b) When H01 ports `LegsComp`'s group-stage stepping (`LegsComp.java:150-190`), call `crate::audio::steps::emit_leg_step(world, def, l.base.x, l.base.y, rng)` at Java `:187` and `floor.walkSound.at(x, y, 1, floor.walkSoundVolume)` via `floor_walk_sound` at `:184` (both helpers from T3). (c) `Weather.updateEffect` `checkTarget` should use elevation thresholds (`< 0.001` grounded, `>= 0.09` flying) instead of `PhysicsComp.flying` (`core/entities/comp/weather.rs` `apply_weather_status`) — H01 owns unit elevation.
* **H02**: (a) `AttributeCrafter` lacks `attrsum` and `attribute.env()` (`AttributeCrafter.java:96,100`); use T9's `crate::world::blocks::power::behavior::env_attr(world, Attribute)`. (b) Surge wall lightning (`Wall.java:167-175`): when ported, call `crate::audio::sim::emit_world_sound(world, SoundId::SHOOT_ARC, tile centre, 1.0, 1.0, 0.1)` (`lightningSound.at(tile, random(0.9, 1.1))`). (c) Launch pad / landing pad sounds (`LaunchPad.java:149` `padLaunch` `1 ± 0.1`; `LandingPad.java:153` `padLand` 0.75) with the same helper when those blocks get behaviour.
* **H03**: (a) `fx_golden` 1-ULP `fx_program_explosion` failure is H03's. (b) `floor.walkEffect` at T3's splash-timer site (`UnitComp.java:739`) and `healEffect` at T4's bullet-heal site (`BulletType.java:471`); T3/T4 leave `// H03:` comments there. (c) T6 adds three `WeatherRenderer` children to the Weather/Debris bands in `build_floor` — keep them when restructuring bands.
* **H04**: T5 inserts the addWeather call after `adopt_map_rules` in `start_sector_with_loadout`; if H04 restructures that flow, keep "fresh generation -> `rules.weather = add_weather(grid, content, rules)` before the CampaignRuntime snapshot".
* **H05**: `MindHud.push_unlock` has no producer; when H05 fires unlocks (`Control.java:161-164` -> `HudFragment.showUnlock`), the existing toast path already plays `uiNotify` (`ui/ui_root.gd` `_on_toast`). H05's new UI call sites only call `MindUi.play_ui_sound(name)`.
* **H06**: the `weathersense`/`weatherset` logic instructions should spawn through T5's `mind_core::entities::comp::weather::create_weather(world, seq, weather, intensity, duration, wind)` and read `WeatherRuntime`; do not duplicate the spawn tuple.
* **H08**: synthetic GUI clicks not reaching Controls (G07 T13 finding) — UI click sounds are verified by calling handlers from evals until H08 fixes the bridge.

Provided by this plan: `emit_world_sound` + `AudioViewRng` (T2), `steps::{emit_leg_step, floor_walk_sound}` (T3), `create_weather` (T5), `weather_views` + `WeatherRenderer` (T6), `env_attr` (T9), `raw_noise_1d` (T8), `active_voice_counts` `#[func]` (T1).

### Deferred (why)

* `Weapon.activeSound` (`Weapon.java:431-433`): no vanilla unit sets it.
* Sector-circle darkness term in `addWeather` (needs Arc 2-D `Noise.noise`; numbered sectors only).
* `ShockwaveTower`, `BurstDrill.drillSound`, `Accelerator`, `PowerGenerator.explodeSound` (Erekir/endgame; no live behaviour site checked).
* Fog-of-war gating of step/construct sounds (`inFogTo`; fog is H03).
* Frame-rate dependence: `LoopMixer`/`SoundLoopState` fade by 1 frame per frame instead of `Time.delta` (identical at 60 fps; revisit only if the client runs uncapped).
