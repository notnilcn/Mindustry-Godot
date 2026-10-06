# AGENTS.md — mind-core/src/input (input, placement & RTS)

`mind-core/src/input` is the Godot-free, tokio-free input layer of Mindustry-Godot: the pure
`InputHandler` state machine, placement/line algorithms, the client build-plan mirror and preview,
RTS unit/building selection and command emission, cursor and focus guards, desktop/mobile decision
trees, camera-rig math and deterministic input logging/replay. It holds device-independent data and
algorithms only. Read the root [`AGENTS.md`](../../../../../AGENTS.md) first. The crate map is
`mind-core/AGENTS.md` (`../../AGENTS.md`).

## Layout

| File | Responsibility |
|---|---|
| `mod.rs` | Module root; declares every submodule and re-exports the public input surface. |
| `action.rs` | `RemoteAction` (every client mutation once) with `CommandTarget`, `InventoryKind`, `PayloadAction`, `name()`/`relayed()`. |
| `binding.rs` | `KeyBindTable`/`BINDS` registry (`KeyKind`, `Category`, `BindingDefault`, `ids`) and the client-local `BindingState` persisted through `SettingsStore`. |
| `keycode.rs` | Arc `KeyCode` display-name table (`KeyCode.getName` -> `Input.getKeyName`) consumed by the keybind dialog's key column. |
| `camera_state.rs` | `CameraState` rig: zoom clamps, `scale_camera`, pan/follow/cutscene, minimap mapping, `CameraView` and `ShakeState`. |
| `caps.rs` | `InputCaps` trait abstracting settings/focus/scene reads, plus the headless `TestCaps` double. |
| `client_input.rs` | `InputState` (`InputHandler` fields), control groups, `update_line`, `refresh_preview`, `flush_plans`, `break_rect`. |
| `command_emit.rs` | `ActionBatcher`/`ActionBatch` turning one `RemoteAction` into ordered `SimCommand`s, `ActionError`, chunk caps. |
| `cursor.rs` | `CursorKind`/`CursorContext` and the `resolve_cursor` precedence chain. |
| `desktop.rs` | `DesktopController`: scroll/rotate gating, drag lines, break rect, control groups, unit selection and `command_tap`. |
| `focus.rs` | `FocusState`, `InputLocks`, `LockId` and the combined `FocusGuards` (`locked`/`can_place`/`can_shoot`). |
| `line.rs` | `iterate_line`/`PlaceLine`/`LineParams`, the `LineHooks` block seam, `rotate_plans`, `flip_plans`. |
| `mobile.rs` | Arc `GestureDetector`/`GestureEvent` and `MobileController` with its touch decision tree. |
| `place_mode.rs` | `PlaceMode` and the mobile-only `MobileMode` flags. |
| `placement.rs` | `PlacementWorld`, line/area normalization, A* (`ASTAR_NODE_LIMIT`), node spacing, the bridge DP and chained upgrade walking. |
| `plan.rs` | `ClientPlan`/`PlanCopy`, insertion-ordered `PlanTree`, `PlanMirror`, `PreviewState` and its JSON handoff. |
| `queue.rs` | `BuildQueue`/`AddOutcome`, the minimal `BuilderComp.plans` insertion/replacement seam. |
| `replay.rs` | `ReplayWorld`, `ReplayHarness`, `MobileReplayHarness` deterministic replay drivers. |
| `rts.rs` | `SelectableUnit`/`SelectableBuilding`, rect/tap/typed selection, `enemy_unit_at` and `command_units_apply`. |
| `sync.rs` | `WireConfig`/`PlanWire`/`PlanSnapshot`, `build_plan_snapshots`, `PlanSnapshotTimer`, `PlayerInputSync`. |
| `input_log.rs` | Versioned JSONL input log (`RawEvent`, `InputHeader`, `InputRecord`, `InputLog`, `InputReplay`). |

## Key types

- State and actions: `InputState`, `PlaceMode`, `MobileMode`, `RemoteAction`, `CommandTarget`,
  `ActionBatcher`/`ActionBatch`, `BuildQueue`, `ClientPlan`/`PlanTree`/`PlanMirror`/`PreviewState`.
- Platform seams: `InputCaps`/`TestCaps`, `PlacementWorld`, `BridgePlacer`, `LineHooks`, `FocusGuards`.
- Controllers: `DesktopController`, `MobileController`/`GestureDetector`/`GestureEvent`, `CameraState`.
- RTS and sync: `SelectableUnit`, `SelectRect`, `command_units_apply`, `PlanWire`, `PlayerInputSync`.

## Invariants

- **Godot-free and tokio-free.** No source here may `use godot`/`use tokio`; the world is read through
  `PlacementWorld` and host settings through `InputCaps`, so everything runs under `cargo test -p mind-core`.
- **No ECS system, no checksum.** Input state is client-local (`I1`–`I3`): this module registers no
  `bevy_ecs` system and none of it enters `determinism::Checksum`.
- **View never feeds sim.** Gestures only produce `RemoteAction`s; mutations cross into simulation as
  `SimCommand`s or through `BuildQueue`, never as direct sim writes.
- **Host owns translation.** Godot key/mouse/touch translation, cursor application and settings
  persistence live in `mind-gdext::input`; this folder owns the platform-neutral branch logic.
- **Determinism.** A* orders its heap by `(f, packed_pos)`; `PlanTree` iterates in insertion order;
  only client-local clocks (`clock_ms`, snapshot timers) are used, never sim time.

## Rules

- Add new key bindings to the append-only `BINDS` table and the `ids` module; names are settings and
  bundle keys (parity ABI).
- Keep platform-specific behavior behind `mind-gdext::input`; extend `InputCaps`/`PlacementWorld`
  rather than reading Godot or the world directly.
- Port upstream Mindustry and cite it in file headers; files are UTF-8/LF, GPL-3.0-only.

## Verification

Run from `client/rust/`, or add `--manifest-path client/rust/Cargo.toml` from the repo root.

```bash
cargo test -p mind-core input
cargo clippy -p mind-core
cargo fmt --all -- --check

# boundary gate (must print nothing / find nothing)
grep -RnE 'use (godot|tokio)' client/rust/mind-core/src/input && echo FAIL
```
