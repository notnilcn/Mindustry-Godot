# PLAN.md — Applying the `godot-bevy` / `bevy_godot4` / `demo-projects` ideas

SPDX-License-Identifier: GPL-3.0-only

This is a **working plan**, not a source of truth. Per the root `AGENTS.md`, root
`*_PLAN.md` files are scratch material and are never cited as current-state
documentation. When a track below lands, fold its durable facts into the nearest
`AGENTS.md` (and, if it changes an invariant, `HIGH_LEVEL_PLAN.md`) instead of
leaving roadmap language in code or docs.

Scope: this plan only covers concrete changes we would make to Mindustry-Godot,
derived from reading `godot-bevy/`, `bevy_godot4/`, and `demo-projects/`. It is
deliberately conservative: the repo's `bevy_ecs`-as-a-library architecture,
deterministic single-threaded schedule, and `.tscn`-first / GDScript-UI-only
rules are treated as fixed.

## 0. Verdict summary

| Source | What it is | Verdict for Mindustry-Godot |
|---|---|---|
| `demo-projects/hot-reload` | godot-rust demo; reload via the `.gdextension` `reloadable` flag | **Already enabled** in `client/mind.gdextension:4`. No code to port; only the *survivability* work below matters. |
| `godot-bevy` | Integration library embedding a full Bevy `App` in a Godot node | **Pattern library only.** Library itself is not a dependency (couples to `bevy_app` + `MainScheduleOrder`). Borrow a short list of patterns. |
| `bevy_godot4` | Older integration embedding a full Bevy `App` | **Skip.** Full-Bevy, no signals/input, no despawn; version-sensitive internals. Nothing worth porting. |
| `demo-projects` (other 3) | Dodge/Squash the Creeps, Net Pong | **Skip.** Ordinary game demos, no architectural lever. |

Three tracks follow: **T1 reload survivability**, **T2 editor-navigable UI tree**,
**T3 selective `godot-bevy` patterns**. T4 lists explicit non-goals.

---

## 1. Findings this plan is built on

### 1.1 `game.tscn` reads as bare because the UI is manifest-instantiated

`client/scenes/game.tscn` is a *composition rig*, not the content: it declares the
native `Mind*` facade nodes and instances only three sub-scenes
(`ui_root.tscn`, `map_editor_dialog.tscn`, `state_inspector.tscn`). Most content
lives in `client/scenes/ui/dialogs/` (~35 scenes), `.../fragments/` (14), widgets,
and `client/scenes/editor/` (~15) — reachable by opening each scene directly.

`client/scenes/ui/ui_root.tscn` declares only five **empty** layer groups
(`MenuGroup`, `HudGroup`, `DialogLayer`, `OverlayLayer`, `LoadingLayer`).
`client/ui/ui_root.gd:_load_manifest()` reads `client/ui/dialogs_manifest.json`
and **code-instantiates** every dialog and fragment at boot
(`ui_root.gd:73-112`, with a `# code-instantiated:` comment). So the editor's
`UiRoot` never expands to show dialogs/fragments: the user cannot navigate the UI
tree in-editor, which is exactly the tension behind the original question.
Track T2 closes that gap without breaking the manifest ABI.

### 1.2 Hot reload already works at the container level

`client/mind.gdextension:4` already sets `reloadable = true`. So editing Rust,
running `tools/build.sh` (debug `cargo build`; `tools/build.sh:12-16`), and
letting the editor reload the cdylib already works. What does **not** work is
surviving the reload: Track T1.

### 1.3 What breaks on reload (concrete)

- On reload Godot reconstructs the Rust object with a fresh `init()` and restores
  only `STORAGE` properties; **`ready()` is not re-run** (documented in gdext,
  `godot-macros/src/lib.rs:582-598`).
- `MindSimHost::init()` builds a brand-new default world
  (`sim_host.rs:80`), so the live match resets.
- `MindSimHost::ready()` installs everything the class needs at runtime:
  `content_snapshot`, `GlobalVars::install_world`, `SimIoHandler`, the
  `AudioSinkRes`, settings, capture args, the `../MindFx` hook
  (`sim_host.rs:98-160`). After a reload those are absent → degraded behavior or
  panic.
- The same `ready()`-bootstraps pattern is used by other `Mind*` classes
  (e.g. `camera.rs`, `render.rs`, `stdb.rs`, `ui/ui_host.rs`).
- There is **no `EXTENSION_RELOADED` handler anywhere** in `mind-gdext`.

---

## 2. T1 — Make Rust hot reload survivable

Goal: after an editor reload, every `Mind*` node re-establishes the state it set
up in `ready()`, and the failure mode is a clean re-seeded world rather than a
half-initialized host. No simulation state is carried across the FFI boundary
(that is unsafe with a statically linked ECS); continuity is via the repo's own
deterministic save/scenario path.

### T1.1 Factor `ready()` into an idempotent `bootstrap()`

For each `Mind*` node that does setup in `ready()`:

- Extract the body of `ready()` into `fn bootstrap(&mut self)`.
- `ready()` calls `self.bootstrap()`.
- Keep it idempotent: guard any `connect`/`insert_resource`/`install_world`
  against double-application (the reload path calls it on a fresh instance, but
  future call sites should be safe).

Start with `MindSimHost` (`client/rust/mind-gdext/src/sim_host.rs:98-160`), then
the other classes that bootstrap in `ready()` (`camera.rs`, `render.rs`,
`stdb.rs`, `ui/ui_host.rs` — audit each).

### T1.2 Handle `EXTENSION_RELOADED` on each `Mind*` node

Add an `on_notification` arm that re-runs `bootstrap()`:

```rust
// sim_host.rs (indicative; use the notification enum for the class's base)
#[godot_api]
impl INode for MindSimHost {
    fn ready(&mut self) { self.bootstrap(); }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}
```

Notes for implementation:

- The gdext hot-reload itest shows the `IObject` form
  (`gdext/itest/hot-reload/rust/src/lib.rs:97-105`,
  `ObjectNotification::EXTENSION_RELOADED`). For `Node`/`CanvasItem` subclasses,
  use that class's notification enum (`NodeNotification`,
  `CanvasItemNotification` — see `camera.rs:138` for the existing pattern) and
  confirm the `EXTENSION_RELOADED` constant is exposed on it. If a subclass's enum
  does not surface it, route through a small `Object`-derived helper node that
  forwards the notification. Verify by compiling; do not assume.
- A node that is not in the tree yet when the notification fires should no-op;
  `try_get_node_as` already returns `None` and the code warns.
- Do not move node lookups into `init()` (not in tree); `bootstrap()` is called
  from the notification, when the node is live.

### T1.3 Optional (dev-only): rehydrate sim state on reload

To avoid losing a live match during iteration:

- Add an opt-in dev setting (e.g. `user://settings.json` key, read by
  `settings.rs`) that, on `EXTENSION_RELOADED`, re-loads the current world from
  the repo's deterministic save path (`mind_core::io`) or a scenario JSON instead
  of the default 32x32 stub in `init()`.
- Do **not** attempt to keep `bevy_ecs` entities/resources alive across the
  reload; re-seed from bytes. This matches the project's determinism design.
- Keep it default-off; the common case (tuning logic re-derived each frame) does
  not need it.

### T1.4 Optional: watch helper

Add `tools/watch.sh` + `tools/watch.ps1` (`.sh` canonical, `.ps1` twin) that
re-runs `tools/build.sh` on `client/rust/**` changes so the editor reload fires
without a manual build. Keep it out of `ci.sh`; it is a developer convenience.
Update `tools/AGENTS.md` if added.

### T1.5 Verification

- `cargo check --manifest-path client/rust/Cargo.toml -p mind-gdext`
- `bash tools/godot.sh --headless --editor --quit --path client` (import gate)
- Manual in-editor check (per `.opencode/skills/playtest/SKILL.md`): open the
  project, edit a Rust-returned value/string, `tools/build.sh`, confirm the
  editor reloads, the sim host does not panic, and `get_state_json()` still
  returns a valid frame. Inspect `user://last_log.txt`.
- `tools/ci.sh` for the full gate.

### T1.6 Risks

- Per-class `ready()` bodies may not be fully idempotent; audit each before
  adding the reload arm.
- Duplicated signal connections after reload are auto-disconnected by gdext, but
  re-connecting in `bootstrap()` must not assume that; guard with
  `is_connected`/`is_connected`-style checks as `ui_root.gd` already does.

---

## 3. T2 — Make the UI tree editor-navigable

Goal: a human can open `client/scenes/game.tscn`, expand `Ui/UiRoot`, and see the
dialogs and HUD fragments as children of their layer groups, while the manifest
stays the ABI/verification source of truth.

### T2.1 Declare dialogs/fragments as instances in `ui_root.tscn`

Add one instanced node per manifest entry under the correct parent:

```ini
[ext_resource type="PackedScene" path="res://scenes/ui/dialogs/about_dialog.tscn" id="dlg_about"]
; ...one ext_resource per dialog/fragment...

[node name="about" parent="DialogLayer" instance=ExtResource("dlg_about")]
```

Placement per `dialogs_manifest.json`:

- `dialogs` → `DialogLayer`.
- `fragments` with `group`: `menu` → `MenuGroup`, `hud` → `HudGroup`,
  `loading` → `LoadingLayer`, `overlay` → `OverlayLayer`.
- Node names must equal the manifest `name` (e.g. `fade_in`, `loading`), because
  `ui_root.gd` looks them up by that name (`_boot`, `set_menu_visible`).

This adds ~49 one-line instanced nodes; the file stays small because instanced
scenes are references, not expanded subtrees. `@onready` group refs are
unchanged.

### T2.2 Replace `_instantiate` with name-binding

Rewrite `client/ui/ui_root.gd`:

```gdscript
func _bind_manifest() -> void:
    var manifest := _read_manifest()
    for entry in manifest.get("dialogs", []):
        _bind(entry, dialog_layer, false)
    for entry in manifest.get("fragments", []):
        _bind(entry, _group_for(str(entry.get("group", ""))), true)

func _bind(entry: Dictionary, parent: Control, is_fragment: bool) -> void:
    var node_name := str(entry.get("name", ""))
    var instance := parent.get_node_or_null(NodePath(node_name))
    if instance == null:
        push_error("[ui] manifest entry `%s` has no scene node under %s" % [node_name, parent.name])
        return
    if is_fragment:
        instance.visible = not bool(entry.get("hidden", false))
        return
    _dialogs[node_name] = instance
    var ui := _ui()
    if ui != null:
        ui.call("register_dialog", node_name, instance, bool(entry.get("pause", false)))
```

Keep `_boot()`, `set_menu_visible()`, prompt/toast handling unchanged. The
transient toasts and prompt panels stay `# code-instantiated:` (they are genuinely
request-driven and high-churn) — keep those comments.

### T2.3 Keep the ABI honest and update the docs

- Add a boot-time cross-check: every manifest `name` resolves to a scene child,
  and every static child under the four group nodes appears in the manifest.
  `push_error` on mismatch (a `verify()` style check, mirroring
  `MindThemeBuilder.verify`). This makes the `.tscn` a third participant in the
  UI ABI alongside `dialogs_manifest.json` and `mind-core::ui::manifest`'s
  `EXPECTED_*` lists.
- Update `client/AGENTS.md`:
  - The `scenes/ui/ui_root.tscn` row / invariants must now say dialogs/fragments
    are declared as static instances in `ui_root.tscn` **and** listed in the
    manifest and the Rust `EXPECTED_*` lists.
  - Amend the "Manifest ABI" invariant: a new dialog/fragment must be added to
    all three places (scene instance, manifest, `mind-core::ui::manifest`).
- Do not change the Rust `EXPECTED_*` lists in this track; the set is unchanged.

### T2.4 Verification

- `bash tools/godot.sh --headless --editor --quit --path client` (parse/import).
- In-editor (playtest skill): open `game.tscn`, expand `Ui/UiRoot`, confirm all
  dialogs/fragments appear under the right groups; run the game and confirm boot
  presentation (menu visible, HUD hidden, fade cover) and dialog open/close still
  work, and a couple of prompts/toasts render.
- `tools/ci.sh`.

### T2.5 Risks

- Node-name collisions per group; all manifest names are unique and the grouping
  splits fragments across groups, so this should hold — but assert it in the
  boot cross-check.
- `game.tscn`'s `UiRoot` instance will now visibly carry the full child list; keep
  the `.tscn` diff mechanical (generated from the manifest) to ease review.
- If a dialog scene has a different root node name than the manifest `name`, only
  the *instance* name matters (override on the instanced node), so this is safe.

---

## 4. T3 — Selective `godot-bevy` patterns

Adopt only where the repo lacks an equivalent. Each item is independent; land in
priority order and stop when the value is not clear.

| # | Pattern (source) | Priority | Reason |
|---|---|---|---|
| T3.1 | Shadow value-guard for view-synced properties | Adopt | Cheap correctness win at the Godot↔Rust view seam. |
| T3.2 | Editor-authored component derive (`BevyComponents` / `AttachableComponent`) | Evaluate | Serves `.tscn`-first, but large; pilot on one subsystem. |
| T3.3 | Reflect-based in-editor ECS inspector | Evaluate | Extends existing JSON `StateInspector`; nice-to-have. |
| T3.4 | Bulk `PackedArray` snapshot for hot reads | Evaluate | Only if a per-node poll becomes hot. |
| T3.5 | Typed signal→event bridge | Mostly already present | `pending_commands` covers the command path; only useful for UI signals. |
| T3.6 | `NodeEntityIndex` (InstanceId↔Entity by hooks) | Defer | Only worthwhile if per-entity Godot nodes appear (render/FX). |

### T3.1 Shadow guard for view-synced values

Where two sides can author the same value (camera ↔ `mind_core::input::CameraState`,
UI-edited node properties, FX parameters), keep a `shadow` of the last exchanged
value and skip writes/reads that match it, as `godot-bevy` does with
`TransformSyncMetadata::shadow`. Reference:
`godot-bevy/src/plugins/transforms/{change_filter,sync_systems}.rs`.

- Apply first to `MindCamera2D` (`camera.rs`) which both polls input and is driven
  by `mind_core::input::CameraState`.
- Implement in `mind-gdext` (not `mind-core`); no boundary change.

### T3.2 Editor-authored component derive (pilot)

A derive/macro that reads `#[export]` inspector properties off an authored `.tscn`
node and feeds them into a `mind-core` resource/entity at boot would let designers
configure spawners, debug views, or scenario parameters in-editor. `godot-bevy`
does this with `inventory`-keyed registries
(`godot-bevy/src/plugins/scene_tree/autosync.rs`,
`godot-bevy-macros/src/{lib,emit}.rs`).

- This is the highest architectural value but the largest effort, and it touches
  the "Rust owns behavior, not scene construction" boundary. Pilot on one
  low-risk subsystem (e.g. a debug/FX tuning node) before generalizing.
- If it cannot be cleanly confined to `mind-gdext` + a new `mind-macros` derive
  that emits *metadata only* (repo rule: proc macros emit metadata only), drop
  it.

### T3.3 Reflect-based inspector

`godot-bevy/src/plugins/debugger.rs` dumps reflected entities to the Godot
debugger and a GDScript panel renders them. The repo already ships a JSON-based
`StateInspector`; a reflect-backed live ECS view would be an additive debug tool.
Evaluate after T1/T2; gate behind a debug flag.

### T3.4 Bulk snapshots

`godot-bevy` beats per-node FFI with PackedArray snapshots computed in GDScript
(`addons/godot-bevy/optimized_scene_tree_watcher.gd`). The repo already dumps JSON
and batches minimap updates; only adopt if profiling shows a hot per-node read.
Do not pre-optimize.

### T3.5 Typed signal/event bridge

The repo already queues foreign commands via `pending_commands` and drains them at
tick start — the cross-boundary pattern `godot-bevy` uses
(`godot-bevy/src/plugins/{signals,event_bridge}.rs`). The only remaining candidate
is UI signals (`MindUi`/`MindHud` → Rust), which today are wired manually in
GDScript. If adopted, keep it in `mind-gdext` and enqueue into `mind-core`
commands/events; never let GDScript mutate sim state.

### T3.6 `NodeEntityIndex`

Only relevant if we start keeping one Godot node per sim entity (bullets/units/FX).
Today the renderer bands and FX pools the view side, so defer. If it becomes
relevant, adopt the hook-maintained `InstanceId → Entity` index
(`godot-bevy/src/plugins/scene_tree/plugin.rs`), never a per-frame archetype scan.

---

## 5. T4 — Explicit non-goals

- **Do not depend on `godot-bevy` or `bevy_godot4`.** Both own a full Bevy `App`
  and its `MainScheduleOrder`; `mind-core` intentionally uses `bevy_ecs` only,
  with a `SingleThreadedExecutor` for determinism. Adopting either would fight the
  architecture.
- **Do not port `bevy_godot4` code.** Full-Bevy 0.17, no signals/input, no
  despawn, and version-sensitive `RawGd`/`DynMemory` internals.
- **Do not mirror the Godot scene tree into ECS entities.** Sim entities are not
  Godot nodes; the `.tscn`-first rule and GDScript-UI-only rule stand.
- **Do not use Godot `STORAGE` to carry sim state across reload.** The ECS world
  cannot be represented as exported properties; re-seed from the deterministic
  save/scenario path if continuity is needed.
- **Do not add a Rust watch daemon to CI.** It is a local dev convenience only.

---

## 6. Sequencing and gates

1. **T1** (T1.1 → T1.2 → verify), then optionally T1.3/T1.4. Small, self-contained,
   immediately useful.
2. **T2** (T2.1 → T2.2 → T2.3 → verify). Directly resolves the original
   editor-navigability concern; touches one scene + one script + docs.
3. **T3** in priority order, each behind its own verification; drop any item that
   does not show value.

Every landing runs the applicable gate from `client/AGENTS.md` /
`client/rust/AGENTS.md`; the full gate is `tools/ci.sh`. New/changed Rust and
GDScript files stay UTF-8/LF, GPL-3.0-only, and cite ported Mindustry sources.
Update the nearest `AGENTS.md` when a track changes structure or invariants.

## 7. Reference map (for implementation)

- Reload: `client/mind.gdextension:4`; `client/rust/mind-gdext/src/sim_host.rs:77-160`;
  `gdext/itest/hot-reload/rust/src/lib.rs:97-105`;
  `gdext/godot-macros/src/lib.rs:582-598`.
- UI tree: `client/scenes/ui/ui_root.tscn`; `client/ui/ui_root.gd:73-112`;
  `client/ui/dialogs_manifest.json`; `client/AGENTS.md` (Manifest ABI);
  `client/rust/mind-core/src/ui/manifest.rs` (`EXPECTED_*`).
- Build/verify: `tools/build.sh`; `tools/ci.sh`; `tools/godot.sh`;
  `.opencode/skills/playtest/SKILL.md`.
- Patterns: `godot-bevy/src/plugins/transforms/{change_filter,sync_systems}.rs`;
  `godot-bevy/src/plugins/scene_tree/{plugin,autosync}.rs`;
  `godot-bevy/src/plugins/{signals,event_bridge,debugger}.rs`;
  `godot-bevy-macros/src/{lib,emit}.rs`.
