# AGENTS.md — client/rust/ (Cargo workspace)

The Rust side of Mindustry-Godot: one Cargo workspace that implements essentially all behavior —
the deterministic simulation, content, world, IO, assets, the Godot 4.7 GDExtension, the
SpacetimeDB client, the headless oracle/dedicated server, and the offline asset CLI. The Godot tree
in `../` owns scenes and GDScript UI; behavior lives here. Read the root
[`AGENTS.md`](../../AGENTS.md) first; the client-level map is [`client/AGENTS.md`](../AGENTS.md).
This file owns the workspace shape — members, dependency direction, lints, toolchain and workspace
commands. Crate-internal detail lives in the crate guides below.

## Layout

| Crate | Responsibility | Local guide |
|---|---|---|
| `mind-core` | Godot-free, tokio-free simulation core (`content`, `world`, `sim`, `io`, `game`, `entities`, `logic`, `render` data, `combat`, `ai`, `assets`, `maps`, `mods`). Owns all game rules. | [`mind-core/AGENTS.md`](mind-core/AGENTS.md) |
| `mind-headless` | Library + binary test-rig/oracle: scenario registry, goldens and checksums, bench harness, and the dedicated `server`/`serve` modes. Never links Godot. | [`mind-headless/AGENTS.md`](mind-headless/AGENTS.md) |
| `mind-gdext` | Godot 4.7 GDExtension `cdylib`: the `Mind*` classes, fixed-step frame pump, view drivers, input, audio, FX, net and platform glue. Holds no game rules. | [`mind-gdext/AGENTS.md`](mind-gdext/AGENTS.md) |
| `mind-stdb` | SpacetimeDB client facade: connector lifecycle, subscription waves, typed table binders, checksums and the ordered command relay. | [`mind-stdb/AGENTS.md`](mind-stdb/AGENTS.md) |
| `mind-tools` | Offline asset pipeline CLI (`migrate`, `pack`, `icons`, `sounds`, `shaders`, `determinism`, `mods classmap`); library half shared with tests. | [`mind-tools/AGENTS.md`](mind-tools/AGENTS.md) |
| `mind-atlas` | Pure-Rust sprite atlas library: pixmaps, packer, autotile, ninepatch, manifests, PNG IO. | — |
| `mind-macros` | Proc macros for sim component/entity metadata (`SimComponent`, `entity_def!`, `LoadRegions`); emits metadata impls only. | — |
| `mind-derive` | Proc macros for entity IO codegen (`EntityIo`, `ObjectiveFields`), revision-aware save/sync read and write chains. | — |

`mind-atlas`, `mind-derive` and `mind-macros` have no crate guide; their `src/lib.rs` docs are the
map. Deep `mind-core` subsystem notes live under `mind-core/src/*/AGENTS.md` and are indexed by the
`mind-core` guide. `Cargo.lock` is checked in; `.gdignore` keeps Godot from importing this tree.

## Dependency direction

- **One-way.** `mind-core` depends only on `mind-derive`, `mind-macros` and plain libraries.
  `mind-headless` and `mind-tools` depend on `mind-core`; `mind-headless` additionally on
  `mind-atlas` and `mind-stdb`; `mind-tools` also on `mind-atlas`. `mind-gdext` depends on `godot`,
  `mind-core` and `mind-stdb`. `mind-stdb` depends on `spacetimedb-sdk` and no internal crate. The
  proc-macro crates depend on no internal crate.
- **`default-members = ["mind-core", "mind-headless", "mind-stdb", "mind-tools"]`.** A bare
  `cargo build`/`cargo test` at the workspace covers those host-friendly crates; target `mind-gdext`
  (a `cdylib`), `mind-atlas` and the proc-macro crates explicitly with `-p`.
- `mind-headless` is both the oracle for `mind-core` and the standalone dedicated server
  (`server`/`serve`). The in-game host/join path is `mind-gdext`'s `MindNet` over `mind-stdb`.
- Generated STDB bindings live in `mind-stdb/src/module_bindings/`; regeneration, publishing, the
  drift gate and the no-hand-edit rule are owned by [`server/AGENTS.md`](../../server/AGENTS.md).
  The client-side contract is in the `mind-stdb` guide.

## Lints and toolchain

- Workspace lints deny `unsafe_code`, clippy `all`, `unwrap_used` and `expect_used`; every crate
  opts in with `[lints] workspace = true`. `mind-gdext` sets `[lints.rust] unsafe_code = "allow"` so
  GDExtension glue can use `unsafe`; keep those blocks confined to the glue.
- Crates that need test convenience opt tests out of `unwrap`/`expect` with a crate-level
  `cfg_attr(test, ...)` (`mind-core`, `mind-headless`, `mind-tools`); `mind-stdb`'s integration test
  allows `expect` at the test-crate root. Library code stays lint-clean.
- **`mind-core` stays Godot-free and tokio-free.** The dependency/source boundary gate (and its
  commands) is owned by [`mind-core/AGENTS.md`](mind-core/AGENTS.md).
- `[workspace.package]`: `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.95"`,
  `license = "GPL-3.0-only"`; each crate inherits via `version.workspace = true` and friends.
- The toolchain is pinned by the repo-root `rust-toolchain.toml` (`channel = "1.98.1"`, `rustfmt`
  and `clippy`).
- Pinned workspace dependencies include `bevy_ecs = "=0.19.1"`, `godot = "0.5.5"` (`api-4-7`),
  `spacetimedb-sdk = "=2.10.1"`, `png = "=0.17.16"`, `sha2 = "=0.10.8"`, plus `serde`,
  `serde_json`, `thiserror`, `log`, `indexmap`, `smallvec`, `xxhash-rust` (`xxh3`), `flate2`
  (`rust_backend`), `syn`/`quote`/`proc-macro2`, `clap`, `anyhow`, `tokio`, `criterion`, `zip` and
  `data-encoding`.

## Rules

- Add external crates through `[workspace.dependencies]` in `Cargo.toml` and reference them with
  `crate.workspace = true` in each crate's `[dependencies]` table.
- Do not relax the boundary rules to land a feature; move shared logic into `mind-core` instead.
- Keep `mind-core` runnable with a plain `cargo test -p mind-core` and no network or Godot present.
- File headers, SPDX and upstream citation follow the root guide; do not restate per-crate variants
  that the crate guides own.

## Verification

Run from the repo root (Linux).

```bash
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo check --manifest-path client/rust/Cargo.toml --workspace
cargo test --manifest-path client/rust/Cargo.toml -p mind-core

# Oracle/golden/bench commands and the pinned spine checksum: mind-headless/AGENTS.md
# GDExtension + oracle build into client/bin/rust, then scenario sync
bash tools/build.sh

# Full local gate (composition: tools/AGENTS.md)
bash tools/ci.sh
```
