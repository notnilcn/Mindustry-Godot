# AGENTS.md — addons/phantom_camera_rust/ (Rust Phantom Camera addon)

The shipped Phantom Camera addon: the second GDExtension library plus the ported 2D/3D demo
scenes that double as the parity benchmark. Read the root [`AGENTS.md`](../../../AGENTS.md) first;
the project map is [`client/AGENTS.md`](../../AGENTS.md), the crate is
[`rust/phantom-camera-rust/`](../../rust/phantom-camera-rust/AGENTS.md), and the port contract is
[`PORT-PLAN.md`](PORT-PLAN.md).

## Origin

Port of [Phantom Camera](https://github.com/ramokz/phantom-camera) 0.11.0.3 (MIT, Marcus Skov),
formerly vendored GDScript at `client/addons/phantom_camera/`. That tree now lives at
`parity/pcam_oracle/phantom_camera/` as the read-only parity oracle: its `class_name`s
(`PhantomCamera2D`, …) would hide the native classes if it stayed inside the project, so it moved
outside `client/`. `tools/pcam_parity.sh --baseline` re-installs it temporarily.

## Layout

| Path | Responsibility |
|---|---|
| `phantom_camera_rust.gdextension` | Second GDExtension: `entry_symbol = "gdext_rust_init"`, libraries at `res://bin/rust/{debug,release}/[lib]phantom_camera_rust.*`. Built by `tools/build.sh`. |
| `phantom_camera_manager.tscn` | `[autoload] PhantomCameraManager` root node (`type="PhantomCameraManager"`). Must be the first autoload so it exists before any scene node enters the tree. |
| `examples/` | The 18 ported demo scenes (`example_scenes/2D`, `example_scenes/3D`, `sub_scenes`), `models/`, `ui/`, `resources/tween/`, `textures/`, and `scripts/` — the demo helper GDScript kept **byte-identical** to the oracle so both sides run the same driver. |
| `fonts/` | `Nunito-{Black,Regular}.ttf`, referenced by the demo scenes. |
| `dev/` | Development tools: `abi_dump.gd` (per-class ABI JSON), `easing_dump.gd` (Godot `Tween` easing table), `rewrite_examples.py` / `verify_examples.py` (scene port + check), `record.tscn` (parity recorder scene), `probe_check.tscn` (library-loaded smoke check). |
| `PORT-PLAN.md` | The frozen ABI rules, gdext idioms and module ownership. |

## Invariants

- **The scene ABI is frozen.** Native class names, exported property names/types/defaults, signal
  names/arities and enum integer values equal the oracle's. The demo scenes store properties as
  strings; changing a name breaks them silently. Regenerate the reference table with
  `godot4 --headless --path client --script "res://addons/phantom_camera_rust/dev/abi_dump.gd" -- <out.json>`.
- **The demo scripts are the shared driver.** `examples/scripts/**/*.gd` stays identical to the
  oracle's copies so a parity run differs only in the addon implementation. Do not "port" them to
  Rust and do not reformat them. **One documented exception**:
  `scripts/3D/player_controller_third_person.gd` compares `get_follow_mode() == 6` instead of
  `_player_pcam.FollowMode.THIRD_PERSON`, because gdext 0.5.5 cannot register a class enum
  (`ConstantKind::Enum` is never constructed by the macro). Semantically identical.
- **No GDScript beyond `examples/scripts/` and `dev/`.** Both are addon-internal test/demo tooling;
  the shipped implementation is `rust/phantom-camera-rust/`.
- **One GDExtension per library.** Never fold these classes into `mind-gdext`; the whole point is
  that the addon is a self-contained second library that can be dropped or swapped.
- Node references exported to scenes are `Option<Gd<T>>` or `Array<Gd<T>>`; plain `NodePath` for the
  oracle's `NodePath` exports. Do **not** use `OnEditor<Gd<T>>`: its `Deref` panics when unset, and
  the port's getters read those fields. See `PORT-PLAN.md`.

## Verification

Run from the repo root.

```bash
bash tools/build.sh                                                    # builds this cdylib too
godot4 --headless --path client --editor --quit                        # refresh extension_list.cfg
godot4 --headless --path client --quit-after 5 res://addons/phantom_camera_rust/dev/probe_check.tscn
python client/addons/phantom_camera_rust/dev/verify_examples.py         # no oracle refs left
bash tools/pcam_parity.sh                                              # record + compare vs the oracle
```
