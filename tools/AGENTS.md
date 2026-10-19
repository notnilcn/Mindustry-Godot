# AGENTS.md — tools/ (build & verification scripts)

`tools/` holds the repo's developer and CI tooling: compiling the Rust GDExtension and headless oracle, launching Godot, mirroring scenarios, checking and regenerating parity goldens, driving the local server, running the offline asset pipeline, emitting version metadata, installing file associations, and running the local and nightly parity gates. Read the root [`AGENTS.md`](../AGENTS.md) first.

## Layout

| Script | Responsibility |
|---|---|
| `build.sh` | Build `mind-gdext` + `mind-headless` into `client/bin/rust` (debug profile; pass `--release` for `client/bin/rust/release/libmind_gdext.so`), then run `sync_scenarios.sh` when present. |
| `ci.sh` | Full local gate; exact stage composition is documented under **Responsibilities**. |
| `godot.sh` | Resolve the Godot 4.7 binary (`$GODOT_BIN` → `godot4` → `godot4-mono` → `godot`) and launch it against `client/` unless `--path` is passed. |
| `parity.sh` | Local driver for the `mind-headless parity …` harness (structural check, mirror, suites, gate reports, bench, soak, checksums). |
| `server.sh` | Run `mind-headless server`, or drive a running server's console socket over TCP. |
| `sync_scenarios.sh` | Mirror canonical `scenarios/*.json` into the gitignored `client/scenarios/`. |
| `pack.sh` | Forward arguments to the offline asset pipeline `mind-tools` (release). |
| `regen_goldens.sh` | Verify or regenerate oracle goldens and update `parity/golden_manifest.json` sha256 hashes. |
| `associate.sh` | Opt-in installer for `.msav`/`.msch` file associations and the `mindustry://` / `mindustry-godot://` schemes. |
| `export.sh` | Plan-22 export wrapper: checks the matching Godot export templates, runs a headless export for one of the two committed presets (`windows`, `linux`), copies the runtime `assets/` subset next to the artifact (`--no-assets` skips; `--assets-only DIR` copies without exporting), and optionally verifies the artifact. Outputs default under the gitignored `client/bin/export/`. |
| `version.sh` | Write `client/assets/version.properties` (upstream key set) from flags and git state. |
| `mcp-smoke.sh` + `mcp_smoke.py` | In-engine MCP spine smoke over a stdio `open-godot-mcp` server; needs the editor already running. |
| `gmcp.py` | Shell driver for `open-godot-mcp` tool calls (for sessions without an MCP connection): positional tool/JSON pairs, `--eval`, `--screenshot`, `--text`, `--batch`, and an eval watchdog. |
| `parity-session.sh` | SSH/no-display session bootstrap for the parity workflow: Xvfb `:10` (optionally headless weston for Godot), Godot editor on bridge 6970 for session 1, env summary; `--run` execs opencode at the workspace root, `--stop` tears down. |
| `session-play.sh` | One-call restart/play for a parity session's Godot client: `N` selects the session, `--import` imports the full-tech-tree Game Data profile, `--play` replays without restarting the editor; drives `gmcp.py`. |
| `mp-local.sh` | Local SpacetimeDB harness for multiplayer work (I08): `up` starts the standalone server on 3000 (log `/tmp/opencode/stdb.log`, pidfile `/tmp/opencode/stdb.pid`), publishes the module with `bash server/build.sh` and prints the db address (`127.0.0.1:3000/mindustry`); `down` stops what `up` started; `status` reports the listener + server ping. |
| `watch.sh` | Watch `client/rust/**` for `.rs`/`Cargo.toml` edits and re-run `build.sh`; ignores `client/rust/target/` and `client/bin/`. |
| `twin.sh` + `twin_pointer.py` | Sourced Java↔Godot twin-walk harness (J10 T1): `JW`/`JM`/`JD`/`JK`/`JS` (Java leg; the click origin is re-read from `xwininfo` on every call), `GC`/`GW`/`GK`/`GS`/`GE`/`GP` (Godot leg; `GP` warps the real pointer over the engine window via `twin_pointer.py`), `SIDE`/`ZOOM` compare, and the `twin_launch`/`twin_place_java`/`twin_stop` lifecycle (settings.bin backup/restore, save-snapshot teardown). |
| `java_xkeys.py` | XSendEvent key delivery to the Java reference window on a parity-session display (XTEST focus stays on Godot); literal text or `@Keysym`, `--hold-ms` for held movement keys. |

## Responsibilities

- `build.sh` compiles `-p mind-gdext -p mind-headless` with `--target-dir client/bin/rust` so `mind.gdextension` finds the shared library, then delegates scenario mirroring to `tools/sync_scenarios.sh`. It forwards extra arguments to cargo, so `tools/build.sh --release` produces `client/bin/rust/release/libmind_gdext.so` for `export.sh`.
- `watch.sh` is a developer loop: it watches `client/rust/**` and re-runs `build.sh` on `.rs`/`Cargo.toml` edits, ignoring `client/rust/target/` and `client/bin/` so its own build output never retriggers a rebuild. It prefers `inotifywait` and falls back to a 2s polling loop when unavailable. It is not part of `ci.sh`.
- `ci.sh` is the single local gate. It runs `cargo fmt --check`, `clippy -D warnings`, `cargo check`, `cargo test -p mind-core`, `parity check --tests`, `parity goldens`, `run-all --tier T0`, the three spine goldens, a scenario sync + `diff -r`, a release `bench --ticks 100000 --scenario bench_baseline` budget (warn +20% / fail +50% over `BASE_P50_NS=235` / `BASE_P99_NS=301` ns, measured pinned — see the note in the script), `build.sh`, a `godot.sh --headless --editor --quit` import/parse scan, the `ui_layout_check.gd` and `ui_widgets_check.gd` UI oracles, `mind-core` godot/tokio boundary greps, `cargo check -p mind-stdb`, STDB module typecheck, and `bash server/build.sh --check`. It does **not** include the MCP smoke.
- `parity.sh` forwards to `mind-headless parity …`: `--check`, `--mirror`, `--suite smoke|gate|full`, `--phase Pn`, `--gate Pn` → `parity/reports/gate_Pn.json`, `--mcp`, `--bench`, `--soak mid|stress`, `--checksums`, `--json`. With no arguments it runs `--check` plus the `smoke` suite. Output goes to `PARITY_OUT_DIR` (`client/rust/target/parity` by default).
- `server.sh` uses `MIND_HEADLESS_BIN` (`client/rust/target/debug/mind-headless`), `MIND_SERVER_HOST` (`127.0.0.1`), and `MIND_SERVER_PORT` (`6859`); `--socket "cmd,…"` writes newline commands and prints the reply, and `--serve` starts then tears down the server.
- `sync_scenarios.sh` deletes stale JSON from `client/scenarios/` and copies the canonical set; `client/scenarios/` is a gitignored mirror and is never edited by hand.
- `regen_goldens.sh` defaults to `--check` (reads committed files, no JVM); `--only <id>` / `--all` regenerate via `parity/java/extract_source_goldens.py` and rewrite hashes, and `--record <id>` re-hashes one committed file (for harness goldens). `MIND_JAVA_PARITY=1` uses the JVM dumpers in `parity/java/` when present; `MINDY_SRC` overrides the upstream source path.
- `version.sh` writes LF-only keys `type`, `number`, `modifier`, `commitHash`, `buildDate`, `build`; commit hash and date default to the current git state. The output is gitignored.
- `associate.sh` mirrors `mind_core::platform::assoc`: a Linux `.desktop` under `$XDG_DATA_HOME` (plus `update-desktop-database` / `xdg-mime` when available) or the Windows registry body via `--windows`/`--print`. `MIND_EXE` overrides the exported binary path.
- `export.sh` resolves the Godot binary (`$GODOT_BIN` → `godot4` → `godot4-mono` → `godot`), derives the `<ver>.stable` templates directory (`~/.local/share/godot/export_templates/`; `GODOT_TEMPLATES_DIR` overrides), and fails with an install hint when it is absent. `--platform` selects the preset (`windows`→`Windows Desktop`, `linux`→`Linux/X11`), `--preset` overrides it, and `--out` selects the artifact path. `--with-assets` (default) copies the runtime subset of `assets/` to `<artifact dir>/assets/` (`sprites/` with `fallback/` and `block_colors.png`, `fonts/`, `bundles/`, `locales/`, `icons/`, `sounds/`, `music/`, `maps/`, `shaders/`, `baseparts/`, `cursors/`, `planets/`, `contributors`, `sounds.index.json`), running `bash tools/pack.sh pack` first when `assets/sprites/sprites.atlas.json` is missing; `--no-assets` skips the copy and `--assets-only DIR` copies it without running an export (no templates needed).
- `mcp-smoke.sh` execs `python3 tools/mcp_smoke.py`, which spawns its own stdio `open-godot-mcp` server (bridge `ws://127.0.0.1:6970`) and drives `res://scenes/game.tscn` end-to-end. Flags: `--mcp-bin`, `--repo-root`, `--server-arg`, `--keep-running`, `--verbose`; `OPEN_GODOT_MCP_BIN` overrides binary discovery. Exits `0` pass, `1` assertion/tool failure, `2` bridge not connected.
- `gmcp.py` is the shell-side MCP client: `tools/gmcp.py <tool> '<json>'` calls one tool, several pairs run in one session, and `--eval`, `--screenshot`, `--text` and `--batch` cover the common agent calls. `--eval-timeout` (default 10 s) runs a stop/play watchdog on a hung eval; `--no-watchdog` leaves recovery to the caller.
- `parity-session.sh` is the only script that manages displays: it starts Xvfb `:10` (the Java-leg display; sessions N >= 2 use `:(9+N)`) and the editor bridge (6970, or `6970+10(N-1)`), with runtime state under the gitignored `.opencode/parity-session/`. It never starts the Java client — the parity-eval skill does that per run.
- `session-play.sh N` restarts session N's editor and plays `res://scenes/game.tscn` through `gmcp.py`; `--import` then imports `PROFILE_ZIP` (default `../mindustry.save.full.tech.tree.zip`) through `MindUi.data_import`, and `--play` skips the restart.
- `mp-local.sh` is the multiplayer local-server harness (I08 T1): `up` reuses a listener on `:3000` or starts `spacetime start` detached (`STDB_LOG`, `STDB_PID` override the log/pidfile), waits for the port, publishes `mindustry` through `bash server/build.sh` (which also regenerates `mind-stdb/src/module_bindings/`) and prints `127.0.0.1:3000/mindustry` — the address the client's `stdb-server` setting defaults to (`STDB_DB` only changes that printed address; the publish uses `build.sh`'s default). `down` kills only the pid it started; `status` pings `spacetime server ping local`.

## Conventions

- The Bash `.sh` scripts are canonical and run on the Linux dev host. Each derives the repo root from `${BASH_SOURCE[0]}` and uses `set -euo pipefail`; the toolchain-driving ones export `$HOME/.cargo/bin:$HOME/.local/bin` (see `ci.sh`). Invoke a script with `bash <script>` if its checkout mode is not executable.
- The Python helpers need `python3`; `java_xkeys.py` additionally needs `python-xlib` and runs with the parity-eval venv.
- Script roles here and in the root README stay in sync; adding a gate means editing `ci.sh`.
- Generated outputs are gitignored: `client/bin/rust/`, `client/rust/target/parity/`, `client/scenarios/`, and `client/assets/version.properties`.
- Environment overrides: `GODOT_BIN`, `GODOT_VERBOSE`, `GODOT_TEMPLATES_DIR`, `MIND_HEADLESS_BIN`, `MIND_SERVER_HOST`, `MIND_SERVER_PORT`, `MIND_EXE`, `MINDY_SRC`, `MIND_JAVA_PARITY`, `PARITY_OUT_DIR`, `OPEN_GODOT_MCP_BIN`, `PROFILE_ZIP`, and mp-local's `STDB_PORT`/`STDB_LOG`/`STDB_PID`/`STDB_DB`.

## Export layout

An exported build is the Godot binary (release template, PCK embedded via
`binary_format/embed_pck=true`, the `libmind_gdext.so` GDExtension beside it) plus an
`assets/` directory next to the binary:

```
client/bin/export/
  Mindustry-Godot.x86_64        # release template + embedded PCK
  libmind_gdext.so              # GDExtension (Godot exports it beside the binary)
  assets/                       # runtime subset copied by tools/export.sh
    sprites/ fonts/ bundles/ locales/ icons/ sounds/ music/ maps/ shaders/
    baseparts/ cursors/ planets/ contributors sounds.index.json
```

`bash tools/export.sh --with-assets` (the default) runs `bash tools/pack.sh pack` when
`assets/sprites/sprites.atlas.json` is missing, then copies exactly the entries listed in
its `copy_runtime_assets` (each names its reader; never `assets/parity/`, `assets-raw/`,
`assets/scripts/`, `assets/bloomshaders/` or `assets/cubemaps/`). At runtime the build
finds that directory through `client/rust/mind-gdext/src/assets/loader.rs`:

1. `ProjectSettings` `mindustry/assets_dir`;
2. `--assets=<dir>` (user args after `--`);
3. `MINDUSTRY_GODOT_ASSETS`;
4. `res://assets` (in an exported build `res://` also falls back to the executable
   directory, so this is the path the beside-the-binary copy normally takes);
5. `<binary dir>/assets` (the export layout, reached directly when 4 is not);
6. `<res://>/../assets` (editor/dev; also the logged fallback when nothing is valid).

A candidate is valid when `sprites/sprites.atlas.json` exists; the loader logs the chosen
root once (`[assets] assets root: \`…\` (source: …)`). The `export-smoke` CI job
(`.github/workflows/ci.yml`; manual `workflow_dispatch` or nightly schedule) downloads
Godot 4.7.2 and its export templates, runs `bash tools/build.sh --release`,
`bash tools/pack.sh pack` and `bash tools/export.sh linux --verify`, then launches the export with
`--headless --quit-after 300` and fails when the log mentions a missing manifest or the
assets root was not resolved. Reproduce locally with:

```bash
bash tools/build.sh --release
bash tools/pack.sh pack                  # skipped when assets/sprites/sprites.atlas.json exists
bash tools/export.sh linux --verify
client/bin/export/Mindustry-Godot.x86_64 --headless --quit-after 300
```
