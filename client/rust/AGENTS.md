# AGENTS.md — client/rust/ (Cargo workspace)

The Rust side of Mindustry-Godot: one Cargo workspace that implements essentially all behavior —
the deterministic simulation, content, world, IO, assets, the Godot 4.7 GDExtension, the
SpacetimeDB client, the headless oracle/dedicated server, and the offline asset CLI. The Godot tree
in `../` owns scenes and GDScript UI; behavior lives here. Read the root
[`AGENTS.md`](../../AGENTS.md) first. The client-level map is `client/AGENTS.md`.

## Layout

| Crate | Responsibility |
|---|---|
| `mind-core` | Godot-free, tokio-free simulation core (`content`, `world`, `sim`, `io`, `game`, `entities`, `logic`, `render` data, `combat`, `ai`, `assets`, `maps`, `mods`). Owns all game rules. |
| `mind-headless` | Library + binary test-rig/oracle: scenario registry, goldens and checksums, bench harness, parity tooling, and the dedicated `server`/`serve` modes. Never links Godot. |
| `mind-gdext` | Godot 4.7 GDExtension `cdylib`: the `Mind*` classes, fixed-step frame pump, view drivers, input, audio, FX, net and platform glue. Holds no game rules. |
| `mind-stdb` | SpacetimeDB client facade: connector lifecycle, subscription waves, typed table binders, checksums and the ordered command relay. |
| `mind-tools` | Offline asset pipeline CLI (`migrate`, `pack`, `icons`, `sounds`, `shaders`, `determinism`, `mods classmap`); library half shared with tests. |
| `mind-atlas` | Pure-Rust sprite atlas library: pixmaps, packer, autotile, ninepatch, manifests, PNG IO. |
| `mind-macros` | Proc macros for sim component/entity metadata (`SimComponent`, `entity_def!`, `LoadRegions`); emits metadata impls only. |
| `mind-derive` | Proc macros for entity IO codegen (`EntityIo`, `ObjectiveFields`), revision-aware save/sync read and write chains. |

`Cargo.lock` is checked in; `.gdignore` keeps Godot from importing this tree.

## Responsibilities

- **Dependency direction is one-way.** `mind-core` depends only on support crates
  (`mind-derive`, `mind-macros`) and plain libraries. `mind-headless` and `mind-tools` depend on
  `mind-core`; `mind-headless` additionally on `mind-atlas` and `mind-stdb`; `mind-tools` also on
  `mind-atlas`. `mind-gdext` depends on `godot`, `mind-core` and `mind-stdb`. `mind-stdb` depends
  on `spacetimedb-sdk` and no internal crate. The proc-macro crates depend on no internal crate.
- **`default-members = ["mind-core", "mind-headless", "mind-stdb", "mind-tools"]`.** A bare
  `cargo build`/`cargo test` at the workspace covers those host-friendly crates; target `mind-gdext`
  (a `cdylib`), `mind-atlas` and the proc-macro crates explicitly with `-p`.
- `mind-headless` is both the oracle for `mind-core` and, via `Command::Server` / `Command::Serve`,
  the dedicated server shape driven by `mind-gdext`'s `MindNet`.
- Generated STDB bindings live in `mind-stdb/src/module_bindings/`; they are never hand-edited.

## Key types

- Simulation: `mind_core::Sim`, `mind_core::sim::FixedStepRunner`, `mind_core::ecs::MindWorld`,
  `EntitySeq`, `BuildingComp`, `mind_core::command::Command`, `mind_core::determinism::SimCommand`.
- Content and IO: `mind_core::content::ContentRegistry`, `create_base_content`, `MemoryBundle`,
  `MemoryUnlockStore`; `mind_core::io::entity::EntityCodec`; `mind_core::sim::dump` state JSON.
- Godot classes: `mind_gdext::MindSimHost` (`place_block`, `break_block`, `get_state_json`,
  `get_checksum`, `step`, `set_paused`), `MindCamera2D`, `MindTileGrid`, `MindNet`, `MindAudio`,
  `MindRender`; `StdbConnector` / `StdbBinder`.
- SpacetimeDB: `mind_stdb::{Connector, ConnectorEvent, ConnectorState}`, `StdbMode`,
  `TableBinder` / `RowChange`, `CommandStream`, `MatchSession`, `SubscriptionWaves`, `WaveName`.
- Macros: `mind_derive::EntityIo` and `ObjectiveFields` (see `mind-derive/src/lib.rs`);
  `mind_macros` derive/macro entry points.

## Invariants

- **`mind-core` stays Godot-free and tokio-free.** The dependency tree must not contain `godot` or
  `tokio`, and no source under `client/rust/mind-core` may `use godot`/`use tokio`. CI enforces both
  via `cargo tree -p mind-core` and
  `grep -RnE --include='*.rs' 'use (godot|tokio)' client/rust/mind-core`
  (`.github/workflows/ci.yml`).
- Workspace lints deny `unsafe_code`, clippy `all`, `unwrap_used` and `expect_used`.
  `mind-gdext` allows `unsafe` only for GDExtension glue; unit tests opt out of `unwrap`/`expect`
  with a crate-level `cfg_attr(test, ...)`.
- Proc macros emit metadata only (trait impls, field descriptors); they never merge method bodies.
- Entity field order is serialization order: append `#[entity(since = N)]` fields, never reorder;
  `removed_in` tombstones keep older revision read arms compiling.
- `mind-gdext` holds no game rules: every simulation mutation crosses into `mind-core`.

## Rules

- Add external crates through `[workspace.dependencies]` in `Cargo.toml` and reference them with
  `crate.workspace = true` in each `[dependencies]` table.
- Every crate opts into the workspace lints with `[lints] workspace = true`.
- Keep `mind-core` runnable with a plain `cargo test -p mind-core` and no network or Godot present.
- Port Mindustry source and cite it in file headers; new files are LF/UTF-8 and GPL-3.0-only.
- Do not relax the boundary rules to land a feature; move shared logic into `mind-core` instead.

## Conventions

- `[workspace.package]`: `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.95"`,
  `license = "GPL-3.0-only"`; each crate inherits via `version.workspace = true` and friends.
- The toolchain is pinned by the repo-root `rust-toolchain.toml` (`channel = "1.98.1"`, `rustfmt`
  and `clippy`).
- Pinned workspace dependencies include `bevy_ecs = "=0.19.1"`, `godot = "0.5.5"` (`api-4-7`),
  `spacetimedb-sdk = "=2.10.1"`, `png = "=0.17.16"`, `sha2 = "=0.10.8"`, plus `serde`,
  `serde_json`, `thiserror`, `log`, `indexmap`, `smallvec`, `xxhash-rust` (`xxh3`), `flate2`
  (`rust_backend`), `syn`/`quote`/`proc-macro2`, `clap`, `anyhow`, `tokio`, `criterion`, `zip` and
  `data-encoding`.
- Crate-level notes live next to their crates (`mind-stdb/AGENTS.md`, `mind-gdext/AGENTS.md`).

## Verification

Run from the repo root (WSL2 Ubuntu); prefix with `wsl -d Ubuntu -e bash -lc '<cmd>'` from Windows.

```bash
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo check --manifest-path client/rust/Cargo.toml --workspace
cargo test --manifest-path client/rust/Cargo.toml -p mind-core

# Headless oracle (golden spine_place_break -> e53c9277bb8c28d1)
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- run spine_place_break --json
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- bench --ticks 100000

# GDExtension + oracle build into client/bin/rust, then scenario sync
tools/build.sh

# Full local gate (fmt/clippy/tests/goldens/bench/godot/STDB)
tools/ci.sh
```
