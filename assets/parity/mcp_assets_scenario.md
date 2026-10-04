# MCP assets scenario (plan 03 §7.1c) — copy-paste evals

> **Status: DEFERRED to the orchestrator.** The in-engine probe runs on the
> single Godot editor mutex; lane/03-assets cannot hold it. The Rust runtime
> binding (`MindAssets`) and the spine inspector fixture are implemented (M5);
> this file records the exact `godot_exec`/`godot_input` payloads the
> orchestrator runs once the editor is free. Keep this file byte-for-byte in
> sync with `03_ASSETS_IMPLEMENTATION_PLAN.md` §7.1c.

## Preconditions

- `cargo run -q --release -p mind-tools -- pack --root .` has produced
  `assets/sprites/sprites.atlas.json` + pages (fallback optional).
- The editor is running the project at `client/` (repo path
  `/home/c/g/code_examples/mg-lanes/03-assets`); the `MindAssets` autoload
  resolves `<project>/../assets`. Override with the project setting
  `mindustry/assets_dir` if the tree is elsewhere.
- `godot_health check` → `ok: true`; open + play `res://scenes/game.tscn`.

## Steps

1. **Health + identity.**
   `godot_health {"action":"check"}`
   then
   `godot_exec {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"project\": ProjectSettings.globalize_path(\"res://\")}"}}`
   The `project` path must contain `mindustry-godot`/`mg-lanes` and `pid` must
   stay constant across the run.

2. **Load spine + wait for assets.**
   `godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}`
   then poll:
   `godot_log {"action":"get","params":{"source":"game","count":50}}`
   for `[assets] ready ok=true …`.

3. **Region lookup.**
   ```gdscript
   var p = MindAssets.probe("copper-wall")
   return [p.found, p.pageType, p.w, p.h, p.splits]
   ```
   Expect `[true, "main", 32, 32, null]`.

4. **Negative lookup (no silent `error` fallback).**
   ```gdscript
   return MindAssets.probe("this-region-does-not-exist").found
   ```
   Expect `false`.

5. **Transparency (proves the PNG decoded).**
   ```gdscript
   return [MindAssets.find_region("blank").get_image().get_pixel(1, 1).a, MindAssets.find_region("copper-wall").get_image().get_pixel(16, 16).a]
   ```
   Expect the first `0.0`; record the second (non-zero at an opaque pixel).

6. **Inspector fixture (region + icon).**
   ```gdscript
   var n = get_node("/root/Spine/Ui/StateInspector")
   var region_ok = n.show_region("copper-wall")
   var icon_ok = n.show_icon("copper-wall")
   return [region_ok, icon_ok]
   ```
   Expect `[true, true]` (icon = generated `ui/block-copper-wall-ui`; the
   `Iconc` font-glyph path lands with M7). Capture before/after:
   `godot_screenshot {"action":"game"}` and assert the `RegionPreview` rect
   (10,476)–(138,604) is non-blank in the after shot.

7. **Bundle check** (arrives with M7; keep in the scenario).
   ```gdscript
   return [MindAssets.bundle_get("block.copper-wall.name"), MindAssets.bundle_get("definitely.missing.key")]
   ```
   Expect `["Copper Wall", "definitely.missing.key"]`.

8. **Duplicate check.**
   ```gdscript
   return MindAssets.atlas_duplicates().is_empty()
   ```
   Expect `true`.

9. **Teardown.**
   `godot_log {"action":"errors"}` → empty; `godot_game {"action":"stop"}`.

## Notes

- `MindAssets.find_region(name)` returns an `AtlasTexture` (null when absent);
  `MindAssets.probe(name)` returns `{found,page,pageType,x,y,w,h,splits?,pads?}`
  as a `Dictionary`.
- All evals are pid-stamped; if `pid` changes, re-run from step 1.
- The M5 fixture uses the generated `ui/<type>-<name>-ui` region as the icon;
  M7 replaces `show_icon` with the `Iconc` glyph + `icon.ttf` rendering.
