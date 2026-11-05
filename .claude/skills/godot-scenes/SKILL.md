---
name: godot-scenes
description: Rules for creating or changing Godot scenes (.tscn) and Godot-facing node classes (kg_gdext `Kg*` classes, kg_editor tool classes) in the Khvor' Gates client. Use it whenever you add a node, a scene, a component, a signal, or wire one node to another under client/.
---

# Godot scenes and nodes (Khvor' Gates client)

Godot 4.7, Rust only through godot-rust gdext 0.5 (`client/rust/kg_gdext`). No
GDScript (`docs/POLICY/no-gdscript.md`). These rules condense the Godot best-practice
docs to what applies here. When you need the original text, do targeted reads of:

- `docs/AI/REFERENCES/godot-docs/tutorials/best_practices/scene_organization.rst`
  (dependency injection, choosing a tree structure)
- `.../best_practices/project_organization.rst` (folders, naming, `.gdignore`)
- `.../best_practices/autoloads_versus_regular_nodes.rst` (when not to use autoloads)
- `.../best_practices/node_alternatives.rst` (Object / RefCounted / Resource instead of nodes)
- `.../best_practices/scenes_versus_scripts.rst` (declarative scene vs code)

Don't grep or glob the whole docs tree. Read the section you need.

## 1. Tree shape

The game has one entry point. Each subsystem gets its own branch.

```
Main (KgMain, Node)              entry point: owns launch args, mediates between branches
├ <data/service nodes>           e.g. Assets, WorldModel, Net (plain Node, no transform)
├ World (Node2D)                 everything drawn in world space
│ ├ <world components>           renderers, overlays, spawners (*Component2D)
│ └ <entity scene instances>     LocalPlayer, spawned RemotePlayers, ...
├ Camera (Node)                  Camera2D, Phantom Camera nodes, camera components
└ GUI (CanvasLayer)              widgets: Control-rooted scenes
```

- A node goes under a parent only if it is an element of that parent: if you
  remove the parent, the child should go too. Otherwise make it a sibling, or put
  it in another branch.
- To keep a CanvasItem out of its parent's transform, put a plain `Node` between
  them (the declarative option). Use `top_level` only when you can't.
- Networked: logic that only the local client needs (input, reporting) lives in
  the local player's scene. Never put it on the shared puppet scene.

## 2. Entities, components, widgets: naming

| Kind | Root type | Node name | Rust class | Example |
|---|---|---|---|---|
| Entity scene (a game concept) | closest built-in (`Node2D`, `Node3D`, ...) | `PascalCase` noun | `Kg<Name>` | `LocalPlayer` / `KgLocalPlayer` |
| Component, no transform | `Node` | `<Name>Component` | `Kg<Name>Component` | `CameraRigComponent`, `PositionSyncComponent` |
| Component, 2D (draws or positions in 2D) | `Node2D` | `<Name>Component2D` | `Kg<Name>Component2D` | `HexGridOverlayComponent2D`, `UnitSpritesComponent2D` |
| Component, 3D | `Node3D` | `<Name>Component3D` | `Kg<Name>Component3D` | `TerrainComponent3D` (Phase 9) |
| GUI widget | `Control` subtype | `PascalCase` noun | `Kg<Name>` | `DebugHud`, `StatusOverlay` |
| Data or service node | `Node` | `PascalCase` noun | `Kg<Name>` | `WorldModel`, `Net`, `Assets` |
| Drawing primitive (like `Sprite2D`) | `Node2D` | descriptive | `Kg<Name>` | `KgPatchPlates` |

- A **component** is one responsibility attached to an entity (or to `World` or
  `Main`). It is reusable in any scene that provides its dependencies. The suffix
  follows the base type, the same way Godot names `Camera2D` and `Camera3D`. Use
  `Component` with no suffix only for `Node`-based components, such as logic
  shared by the 2D and 3D presenters.
- The node name equals the class name without the `Kg` prefix:
  `CameraRigComponent` is a `KgCameraRigComponent`. File names are the
  snake_case of the node name: `camera_rig_component.rs`,
  `unit_sprites_component_2d.tscn`.
- Node names are PascalCase. Files and folders are snake_case.

## 3. Dependencies: no reaching out

**A node never looks up a node outside its own subtree by path.** No
`get_node("../../World")`, no `/root/Main/...`, no `get_parent()` chains, no
group lookups to find a single collaborator. Scenes must work wherever they are
instanced.

When a node needs something external, it exposes the dependency and lets the
owner provide it, in this order of preference (scene_organization.rst):

1. **Signal** (child → owner), to *respond* to something. Use past-tense names:
   `world_entered`, `teleported`, `notice_changed`, `hint_changed`.
2. **Method call** (owner → child), to *start* behaviour: `hard_set(pos)`,
   `show_notice(text, is_error)`, `register_camera(cam)`.
3. **Node reference property** (`#[export] Option<Gd<T>>`), when the node has to
   query something every frame (the world model, the net session, the render
   focus).

Who sets a reference property:

- Inside one scene file: the scene wires it with `node_paths=PackedStringArray(...)`.
  This covers a component pointing at its entity root (`NodePath("..")`) or at a
  sibling component. The owning scene is the mediator, so siblings stay
  decoupled.
- Across scenes: the instancing scene sets the exported property on the
  **instance root** (in `main.tscn`, for example). The entity root then forwards
  it to its own components. Never set properties on an instanced scene's
  internal nodes (no "editable children" wiring).
- At runtime (spawners): the spawner injects dependencies right after
  `instantiate()` and before or right after `add_child`.

Keep data in-house wherever you can. Each external dependency is a contract.
A node whose required dependency is missing logs one `log::warn!` naming the
property and degrades gracefully. Don't panic. Don't use `OnEditor<Gd<T>>`
for dependencies that are optional in dev scenes: it panics before `ready()`.
Don't make a class `#[class(tool)]` just to get `get_configuration_warnings`,
because tool classes run their callbacks inside the editor.

Before reaching for an **autoload**, check that the system tracks all of its
own data, has to be globally reachable, and lives in isolation. If it modifies
other systems' data, it is a regular node that gets injected. This client has no
autoloads of its own (the addons register theirs).

## 4. Ownership and process order

- The entity root is the scene's public API and mediator. Outside code talks to
  the root, never to its components. The root relays component signals as its
  own signals when outsiders need them.
- `Main` connects signals between branches, for example
  `Net.world_entered → LocalPlayer.hard_set` and
  `LocalPlayer.teleported → Camera2DPresenterComponent.snap`. Keep `Main` a
  wiring hub with no game logic.
- When per-frame order matters across nodes, set `process_priority` explicitly
  and document it in the module docs. The render-frame owner, for example, runs
  at `-10` so renderers read the re-based frame. Don't rely on tree order
  silently.

## 5. Scenes versus code

- Each game concept is a `.tscn`: entities, widgets, and any component with child
  nodes or look-and-feel. Composition and styling go in the scene: sprite
  children, scales, theme overrides, anchors, default exports.
- A component with no children can be a bare registered class added in the
  scene as a typed node (`type="KgPositionSyncComponent"`). It doesn't need its
  own `.tscn`.
- Build nodes in code only for data-driven, variable-count content (per-patch
  layers, per-entity sprites, pooled nodes). Every such site carries
  `// code-instantiated: <reason>`. If a fixed structure is built in code, it
  probably belongs in a scene.
- For many small things that don't need the tree (caches, per-hex data, terrain
  LRU), use plain Rust structs, or `RefCounted`/`Resource` when Godot has to see
  them, not nodes (node_alternatives.rst).

## 6. gdext specifics

- Node references: `#[export] name: Option<Gd<KgFoo>>` (gdext exports
  `Option<Gd<T>>` for node classes; in `.tscn` it is a `NodePath` listed in
  `node_paths`). Check `is_instance_valid()` before using a reference to a node
  that can be freed.
- `#[export]` generates `set_<field>`/`get_<field>`: name your own Rust setters
  differently (`inject`, `inject_assets`) or the impls collide.
- Signals: declare with `#[signal] fn world_entered(q: i32, r: i32);` inside a
  `#[godot_api] impl` and emit with `self.signals().world_entered().emit(q, r)`.
  Parameters must be Godot-convertible (`i32`, `GString`, `Vector2`, `Gd<T>`),
  not Rust domain types. **Connect by method name** to a `#[func]`
  (`wiring::connect_once(&source, "signal", &target, "method")`), not with a
  Rust closure: name-based connections survive a library reload, and the
  `is_connected` guard makes re-wiring from a reload handler idempotent.
- Borrows: never hold `bind()` or `bind_mut()` across a call that can emit a
  signal or re-enter Rust (`set`, `call`, `emit`, `add_child`, `queue_free`).
  Copy the data out, drop the guard, then call out. A signal connected back
  into the emitter's own entity is the classic double-borrow panic. Two
  patterns prevent it:
  - a `#[func]` reached through a signal that then drives components takes the
    handle instead of `&mut self`: `#[func(gd_self)] fn hard_set(this: Gd<Self>, ..)`;
    root methods called by spawners likewise take `&mut Gd<Self>` and release
    their own borrow before calling a component;
  - a component's methods called by its owner never emit; the owner emits
    afterwards. Components emit only from their own `process`/`input`.
- Hot reload: `reloadable = true` reloads the library **in the editor** only, and
  runtime classes (no `#[class(tool)]`) are placeholders there, so their
  callbacks never run in the editor at all. Still keep a node's cached state
  rebuildable from an idempotent `bootstrap()` called by `ready()` and
  `on_notification(EXTENSION_RELOADED)`, and keep injected dependencies in
  `#[export]` fields (Godot restores properties on reload).
- Injection timing: an entity root's `ready()` runs **after** its children's
  `ready()`, so the root forwards its dependencies to its components from its
  own `ready()` and components accept late injection (an `inject(..)` method
  that applies immediately, or work that only happens in `process`/`input`).
  To push a value into children *before* their `ready()`, use the parent's
  `enter_tree()`: exported node references wired in the `.tscn` are already
  resolved there (`KgMain` configures `WorldModel` and `Net` this way).
- Spawners: `instantiate()`, set the exported dependencies on the new root,
  `add_child` (its `ready()` forwards them), then call its configure method.
- Per-frame order inside one entity: if a component's result must be consumed
  in the same frame by its owner, either give the component a
  `process_priority` (document it) or let the owner call it from its own
  `process` (`RemotePlayer` advances its `InterpolationComponent` this way).
- Logging goes through `log::*`. Class names and `#[func]`/`#[signal]` names
  are the scene ABI: when you rename one, update every `.tscn` that uses it in
  the same change.

## Editor classes (kg_editor)

`client/rust/kg_editor` is a separate, editor-only cdylib (the KG dev dock,
`client/rust/kg_editor/AGENTS.md`). Different rules apply there:

- Every class is `#[class(tool)]`, so its callbacks **do** run in the editor,
  and it may use editor singletons (`EditorInterface`, `EditorSettings`,
  `EditorPlugin` APIs). The "don't make a class `tool`" rule in section 3 is
  for `kg_gdext` only.
- `kg_gdext` and the game scenes must never reference a `kg_editor` class (it
  is not loaded in a game run or an export), and `kg_editor` never depends on
  `kg_gdext`.
- The dock UI is built in code (v1, no `.tscn`): every code-built node carries
  `// code-instantiated: <reason>`. Panels are `Kg*Panel` classes registered by
  one `add_tab::<KgFooPanel>()` line in `KgDevDock::register_panels`; the tab
  title equals the node name (`DockPanel::TITLE`).
- Docking uses the 4.6+ `EditorDock` API (`EditorPlugin::add_dock` /
  `remove_dock`, `default_slot = DOCK_SLOT_BOTTOM`), not the deprecated
  `add_control_to_bottom_panel` / `add_control_to_dock`.
- The dock injects its services (`DockContext { jobs }`, the shared
  `KgJobService`) into each panel via `DockPanel::attach`; panels never call
  the dock or look each other up. Panels connect to the service's signals by
  method name; the same borrow rules as section 6 apply.

## 7. Checklist before you finish

- [ ] No `get_node`/`try_get_node_as` with `..` or `/root` in new code. Paths
      only go into the node's own subtree, and exported references are preferred.
- [ ] Every component name ends in `Component`, `Component2D` or `Component3D`
      to match its base type. Class name = `Kg` + node name. File = snake_case.
- [ ] Dependencies are exported properties, method calls or signals, and the
      owner wires them (`.tscn` `node_paths`, entity-root forwarding, or `Main`).
- [ ] Siblings don't know each other unless the owning scene wires them.
- [ ] Structure and styling live in a `.tscn`. Code-instantiated nodes carry the
      comment.
- [ ] `EXTENSION_RELOADED` rebuilds state and re-forwards dependencies;
      signal connections are by method name (`wiring::connect_once`).
- [ ] The scene runs standalone: open it alone (or the dev scene that hosts it)
      and it logs one warning per missing dependency instead of crashing.
- [ ] `tools/ci.sh --client` passes, or the scene change is recorded in
      `CI_PENDING.json` (`tools/dev/ci-pending.sh add`). The check includes the
      headless Godot import, which catches broken `.tscn` class names and paths.
