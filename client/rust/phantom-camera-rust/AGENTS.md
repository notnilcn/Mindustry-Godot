# AGENTS.md — phantom-camera-rust (second GDExtension crate)

Rust port of the Phantom Camera 0.11.0.3 addon, compiled as an independent `cdylib` that Godot
loads as a second GDExtension beside `mind-gdext`. Read the root [`AGENTS.md`](../../../AGENTS.md)
and [`client/rust/AGENTS.md`](../AGENTS.md) first; the addon directory that ships it is
[`client/addons/phantom_camera_rust/`](../../addons/phantom_camera_rust/AGENTS.md), whose
[`PORT-PLAN.md`](../../addons/phantom_camera_rust/PORT-PLAN.md) is the port contract.

## Layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Module list, public class re-exports, `REGISTERED_CLASSES`, the `PhantomCameraExtension` `ExtensionLibrary`. |
| `src/abi.rs` | Frozen enum/constant table (integer values are scene ABI) plus the `PcamKind` 2D/3D discriminator and the layer-mask helpers. |
| `src/math.rs` | Godot `Tween.interpolate_value` easing table (11 transitions × 4 eases) and the oracle's `_smooth_damp`. Unit-tested against a dumped Godot table. |
| `src/resources.rs` | `PhantomCameraTween`, `Camera3DResource`, `PhantomCameraNoise2D/3D`, `TweenDirectorResource`. |
| `src/manager.rs` | `PhantomCameraManager` — the registry/host-list/key-signal singleton (`active()` plus `notify_*` seams for the other modules). |
| `src/pcam2d.rs`, `src/pcam3d.rs` | `PhantomCamera2D` / `PhantomCamera3D` — the per-frame rig algorithms (follow modes, damping, lookahead, framed dead zones, limits, third-person spring arm, look-at). |
| `src/host.rs` | `PhantomCameraHost` — priority arbitration, activation signal order, the tween state machine and the per-frame pump that writes the real `Camera2D`/`Camera3D`. |
| `src/tween_director.rs` | `PhantomCameraTweenDirector` — data container consulted by the host. |
| `src/emitters.rs` | `PhantomCameraNoiseEmitter2D/3D` — the trauma grow/duration/decay state machine. |
| `src/dev/` | Parity harness nodes (`PcamRecorder`). No oracle counterpart. |
| `src/probe.rs` | `PcamRustProbe` — library-loaded/class-registered smoke check used by the harness. |

## Invariants

- **ABI is append-only and frozen.** Exported class names, `#[export]` property names, `#[func]`
  signatures, `#[signal]` names/arities and the `abi.rs` integers must equal the oracle. `tests` and
  the demo scenes reference them by string.
- **No `log` crate.** Each cdylib has its own `log` sink, so records never reach `mind-gdext`'s
  bridge: use `godot_print!` / `godot_warn!` / `godot_error!`.
- **gdext 0.5.5 idioms.** `#[var(set = setter_fn)]` above `#[export]` with the setter as a
  `#[func]`; enum exports are `i64`; `#[class(base=…)]` with a manual `fn init(base)`; node
  references are `OnEditor<Gd<T>>` (never `Option<OnEditor<…>>`) or `Array<Gd<T>>`.
- **No panics.** clippy `all`, `unwrap_used` and `expect_used` are denied; the oracle divides by
  zero by design (`growth_time`, `decay_time`, `damping_time`) — guard those explicitly.
- **Parity includes bugs.** Where the oracle's behavior is wrong (dead code that must stay dead,
  previous-value mix-ups in the host tween diff, inert `keep_aspect` override), port the bug and
  record it in a `// parity:` comment with the oracle line anchor.
- **The crate stays dependency-light:** `godot` only. Nothing here may depend on `mind-core`.

## Verification

Run from the repo root.

```bash
cargo build --manifest-path client/rust/Cargo.toml -p phantom-camera-rust --target-dir client/bin/rust
cargo clippy --manifest-path client/rust/Cargo.toml -p phantom-camera-rust --all-targets -- -D warnings
cargo test --manifest-path client/rust/Cargo.toml -p phantom-camera-rust
bash tools/pcam_parity.sh
```
