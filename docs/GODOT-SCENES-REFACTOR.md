# 1. Godot scenes refactor — scene-spine dependency wiring

- **Status:** in progress · **Date:** 2026-11-05
- **Source skill:** `.opencode/skills/godot-scenes/SKILL.md` — written for the Khvor' Gates client (`kg_gdext`, `Kg*`, autoload-free tree); this repo
  is `mind-gdext`, `Mind*`, `Spine`.
- **Scope:** replace the outward node lookups of the **scene spine** with `#[export]`ed node references wired from `client/scenes/game.tscn`; UI
  dialog/fragment controllers, the code-instantiated render pass children and the `Spine` root name are **not** migrated (§8).

## 2. Skill → repo mapping

| Skill rule | This repo's reality |
|---|---|
| `Kg<Name>`, `kg_gdext`, `kg_editor` | `Mind<Name>` in `client/rust/mind-gdext`; no separate editor crate (views in `src/ui/editor_views.rs`, facade in `src/editor/`) |
| skill §1 one entry point `Main (KgMain, Node)` | `client/scenes/game.tscn` root is `Spine`, class plain `Node`. Name **kept** — the established scene/MCP ABI (`$"/root/Spine/..."` in docs, `AGENTS.md`) — §8 |
| skill §1 branch shape (`World`/`Camera`/`GUI`) | `SimHost`, `Input`, `MindAudio`, `MindNet`, `MindCampaign`, `MindLogic`, `MindEditor`, `MindPreview`, `MindPlatform`, `MindFx`, `MindRender` are top-level `Spine` children; world 2D under `World/` (`TileGrid`, `Camera2D`, `Renderer`, `LandingCore`, `MinimapProvider`, `Bloom`, `LowRes`); **the camera lives under `World`**, there is no `Camera` branch; GUI is the `Ui` `CanvasLayer` |
| skill §2 `Component2D` suffix | forward convention for new classes only; **not** retrofitted — renaming registered classes/nodes is a scene + MCP ABI break (§8) |
| skill §3 no reaching out | this pass's target: today **188** `/root/Spine` + **28** `"../"` literals (§3) |
| skill §3 signal → method → node reference | unchanged: `.tscn` `[connection]`/`#[signal]` stay the default; references only where a class queries a sibling every frame or from many methods |
| skill §3 `#[export] Option<Gd<T>>` + `node_paths` | already proven **here** by the sibling extension: `client/rust/phantom-camera-rust/src/pcam2d.rs` (`#[export] #[var(set = …, get = …)] pub follow_target: Option<Gd<Node2D>>`) and `client/addons/phantom_camera_rust/examples/example_scenes/2D/2d_example_scene.tscn` (`follow_target = NodePath("../CharacterBody2D")`); a Godot 4.7 forward-reference test proved a node may reference a sibling declared later in the same `.tscn` |
| skill §3 autoloads discouraged | **does not apply**: `client/project.godot` has five intentional `Mind*` autoloads (`MindAssets`, `MindMods`, `StdbConnector`, `MindUi`, `MindHud`) plus the addon's `PhantomCameraManager`/`McpRuntimeAutoload`. Global isolated services — they stay. `/root/Mind*` (52) and `/root/StdbConnector` (2) lookups are **allowed, not counted** |
| skill §4 owner is mediator; `Main` wires | `game.tscn` becomes the wiring hub; `MindUiRoot` becomes the mediator for its HUD fragments (`MindInput`/`MindAudio`/`MindLandingCore` ask it instead of walking `../Ui/UiRoot/...`) |
| skill §4/§5 `process_priority`, `.tscn`-first, `// code-instantiated:`, no GDScript | ordering unchanged (this pass moves lookups only). `.tscn`-first and no-GDScript are **already enforced** (`client/AGENTS.md`); `render.rs` alone has 32 `code-instantiated` sites |
| skill §6 gdext specifics | `#[export]` generates `set_/get_<field>`: checked against all 15 target files, no existing `pub fn set_<field>`/`get_<field>` collides with the proposed names. No shared `wiring` module here (`connect_once` is duplicated privately in `ui/dialogs/batch_c.rs` and `batch_campaign2.rs`; `game.tscn` uses declarative `[connection]`) — no new signal wiring in this pass |
| skill §6 names are the scene ABI | enforced: exports are **additions** only, no rename/signature change (`mind-gdext/AGENTS.md`: append-only) |

## 3. Current state

Read-only greps on `godot-scenes-refactor`, paths relative to `client/rust/mind-gdext/src` — **216** violations over 66 files (188 absolute + 28
parent-relative), bucketed by **who owns the node**; 3 fall outside all three buckets (`mods.rs:1`, `editor/editor_renderer.rs:1`, `input/desktop.rs:1`).

| Pattern | Matches | Verdict |
|---|---|---|
| `"/root/Spine` | **188** in 64 files (184 code + 4 in a `//` recipe block at `sim_host.rs:2145-2148`) | violation |
| `"\.\./` | **28** in 10 files | violation |
| `"/root/Mind*` | **52** (`MindUi` 28, `MindAssets` 17, `MindHud` 4, `MindMods` 3) | allowed autoload |
| `"/root/StdbConnector` | **2** | allowed autoload |

**(a) Scene spine — migrated here: 48 abs + 28 rel = 76.** Single-instance children of `Spine`/`Ui`/`World`, declared by name in `game.tscn`, so the
owning `.tscn` wires them directly.

| file | `/root/Spine` | `"../"` | exported deps |
|---|---|---|---|
| `campaign.rs` | 10 | 1 | sim_host, camera, audio, fx, editor, ui_root |
| `camera.rs` | 1 | 2 | sim_host, input, fx (optional) |
| `tile_grid.rs` | 0 | 1 | sim_host |
| `fx.rs` | 1 | 0 | camera |
| `render.rs` | 5 | 6 | `MindWorldRenderer`: sim_host, camera, fx, landing_core, ui_root, tile_grid · `MindRender`: renderer, tile_grid, camera, sim_host |
| `render/minimap.rs` | 0 | 2 | sim_host, camera |
| `sim_host.rs` | 6 (2 code + 4 doc) | 3 | input, camera, fx, preview |
| `net/mod.rs` | 4 | 1 | sim_host, campaign, ui_root |
| `input/mod.rs` | 4 | 8 | sim_host, camera, campaign, audio, ui_root |
| `ui/root.rs` | 3 | 2 | menu_background, editor_dialog, platform |
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
4. **Optional deps stay silent** (the class already guards the path and its output survives): `WorldRenderer.{ui_root, landing_core, tile_grid}`,
   `Render.{camera, tile_grid}`, `Minimap.camera`, `Camera.fx` (landing dust), `SimHost.preview`, `Net.{campaign, ui_root}`, `Input.{campaign, audio}`,
   `UiRoot.{menu_background, editor_dialog, platform}`, `Audio.{ui_root, menu_background, editor_dialog}`, `Campaign.{camera, audio, fx, editor,
   ui_root}`, `StateInspector.{audio, editor}`, `LandingCore.{fx, ui_root}`.
5. **Resolve in `ready()`/`bootstrap()`**, never in `process`; classes with a `bootstrap()` re-adopt the same fields on `EXTENSION_RELOADED`.
   `MindUiRoot`, `MindDialog` and the fragments must **not** gain an `on_notification` arm (re-entrant `bind_mut`).
6. **Late injection still works:** keep existing `#[func]` injectors (`MindTileGrid::set_host`) for the runtime/spawner path; the export wins when
   set. `#[export]` auto-generates `set_/get_<field>`, so never add a `#[func]` with that name; use `#[var(set = inject_x)]` when assignment must act.
7. **ABI append-only:** no class/`#[func]`/`#[signal]` rename or signature change; existing `*_path` exports (`MindStateInspector`) stay as the
   dev-scene fallback, typed deps win. **No `get_parent()`, no `get_node("/root/…")`, no `get_node("../…")`** in a migrated class.
8. `ui/kit::{campaign, sim_host, editor, net, logic}` stay for bucket (b); migrated classes must not call them (residual: §8).

## 5. Phased plan (binding)

One commit-sized unit per phase (code → `.tscn` wiring → targeted gate); missing wiring shows in the boot log (§7).

**S2 — core view.** `tile_grid.rs` (sim_host); `camera.rs` (sim_host, input, fx — the last is optional and covers the
`update_land_dust` shake/dust spawn that used to reach `/root/Spine/MindFx`); `fx.rs` (camera); `render.rs` **both classes** — `MindWorldRenderer`
(sim_host, camera, fx, landing_core, ui_root, tile_grid) and `MindRender` (renderer, tile_grid, camera, sim_host); `render/minimap.rs` (sim_host,
camera). Add `src/wiring.rs`; wire `game.tscn` (6 nodes, §6). `MindWorldRenderer.tile_grid` has no consumer today (`set_tile_grid_visible` is on
`MindRender`): wire it only if S2 moves that method, else omit it.

**S3a — pump.** `sim_host.rs` (input, camera, fx, preview; `fx_host: Option<Gd<Node>>` becomes `fx: Option<Gd<MindFx>>` and `install_fx_sink` reads
the field instead of re-looking it up); `net/mod.rs` (sim_host, campaign, ui_root; `campaign_call` becomes a direct `MindCampaign` call). Wire
`SimHost`, `MindNet`.

**S3b — input + UI root.** `input/mod.rs` (sim_host, camera, campaign, audio, ui_root): delete `BLOCK_CONFIG_PATH`, `BLOCK_INVENTORY_PATH`,
`HUD_FRAGMENT_DIR` and the literal `../Ui/UiRoot/HudGroup/…`/`MenuGroup` paths — route through `MindUiRoot` methods. `ui/root.rs` (menu_background,
editor_dialog, platform) plus owner-mediator accessors: `#[func] hud_group_node() -> Option<Gd<Control>>`, `hud_fragment(name) -> Option<Gd<Control>>`,
`menu_group_node()`, `fade_in_node()`, `is_menu_visible()`; keep `dialog(name)`, `set_menu_visible`, `set_editor_open`. Internally swap
`control_at("../MenuBackground")` for the new `menu_background` field and `kit::node("/root/Spine/Ui/EditorDialog")` for `editor_dialog`. Wire
`Ui/UiRoot` **on the instance root**.

**S4 — facades/display.** `audio/mod.rs` (sim_host, camera, ui_root, menu_background, editor_dialog); `campaign.rs` (sim_host, camera, audio, fx,
editor, ui_root); `ui/state_inspector.rs` (typed sim_host, camera, audio, editor; net may stay an autoload lookup); `ui/menu_background.rs` +
`ui/menu_flyers.rs` (render); `ui/landing.rs` (sim_host, camera, fx, ui_root — fade-in via `ui_root.fade_in_node()`). Wire `MindAudio`, `MindCampaign`,
`Ui/StateInspector`, `Ui/MenuBackground`, `MenuFlyers`, `World/LandingCore`.

**S5 — conformance.** Update the text this refactor invalidates: `client/rust/mind-gdext/AGENTS.md` ("scene nodes are addressed by fixed `/root/...`
paths"), `client/AGENTS.md` (the `scenes/game.tscn` map, the `/root/Spine/MindEditor` note), and `src/ui/PORTING.md` if it repeats it. Optional
source-scanning allowlist **only** if it can be honest and low-noise (grep for `"/root/Spine`/`"../` with a checked-in per-file allowlist); skip it
if it would just restate the deferred buckets. Then `python3 tools/ci_pending.py add …` and §7.

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
| `World/Renderer` | MindWorldRenderer | sim_host, camera, fx, landing_core, ui_root, (tile_grid) | `../../SimHost`, `../Camera2D`, `../../MindFx`, `../LandingCore`, `../../Ui/UiRoot`, (`../TileGrid`) |
| `World/LandingCore` | MindLandingCore | sim_host, camera, fx, ui_root | `../../SimHost`, `../Camera2D`, `../../MindFx`, `../../Ui/UiRoot` |
| `World/MinimapProvider` | MindMinimap | sim_host, camera | `../../SimHost`, `../Camera2D` |
| `MindRender` | MindRender | renderer, tile_grid, camera, sim_host | `../World/Renderer`, `../World/TileGrid`, `../World/Camera2D`, `../SimHost` |
| `MindFx` | MindFx | camera | `../World/Camera2D` |
| `MindCampaign` | MindCampaign | sim_host, camera, audio, fx, editor, ui_root | `../SimHost`, `../World/Camera2D`, `../MindAudio`, `../MindFx`, `../MindEditor`, `../Ui/UiRoot` |
| `MindNet` | MindNet | sim_host, campaign, ui_root | `../SimHost`, `../MindCampaign`, `../Ui/UiRoot` |
| `Ui/MenuBackground` | MindMenuBackground | render | `../../MindRender` |
| `Ui/MenuBackground/MenuFlyers` | MindMenuFlyers | render | `../../../MindRender` |
| `Ui/UiRoot` **(instance root)** | MindUiRoot | menu_background, editor_dialog, platform | `../MenuBackground`, `../EditorDialog`, `../../MindPlatform` |
| `Ui/StateInspector` **(instance root)** | MindStateInspector | sim_host, camera, audio, editor | `../../SimHost`, `../../World/Camera2D`, `../../MindAudio`, `../../MindEditor` |

`node_paths` is a node-header attribute after `instance=ExtResource(...)` when present; attribute order is irrelevant to the parser (Godot rewrites the file on save).

## 7. Verification

```bash
tools/run-cpu-intensive-task.sh --cargo-check -p mind-gdext                  # typecheck (queued)
tools/run-cpu-intensive-task.sh --cmd -- \                                   # scoped clippy
  cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check            # formatting (cheap)
godot --headless --path client --quit-after 180 res://scenes/game.tscn       # headless boot
#   queued: tools/run-cpu-intensive-task.sh --godot --headless --quit-after 180 res://scenes/game.tscn
# after the pass only the deferred buckets may remain:
grep -rn '"/root/Spine' --include="*.rs" client/rust/mind-gdext/src | wc -l
grep -rn '"\.\./'      --include="*.rs" client/rust/mind-gdext/src | wc -l
```

**Expected new warnings if a wiring line is missed** — ``{class}: missing required `{property}` (wire it in the owning scene)``, one per class/property,
in the boot log and `user://last_log.txt`: `MindTileGrid` sim_host · `MindCamera2D` sim_host, input · `MindFx` camera · `MindWorldRenderer` sim_host,
camera, fx · `MindRender` renderer, sim_host · `MindMinimap` sim_host · `MindSimHost` input, camera, fx · `MindNet` sim_host · `MindInput` sim_host,
camera, ui_root · `MindAudio` sim_host, camera · `MindCampaign` sim_host · `MindStateInspector` sim_host, camera · `MindLandingCore` sim_host, camera
· `MindMenuBackground` render · `MindMenuFlyers` render. Optional deps (§4.4) log nothing.

**Known baseline (pre-existing, not caused by this refactor):** scoped clippy currently fails on `clippy::collapsible_if` at `client/rust/mind-gdext/src/editor/mod.rs:2138` — `if let Some(current) = self.file.as_ref() {` wrapping a single `if !current.starts_with(&dir)` at :2139, in the editor facade S2–S4 do not touch. Do not fix it here; S5 records it as a `CI_PENDING.json` exception or leaves it to the CI fixer.

## 8. Deferred / follow-ups

- **UI dialog & fragment controllers (b).** Cannot take scene-wired refs without reaching into instanced-scene internals. Proposed direction, no code
  now: a value-object `UiContext` (`RefCounted` or `VarDictionary`) that `MindUiRoot`/`MindDialog`/`MindMapEditorDialog` populate once and pass down;
  controllers read `ctx.sim_host`, `ctx.ui`, `ctx.campaign` instead of path strings, leaving the manifest ABI and node names untouched.
- **Render pass children (c).** In no `.tscn`; `MindWorldRenderer` builds them with `new_alloc()`, so it must inject (`StatusRenderer`,
  `WeatherRenderer`, … gain `inject(sim_host, camera)` right after construction) — the spawner branch, not a scene edit.
- **`ui/root.rs` residual.** `return_to_menu`/`check_playtest` still resolve the campaign via `kit::campaign()`; a `campaign` export on `MindUiRoot`
  is safe but was outside S3b's scope. Same for `ui/kit/mod.rs`'s five path helpers, which serve bucket (b).
- **Component suffix naming** is for new classes only: retrofitting `Component2D` onto `MindTileGrid`/`MindWorldRenderer`/`MindFx` would rename
  registered classes and every `.tscn` naming them — an ABI break with no behavioural gain.
- **`Spine` → `Main`.** Kept: `Spine` is referenced by docs, MCP recipes (`$"/root/Spine/..."`), the `playtest` skill and `AGENTS.md`; renaming is
  churn against an ABI.
- **Autoload exceptions.** `MindAssets`, `MindMods`, `StdbConnector`, `MindUi`, `MindHud` stay — each owns global isolated state and is read from
  dozens of sites (52 `/root/Mind*` + 2 `/root/StdbConnector`). Only *scene* deps move out of path strings.

## 9. Checklist (adapted from skill §7)

- [ ] No `get_node`/`try_get_node_as` with `..` or `/root` in a migrated class (bucket (a) grep clean; `/root/Mind*` allowed).
- [ ] Every dep is an `#[export] Option<Gd<MindX>>` named for its role, listed in the owning scene's `node_paths`, path relative to the carrying node.
- [ ] Cross-scene deps set on the instance root only (`Ui/UiRoot`, `Ui/StateInspector`, `Ui/EditorDialog` untouched internally).
- [ ] Missing required dep ⇒ one `log::warn!` naming the property, then graceful degrade; optional deps silent.
- [ ] `ready()`/`bootstrap()` re-adopts the deps; no new `on_notification` on `MindUiRoot` / `MindDialog` / fragments.
- [ ] `MindUiRoot` accessors replace the `../Ui/UiRoot/...` strings in `MindInput`/`MindAudio`/`MindLandingCore`; `hud_fragment(name)` covers block_config/block_inventory/console/chat.
- [ ] Structure stays in `.tscn`; no new code-instantiated node without `// code-instantiated: <reason>`.
- [ ] `#[func]`/`#[signal]`/class names unchanged (append-only); `set_<field>` collisions checked.
- [ ] `--cargo-check -p mind-gdext`, scoped clippy (baseline `editor/mod.rs:2138` recorded) and `cargo fmt --check` run; headless boot clean of the §7 warnings.
- [ ] `CI_PENDING.json` entry via `python3 tools/ci_pending.py add` (gates `fmt,clippy,cargo-check,godot-import`); `/root/Mind*` lookups not counted as violations.
