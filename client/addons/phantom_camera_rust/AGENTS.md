# AGENTS.md — addons/phantom_camera_rust/ (Rust Phantom Camera addon)

The shipped Phantom Camera addon: the second GDExtension library plus its `PhantomCameraManager`
autoload. Read the root [`AGENTS.md`](../../../AGENTS.md) first; the project map is
[`client/AGENTS.md`](../../AGENTS.md), the crate is
[`rust/phantom-camera-rust/`](../../rust/phantom-camera-rust/AGENTS.md), and the port contract is
[`PORT-PLAN.md`](PORT-PLAN.md).

## Origin

Port of [Phantom Camera](https://github.com/ramokz/phantom-camera) 0.11.0.3 (MIT, Marcus Skov),
formerly vendored GDScript at `client/addons/phantom_camera/`. The GDScript tree is no longer in
this checkout: the Rust port owns the canonical class names (`PhantomCamera2D`, …), so the
GDScript `class_name`s would hide the native classes if both were imported. The crate carries the
upstream MIT attribution, and the removed GDScript/C# tree remains recoverable from git history.

## Layout

| Path | Responsibility |
|---|---|
| `phantom_camera_rust.gdextension` | Second GDExtension: `entry_symbol = "gdext_rust_init"`, libraries at `res://bin/rust/{debug,release}/[lib]phantom_camera_rust.*`. Built by `tools/build.sh`. |
| `phantom_camera_manager.tscn` | `[autoload] PhantomCameraManager` root node (`type="PhantomCameraManager"`). Must be the first autoload so it exists before any scene node enters the tree. |
| `probe_check.tscn` | `PcamRustProbe` root node: the library-loaded smoke check `tools/ci.sh` runs. |
| `PORT-PLAN.md` | The frozen ABI rules, gdext idioms and module ownership. |

## Invariants

- **The scene ABI is frozen.** Native class names, exported property names/types/defaults, signal
  names/arities and enum integer values equal the upstream addon's. Changing a name breaks scenes
  that store properties as strings. The reference table is frozen in `PORT-PLAN.md` and the crate's
  `src/abi.rs`; there is no in-tree oracle to regenerate it from.
- **One GDExtension per library.** Never fold these classes into `mind-gdext`; the whole point is
  that the addon is a self-contained second library that can be dropped or swapped.
- **No GDScript or C# in this tree.** The shipped implementation is `rust/phantom-camera-rust/`;
  every addon-side file is `.rs`, `.tscn`, `.gdextension` or documentation.
- Node references exported to scenes are `Option<Gd<T>>` or `Array<Gd<T>>`; plain `NodePath` for the
  upstream `NodePath` exports. Do **not** use `OnEditor<Gd<T>>`: its `Deref` panics when unset, and
  the port's getters read those fields. See `PORT-PLAN.md`.

## Verification

Run from the repo root.

```bash
bash tools/build.sh                                                    # builds this cdylib too
godot4 --headless --path client --editor --quit                        # refresh extension_list.cfg
godot4 --headless --path client --quit-after 5 res://addons/phantom_camera_rust/probe_check.tscn
```
