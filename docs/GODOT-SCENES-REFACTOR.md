# 1. Godot scenes refactor — scene-spine dependency wiring

- **Status:** implemented — scene spine (S2–S4), deferred buckets documented · **Date:** 2026-11-05 · **Closeout:** S5
- **Source skill:** `.opencode/skills/godot-scenes/SKILL.md` — written for the Khvor' Gates client (`kg_gdext`, `Kg*`, autoload-free tree); this repo
  is `mind-gdext`, `Mind*`, `Spine`.
- **Scope:** replace the outward node lookups of the **scene spine** with `#[export]`ed node references wired from `client/scenes/game.tscn`; UI
  dialog/fragment controllers, the code-instantiated render pass children and the `Spine` root name are **not** migrated (§8).

### Results

- **73 code lookups removed** — 44 code `"/root/Spine"` + 28 `"../"` + 1 `input/desktop.rs` campaign lookup — across
  16 classes: `MindTileGrid`, `MindCamera2D`, `MindFx`, `MindWorldRenderer`, `MindRender`, `MindMinimap`,
  `MindSimHost`, `MindNet`, `MindInput` (+ `DesktopBridge`), `MindUiRoot`, `MindAudio`, `MindCampaign`,
  `MindStateInspector`, `MindMenuBackground`, `MindMenuFlyers`, `MindLandingCore`.
- **Final greps** (2026-11-05, `client/rust/mind-gdext/src`): `"/root/Spine` = **143** lines = **139** code +
  **4** recipe comments (`sim_host.rs:2159-2162`); `"\.\./` = **0**. Remaining code: **92** UI controllers +
  **45** render-pass children + **2** outside buckets (`mods.rs`, `editor/editor_renderer.rs`).
- **Verified** (orchestrator runs): `--build` exit 0 (39 s); headless `game.tscn` boot exit 0 with 0 `missing required`
  warnings and 0 ERROR/SCRIPT ERROR; SceneTree probe `CHECKED 16 nodes, null deps: 0`; `SHADERCHECK: total=30
  failed=0`, `UICHECK: failed=0`, `LAYOUTCHECK: failed=0`, `DIALOGCHECK: failed=0`; `cargo check`, `cargo fmt
  --check` and scoped clippy (with the pre-existing `collapsible_if` baseline allow) green. Details in §7.

## 2. Skill → repo mapping

| Skill rule | This repo's reality |
|---|---|
| `Kg<Name>`, `kg_gdext`, `kg_editor` | `Mind<Name>` in `client/rust/mind-gdext`; no separate editor crate (views in `src/ui/editor_views.rs`, facade in `src/editor/`) |
| skill §1 one entry point `Main (KgMain, Node)` | `client/scenes/game.tscn` root is `Spine`, class plain `Node`. Name **kept** — the established scene/MCP ABI (`$"/root/Spine/..."` in docs, `AGENTS.md`) — §8 |
| skill §1 branch shape (`World`/`Camera`/`GUI`) | `SimHost`, `Input`, `MindAudio`, `MindNet`, `MindCampaign`, `MindLogic`, `MindEditor`, `MindPreview`, `MindPlatform`, `MindFx`, `MindRender` are top-level `Spine` children; world 2D under `World/` (`TileGrid`, `Camera2D`, `Renderer`, `LandingCore`, `MinimapProvider`, `Bloom`, `LowRes`); **the camera lives under `World`**, there is no `Camera` branch; GUI is the `Ui` `CanvasLayer` |
| skill §2 `Component2D` suffix | forward convention for new classes only; **not** retrofitted — renaming registered classes/nodes is a scene + MCP ABI break (§8) |
| skill §3 no reaching out | this pass's target: baseline **188** `/root/Spine` + **28** `"../"` literals (§3) |
| skill §3 signal → method → node reference | unchanged: `.tscn` `[connection]`/`#[signal]` stay the default; references only where a class queries a sibling every frame or from many methods |
| skill §3 `#[export] Option<Gd<T>>` + `node_paths` | already proven **here** by the sibling extension: `client/rust/phantom-camera-rust/src/pcam2d.rs` (`#[export] #[var(set = …, get = …)] pub follow_target: Option<Gd<Node2D>>`) and `client/addons/phantom_camera_rust/examples/example_scenes/2D/2d_example_scene.tscn` (`follow_target = NodePath("../CharacterBody2D")`); a Godot 4.7 forward-reference test proved a node may reference a sibling declared later in the same `.tscn` |
| skill §3 autoloads discouraged | **does not apply**: `client/project.godot` has five intentional `Mind*` autoloads (`MindAssets`, `MindMods`, `StdbConnector`, `MindUi`, `MindHud`) plus the addon's `PhantomCameraManager`/`McpRuntimeAutoload`. Global isolated services — they stay. `/root/Mind*` (52) and `/root/StdbConnector` (2) lookups are **allowed, not counted** |
| skill §4 owner is mediator; `Main` wires | `game.tscn` becomes the wiring hub; `MindUiRoot` becomes the mediator for its HUD fragments (`MindInput`/`MindAudio`/`MindLandingCore` ask it instead of walking `../Ui/UiRoot/...`) |
| skill §4/§5 `process_priority`, `.tscn`-first, `// code-instantiated:`, no GDScript | ordering unchanged (this pass moves lookups only). `.tscn`-first and no-GDScript are **already enforced** (`client/AGENTS.md`); `render.rs` alone has 32 `code-instantiated` sites |
| skill §6 gdext specifics | `#[export]` generates `set_/get_<field>`: checked against all 15 target files, no existing `pub fn set_<field>`/`get_<field>` collides with the proposed names. No shared `wiring` module existed before this pass (`connect_once` was duplicated privately in `ui/dialogs/batch_c.rs` and `batch_campaign2.rs`; `game.tscn` uses declarative `[connection]`); S2 added `src/wiring.rs` for dep adoption — no new signal wiring in this pass |
| skill §6 names are the scene ABI | enforced: exports are **additions** only, no rename/signature change (`mind-gdext/AGENTS.md`: append-only) |

## 3. Current state

Read-only greps on `godot-scenes-refactor`, paths relative to `client/rust/mind-gdext/src` — **216** violations over 66 files (188 absolute + 28
parent-relative), bucketed by **who owns the node**; 3 fall outside all three buckets at the baseline (`mods.rs:1`, `editor/editor_renderer.rs:1`, `input/desktop.rs:1`).

**Final state after S2–S4 (2026-11-05):** `"/root/Spine` = **143** lines = **139** code + **4** comment lines (all four in `sim_host.rs:2159-2162`, the MCP
recipe block); `"\.\./` = **0**. Remaining code lookups: **(b) 92** + **(c) 45** + **outside 2** (`mods.rs`, `editor/editor_renderer.rs`; the
`input/desktop.rs` lookup was migrated). **73** code lookups removed in total.

| Pattern | Matches | Verdict |
|---|---|---|
| `"/root/Spine` | **188** in 64 files (184 code + 4 in a `//` recipe block at `sim_host.rs:2145-2148` at the baseline; **2159-2162** final) | violation |
| `"\.\./` | **28** in 10 files | violation |
| `"/root/Mind*` | **52** (`MindUi` 28, `MindAssets` 17, `MindHud` 4, `MindMods` 3) | allowed autoload |
| `"/root/StdbConnector` | **2** | allowed autoload |

**(a) Scene spine — target: 48 abs (44 code + 4 comments) + 28 rel = 72 code lookups. Migrated in S2–S4: those 44 code absolute + 28 parent-relative + 1 `input/desktop.rs` campaign lookup = 73 removed.** Single-instance children of `Spine`/`Ui`/`World`, declared by name in `game.tscn`, so the
owning `.tscn` wires them directly.

| file | `/root/Spine` | `"../"` | exported deps |
|---|---|---|---|
| `campaign.rs` | 10 | 1 | sim_host, camera, audio, fx, editor, ui_root |
| `camera.rs` | 1 | 2 | sim_host, input, fx (optional) |
| `tile_grid.rs` | 0 | 1 | sim_host |
| `fx.rs` | 1 | 0 | camera |
| `render.rs` | 5 | 6 | `MindWorldRenderer`: sim_host, camera, fx, landing_core, ui_root · `MindRender`: renderer, tile_grid, camera, sim_host |
| `render/minimap.rs` | 0 | 2 | sim_host, camera |
| `sim_host.rs` | 6 (2 code + 4 doc) | 3 | input, camera, fx, preview |
| `net/mod.rs` | 4 | 1 | sim_host, campaign, ui_root |
| `input/mod.rs` | 4 | 8 | sim_host, camera, campaign, audio, ui_root |
| `ui/root.rs` | 3 | 2 | menu_background, editor_dialog, platform, campaign |
| `audio/mod.rs` | 3 | 2 | sim_host, camera, ui_root, menu_background, editor_dialog |
| `ui/state_inspector.rs` | 2 | 0 | sim_host, camera, audio, editor |
| `ui/menu_background.rs` | 1 | 0 | render |
| `ui/menu_flyers.rs` | 1 | 0 | render |
| `ui/landing.rs` | 7 | 0 | sim_host, camera, fx, ui_root |
| **total (15 files)** | **48** | **28** | |

**(b) UI dialog/fragment/editor-view controllers — deferred: 92 abs + 0 rel, 27 files.** `ui/dialogs/` 48 (`batch_d:8 batch_c:7 batch_b:6 batch_i:5
batch_f:5 batch_campaign2:5 batch_j:3 batch_h:3 batch_a:3 batch_g:1 batch_e:1 misc:1`), `ui/fragments/` 24 (`hud:8 block_config:7 placement:3 hints:2
block_inventory:2 minimap:1 menu:1`), `ui/kit/` 8 (`mod:5 widgets/player_status:1 widgets/minimap:1 widgets/item_selection:1`), other `ui/` 12
(`editor_views:6 hud:3 ui_host:2 dialog:1`).

**(c) Render pass children — deferred: 45 abs + 0 rel, 21 files.** `status:4 hazards:4 weather:3 overlay_top:3 objective_markers:3 links:3
unit_overlays:2 unit_build:2 shields:2 plans:2 logic_displays:2 command_overlay:2 cached_cracks:2 bullets:2 build_beam:2 block_extras:2 unit_parts:1
unit_abilities:1 turret:1 pods:1 canvas:1`.

**Why the split is safe.** The buckets have different *owners*, so they need different mechanisms and cannot be mixed without breaking skill §3. (a)'s
owner is `game.tscn`: additive `#[export]` fields plus `.tscn` lines, and every path the code uses today is already the relative path the scene will
store (`../../SimHost`, …). (b) lives **inside instanced scenes** (dialogs under `UiRoot/DialogLayer` — the `ui/dialogs_manifest.json` ABI —,
fragments under `MenuGroup`/`HudGroup`, editor views inside `Ui/EditorDialog`), so their deps must come from the owner (`UiContext`, §8); wiring them
from `game.tscn` would point `node_paths` at internal nodes. (c) nodes are `new_alloc()`-built by `MindWorldRenderer` and in no `.tscn`, so the
renderer must inject them (spawner branch).

## 4. Target pattern

```rust
// src/audio/mod.rs
pub struct MindAudio {
    base: Base<GdNode>,
    /// `Spine/SimHost` — required.
    #[export] pub sim_host: Option<Gd<MindSimHost>>,
    /// `Spine/Ui/UiRoot` — optional, silent when absent.
    #[export] pub ui_root: Option<Gd<MindUiRoot>>,
}
```
```ini
; client/scenes/game.tscn
[node name="MindAudio" type="MindAudio" parent="." unique_id=889813358 node_paths=PackedStringArray("sim_host", "ui_root")]
sim_host = NodePath("../SimHost")
ui_root = NodePath("../Ui/UiRoot")
```

1. **One property per dependency**, named for its role (`sim_host`, `camera`, `fx`, `ui_root`, `editor`, …), typed `Option<Gd<MindX>>` — never
   `Gd<Node>`, never a `NodePath` string, never a dynamic `call()` when a typed class exposes the `#[func]` (same ABI, compile-checked).
2. **Wire from the owning scene:** `node_paths=PackedStringArray(…)` in the node header plus `prop = NodePath(…)`. **Cross-scene: set it on the
   instance root only** (`Ui/UiRoot`, `Ui/StateInspector`, `Ui/EditorDialog`) — never an internal node of an instanced scene. Paths are **relative to
   the node carrying the property** (as in `2d_example_scene.tscn`), so most entries keep the exact string the code used before.
3. **Missing required dep ⇒ exactly one `log::warn!` naming the property, then degrade:** ``log::warn!("{class}: missing required `{property}` (wire it
   in the owning scene)")``. Add one shared helper in S2 — `src/wiring.rs`, `pub(crate) fn required<T: Inherits<Node>>(owner, property, dep) ->
   Option<Gd<T>>` (~40 lines) — so the format cannot drift; inlining the identical string is the fallback if S2 rejects a new module.
4. **Optional deps stay silent** (the class already guards the path and its output survives): `WorldRenderer.{ui_root, landing_core}` (tile_grid was not wired — no consumer),
   `Render.{camera, tile_grid}`, `Minimap.camera`, `Camera.fx` (landing dust), `SimHost.{fx, preview}`, `Net.{campaign, ui_root}`, `Input.{campaign, audio}`,
   `UiRoot.{menu_background, editor_dialog, platform, campaign}`, `Audio.{sim_host, camera, ui_root, menu_background, editor_dialog}`, `Campaign.{camera, audio, fx, editor,
   ui_root}`, `StateInspector.{camera, audio, editor}`, `LandingCore.{sim_host, camera, fx, ui_root, campaign}`.
5. **Resolve in `ready()`/`bootstrap()`**, never in `process`; classes with a `bootstrap()` re-adopt the same fields on `EXTENSION_RELOADED`.
   `MindUiRoot`, `MindDialog` and the fragments must **not** gain an `on_notification` arm (re-entrant `bind_mut`).
6. **Late injection still works:** keep existing `#[func]` injectors (`MindTileGrid::set_host`) for the runtime/spawner path; the export wins when
   set. `#[export]` auto-generates `set_/get_<field>`, so never add a `#[func]` with that name; use `#[var(set = inject_x)]` when assignment must act.
7. **ABI append-only:** no class/`#[func]`/`#[signal]` rename or signature change; existing `*_path` exports (`MindStateInspector`) stay as the
   dev-scene fallback, typed deps win. **No `get_parent()`, no `get_node("/root/…")`, no `get_node("../…")`** in a migrated class.
8. `ui/kit::{campaign, sim_host, editor, net, logic}` stay for bucket (b); migrated classes must not call them (residual: §8).

## 5. Phased plan (binding)

One commit-sized unit per phase (code → `.tscn` wiring → targeted gate); missing wiring shows in the boot log (§7).

**S2 — core view (done, `a39084e0`).** `tile_grid.rs` (sim_host); `camera.rs` (sim_host, input, fx — the last is optional and covers the
`update_land_dust` shake/dust spawn that used to reach `/root/Spine/MindFx`); `fx.rs` (camera); `render.rs` **both classes** — `MindWorldRenderer`
(sim_host, camera, fx, landing_core, ui_root; tile_grid omitted — no consumer) and `MindRender` (renderer, tile_grid, camera, sim_host); `render/minimap.rs` (sim_host,
camera). Add `src/wiring.rs`; wire `game.tscn` (6 nodes, §6). `MindWorldRenderer.tile_grid` has no consumer today (`set_tile_grid_visible` is on
`MindRender`): wire it only if S2 moves that method, else omit it.

**S3a — pump (done, `57ee67ea`).** `sim_host.rs` (input, camera, fx, preview; `fx_host: Option<Gd<Node>>` becomes `fx: Option<Gd<MindFx>>` and `install_fx_sink` reads
the field instead of re-looking it up; `tick_view`/`map_path` are typed calls); `net/mod.rs` (sim_host, campaign, ui_root; `campaign_call` calls the
wired `MindCampaign` under its dynamic `has_method` guard; the join path uses typed `start_joined_world`, and `ui_root.set_menu_visible(false)`
replaces the path lookup). Wire `SimHost`, `MindNet`.

**S3b — input + UI root (done, `fbcd5cd2`).** `input/mod.rs` (sim_host, camera, campaign, audio, ui_root): delete `BLOCK_CONFIG_PATH`, `BLOCK_INVENTORY_PATH`,
`HUD_FRAGMENT_DIR` and the literal `../Ui/UiRoot/HudGroup/…`/`MenuGroup` paths — route through `MindUiRoot` methods. `ui/root.rs` (menu_background,
editor_dialog, platform, campaign) plus owner-mediator accessors (**`pub(crate)`, not `#[func]`** — Rust-only, so no ABI change):
`hud_group_node() -> Option<Gd<Control>>`, `hud_fragment(name) -> Option<Gd<Control>>`, `menu_group_node()`, `fade_in_node()` (a planned `is_menu_visible()`
seam was dropped in S5 — no caller); keep `dialog(name)`, `set_menu_visible`, `set_editor_open`. Internally swap `control_at("../MenuBackground")` for the new `menu_background` field,
`kit::node("/root/Spine/MindPlatform")` for `platform` (typed `finish_launch`), `kit::node("/root/Spine/Ui/EditorDialog")` for `editor_dialog`
(typed `show_dialog`), and `kit::campaign()` for `campaign` (typed `reset_to_menu`/`check_playtest`/`get_sector_state`). Wire `Ui/UiRoot` **on the
instance root**.

**S4a — facades (done, `7b3ec826`).** `audio/mod.rs` (sim_host, camera, ui_root, menu_background, editor_dialog); `campaign.rs` (sim_host, camera, audio, fx, editor,
ui_root). Wire `MindAudio`, `MindCampaign`.

**S4b — inspector/menu/landing (done, `11fbed54`).** `ui/state_inspector.rs` (sim_host required, camera/audio/editor optional; net may stay an autoload lookup; the
`*_path` exports stay as the dev-scene fallback); `ui/menu_background.rs` + `ui/menu_flyers.rs` (render); `ui/landing.rs` (sim_host, camera, fx,
ui_root, campaign — all optional; fade-in via `ui_root.fade_in_node()`, sector announce via the wired `campaign`). Wire `Ui/StateInspector`,
`Ui/MenuBackground`, `MenuFlyers`, `World/LandingCore`.

**S5 — closeout.** Updated the text this refactor invalidates: `client/rust/mind-gdext/AGENTS.md` (the old "scene nodes are addressed by fixed
`/root/...` paths" invariant became the injected-dep convention plus a no-new-spine-lookup rule), `client/AGENTS.md` (the `scenes/game.tscn` map, a new
scene-spine invariant, the `/root/Spine/MindEditor` note qualified as a remaining deferred group), and this doc (Results, §3, §5, §7, §9).
`src/ui/PORTING.md` was checked — it does not repeat the phrase, so it is unchanged. Removed the unused `MindUiRoot::is_menu_visible` accessor
(`menu_group_node` stays, used by `MindInput`). No source-scanning allowlist added: it would only restate the deferred buckets (b)/(c). Added the
`CI_PENDING.json` entry (§7).

## 6. Wiring map (`client/scenes/game.tscn`)

Property and `NodePath` lists are in the same order. `Bloom`/`LowRes` are class-less `Node2D` placeholders (`render.rs` builds its own `BloomGroup` in
code) — untouched.

| node (under `Spine`) | class | exported properties | node_paths (ordered) |
|---|---|---|---|
| `SimHost` | MindSimHost | input, camera, fx, preview | `../Input`, `../World/Camera2D`, `../MindFx`, `../MindPreview` |
| `Input` | MindInput | sim_host, camera, campaign, audio, ui_root | `../SimHost`, `../World/Camera2D`, `../MindCampaign`, `../MindAudio`, `../Ui/UiRoot` |
| `MindAudio` | MindAudio | sim_host, camera, ui_root, menu_background, editor_dialog | `../SimHost`, `../World/Camera2D`, `../Ui/UiRoot`, `../Ui/MenuBackground`, `../Ui/EditorDialog` |
| `World/TileGrid` | MindTileGrid | sim_host | `../../SimHost` |
| `World/Camera2D` | MindCamera2D | sim_host, input, fx | `../../SimHost`, `../../Input`, `../../MindFx` |
| `World/Renderer` | MindWorldRenderer | sim_host, camera, fx, landing_core, ui_root | `../../SimHost`, `../Camera2D`, `../../MindFx`, `../LandingCore`, `../../Ui/UiRoot` |
| `World/LandingCore` | MindLandingCore | sim_host, camera, fx, ui_root, campaign | `../../SimHost`, `../Camera2D`, `../../MindFx`, `../../Ui/UiRoot`, `../../MindCampaign` |
| `World/MinimapProvider` | MindMinimap | sim_host, camera | `../../SimHost`, `../Camera2D` |
| `MindRender` | MindRender | renderer, tile_grid, camera, sim_host | `../World/Renderer`, `../World/TileGrid`, `../World/Camera2D`, `../SimHost` |
| `MindFx` | MindFx | camera | `../World/Camera2D` |
| `MindCampaign` | MindCampaign | sim_host, camera, audio, fx, editor, ui_root | `../SimHost`, `../World/Camera2D`, `../MindAudio`, `../MindFx`, `../MindEditor`, `../Ui/UiRoot` |
| `MindNet` | MindNet | sim_host, campaign, ui_root | `../SimHost`, `../MindCampaign`, `../Ui/UiRoot` |
| `Ui/MenuBackground` | MindMenuBackground | render | `../../MindRender` |
| `Ui/MenuBackground/MenuFlyers` | MindMenuFlyers | render | `../../../MindRender` |
| `Ui/UiRoot` **(instance root)** | MindUiRoot | menu_background, editor_dialog, platform, campaign | `../MenuBackground`, `../EditorDialog`, `../../MindPlatform`, `../../MindCampaign` |
| `Ui/StateInspector` **(instance root)** | MindStateInspector | sim_host, camera, audio, editor | `../../SimHost`, `../../World/Camera2D`, `../../MindAudio`, `../../MindEditor` |

`node_paths` is a node-header attribute after `instance=ExtResource(...)` when present; attribute order is irrelevant to the parser (Godot rewrites the file on save).

## 7. Verification

```bash
tools/run-cpu-intensive-task.sh --cargo-check -p mind-gdext                  # typecheck (queued)
tools/run-cpu-intensive-task.sh --cmd -- \                                   # scoped clippy (baseline allow)
  cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings -A clippy::collapsible_if
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check            # formatting (cheap)
godot --headless --path client --quit-after 180 res://scenes/game.tscn       # headless boot
#   queued: tools/run-cpu-intensive-task.sh --godot --headless --quit-after 180 res://scenes/game.tscn
# after the pass only the deferred buckets may remain:
grep -rn '"/root/Spine' --include="*.rs" client/rust/mind-gdext/src | wc -l
grep -rn '"\.\./'      --include="*.rs" client/rust/mind-gdext/src | wc -l
```

**Expected new warnings if a wiring line is missed** — ``{class}: missing required `{property}` (wire it in the owning scene)``, one per class/property,
in the boot log and `user://last_log.txt`: `MindTileGrid` sim_host · `MindCamera2D` sim_host, input · `MindFx` camera · `MindWorldRenderer` sim_host,
camera, fx · `MindRender` renderer, sim_host · `MindMinimap` sim_host · `MindSimHost` input, camera · `MindNet` sim_host · `MindInput` sim_host,
camera, ui_root · `MindCampaign` sim_host · `MindStateInspector` sim_host
· `MindMenuBackground` render · `MindMenuFlyers` render. Optional deps (§4.4) log nothing: `MindAudio` has no required deps (all five are optional),
`MindLandingCore` is all-optional, and `MindStateInspector`'s camera/audio/editor are optional too.

**Known baseline (pre-existing, not caused by this refactor):** scoped clippy currently fails on `clippy::collapsible_if` at `client/rust/mind-gdext/src/editor/mod.rs:2138` — `if let Some(current) = self.file.as_ref() {` wrapping a single `if !current.starts_with(&dir)` at :2139, in the editor facade S2–S4 do not touch. Do not fix it here; S5 recorded it in the `CI_PENDING.json` risk note and left it to the CI fixer.

**Actual verification (S5 closeout, 2026-11-05).** Recorded from the orchestrator's runs (S5 did not re-run them):

- `tools/run-cpu-intensive-task.sh --build` — exit 0 (39 s).
- Headless `godot --headless --path client --quit-after 240 res://scenes/game.tscn` — exit 0; **0** `missing required`
  warnings; 0 `ERROR`/`SCRIPT ERROR` lines.
- One-off SceneTree probe (no repo file added) over all 16 wired nodes' exported refs — `CHECKED 16 nodes, null deps: 0`.
- Oracles — `SHADERCHECK: total=30 failed=0`; `UICHECK: failed=0`; `LAYOUTCHECK: failed=0`; `DIALOGCHECK: failed=0`.
- Rust — `cargo check -p mind-gdext` green; `cargo fmt --all -- --check` green; scoped clippy green with
  `-A clippy::collapsible_if` (the `editor/mod.rs:2138` baseline above remains pre-existing and was not fixed here).
- `cargo test -p mind-gdext --lib` — **185 passed / 17 failed**; every failure is "Godot engine not available" in
  engine-dependent UI helper tests (pre-existing environment limitation: this crate has no engine in `cargo test`).
- `CI_PENDING.json` — one entry (`CP-20261105-042043`, `--agent godot-scenes-refactor`, area `client/rust/mind-gdext/src`,
  8 gates) carrying the file list, repro and risk note for the CI fixer.

## 8. Deferred / follow-ups

- **UI dialog & fragment controllers (b).** Cannot take scene-wired refs without reaching into instanced-scene internals. Proposed direction, no code
  now: a value-object `UiContext` (`RefCounted` or `VarDictionary`) that `MindUiRoot`/`MindDialog`/`MindMapEditorDialog` populate once and pass down;
  controllers read `ctx.sim_host`, `ctx.ui`, `ctx.campaign` instead of path strings, leaving the manifest ABI and node names untouched.
- **Render pass children (c).** In no `.tscn`; `MindWorldRenderer` builds them with `new_alloc()`, so it must inject (`StatusRenderer`,
  `WeatherRenderer`, … gain `inject(sim_host, camera)` right after construction) — the spawner branch, not a scene edit.
- **`ui/kit/mod.rs` residual.** Its five path helpers (`campaign`, `sim_host`, `editor`, `net`, `logic`) still serve bucket (b), which cannot take
  scene-wired refs.
- **Component suffix naming** is for new classes only: retrofitting `Component2D` onto `MindTileGrid`/`MindWorldRenderer`/`MindFx` would rename
  registered classes and every `.tscn` naming them — an ABI break with no behavioural gain.
- **`Spine` → `Main`.** Kept: `Spine` is referenced by docs, MCP recipes (`$"/root/Spine/..."`), the `playtest` skill and `AGENTS.md`; renaming is
  churn against an ABI.
- **Autoload exceptions.** `MindAssets`, `MindMods`, `StdbConnector`, `MindUi`, `MindHud` stay — each owns global isolated state and is read from
  dozens of sites (52 `/root/Mind*` + 2 `/root/StdbConnector`). Only *scene* deps move out of path strings.

## 9. Checklist (adapted from skill §7)

- [x] No `get_node`/`try_get_node_as` with `..` or `/root` in a migrated class (bucket (a) grep clean; `/root/Mind*` allowed).
- [x] Every dep is an `#[export] Option<Gd<MindX>>` named for its role, listed in the owning scene's `node_paths`, path relative to the carrying node.
- [x] Cross-scene deps set on the instance root only (`Ui/UiRoot`, `Ui/StateInspector`, `Ui/EditorDialog` untouched internally).
- [x] Missing required dep ⇒ one `log::warn!` naming the property, then graceful degrade; optional deps silent.
- [x] `ready()`/`bootstrap()` re-adopts the deps; no new `on_notification` on `MindUiRoot` / `MindDialog` / fragments.
- [x] `MindUiRoot` accessors replace the `../Ui/UiRoot/...` strings in `MindInput`/`MindAudio`/`MindLandingCore`; `hud_fragment(name)` covers block_config/block_inventory/console/chat.
- [x] Structure stays in `.tscn`; no new code-instantiated node without `// code-instantiated: <reason>`.
- [x] `#[func]`/`#[signal]`/class names unchanged (append-only); `set_<field>` collisions checked.
- [x] `--cargo-check -p mind-gdext`, scoped clippy (baseline `editor/mod.rs:2138` recorded) and `cargo fmt --check` run; headless boot clean of the §7 warnings.
- [x] `CI_PENDING.json` entry via `python3 tools/ci_pending.py add` (gates `fmt,clippy,cargo-check,godot-import,ui-widgets-check,ui-layout-check,dialog-sweep-check,shaders-check`); `/root/Mind*` lookups not counted as violations.

Deferred follow-ups, intentionally left unchecked:

- [ ] UI dialog/fragment controllers (bucket (b), 92 lookups) — needs a shared `UiContext` value object; no code in this pass (§8).
- [ ] Code-built render-pass children (bucket (c), 45 lookups) — needs injection from `MindWorldRenderer`; no scene edit can reach them (§8).
- [ ] `ui/kit` path-helper removal (`campaign`, `sim_host`, `editor`, `net`, `logic`) — still serves bucket (b) (§8).
- [ ] `Component2D` suffix retrofit and `Spine` → `Main` — renaming would break the scene/MCP ABI; not done (§8).
- [ ] Autoload consolidation — intentional global services; out of scope (§8).
