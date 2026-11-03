# Phantom Camera → Rust: port plan and contract

SPDX-License-Identifier: GPL-3.0-only

Port of the vendored [Phantom Camera](https://phantom-camera.dev) addon
(v0.11.0.3, MIT, Marcus Skov) from GDScript to Rust as a second GDExtension
library beside `mind-gdext`.

## Why the oracle moved

GDScript `class_name PhantomCamera2D` **hides** a native (GDExtension) class of
the same name: the two cannot be imported into one project. The Rust port owns
the canonical class names, so the GDScript tree is preserved unmodified at
`parity/pcam_oracle/phantom_camera/` — outside the Godot project root, so it is
never imported — and is re-installed only for a baseline capture by
`tools/pcam_parity.sh`.

## Layout

| Path | Role |
|---|---|
| `client/rust/phantom-camera-rust/` | The crate (`cdylib`, lib name `phantom_camera_rust`). |
| `client/addons/phantom_camera_rust/` | The addon: `.gdextension`, `examples/` (ported demos), `dev/` (ABI dump + harness scenes), this file. |
| `parity/pcam_oracle/phantom_camera/` | The GDScript oracle (read-only, not imported). |
| `client/bin/rust/{debug,release}/[lib]phantom_camera_rust.{dll,so,dylib}` | Built library (gitignored) referenced by the `.gdextension`. |

## Hard rules

1. **ABI is frozen.** Exported property names, signal names/arities, `#[func]`
   names and enum integer values match `parity/pcam_oracle/abi_gd.json` exactly
   (regenerate with `client/addons/phantom_camera_rust/dev/abi_dump.gd`). The 18
   example scenes are the contract: they name classes and store properties as
   strings.
2. **gdext 0.5.5 idioms.** Exports are `#[export]` with an optional
   `#[var(set = setter_fn)]` above it; the setter must be a `#[func]`. There is
   **no** `#[export(set = …)]` (unrecognized key). Enum-typed exports are plain
   `i64` (the oracle's property type is `int`); the Rust enum lives in `abi.rs`
   for internal use. Nodes/resources use `#[class(base=…)]` + manual
   `fn init(base)` — `#[class(init)]` collides with a manual `init`.

   Node references (verified against Godot 4.7.2): the oracle's
   `@export var follow_target: Node2D`, stored in `.tscn` as
   `follow_target = NodePath("…")`, round-trips into **either**
   `Option<Gd<Node2D>>` or `OnEditor<Gd<Node2D>>`; this port uses
   `Option<Gd<T>>` because `OnEditor`'s `Deref` panics when uninitialized.
   `Option<OnEditor<Gd<T>>>` does not compile. Node-reference arrays are plain
   `Array<Gd<T>>` (`Array<OnEditor<…>>` does not compile). Plain `NodePath`
   exports round-trip verbatim.

   **Class enums cannot be registered** by gdext 0.5.5 (`ConstantKind::Enum` is
   never constructed by the macro), so `pcam.FollowMode.THIRD_PERSON` cannot
   resolve against a native class. The one demo-driver line that used it is a
   literal `6` in *both* the oracle and the port copy, so both legs run the same
   driver.
3. **No `log` crate.** A second cdylib has its own `log` sink, so records never
   reach `mind-gdext`'s bridge. Use `godot_print!` / `godot_warn!` /
   `godot_error!` from `godot::global`.
4. **Lints.** `clippy::all`, `unwrap_used`, `expect_used` are `deny`; no
   `panic!`/`todo!` in shipped paths. Use `unwrap_or`, `map_or`, `let … else`,
   and saturating/checked arithmetic. `unsafe` is allowed only for GDExtension
   glue (there should be none in this crate).
5. **Every file carries** `// SPDX-License-Identifier: GPL-3.0-only` and a `//!`
   module doc citing the upstream GDScript file it ports, with line anchors for
   each algorithm.
6. **Node created in code** needs a `# code-instantiated: <reason>` comment.
7. **Never edit** `src/lib.rs`, `Cargo.toml`, `client/rust/Cargo.toml`,
   `tools/build.sh` or another module's file. One owner per file.

## Module ownership

| File | Godot classes | Ports (oracle) |
|---|---|---|
| `src/abi.rs` | — | `phantom_camera_constants.gd`, all enum tables, shared helpers |
| `src/math.rs` | — | `_smooth_damp`, Godot `Tween.interpolate_value` easing |
| `src/resources.rs` | `PhantomCameraTween`, `Camera3DResource`, `PhantomCameraNoise2D`, `PhantomCameraNoise3D`, `TweenDirectorResource` | `scripts/resources/*.gd` |
| `src/manager.rs` | `PhantomCameraManager` | `scripts/managers/phantom_camera_manager.gd` |
| `src/tween_director.rs` | `PhantomCameraTweenDirector` | `scripts/phantom_camera/phantom_camera_tween_director.gd` |
| `src/emitters.rs` | `PhantomCameraNoiseEmitter2D/3D` | `scripts/phantom_camera/phantom_camera_noise_emitter_*.gd` |
| `src/pcam2d.rs` | `PhantomCamera2D` | `scripts/phantom_camera/phantom_camera_2d.gd` (1968 L) |
| `src/pcam3d.rs` | `PhantomCamera3D` | `scripts/phantom_camera/phantom_camera_3d.gd` (2477 L) |
| `src/host.rs` | `PhantomCameraHost` | `scripts/phantom_camera_host/phantom_camera_host.gd` (1473 L) |
| `src/dev/` | harness nodes | — (no oracle counterpart) |

## Cross-class contract

The oracle resolves its manager through `Engine.get_singleton("PhantomCameraManager")`
and `/root/PhantomCameraManager`; hosts and emitters do the same. The Rust port
keeps the engine singleton name but **also** exposes the manager as a Rust
`&'static` slot so a host never depends on node-tree lookup order:

```rust
// src/manager.rs
pub fn active() -> Option<Godot<PhantomCameraManager>>;   // the registered instance
```

The oracle's `get_class()`/`is_class()` overrides cannot exist in gdext, and the
manager's 2D/3D split depends on them. Both classes therefore expose

```rust
#[func] fn pcam_kind(&self) -> GString;   // "2d" | "3d"
```

and the manager registers through `pcam_added(kind, node)`-style dispatch instead
of `is_class`. Scene-facing behaviour is unchanged; only the internal
discriminator differs.

Cross-class calls go through the concrete Rust types (no duck-typed `call()`),
except in `dev/` harness nodes.

## Parity harness

`tools/pcam_parity.sh` drives three modes over the ported demo scenes:

- `--baseline` — installs the oracle, disables this library, records goldens into
  `parity/pcam_golden/`, restores.
- `--candidate` — enables this library, records into `client/rust/target/pcam/`.
- `--compare` — diffs candidate against goldens; per-frame camera telemetry
  (position/rotation/zoom/quaternion/fov/limits) plus the emitted-signal stream.
  Exit `0` pass, `1` mismatch, `2` setup error.

Recording is deterministic: `dev/recorder.rs` drives a fixed tick count with a
canned input script and writes one JSON line per frame.

## Verification

```bash
cargo build --manifest-path client/rust/Cargo.toml -p phantom-camera-rust --target-dir client/bin/rust
cargo clippy --manifest-path client/rust/Cargo.toml -p phantom-camera-rust --all-targets -- -D warnings
godot4 --headless --path client --editor --quit          # refresh extension_list.cfg
godot4 --headless --path client res://addons/phantom_camera_rust/dev/probe_check.tscn
bash tools/pcam_parity.sh --compare
```
