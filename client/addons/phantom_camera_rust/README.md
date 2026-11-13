# Phantom Camera (Rust port)

Rust GDExtension port of [Phantom Camera](https://github.com/ramokz/phantom-camera) `0.11.0.3`
(MIT, Marcus Skov) for Godot 4.7, shipped as a self-contained second GDExtension library next to
`mind-gdext`.

The addon is consumed by the client: `client/project.godot` autoloads `PhantomCameraManager` from
`phantom_camera_manager.tscn`, and scenes name the native classes directly (`type="PhantomCamera2D"`,
…). The GDScript/C# upstream tree is no longer in this checkout; the Rust port owns the canonical
class names.

## Install / build

```bash
bash tools/build.sh                                  # also builds phantom-camera-rust
godot4 --headless --path client --editor --quit       # refresh .godot/extension_list.cfg
godot4 --headless --path client res://addons/phantom_camera_rust/probe_check.tscn
```

Library: `res://bin/rust/{debug,release}/[lib]phantom_camera_rust.{dll,so,dylib}`.

## Classes

| Class | Base | Notes |
|---|---|---|
| `PhantomCamera2D` | `Node2D` | `NONE`/`GLUED`/`SIMPLE`/`GROUP`/`PATH`/`FRAMED`, damping, lookahead, dead zones, limits, noise, tween-on-switch |
| `PhantomCamera3D` | `Node3D` | 2D's modes plus `THIRD_PERSON` (runtime `SpringArm3D`), look-at modes, `Camera3DResource` overrides |
| `PhantomCameraHost` | `Node` | child of the `Camera2D`/`Camera3D`; priority arbitration, activation signal order, tween state machine, per-frame pump |
| `PhantomCameraManager` | `Node` | autoload singleton: registries + 16 fan-out signals; also `Engine.get_singleton("PhantomCameraManager")` |
| `PhantomCameraNoiseEmitter2D/3D` | `Node2D`/`Node3D` | trauma grow → hold → decay state machine |
| `PhantomCameraNoise2D/3D` | `Resource` | `FastNoiseLite` perlin, positional/rotational multipliers |
| `PhantomCameraTween` | `Resource` | `duration`/`transition`/`ease`, ordinals equal to Godot's `Tween` enums |
| `PhantomCameraTweenDirector` | `Node` | data container consulted by the host |
| `TweenDirectorResource` | `Resource` | From/To matching for conditional tweens |
| `Camera3DResource` | `Resource` | `Camera3D` overrides applied on activation |
| `PcamRustProbe` | `Node` | port/upstream version + registered-class list (loading smoke check) |
| `PcamRecorder` (dev) | `Node` | per-frame camera telemetry recorder for offline debugging |

All exported property names, defaults, signal names/arities, `#[func]` signatures and enum integer
values match the upstream GDScript addon.

## Scope

Ported: every runtime behavior the 18 upstream demo scenes exercised, including the tween/easing
math (Godot's `Tween.interpolate_value` table, verified numerically) and `_smooth_damp`.

Not ported, deliberately:

- **Editor layer** — the inspector plugin, the 3D gizmo plugins, the bottom-panel viewfinder dock,
  the self-updating updater panel, themes and icons. None of it is on a game path, and it is all
  editor UI rather than camera behavior. `show_viewfinder_in_play` exists as a stored property and
  is inert.
- **`@tool` niceties** — `_validate_property` inspector hiding, `_get_configuration_warnings`,
  frame-preview `_draw`, gizmo line drawing, the "Align with view" tool buttons.
- **C# facade files** — the upstream `.cs` files were never Godot scripts and are not compiled in
  this project.

Behavioral quirks preserved on purpose (parity includes bugs): the 3D `FRAMED` dead branch whose
"rotation changed" comparison is unreachable, the host tween diff reading two wrong `_prev_cam_*`
values, the inert `keep_aspect` override, and 2D `FRAMED`'s incremental (not absolute) dead-zone
mutation. Each is marked with a `// parity:` comment naming the upstream line.

## Parity status

The in-engine port was validated against the upstream GDScript addon with a dedicated harness
(`tools/pcam_parity.sh`, per-frame telemetry + signal-stream goldens under `parity/pcam_golden/`).
That harness and the vendored GDScript oracle were retired when the repository dropped GDScript and
C# sources; the recorded results stay in git history (`tools/pcam_parity.sh`,
`parity/pcam_golden/`, `parity/pcam_parity_report.md`). The Rust port is the sole implementation now.

## Layout

| Path | What |
|---|---|
| `phantom_camera_rust.gdextension` | library mapping |
| `phantom_camera_manager.tscn` | the autoload node |
| `probe_check.tscn` | library-loaded smoke check (`PcamRustProbe`) |
| `PORT-PLAN.md` | port contract (contributor rules: `AGENTS.md`) |
