# AGENTS.md — tools/ (build & verification scripts)

`tools/` holds the repo's developer and CI tooling: compiling the Rust GDExtension and headless oracle, launching Godot, mirroring scenarios, checking and regenerating parity goldens, driving the local server, running the offline asset pipeline, emitting version metadata, installing file associations, and running the local and nightly parity gates. Read the root [`AGENTS.md`](../AGENTS.md) first.

## Layout

| Script (`.sh` authoritative, `.ps1` twin) | Responsibility |
|---|---|
| `build.sh` / `build.ps1` | Build `mind-gdext` + `mind-headless` into `client/bin/rust`, then run `sync_scenarios.sh` when present. |
| `ci.sh` / `ci.ps1` | Full local gate: fmt, clippy, check, `mind-core` tests, parity matrix, goldens, scenario mirror, bench budget, Godot headless import, boundary greps, STDB typecheck + bindings drift. |
| `godot.sh` / `godot.ps1` | Resolve the Godot 4.7 binary (`$GODOT_BIN` → `godot4` → `godot4-mono`) and launch it against `client/` unless `--path` is passed. |
| `parity.sh` / `parity.ps1` | Local driver for the `mind-headless parity …` harness (structural check, mirror, suites, gate reports, bench, soak, checksums). |
| `server.sh` / `server.ps1` | Run `mind-headless server`, or drive a running server's console socket over TCP. |
| `sync_scenarios.sh` / `sync_scenarios.ps1` | Mirror canonical `scenarios/*.json` into the gitignored `client/scenarios/`. |
| `pack.sh` / `pack.ps1` | Forward arguments to the offline asset pipeline `mind-tools` (release). |
| `regen_goldens.sh` / `regen_goldens.ps1` | Verify or regenerate oracle goldens and update `parity/golden_manifest.json` sha256 hashes. |
| `associate.sh` / `associate.ps1` | Opt-in installer for `.msav`/`.msch` file associations and the `mindustry://` / `mindustry-godot://` schemes. |
| `export.sh` / `export.ps1` | Plan-22 export wrapper: checks the matching Godot 4.7.2 export templates, runs a headless export for a platform preset, and optionally verifies the artifact. Outputs default under the gitignored `client/bin/export/`. |
| `version.sh` / `version.ps1` | Write `client/assets/version.properties` (upstream key set) from flags and git state. |
| `mcp-smoke.sh` / `mcp-smoke.ps1` + `mcp_smoke.py` | In-engine MCP spine smoke over a stdio `open-godot-mcp` server; needs the editor already running. |
| `watch.sh` / `watch.ps1` | Watch `client/rust/**` for `.rs`/`Cargo.toml` edits and re-run `build.sh`; ignores `client/rust/target/` and `client/bin/`. |

## Responsibilities

- `build.sh` compiles `-p mind-gdext -p mind-headless` with `--target-dir client/bin/rust` so `mind.gdextension` finds the shared library, then delegates scenario mirroring to `tools/sync_scenarios.sh`.
- `watch.sh` is a developer loop: it watches `client/rust/**` and re-runs `build.sh` on `.rs`/`Cargo.toml` edits, ignoring `client/rust/target/` and `client/bin/` so its own build output never retriggers a rebuild. It prefers `inotifywait` and falls back to a 2s polling loop when unavailable; `watch.ps1` mirrors it with `System.IO.FileSystemWatcher`. It is not part of `ci.sh`.
- `ci.sh` is the single local gate. It runs `cargo fmt --check`, `clippy -D warnings`, `cargo check`, `cargo test -p mind-core`, `parity check --tests`, `parity goldens`, `run-all --tier T0`, the three spine goldens, a scenario-mirror `diff -r`, a release `bench --ticks 100000 --scenario bench_baseline` budget (warn +20% / fail +50% over `BASE_P50_NS=130` / `BASE_P99_NS=301` ns), `build.sh`, a `godot.sh --headless --editor --quit` import/parse check, `mind-core` godot/tokio boundary greps, `cargo check -p mind-stdb`, STDB module typecheck, and `server/build.sh --check`. It does **not** include the MCP smoke.
- `parity.sh` forwards to `mind-headless parity …`: `--check`, `--mirror`, `--suite smoke|gate|full`, `--gate Pn` → `parity/reports/gate_Pn.json`, `--mcp`, `--bench`, `--soak mid|stress`, `--checksums`, `--json`. With no selector it runs `--check` plus the `smoke` suite. Output goes to `PARITY_OUT_DIR` (`client/rust/target/parity` by default).
- `server.sh` uses `MIND_HEADLESS_BIN` (`client/rust/target/debug/mind-headless`), `MIND_SERVER_HOST` (`127.0.0.1`), and `MIND_SERVER_PORT` (`6859`); `--socket "cmd,…"` writes newline commands and prints the reply, and `--serve` starts then tears down the server.
- `sync_scenarios.sh` deletes stale JSON from `client/scenarios/` and copies the canonical set; `client/scenarios/` is a gitignored mirror and is never edited by hand.
- `regen_goldens.sh` defaults to `--check` (reads committed files, no JVM); `--only <id>` / `--all` regenerate via `parity/java/extract_source_goldens.py` and rewrite hashes. `MIND_JAVA_PARITY=1` uses the JVM dumpers in `parity/java/` when present; `MINDY_SRC` overrides the upstream source path.
- `version.sh` writes LF-only keys `type`, `number`, `modifier`, `commitHash`, `buildDate`, `build`; commit hash and date default to the current git state. The output is gitignored.
- `associate.sh` mirrors `mind_core::platform::assoc`: a Linux `.desktop` under `$XDG_DATA_HOME` (plus `update-desktop-database` / `xdg-mime` when available), or a Windows `.reg` imported through `reg.exe` from WSL. `MIND_EXE` overrides the exported binary path.
`export.sh`/`export.ps1` resolve the Godot binary (`$GODOT_BIN` → `godot4`), derive the `<ver>.stable` templates directory (`~/.local/share/godot/export_templates/` on Linux, `%APPDATA%\Godot\export_templates\` on Windows; `GODOT_TEMPLATES_DIR` overrides), and fail with an install hint when it is absent. `--platform`/`-Platform` selects the preset (`windows`→`Windows Desktop`), `--preset`/`-Preset` overrides it, and `--out`/`-Out` selects the artifact path.
- `mcp-smoke.sh` execs `python3 tools/mcp_smoke.py`, which spawns its own stdio `open-godot-mcp` server (bridge `ws://127.0.0.1:6970`) and drives `res://scenes/game.tscn` end-to-end. Flags: `--mcp-bin`, `--repo-root`, `--server-arg`, `--keep-running`, `--verbose`; `OPEN_GODOT_MCP_BIN` overrides binary discovery. Exits `0` pass, `1` assertion/tool failure, `2` bridge not connected.

## Conventions

- The Bash `.sh` scripts are canonical and run on the WSL2 Ubuntu dev host. Each derives `REPO_ROOT` from `${BASH_SOURCE[0]}`, uses `set -euo pipefail`, and exports `$HOME/.cargo/bin:$HOME/.local/bin` (see `ci.sh`).
- `.ps1` twins are the Windows entrypoints. `ci.ps1`, `parity.ps1`, and `regen_goldens.ps1` locate the repo inside WSL (`wsl -d Ubuntu -e wslpath -u`) and invoke the matching `.sh`, keeping one implementation. `build.ps1`, `godot.ps1`, `server.ps1`, `sync_scenarios.ps1`, `pack.ps1`, `associate.ps1`, `export.ps1`, `version.ps1`, and `mcp-smoke.ps1` are native PowerShell mirrors that expose the same arguments and outputs for host-only steps.
- Scripts and their README mentions stay in sync; adding a gate means editing `ci.sh` and, when the argument surface changes, the matching `.ps1`.
- Generated outputs are gitignored: `client/bin/rust/`, `client/rust/target/parity/`, `client/scenarios/`, and `client/assets/version.properties`.
- Environment overrides: `GODOT_BIN`, `GODOT_VERBOSE`, `MIND_HEADLESS_BIN`, `MIND_SERVER_HOST`, `MIND_SERVER_PORT`, `MIND_EXE`, `MINDY_SRC`, `MIND_JAVA_PARITY`, `PARITY_OUT_DIR`, `OPEN_GODOT_MCP_BIN`.

## Verification

```bash
# Full local gate (WSL2 Ubuntu, from the repo root)
tools/ci.sh

# Build the Rust side and mirror scenarios
tools/build.sh

# Run the goldens and bench directly
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- run spine_place_break --json
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- bench --ticks 100000 --scenario bench_baseline

# Parity subset and golden drift check
tools/parity.sh --check --suite gate
tools/regen_goldens.sh --check

# Server wrapper
tools/server.sh --socket "status,exit"

# Headless desktop export (templates must be installed)
tools/export.sh --platform windows --target release --verify

# In-engine MCP smoke (editor already running; never run in CI)
tools/mcp-smoke.sh
```

`.github/workflows/ci.yml` runs the Rust, parity matrix, STDB, and Godot-import jobs on push/PR to `main`; the MCP smoke stays local because it needs a live editor.
