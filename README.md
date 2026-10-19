<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Mindustry-Godot

A Godot 4.7 port of Mindustry. The gameplay lives in Rust (`client/rust/`, compiled
into the `mind-gdext` GDExtension); `client/` is the Godot project.

Two build outputs are **not** tracked in git (they are gitignored) and must be
produced before the game can launch:

1. the GDExtension library (`client/bin/rust/<profile>/mind_gdext.dll` on Windows,
   `libmind_gdext.so` on Linux), loaded through `client/mind.gdextension`;
2. the packed runtime asset tree (`assets/sprites/sprites.atlas.json`, the atlas
   pages, and the icon/sound/shader indexes),

plus the generated scenario mirror in `client/scenarios/`.

## Prerequisites

- **Rust** via [rustup](https://rustup.rs). The repo pins toolchain `1.98.1`
  (`rust-toolchain.toml`); rustup installs it automatically on the first `cargo` run.
- **Godot 4.7.x** (4.7.2 recommended):
  - Linux: `godot4`, `godot4-mono`, or `godot` on `PATH`; `tools/godot.sh` also
    honors `GODOT_BIN`.
  - Windows: `godot` on `PATH`, or substitute the full path to `godot.exe`.
- Windows only: the MSVC build tools (the default `x86_64-pc-windows-msvc` Rust
  target needs the Visual Studio linker).

## Linux

One-shot bootstrap (Rust build + asset pack + scenario sync):

```bash
./everything_you_need.sh          # add --release for an optimized library
```

Or step by step:

```bash
tools/build.sh                    # GDExtension + headless oracle -> client/bin/rust, then syncs scenarios
tools/pack.sh pack                # offline asset pipeline -> assets/ (first run takes a few minutes)
tools/godot.sh                    # launch the client (resolves godot4/godot4-mono/godot)
```

`tools/godot.sh` runs the game; add `-e` to open the editor instead.
`./everything_you_need.sh --import` additionally runs the headless Godot
import/parse pass.

## Windows

The `tools/*.sh` scripts assume a Linux/bash host. On Windows, run the equivalent
commands in PowerShell from the repo root:

```powershell
# 1. Build the GDExtension (debug) and the headless oracle into client/bin/rust
cargo build --manifest-path client/rust/Cargo.toml -p mind-gdext -p mind-headless --target-dir client/bin/rust

# 2. Pack the runtime assets into assets/ (release build of the asset pipeline; first run takes a few minutes)
cargo run --manifest-path client/rust/Cargo.toml --release -p mind-tools -- pack

# 3. Mirror the canonical scenarios into client/scenarios/
Copy-Item scenarios\*.json client\scenarios\ -Force

# 4. Launch the game
godot --path client
```

Or open `client/project.godot` in the Godot editor and press F5.

## Rebuilding after changes

| Changed | Rerun |
|---|---|
| Rust code under `client/rust/` | step 1 (the Rust build) |
| Art under `assets-raw/` | step 2 (the asset pack) |
| `scenarios/*.json` | step 3 (the scenario sync) |

## Troubleshooting

- `[assets] missing manifest ... sprites.atlas.json` — the asset pack (step 2) has
  not run yet.
- `Can't open GDExtension dynamic library` or a missing `mind_gdext.dll` /
  `libmind_gdext.so` — the Rust build (step 1) has not run yet. If it has, close
  Godot and delete any stale `~*` temp files in `client/bin/rust/<profile>/`
  (leftovers from an interrupted link or a loaded library).
- A boot-time `MindWorldRenderer ... bind_mut() failed, already bound` panic in
  the log is a known pre-existing issue with no frame-loop impact.
