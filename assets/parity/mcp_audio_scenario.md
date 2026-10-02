# Plan 18 §7c — in-engine audio MCP scenario (DEFERRED to the single-editor mutex)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
> Status 2026-10-02: the code + `res://scenes/spine.tscn` node (`/root/Spine/MindAudio`)
> are landed; **none of these evals have been executed yet**. Run them from the
> orchestrator's single-editor lane and paste the outputs into
> `18_AUDIO_IMPLEMENTATION_PLAN.md` §7c/Changelog. Screenshots are not meaningful
> for audio — state assertions only.

Preconditions (repo skill `.opencode/skills/playtest/SKILL.md`): `godot_health check`;
if `BRIDGE_NOT_CONNECTED` launch the editor, wait ~20 s, `godot_instance list`.
Every eval must return `{"pid": OS.get_process_id(), ...}` compared with
`godot_game instances`.

1. Open + run the spine and wait for `[audio] ready`:

   ```text
   godot_editor_edit open_scene res://scenes/spine.tscn
   godot_game play scene=res://scenes/spine.tscn
   godot_log get
   ```

2. Bus layout (expect all indices `> 0`, `ready == true`):

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   return {"pid": OS.get_process_id(), "music": a.bus_index("Music"), "sound": a.bus_index("Sound"),
           "ui": a.bus_index("UI"), "ready": a.is_ready()}
   ```

3. Menu track:

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   a.force_music("menu", true)
   return {"pid": OS.get_process_id(), "track": a.current_track(), "playing": a.current_track_playing()}
   ```

4. Volume path — `musicvol=0`, then restore + force `game1`:

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   a.set_setting("musicvol", 0)
   return {"pid": OS.get_process_id(), "stats": a.stats()}
   ```

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   a.set_setting("musicvol", 100)
   a.force_music("game1", true)
   return {"pid": OS.get_process_id(), "track": a.current_track()}
   ```

5. Pause lowpass — open a dialog, wait 0.6 s, assert `wet >= 0.9` / `cutoff <= 700`;
   close, wait 0.6 s, assert `wet <= 0.1` / `cutoff >= 19000`:

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   return {"pid": OS.get_process_id(), "cutoff": a.lowpass_cutoff(), "wet": a.lowpass_wet()}
   ```

   (`sound_bus_paused()` / `a.set_sound_paused(true)` exercises the pause mirror,
   since the spine has no game-pause source until plan 12/14.)

6. UI one-shot (expect `>= 0`):

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   return {"pid": OS.get_process_id(), "voice": a.play_oneshot("uiButton", 1.0, 1.0, 0.0), "stats": a.stats()}
   ```

7. Loop path:

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   a.debug_emit_loop("loopConveyor", 512.0, 512.0, 1.0)
   return {"pid": OS.get_process_id(), "vol": a.loop_volume("loopConveyor"), "voices": a.loop_voice_count("loopConveyor")}
   ```

8. Music override + stop:

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   a.force_music("boss1", true)
   var t = a.current_track()
   a.stop_music()
   return {"pid": OS.get_process_id(), "forced": t, "playing_after_stop": a.current_track_playing()}
   ```

9. Event drain + clean log:

   ```gdscript
   var a = get_node("/root/Spine/MindAudio")
   return {"pid": OS.get_process_id(), "pending": a.pending_events(), "log": a.drain_log()}
   ```

   `godot_log errors` must be empty.

10. Teardown: `godot_game stop`.

`tools/mcp-smoke.sh` adds steps 1–3 and 9 once these pass.
