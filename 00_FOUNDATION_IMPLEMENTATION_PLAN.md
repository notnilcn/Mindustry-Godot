# 00 — FOUNDATION IMPLEMENTATION PLAN

> Phase P0. This plan defines the spine every later plan extends. It inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | ✅ **COMPLETE 2026-10-01 (M0–M8; P0 gate passed).** See `HIGH_LEVEL_PLAN.md` §13 and this plan's Changelog. |
| **Phase** | P0 Foundation. |
| **Depends on** | none. Requires: `HIGH_LEVEL_PLAN.md`; WSL2 Ubuntu 26.04 (login shell; Windows 11 host, WSLg for GUI); Godot 4.7.2 on PATH (`godot4` standard, `godot4-mono` fallback); Rust stable 1.98.1 (installed); `spacetime` CLI 2.10.1 (installed; 2.10.2 available via `spacetime version upgrade`); `rg` (ripgrep) for boundary greps (`sudo apt install ripgrep` — currently missing); git identity configured in WSL (`git config --global user.name/user.email` — currently unset; M0 commits need it); open-godot-mcp bridge. |
| **Blocks** | `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md`, `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `05_SIM_CORE_IMPLEMENTATION_PLAN.md` directly; `06`–`23` indirectly (D9). |
| **Sources** | `mindustry-godot/HIGH_LEVEL_PLAN.md`, `mindustry-godot/PRELIMINARY_PLAN.md`; `Mindustry/AGENTS.md`, `Mindustry/core/AGENTS.md`, `Mindustry/core/src/mindustry/AGENTS.md`, `Mindustry/core/src/mindustry/core/AGENTS.md`, `Mindustry/desktop/AGENTS.md`; `Mindustry/core/src/mindustry/Vars.java`, `Mindustry/core/src/mindustry/ClientLauncher.java`, `Mindustry/core/src/mindustry/core/Logic.java`; `Mindustry/tests/AGENTS.md` + `Mindustry/tests/src/test/java/**`; `main/AGENTS.md`, `main/server/AGENTS.md`, `main/server/build.sh`, `main/server/spacetime.json`; `client/project.godot`, `client/Scripts/Components/README.md`, `client/sstdbsdk/AGENTS.md`; skills `godot-compositor-testing`, `playtest`. |
| **Extends spine** | N/A — this plan **defines** the spine (D4): Godot window + camera + one tile grid + place/break one block + state inspector overlay + fixed 60 Hz step + `mind-headless` harness. All later plans extend it and must keep the contracts in §3.10 stable. |

## 2. Scope & parity definition

**Delivers:** the workspace, toolchain lock, build/CI scripts, licensing, and a minimal but end-to-end verifiable pure-Rust spine on both oracles (headless + in-engine via MCP).

**Done means (P0 gate, `HIGH_LEVEL_PLAN.md` §5):**

1. `cargo fmt`, `cargo clippy`, `cargo check`, `cargo test` green for the whole `client/rust` workspace.
2. `mind-headless` runs golden scenarios, writes the canonical JSON dump, and reproduces identical checksums across processes and via `replay`.
3. The Godot client opens, renders a camera + tile grid, places/breaks one block through Rust input, shows a state-inspector overlay, and steps a fixed 60 Hz sim.
4. open-godot-mcp can launch, drive, inspect and screenshot the spine (§7c).
5. `server/spacetimedb` skeleton compiles and can be published + generated locally; `mind-stdb` skeleton compiles.
6. The old C# tree (component framework + `sstdbsdk` C#) is deleted, with C# sstdbsdk semantics preserved as docs for plan 01 and the component framework replaced by Bevy ECS.
7. `LICENSE` (GPL-3.0), `THIRD_PARTY_NOTICES.md`, ported-file headers, root/child `AGENTS.md`, and the repo-local `playtest` skill exist.
8. `tools/ci.sh` (and `tools/ci.ps1` Windows parity) run checks 1–2, the Godot headless import check, and the boundary greps; the MCP smoke script is the local integration gate.

**Explicitly deferred (owner plan):** full `ctype`/content model and vanilla content → `02_CONTENT_IMPLEMENTATION_PLAN.md`; assets/atlas/bundles → `03_ASSETS_IMPLEMENTATION_PLAN.md`; save/`TypeIO`/`Settings`/revisions → `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`; the real `Logic` schedule, groups, pooling, `Time` parity ceiling → `05_SIM_CORE_IMPLEMENTATION_PLAN.md`; `Tiles`/`Tile`/`World` parity, floors/overlays/walls, generators → `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md`; real placement/`Build`/multiblocks → `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`; full `sstdbsdk` semantics + STDB schema + command relay → `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md`; rendering/UI/audio/editor/mods/net → `13`–`22`; the continuous oracle → `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`.

**Deliberate deviations carried from `HIGH_LEVEL_PLAN.md`:**

| Deviation | Reason / tracking |
|---|---|
| No C# anywhere after M6; the old tree is kept only until its replacement lands. | D1, §2.1. Deletion sequence in §5/M6. |
| Standard Godot 4.7.2 (`godot4` on PATH in WSL Ubuntu) is the dev/CI host; `godot4-mono` is fallback only. The GDExtension is identical on both. | D7, OD7, §8-OD-R3 (user decision 2026-10-01; host updated to WSL). |
| Project renderer is Godot `mobile` on all platforms (desktop and Android) per user decision 2026-10-01; `gl_compatibility` is the documented emergency fallback. | §8-OD-R2 (locked). |
| Two placeholder content names only (`air`, `stone-wall`); plan 02 replaces the registry, names/IDs are parity ABI and stay valid. | Preserves the “append-only, name-stable” rule early. |
| `mind-core` ships a placeholder `Sim` schedule with the final `Logic.updateEntities()` slot order reserved, but only command application and `Buildings` populated. | D8; plan 05 extends without re-ordering. |

## 3. Target design

### 3.1 Repository layout (implements `HIGH_LEVEL_PLAN.md` §2.1)

```
mindustry-godot/
  AGENTS.md                                  # root agent guide (mirrors main/AGENTS.md structure)
  HIGH_LEVEL_PLAN.md
  PRELIMINARY_PLAN.md
  LICENSE                                    # GPL-3.0 (copied from Mindustry/LICENSE)
  THIRD_PARTY_NOTICES.md
  00_FOUNDATION_IMPLEMENTATION_PLAN.md       # this file
  {01..23}_*_IMPLEMENTATION_PLAN.md
  README.md                                  # short: what it is, how to build/run/verify
  rust-toolchain.toml                        # channel 1.98.1, rustfmt+clippy
  .gitignore                                 # root: build outputs, caches, tokens
  .github/workflows/ci.yml                   # rust + spacetimedb + godot-import jobs
  tools/
    build.sh  build.ps1                      # build mind-gdext + mind-headless + sync scenarios
    godot.sh  godot.ps1                      # resolve Godot binary (GODOT_BIN -> godot4 -> godot4-mono)
    ci.sh  ci.ps1                            # fmt/clippy/check/test + golden + godot import
    mcp-smoke.sh  mcp-smoke.ps1              # local-only MCP playtest smoke (editor + bridge)
    sync_scenarios.sh  sync_scenarios.ps1    # scenarios/ -> client/scenarios/
  scenarios/                                 # canonical headless scenario JSON (format §6.1)
    spine_place_break.json
    spine_determinism.json
    spine_many_commands.json
    bench_baseline.json
  client/                                    # Godot 4.7 project (pure Rust)
    project.godot
    mind.gdextension                         # GDExtension wiring -> bin/rust/*/libmind_gdext.so (.dll on Windows)
    icon.svg
    scenes/
      spine.tscn                             # main scene (D4 rig)
      ui/state_inspector.tscn
    ui/
      state_inspector.gd                     # GDScript: UI layout only, reads SimHost
    scenarios/                               # generated mirror of ../scenarios (gitignored)
    bin/rust/{debug,release}/                # cargo --target-dir output (gitignored)
    addons/
      open_godot_mcp/                        # required for MCP verification
      blastbullets2d/  phantom_camera/       # kept, unused at P0 (evaluated in 10/15)
    rust/                                    # Cargo workspace — all game Rust
      Cargo.toml  Cargo.lock
      mind-core/                             # Godot-free, tokio-free sim
      mind-headless/                         # test-rig binary
      mind-gdext/                            # GDExtension cdylib
      mind-stdb/                             # sstdbsdk Rust port + checked-in STDB bindings
  server/
    build.sh  build.ps1
    spacetime.json
    spacetimedb/                             # STDB module crate (skeleton at P0)
      Cargo.toml
      src/{lib.rs, main/{mod.rs, lifecycle.rs, tables.rs, reducers.rs}}
  docs/
    port-history/                            # retired (NUD-05=C: no archive)
```

### 3.2 Toolchain pinning (chosen versions, verified on this machine)

| Component | Pinned value | Evidence / pin location |
|---|---|---|
| Godot | 4.7.2 stable, on PATH in WSL Ubuntu (`godot4` → 4.7.2.stable.official; mono fallback `godot4-mono` → 4.7.2.stable.mono.official). WSL Vulkan enumerates **llvmpipe (software)** only (no hardware ICD/dzn): in-engine functional checks run software-rendered; GPU/perf-rendering runs use the Windows 4.7.2 standard binary at `C:\Users\Clinton\g\Godot\Godot-stable_win64.exe`. | D7/OD7. `tools/godot.sh` resolves `$GODOT_BIN` → `godot4` (standard) → `godot4-mono` (fallback); fails with an install hint. |
| godot-rust / gdext | `godot = { version = "0.5.5", features = ["api-4-7"] }` | gdext v0.5.4 release notes add the `api-4-7` level; v0.5.5 is latest stable (2026-08-09). v0.5.x requires Rust edition 2024. |
| `bevy_ecs` | `=0.19.1` (exact pin; 0.20 is rc) | `bevy_ecs` only — **no** `bevy_render`/`bevy_app`/window crates (D1). |
| SpacetimeDB | CLI `2.10.1` (installed; v2.10.2 available via `spacetime version upgrade`); `spacetimedb = "2.10.1"` (module), `spacetimedb-sdk = "=2.10.1"` (client) | `spacetime --version`; crates.io. Replaces main's `spacetimedb 2.2.0` pairing so CLI/crates/bindings match. Upgrade CLI + pins together if 2.10.2 is wanted before M5. |
| Rust | stable `1.98.1`, edition 2024, `resolver = "3"` (MSRV 1.95) | `rustc 1.98.1`; `rust-toolchain.toml`. |
| Checksums | `xxhash-rust` xxh3-64 | Deterministic, platform-stable. |
| Serialization | `serde` + `serde_json` (no `preserve_order`; structs/BTreeMap only) | Stable dump/scenario output (§6). |
| Platform | WSL2 Ubuntu-first (Windows 11 host, WSLg for GUI); bash `.sh` primary + PowerShell `.ps1` Windows parity | `HIGH_LEVEL_PLAN.md` §6.5. |

### 3.3 Crate boundaries, dependency graph, build order

```
mind-core            -> bevy_ecs, serde, serde_json, indexmap, smallvec, log, thiserror, xxhash-rust
mind-headless        -> mind-core, clap, anyhow, serde_json
mind-stdb            -> spacetimedb-sdk, tokio (rt-multi-thread), serde, log, thiserror
mind-gdext           -> godot (api-4-7), mind-core, mind-stdb, log
server/spacetimedb   -> spacetimedb 2.10.1, log   (separate crate, NOT a workspace member)
```

Hard rules, enforced by `tools/ci`:

- `mind-core` is **Godot-free and tokio-free**: `cargo tree -p mind-core | rg "godot|tokio"` is empty and `rg "use godot|use tokio" client/rust/mind-core` is empty.
- `mind-gdext` contains no game rules (view drivers, input, autoloads, frame pump only).
- Generated STDB bindings live in `client/rust/mind-stdb/src/module_bindings/`, are **checked in** (HLP §12 C3), and are **never hand-edited** (regenerate via `server/build.sh`; drift gate `build.sh --check`).
- Build order: `mind-core` → `mind-stdb` → `mind-headless` / `mind-gdext`. `cargo build -p mind-gdext` needs no Godot headers (gdext ≥0.5.3 ships prebuilt API bindings).

`client/rust/Cargo.toml` skeleton:

```toml
[workspace]
resolver = "3"
members = ["mind-core", "mind-headless", "mind-gdext", "mind-stdb"]
default-members = ["mind-core", "mind-headless", "mind-stdb"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-only"
rust-version = "1.95"

[workspace.dependencies]
bevy_ecs = "=0.19.1"
godot = { version = "0.5.5", features = ["api-4-7"] }
spacetimedb-sdk = "=2.10.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
log = "0.4"
indexmap = "2"
smallvec = "1"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
```

Workspace lints: `unsafe_code = "deny"` with an explicit `allow` in `mind-gdext` (gdext macros); clippy `all` denied; `clippy::unwrap_used`/`expect_used` denied in `mind-core` (allowed in tests).

### 3.4 Godot project reset and GDExtension wiring

`client/project.godot` target state:

```ini
config_version=5
[application]
config/name="Mindustry-Godot"
run/main_scene="res://scenes/spine.tscn"
config/features=PackedStringArray("4.7")          ; "Mobile" re-added by 22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md
[autoload]
McpRuntimeAutoload="*res://addons/open_godot_mcp/runtime/runtime_autoload.gd"
[editor_plugins]
enabled=PackedStringArray("res://addons/open_godot_mcp/plugin.cfg")
[open_godot_mcp]
screenshot_max_count=50
screenshot_max_age_hours=24
[display]
window/stretch/mode="canvas_items"
window/stretch/aspect="expand"
[physics]
3d/physics_engine="Jolt Physics"
[rendering]
renderer/rendering_method="mobile"                ; §8-OD-R2 (user decision 2026-10-01)
```

Removed at M1: `[dotnet]` + `project/assembly_name`, `rendering_device/driver.windows="d3d12"`, `.godot/` cache. Kept: `McpRuntimeAutoload` + the MCP editor plugin (required by §7c).

`client/mind.gdextension`:

```ini
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.7
reloadable = true

[libraries]
windows.debug.x86_64 = "res://bin/rust/debug/mind_gdext.dll"
windows.release.x86_64 = "res://bin/rust/release/mind_gdext.dll"
linux.debug.x86_64 = "res://bin/rust/debug/libmind_gdext.so"
linux.release.x86_64 = "res://bin/rust/release/libmind_gdext.so"
macos.debug = "res://bin/rust/debug/libmind_gdext.dylib"
macos.release = "res://bin/rust/release/libmind_gdext.dylib"
```

Build outputs live in `client/bin/` (not `client/rust/target/`) so Godot never scans the Cargo target tree: build scripts pass `--target-dir client/bin/rust`. `client/bin/` and `client/rust/target/` are gitignored.

### 3.5 The minimal spine (D4)

Scene `client/scenes/spine.tscn`:

```
Spine (Node)                                  # root
├── SimHost (MindSimHost)                     # Rust: sim owner, fixed-step pump, input, commands
├── World (Node2D)
│   ├── TileGrid (MindTileGrid)               # Rust: _draw grid + one quad per non-air block
│   └── Camera2D (MindCamera2D)               # Rust: pan/zoom, screen<->tile
└── Ui (CanvasLayer)
    └── StateInspector (instance state_inspector.tscn)   # GDScript label overlay
```

Rust classes registered by `mind-gdext`:

| Class | Base | Responsibilities (P0) | Key `#[func]` API |
|---|---|---|---|
| `MindSimHost` | `Node` | owns `mind_core::Sim`; `_process` fixed-step accumulator (D8); `_input` place/break; `state_changed` signal; scripted capture helper | `place_block(x,y,block) -> bool`, `break_block(x,y) -> bool`, `get_state_json() -> GString`, `get_checksum() -> GString`, `get_tick() -> i64`, `is_paused() -> bool`, `set_paused(bool)`, `step(ticks) -> i64`, `load_scenario(path) -> bool`, `selected_block() -> GString`, `select_block(name) -> bool`, `capture(path) -> bool` |
| `MindCamera2D` | `Camera2D` | WASD/edge pan, wheel zoom (0.25–4.0), screen↔tile conversion | `screen_to_tile(x,y) -> Vector2i`, `tile_to_screen(x,y) -> Vector2`, `center_on_tile(x,y)` |
| `MindTileGrid` | `Node2D` | grid lines + colored block quads; redraw on `world_changed`; pulls `tile_blocks()` from SimHost | `set_host(host: Gd<MindSimHost>)`, `redraw_requested()` |
| `MindExtension` | `ExtensionLibrary` | `#[gdextension]` init, class registration, `log` → Godot console bridge | — |

`mind-core` P0 modules (later plans extend in place):

| Module | P0 contents | Extended by |
|---|---|---|
| `mind_core::sim` | `Sim { ecs: World, schedule: Schedule, grid: WorldGrid, state: GameState, events: Events, rng: JavaRandom }`; `Sim::tick()`, `apply(Command) -> Result<(), SimError>`, `checksum() -> u64`, `dump() -> StateDump`; `FixedStepRunner { accumulator: f64, step: f64, max_catchup: u32 }` with `step = 1.0 / 60.0` | 05 (`Logic` schedule parity), 21 (command relay) |
| `mind_core::schedule` | `SimSet` labels reserving `Logic.updateEntities()` order: `PoolCleanup, BulletPhysics, UnitPhysics, Players, Effects, EntityGroups, Units, PowerGraph, Buildings, Bullets, Collisions` (only command application + `Buildings` populated at P0) | 05, 10, 11 |
| `mind_core::world` | `WorldGrid { width, height, tiles: Vec<Option<Entity>>, floors: Vec<BlockId> }`, `TilePos(i16, i16)`, `resize`, `fill`, `set_block`, `block_at`, `clear` | 06 (`Tiles`/`World`/`Edges`), 07 |
| `mind_core::content` | placeholder `ContentType`, `ContentId(u16)`, `BlockId`, `Blocks` registry: `air=0`, `stone-wall=1`, name→id lookup; append-only invariant asserted | 02 (real loader/traits/vanilla content) |
| `mind_core::ecs` | `MindWorld` wrapper, `EntitySeq` monotonic component, `BuildingComp { pos: TilePos, block: BlockId, team: TeamId, rot: u8 }`; stable entity-ordering helper | 05, 07, 11 |
| `mind_core::game` | `GameState { phase: State, tick: u64 }`, `State { Menu, Playing, Paused }`; `StateChange` event | 05, 12 |
| `mind_core::time` | Arc `Time` parity subset: monotonic tick, `delta: f32` (1.0 inside a tick), delayed-run queue (`run(delta, f)`, `Time::update()`) | 05 |
| `mind_core::random` | `JavaRandom` (48-bit LCG port of `java.util.Random`) + `Rand` helper (`random()`, `range`, `chance`) | 06, 11, 23 |
| `mind_core::command` | `Command { Place{x,y,block}, Break{x,y}, SelectBlock{block} }`, tick-stamped `CommandRecord` | 07, 15, 21 |
| `mind_core::event` | `define_events!` macro + `Events` (per-event listener vectors, registration-order dispatch, no type erasure); P0 events: `ClientCreateEvent`, `ClientLoadEvent`, `ContentInitEvent`, `StateChangeEvent`, `BlockPlacedEvent`, `BlockBrokenEvent` | 05 (`EventType` parity), 23 |
| `mind_core::log` | `log` facade + `MindLogger` (console + `last_log.txt`), `[D]/[I]/[W]/[E]` prefixes matching `Vars.loadLogger` | all |
| `mind_core::version` | `MIND_VERSION`, `DUMP_FORMAT = 1`, `SCENARIO_FORMAT = 1`, `COMMAND_LOG_FORMAT = 1` | all |

`mind-headless` P0 commands:

```
mind-headless list
mind-headless run <scenario> [--dump <path>] [--json]
mind-headless sim <ticks> [--seed N] [--width W] [--height H] [--dump <path>]
mind-headless replay <commands.jsonl> [--seed N] [--ticks N] [--dump <path>]
mind-headless bench [--ticks N] [--scenario spine]
mind-headless dump --scenario <name> --out <path>
```

Exit codes: `0` pass, `1` assertion/golden mismatch, `2` usage/IO error. `mind-headless` never links Godot and never opens a socket (D5, §2.2).

### 3.6 Event bus, logging, error strategy (project-wide conventions)

**Event bus (`mind_core::event`).** Ported from `mindustry.game.EventType` + `Events.fire/on`. A macro generates one listener `Vec` per event type; `Events::on_x(f)` pushes in registration order and `Events::fire_x(&event)` invokes in that same order, deterministically. Rules: sim code only fires events; handlers must not mutate ECS state outside the schedule slot that owns that state (documented per event as it lands); registering a listener from inside a fired handler is a debug assert. Events fired during a tick are queued and flushed at the slot boundary in plan 05; P0 flushes synchronously at tick end. UI-only events (`ClientLoadEvent`) stay core-defined so headless fires them too.

**Logging.** Use the `log` facade in `mind-core`/`mind-stdb`; no `println!` in library crates (binaries may). `mind_core::log::init(LogConfig { level, file: Option<PathBuf>, color: bool })` installs `MindLogger` once (idempotent, `OnceLock`). In-engine, `mind-gdext` forwards records to stderr and `user://last_log.txt`; headless writes `<data-dir>/last_log.txt` by default, overridable via `--log-file`. Prefix format matches Mindustry: `[D]`/`[I]`/`[W]`/`[E]` + message; colors are dropped when not a TTY (Windows parity per `Vars.loadLogger`).

**Errors.** `thiserror` enums per crate (`SimError`, `ScenarioError`, `MindDbError`); binaries use `anyhow` at the top level. No `unwrap()`/`expect()` on runtime data in sim paths (clippy-denied in `mind-core`; allowed for compile-time invariants and tests); `panic!` only for violated invariants in debug (`debug_assert!` preferred); `mind-gdext` installs a panic hook that logs before unwinding into Godot; command failures are logged and surfaced to the caller (`place_block -> bool` for MCP/input).

### 3.7 `mind-stdb` and `server/spacetimedb` skeleton (P0 placeholder)

`mind-stdb` module tree (filled by `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md`):

```
src/lib.rs        # re-exports; `MindDb` facade
src/config.rs     # ConnectionConfig { host, db_name, token_append } (token_append from --pN, per sstdbsdk)
src/tokens.rs     # TokenStore: per host+append file under <data-dir>/identity/
src/waves.rs      # SubscriptionWave { Base, Lobby, Game } + table-name lists (generated accessor names)
src/binder.rs     # TableBinder trait: on_insert/on_update/on_delete (Rust analog of TableBinderComponent)
src/conn.rs       # connect(), frame_tick(), reducer facade; P0 live ops return MindDbError::NotImplemented
src/module_bindings/    # spacetime generate --lang rust output, checked in, never hand-edited
```

`server/spacetimedb` P0 tables/reducers (skeleton; plan 01 grows the real schema):

```rust
#[table(accessor = player, public)]
pub struct Player { #[primary_key] pub identity: Identity, pub username: String, pub last_seen: Timestamp }
#[reducer(init)] pub fn init(ctx: &ReducerContext) { /* seed marker */ }
#[reducer(client_connected)]   // create/refresh Player row
#[reducer(client_disconnected)] // stamp last_seen
#[reducer] pub fn set_username(ctx: &ReducerContext, username: String) -> Result<(), String> { /* len <= 40 (Vars.maxNameLength) */ }
```

`server/spacetime.json`: `{ "server": "local", "module-path": "./spacetimedb", "generate": [{ "language": "rust", "out-dir": "../client/rust/mind-stdb/src/module_bindings" }] }`. Module name: `mindustry_godot`. Local `spacetime start` flow, dev wipes expected (§6.3 high-level).

### 3.8 Build, CI and verification scripts

- `tools/build.sh` / `.ps1`: `cargo build -p mind-gdext -p mind-headless --target-dir client/bin/rust [--release]` then `sync_scenarios` (bash primary; `.ps1` is Windows parity).
- `tools/godot.sh`: resolves `$GODOT_BIN` → `godot4` (standard) → `godot4-mono` (fallback); fails with an install hint. `.ps1` twin for Windows.
- `tools/ci.sh` / `.ps1`: `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo check --workspace`; `cargo test -p mind-core`; `cargo run -p mind-headless -- run spine_place_break --json`; `spine_determinism`; Godot `--headless --editor --quit --path client`; boundary greps (§3.3); `cargo check -p mind-stdb`.
- `.github/workflows/ci.yml`: `rust` (windows-latest + ubuntu-latest), `spacetimedb` (`cargo check --manifest-path server/spacetimedb/Cargo.toml --tests`), `godot-import` (downloads the pinned Godot 4.7.2 standard build; runs the headless import check). GPU/MCP smoke is local-only (`tools/mcp-smoke.sh`).

### 3.9 Licensing and attribution (D6)

- `LICENSE`: GPL-3.0 text copied from `Mindustry/LICENSE`; header notes this is a derivative work.
- `THIRD_PARTY_NOTICES.md` entries: Mindustry + assets (GPL-3.0, Anuken & contributors), Arc (license per `../Arc/LICENSE` if present, else the license bundled with Mindustry), godot-rust/`godot` crate (MPL-2.0), `bevy_ecs` (MIT OR Apache-2.0), SpacetimeDB crates + SDK (per `spacetimedb/licenses/`), `open_godot_mcp` addon, retained addons (Phantom Camera MIT; BlastBullets2D per its license). Each entry: name, version, license, upstream URL, purpose.
- Ported-file header, first lines of every file with an original Mindustry source:

  ```rust
  // Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
  // Source: core/src/mindustry/core/Logic.java (updateEntities order)
  ```

  Purely new files use `// SPDX-License-Identifier: GPL-3.0-only`. GDScript/scene files use the matching `##`/`;` comment prefix.

### 3.10 What exists at the end of this plan, and the extension contract

**Exists at P0:** workspace + toolchain lock; a Godot project that opens a window with `Spine` running `SimHost` on a fixed 60 Hz accumulator; a 32×32 grid drawn by Rust; left/right click place/break `stone-wall`; inspector overlay showing tick/paused/selected block/cursor tile/checksum; deterministic `mind-core` `Sim` with checksum + JSON dump; `mind-headless` CLI (`run/sim/replay/dump/bench`); scenario files with goldens; STDB skeleton publishable locally; `mind-stdb` skeleton; CI/build scripts; GPL files; `AGENTS.md` docs; repo `playtest` skill; C# tree deleted.

**Extension contract (stable; changes require a changelog note here and in `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`):**

1. **Headless scenario format** (§6.1): `scenarios/*.json`, `format: 1`; new systems add fields/variants, never repurpose; every plan contributes ≥1 scenario named `{system}_{case}`.
2. **Command-script format** (§6.2): JSONL, one `{ "tick", "op" }` per line; `op` variants append-only. This is the seed of the plan-21 STDB relay wire shape.
3. **JSON state dump schema** (§6.3): `format: 1`; `world.tiles` sparse by default, sorted by `(y, x)`; new sections append; `checksum` always present. Serialized-field additions bump the dump format.
4. **MCP-visible entry points** (§7c): the `MindSimHost` `#[func]` methods and the node paths in §3.5 are a stable test API; new plans add methods, never rename without a changelog note. All MCP calls are pid-stamped.
5. **State inspector protocol**: GDScript-only overlay reads `SimHost.get_state_json()` (same schema as the headless dump) every 250 ms and on `state_changed(tick, checksum)`; it never writes sim state. Later plans extend the JSON, not Rust internals.

**How each later plan extends the spine:** `01` replaces `mind-stdb` stubs and wires the relay behind `MindSimHost`; `02` replaces `mind_core::content` (names/IDs preserved); `03` adds assets (colored quads swap for sprites); `04` adds save/load through the same `Sim` dump path; `05` replaces `Sim::tick` internals with the full `Logic` schedule using `SimSet` labels + queued events; `06` replaces `WorldGrid` with `Tiles`/`World`; `07` replaces placeholder `Command` handling with `Build`/`BuildPlan`; `08`–`18` add ECS systems, view drivers, FX, audio via new `[func]`/signals; `19` adds editor scenes; `20` adds mod roots to scenario loading; `21` makes the command log the multiplayer wire format; `22` reuses `mind-headless` as the dedicated server; `23` runs the whole scenario suite continuously.

## 4. Port map

| Mindustry source | Target (P0) | Notes on adaptation |
|---|---|---|
| `core/src/mindustry/Vars.java` | `mind_core::config::MindConfig` + constants module (`TILESIZE = 8`, `MAX_BLOCK_SIZE = 16`, build range, paths) | Statics become a config/`Resources` struct; data-dir layout mirrors `saves/ maps/ schemes/ mods/ tmp/ screenshots/`; full `Settings` in plan 04. |
| `core/src/mindustry/ClientLauncher.java` (boot flow) | `mind_gdext::MindSimHost` + `mind_headless::bootstrap` | Asset-manager stages become the `ContentInitEvent` → `ClientLoadEvent` ordering; real stages land in plan 03. |
| `core/src/mindustry/core/Logic.java` (`update`, `updateEntities`) | `mind_core::sim::Sim::tick` + `mind_core::schedule::SimSet` | Slot order reserved verbatim (lines 470–496): pool cleanup → bullet/unit physics → players → effects → Groups → units → power graph → buildings → bullets → collisions. Only command application + `Buildings` populated at P0. |
| `core/src/mindustry/core/GameState.java` | `mind_core::game::{GameState, State}` | P0: `Menu/Playing/Paused`, tick counter, `StateChange` event. `Rules`/teams/`Map` in 05/12. |
| `core/src/mindustry/core/World.java`, `world/Tiles.java`, `world/Tile.java` | `mind_core::world::{WorldGrid, TilePos}` | `Vec<Option<Entity>>` indexed `x + y*width`; `Tile.build` becomes the ECS entity. Full `Tile` fields + `Edges` in 06. |
| `core/src/mindustry/ctype/Content.java`, `ContentType.java` | `mind_core::content::{ContentType, ContentId, BlockId, Blocks}` | Placeholder registry; plan 02 replaces internals, keeps name/ID ABI. |
| `core/src/mindustry/entities/comp/*`, `mindustry.gen.*` | `bevy_ecs` components (`BuildingComp`, `EntitySeq`) | No codegen; append-only field discipline becomes revision-tracked serde in plan 04. |
| `arc.util.Time` (`Time.update`, run queue) | `mind_core::time` | Enough for the ported timer tests; `Time.delta` semantics preserved. |
| `arc.math.Rand` / `java.util.Random` | `mind_core::random::JavaRandom` | 48-bit LCG for Java parity; full Arc `Rand` parity reviewed in 06/23. |
| `core/src/mindustry/game/EventType.java` + `Events` | `mind_core::event` (`define_events!`) | Deterministic registration-order dispatch; variants appended as plans land. |
| `arc.util.Log` + `Vars.loadLogger`/`loadFileLogger` | `mind_core::log` (`MindLogger`) | Same `[D]/[I]/[W]/[E]` prefixes; file at `<data-dir>/last_log.txt`. |
| `desktop/.../DesktopLauncher.java` | `mind-gdext` + `client/scenes/spine.tscn` | SDL/GL config and CLI args replaced by Godot; file dialogs/Steam/Discord deferred to plan 22. |
| `server/.../ServerLauncher.java` | `mind-headless` binary | P0 is a test harness, not the dedicated server (plan 22 wraps it). |
| `client/sstdbsdk/DatabaseConnector.cs` | `mind-stdb::conn::MindDb` (plan 01) | Frame pump = `frame_tick()` called from `mind-gdext` `_process`, exactly once per frame. |
| `client/sstdbsdk/TableSubscriber.cs` | `mind-stdb::waves` (plan 01) | Base/Lobby/Game waves; accessor-name lists. |
| `client/sstdbsdk/TableBinderComponent.cs` | `mind-stdb::binder::TableBinder` (plan 01) | Trait with insert/update/delete callbacks + replay-existing-rows flag. |
| `client/Scripts/Components/*.cs` (six bases, `IComponent`, `ComponentRegistration`, `EntityRegistry`) | deleted; `bevy_ecs` replaces it | C# component ancestry/registration has no Rust analog; ECS archetypes are the replacement. |
| `annotations/` codegen | none | Explicit Rust registries/data (`HIGH_LEVEL_PLAN.md` §1). |
| `tools:pack` | `03_ASSETS_IMPLEMENTATION_PLAN.md` | Not in P0. |
| Network `@Remote`/packets | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` via `01` + `mind-stdb` | P0 only defines the command-log shape that becomes the wire format. |

## 5. Milestones & task breakdown

Each milestone is independently verifiable; run its commands and record evidence in the Changelog. The smallest vertical slice is M2 → M3 (headless core + golden scenario); M1 de-risks the engine binding in parallel and can start first because it shares no code with M2.

### M0 — Workspace scaffolding and docs (no engine)

- Root `AGENTS.md` (mirrors `main/AGENTS.md`: overview → where things are map → folders to skip → running → verification), `README.md`, `rust-toolchain.toml`, root `.gitignore`, `client/rust/Cargo.toml` workspace + four member crates with SPDX headers.
- `LICENSE` (copy `Mindustry/LICENSE`), `THIRD_PARTY_NOTICES.md`.
- Init git if absent for the project itself; do **not** archive the legacy C# tree (user decision 2026-10-01, NUD-05=C: delete outright at M6).

**Verify:** `cargo metadata --manifest-path client/rust/Cargo.toml --no-deps` lists 4 members; `LICENSE` and `THIRD_PARTY_NOTICES.md` exist.

### M1 — Godot reset + “hello, gdext”

- Add `client/mind.gdextension`; `mind-gdext` with `MindExtension` + trivial `MindHello` class; `tools/build.sh`/`tools/godot.sh` (`.ps1` Windows twins optional).
- Strip C# from `project.godot` (§3.4); delete `.godot/`; keep the MCP addon.
- Temporarily point `run/main_scene` at `scenes/hello.tscn` (a Node whose script logs `mind-gdext loaded`).

**Verify:** `cargo build -p mind-gdext --target-dir client/bin/rust` succeeds; `tools/godot.sh --headless --quit-after 60 --path client` logs the hello line and exits 0; `--headless --editor --quit` parse check clean.

### M2 — `mind-core` spine + portable unit tests

- Implement `time`, `random`, `event`, `log`, `game`, `world::WorldGrid`, `content` placeholder, `command`, `ecs`, `sim` (fixed step + checksum + dump structs), `schedule::SimSet`.
- Port the timer tests (`ApplicationTests.timers/manyTimers/longTimers`) and `createMap`, plus the new determinism/roundtrip tests (§7a).
- Add boundary greps to `tools/ci`.

**Verify:** `cargo test -p mind-core` green including `time::tests::timer_runs_after_two_updates`, `time::tests::many_timers_same_update`, `time::tests::long_timers_catch_up`, `world::tests::resize_and_fill`, `sim::tests::same_seed_same_checksums`; `cargo tree -p mind-core | rg "godot|tokio"` empty.

### M3 — `mind-headless` harness + goldens

- CLI (`clap`) + Rust scenario registry; `run/sim/replay/dump/bench/list`; checksum stream (§6.4); `scenarios/*.json`; `tools/sync_scenarios.*`.
- Record and commit the golden checksum for `spine_place_break` (fill the placeholder in the scenario file at this milestone).

**Verify:** `cargo run -p mind-headless -- run spine_place_break --json` exits 0 with `"pass":true`; two runs plus `replay` of the emitted JSONL produce identical checksums; `mind-headless dump` JSON round-trips through the serde schema test.

### M4 — In-engine spine (D4 rig)

- `mind-gdext` classes per §3.5; fixed-step accumulator; Rust input; `state_changed`; `capture`; GDScript `state_inspector.gd` + `state_inspector.tscn`; `scenes/spine.tscn`; wire `run/main_scene`; panic hook + log bridge.

**Verify:** windowed run places/breaks via mouse; `godot_exec call` API methods succeed; inspector shows tick/checksum; `-- --capture` writes a PNG; MCP scenario §7c passes end-to-end.

### M5 — STDB + `mind-stdb` skeleton + publish workflow

- `server/spacetimedb` crate, `server/build.sh`/`.ps1`, `server/spacetime.json`; `mind-stdb` module tree with `NotImplemented` live ops; `mind-gdext` constructs `MindDb` behind a `--db` flag.
- Local flow: `spacetime start` (login shell); `server/build.sh` publishes the module to db `mindustry` and generates Rust bindings into `client/rust/mind-stdb/src/module_bindings/` (checked in; `--check` drift gate).

**Verify:** `cargo check -p mind-stdb`; `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests`; `spacetime describe mindustry` lists `player` + `set_username`; module_bindings exist and are drift-clean.

### M6 — C# deletion and migration

Exact ordered steps:

1. Confirm M2–M5 green. No archive: the legacy C# tree is deleted outright (user decision 2026-10-01; §8-OD-R1/NUD-05=C).
2. No `docs/port-history/` copies are made (NUD-05=C); if a port note is still needed, transcribe it into `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` before deletion.
3. Delete `client/Scripts/` (all `.cs` + `.cs.uid`), `client/sstdbsdk/*.cs`, `*.cs.uid`, `client/sstdbsdk/DatabaseConnector.tscn`, `client/sstdbsdk/desktop.ini`, any generated `*.csproj`/`*.sln`, and `client/.godot/mono/`.
4. Delete the now-empty `client/sstdbsdk/` directory.
5. Confirm `project.godot` has no `[dotnet]` block, no `project/assembly_name`, and no C# marker in `config/features`.

**Verify (all empty/absent):** `rg --glob "!client/addons/**" -g "*.cs" client` → no files; `rg "dotnet|DotNet|C#" client/project.godot` → no matches; `rg "Scripts/|sstdbsdk/" client/scenes client/ui client/project.godot` → no references; Godot headless import clean; §7c smoke passes.

### M7 — CI, docs, repo playtest skill

- `tools/ci.sh`/`.ps1`, `tools/mcp-smoke.sh`, `.github/workflows/ci.yml`; `client/AGENTS.md` and `server/AGENTS.md` stubs pointing at the conventions.
- Create the repo skill `mindustry-godot/.opencode/skills/playtest/SKILL.md`, mirroring the existing skill’s structure (frontmatter `name`/`description`/`whenToUse`; “which tool for what”; Part 1 Godot MCP recipes; Part 2 `spacetime` CLI), with these mindustry-godot specifics:
  - launch (when `godot_health check` reports BRIDGE_NOT_CONNECTED): from WSL, `nohup godot4 --editor --path /home/c/g/code_examples/mindustry-godot/client >/tmp/mind-editor.log 2>&1 &`; wait ~20 s; `godot_instance list`; bridge port 6970. WSLg hosts the window; if driving from a Windows terminal, prefix `wsl -d Ubuntu -e bash -lc '...'`.
  - preflight identity: the bridge is 127.0.0.1:6970 — close any other editor holding that port (the sibling `main/` project is Windows-side), then verify `godot_exec eval {"code":"return ProjectSettings.globalize_path(\"res://\")"}` contains `mindustry-godot` before any `godot_game` call.
  - always run `res://scenes/spine.tscn`; node map: `/root/Spine/SimHost`, `/root/Spine/World/TileGrid`, `/root/Spine/World/Camera2D`, `/root/Spine/Ui/StateInspector/Label`.
  - pid-stamp rule: every eval returns `"pid": OS.get_process_id()`, compared against `godot_game instances`; re-establish assumed state if the pid changes.
  - recipes: API place/break via `godot_exec call`; mouse place/break with camera-resolved coordinates; pause + `step`; `get_state_json` assertions; `godot_screenshot game`; `godot_log` reads.
  - Part 2: module name `mindustry_godot`; local publish (`server/build.sh`), `spacetime logs/sql/describe`; the rule that gameplay is driven through MCP, never CLI reducers.
  - mirror copies under `mindustry-godot/.kimi-code/skills/playtest/` when that harness is used (matching `main/`).

**Verify:** `tools/ci.sh` green end-to-end; `tools/mcp-smoke.sh` green; skill frontmatter parses; a cold agent can follow the skill from launch to screenshot without other docs.

### M8 — Foundation exit gate

Run the full §7e checklist, record evidence in the Changelog, and announce P0 complete (unblocks `01`–`05`).

## 6. Data & formats

### 6.1 Scenario format (`scenarios/*.json`, `format: 1`)

```json
{
  "format": 1,
  "name": "spine_place_break",
  "seed": 1,
  "world": { "generator": "flat", "width": 32, "height": 32, "floor": "stone", "wall": "air" },
  "rules": { "waves": false, "infinite_resources": true },
  "commands": [
    { "tick": 0, "op": { "type": "select_block", "block": "stone-wall" } },
    { "tick": 0, "op": { "type": "place", "x": 4, "y": 4, "block": "stone-wall" } },
    { "tick": 30, "op": { "type": "place", "x": 5, "y": 4, "block": "stone-wall" } },
    { "tick": 45, "op": { "type": "break", "x": 4, "y": 4 } }
  ],
  "steps": 60,
  "expect": {
    "checksum": "0000000000000000",
    "tiles": [
      { "x": 4, "y": 4, "block": "air" },
      { "x": 5, "y": 4, "block": "stone-wall" }
    ]
  }
}
```

- Commands are applied before the tick they are stamped with executes; equal ticks preserve file order (stable sort).
- `expect.checksum` is filled at M3 and becomes the golden; `expect.tiles` is a sparse assertion list.
- `generator` variants are append-only (`flat` at P0; plan 06 adds real generators).
- `mind-headless list` prints registered scenario names; canonical files live at repo-root `scenarios/` and are mirrored to `client/scenarios/` for in-engine `load_scenario`.

### 6.2 Command-log format (`*.jsonl`, `format: 1`)

One JSON object per line: `{"tick": <u64>, "op": {"type": "...", ...}}`. `replay` applies the same semantics as a scenario’s `commands`. `op` variants are append-only; this file is the seed of the plan-21 relay payload.

### 6.3 State dump JSON (`format: 1`)

```json
{
  "format": 1,
  "app": "Mindustry-Godot",
  "version": "0.1.0",
  "tick": 60,
  "seed": 1,
  "phase": "playing",
  "checksum": "9f2c000000000000",
  "world": {
    "width": 32, "height": 32, "sparse": true,
    "tiles": [ { "x": 5, "y": 4, "block": "stone-wall", "team": 0, "rot": 0, "build_id": 1 } ]
  },
  "entities": [ { "id": 1, "kind": "building", "x": 5, "y": 4, "block": "stone-wall" } ],
  "events": [ { "tick": 0, "kind": "block_placed", "x": 4, "y": 4, "block": "stone-wall" } ],
  "commands_applied": 4
}
```

- `world.tiles` is sparse (non-air only) and sorted by `(y, x)`; `--all-tiles` emits every tile.
- `entities` is sorted by monotonic `EntitySeq`; at P0 it contains placed blocks only.
- New fields append; `format` bumps only for breaking changes; `SimHost.get_state_json()` returns this exact schema (inspector protocol).

### 6.4 Checksum stream (deterministic)

`xxh3_64` over a little-endian canonical byte stream: `DUMP_FORMAT` (u32) · `tick` (u64) · `seed` (u64) · `phase` (u8) · for each tile row-major by `(y, x)`: `block_id` (u16) · `team` (u8) · `rot` (u8) · `build_id_present` (u8) · `build_id` (u64) · then each entity in `EntitySeq` order (id, kind, fields). Printed as 16 lowercase hex digits. Changing this stream is a breaking change and updates all goldens in the same commit.

### 6.5 Other formats and files

- `.gdextension`: §3.4. Entry symbol `gdext_rust_init`; `compatibility_minimum = 4.7` (gdext `api-4-7`).
- `settings.json` (`user://settings.json`, written only by `mind-gdext`): `{ "format": 1, "selected_block": "stone-wall", "zoom": 1.0 }`; plan 04 replaces it with the full `Settings` port.
- Log file: text, one `[D|I|W|E] message` per line, `<data-dir>/last_log.txt`.
- Data directory: default `~/.local/share/Mindustry-Godot/` on Linux/WSL (honors `$XDG_DATA_HOME`); engine `user://` maps to `~/.local/share/godot/app_userdata/Mindustry-Godot/`; override `--data-dir` / `MINDUSTRY_GODOT_DATA_DIR` (mirrors `Vars` directory layout).
- STDB skeleton: §3.7; generated accessor names (`Player`) are the wave strings for plan 01.
- Scenario mirror: `tools/sync_scenarios.*` copies `scenarios/*.json` → `client/scenarios/` (gitignored); CI asserts sync.
- `rust-toolchain.toml`: `channel = "1.98.1"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`.
- `.gitignore` root entries: `client/.godot/`, `client/bin/`, `client/rust/target/`, `server/spacetimedb/target/`, `client/scenarios/`, `*.log`, `last_log.txt`, token files (`client/**/identity/`). Generated STDB bindings are **not** ignored (checked in).

## 7. Oracle & verification (REQUIRED)

### 7a. Ported Mindustry tests

Inventory source: `Mindustry/tests/src/test/java/**` — `ApplicationTests.java`, `DataAssetTests.java`, `PatcherTests.java`, `LogicTests.java`, `GenericModTest.java`, `ModTestAllure.java`, `power/{PowerTestFixture,PowerTests,DirectConsumerTests,ConsumeGeneratorTests}.java`. Full mapping below; P0 ports are marked **P0**.

| Mindustry test | Rust test target | Plan |
|---|---|---|
| `ApplicationTests.timers` | **P0** `mind_core::time::tests::timer_runs_after_two_updates` | 00 |
| `ApplicationTests.manyTimers` | **P0** `mind_core::time::tests::many_timers_same_update` | 00 |
| `ApplicationTests.longTimers` | **P0** `mind_core::time::tests::long_timers_catch_up` | 00 |
| `ApplicationTests.createMap` | **P0** `mind_core::world::tests::resize_and_fill` | 00 |
| `ApplicationTests.initialization` (non-empty content map) | **P0** `mind_core::content::tests::spine_registry_non_empty` | 00 |
| `ApplicationTests.edges` | `mind_core::world::edges::tests::edge_order` | 06 |
| `ApplicationTests.playMap` / `blockOverlapRemoved` | `mind_core::world::tests::load_map_*` | 06 |
| `ApplicationTests.multiblock` / `blockInventories` | `mind_core::world::blocks::tests::*` | 07 |
| `ApplicationTests.buildingOverlap` / `buildingDestruction` / `inventoryDeposit` | `mind_core::world::blocks::tests::*` | 07/08 |
| `ApplicationTests.writeStringTest` / `writeRules` / `writeRules2` | `mind_core::io::typeio::tests::*` | 04 |
| `ApplicationTests.save` / `saveLoad` / `load{77,85,108,114,152,152BE}Save` | `mind_core::io::save::tests::*` (legacy `.msav` fixtures per OD2) | 04 |
| `ApplicationTests.conveyorCrash` / `conveyorBench` / `routerOutputAll` / `sorterOutputCorrect` / `junctionOutputCorrect` | `mind_core::world::blocks::logistics::tests::*` | 08 |
| `ApplicationTests.liquidOutput` / `liquidJunctionOutput` / `liquidRouterOutputAll` | `mind_core::world::blocks::liquids::tests::*` | 09 |
| `power.PowerTests` / `DirectConsumerTests` / `ConsumeGeneratorTests` (+ fixture) | `mind_core::world::blocks::power::tests::*` | 09 |
| `ApplicationTests.allBlockTest` / `allPayloadBlockTest` | `mind_core::content::tests::*` / `mind_core::world::payloads::tests::*` | 02/08 |
| `ApplicationTests.testSectorValidity` | `mind_core::game::campaign::tests::sector_validity` | 12 |
| `ApplicationTests.spawnWaves` | `mind_core::game::waves::tests::spawn_waves` | 11 |
| `LogicTests` (escapes, sanitize, parse values/colors, invalid numbers, unterminated strings, CRLF) | `mind_core::logic::lparser::tests::*`, `mind_core::logic::lassembler::tests::*` | 13 |
| `DataAssetTests` / `PatcherTests` | `mind_core::mods::*::tests` | 20 |
| `GenericModTest` / `ModTestAllure` (network) | `mind_core::mods::tests::*` (`#[ignore]`, network-gated) | 20/23 |

New P0-only tests (no upstream equivalent): `sim::tests::same_seed_same_checksums`, `sim::tests::command_log_replay_matches_direct_run`, `dump::tests::json_roundtrip`, `event::tests::listeners_fire_in_registration_order`.

### 7b. Headless harness scenarios

| Scenario | Seed / world | Script | Assertions |
|---|---|---|---|
| `spine_place_break` | seed 1, flat 32×32 | place (4,4) t0; place (5,4) t30; break (4,4) t45; 60 steps | golden checksum; sparse tiles (5,4)=`stone-wall`, (4,4)=`air`; dump schema validates |
| `spine_determinism` | seed 2, flat 64×64 | 200 deterministic pseudo-random commands (derived from seed); 600 steps | two in-process runs, a second-process run, and `replay` of the emitted JSONL all yield identical per-tick and final checksums |
| `spine_many_commands` | seed 3, flat 32×32 | 10 000 interleaved place/break ops | completes < 1 s release; checksum stable |
| `bench_baseline` | seed 0, flat 64×64, 256 blocks | `bench` 100 000 ticks | emits p50/p99 JSON; compared against committed baseline within 20% |

Ad-hoc runs: `mind-headless sim 600 --seed 1 --dump out.json` (schema smoke) and `mind-headless dump --scenario spine_place_break --out golden.json` (diffable golden).

### 7c. MCP playtest scenario (concrete)

Preconditions: build via `tools/build.sh`; `godot_health check`; if BRIDGE_NOT_CONNECTED, launch the editor per the repo skill (background: `nohup godot4 --editor --path /home/c/g/code_examples/mindustry-godot/client >/tmp/mind-editor.log 2>&1 &`), wait ~20 s, `godot_instance list`. The bridge binds **127.0.0.1:6970**: if another Godot editor (e.g. the sibling `main/` project, Windows-side) already holds 6970, the MCP server will keep talking to *that* editor. Close the other editor before launching, then verify identity in step 0 — `godot_exec eval {"code":"return ProjectSettings.globalize_path(\"res://\")"}` must contain `mindustry-godot` — before any `godot_game` call.

1. `godot_editor_edit open_scene res://scenes/spine.tscn`; `godot_game play` with `scene: "res://scenes/spine.tscn"` passed explicitly.
2. Pid-stamp + liveness: `godot_exec eval {"code": "return {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"checksum\": str(get_node(\"/root/Spine/SimHost\").get_checksum())}"}` — record pid; compare with `godot_game instances`.
3. Load canonical scenario: `godot_exec call /root/Spine/SimHost load_scenario ["res://scenarios/spine_place_break.json"]` → expect `true`. Do **not** pause before stepping: §6.4 hashes `phase`, so `set_paused(true)` + `step(60)` yields `bf2ab23165daa498`, not the golden (M4 evidence, fixed here 2026-10-01).
4. Engine↔headless parity (while `Playing`): `godot_exec call /root/Spine/SimHost step [60]`; eval `get_tick()` = 60 and `get_checksum()` equals the golden checksum in `spine_place_break.json` (same seed/world/commands). Pause afterwards for the API/input steps.
5. API place/break: `godot_exec call /root/Spine/SimHost place_block [3, 5, "stone-wall"]` → `true`; eval `get_state_json()` contains `"x": 3`, `"y": 5`, `"stone-wall"`; `break_block [3,5]` → `true`; tile becomes `air`.
6. Real input path: for tile (7,7), eval `get_node("/root/Spine/World/Camera2D").tile_to_screen(7, 7)`; `godot_input mouse_button` left click at that viewport position; eval tile (7,7) is `stone-wall` and `get_tick()` advanced. Right click breaks it.
7. Inspector protocol: eval `get_node("/root/Spine/Ui/StateInspector/Label").text` contains `tick`, `checksum`, and the selected block; confirm it updates within ~500 ms of a `state_changed` emission.
8. Visual: `godot_screenshot game` → PNG on disk; verify the grid renders and the placed-block pixel differs from air (read the PNG; non-blank).
9. Logs: `godot_log errors` → no errors; `godot_log get` shows the `[I]` startup lines.
10. Teardown: `godot_game stop`.

`tools/mcp-smoke.sh` automates steps 1–6 and 9 and fails non-zero on any mismatch.

### 7d. Performance budget (spine) + measurement

| Metric | Budget (P0) | Measurement |
|---|---|---|
| Sim tick (`Sim::tick`, 64×64, 256 blocks) | p50 ≤ 100 µs, p99 ≤ 250 µs (well under the §7.4 4 ms mid-game budget) | `mind-headless bench --ticks 100000` → `{p50_us,p99_us}` JSON |
| Scenario run (`spine_determinism`, 600 ticks) | ≤ 300 ms release, ≤ 3 s debug | `cargo run -p mind-headless --release -- run spine_determinism` wall time |
| Checksum + full dump (64×64, `--all-tiles`) | ≤ 5 ms | `mind-headless dump` timing line |
| Godot frame (`_process` sim+sync, 64×64, idle) | ≤ 2 ms CPU | `godot_profiler snapshot`/`series` while the spine runs; corroborate with `Performance.get_monitor` via `godot_exec` |
| Cold start to first frame | ≤ 3 s on the dev host | windowed run wall time (mono host, debug build) |
| Binary sizes | `libmind_gdext.so` release ≤ 20 MB; `mind-headless` release ≤ 10 MB | `stat -c '%s %n' client/bin/rust/release/libmind_gdext.so client/bin/rust/release/mind-headless` |

Regressions block the P0 gate; `bench_baseline.json` stores the committed numbers. CI warns above +20% and fails above +50% until `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` owns the suite.

### 7e. Exit criteria checklist

- [x] `tools/ci.sh` green: `cargo fmt --check`, `clippy -D warnings`, `check`, `test -p mind-core`, boundary greps. (M8 final run, `CI_RC=0`)
- [x] P0 ported tests pass (`time::*`, `world::tests::resize_and_fill`, `content::tests::spine_registry_non_empty`). (29/29)
- [x] `mind-headless run spine_place_break` passes with the committed golden checksum. (`e53c9277bb8c28d1`)
- [x] `spine_determinism` passes across two processes and via `replay`. (`e435247bbe23afb1`)
- [x] Dump JSON validates against §6.3 and is byte-stable across runs.
- [x] Godot `--headless --editor --quit --path client` clean (no SCRIPT/SHADER parse errors) on the dev host.
- [x] Windowed spine: grid + place/break via Rust input, inspector overlay updates, `-- --capture` writes a PNG. (1152×648 PNG, M4)
- [x] MCP scenario §7c passes; every eval pid-stamped; `godot_log errors` clean. (`tools/mcp-smoke.sh` + orchestrator evals, golden parity)
- [x] Perf budget §7d met; `bench_baseline.json` records p50 101 ns / p99 240 ns (file in tree — first git commit pending user go-ahead; see M8).
- [x] `mind-stdb` + `server/spacetimedb` skeletons `cargo check`; local publish + `spacetime describe mindustry --json` OK (db name amended, see M8); bindings checked in and drift-clean.
- [x] Old C# tree deleted; zero `.cs` outside `client/addons/**`; `project.godot` dotnet-free; M6 greps clean.
- [x] `LICENSE`, `THIRD_PARTY_NOTICES.md`, ported-file headers present; root/child `AGENTS.md` written.
- [x] Repo `playtest` skill exists with the launch flow, node map and pid-stamp rule; smoke run recorded.
- [x] README documents build/run/verify in < 1 page; changelog below updated with evidence.

## 8. Risks & open decisions

All items below were resolved on 2026-10-01; the authoritative answer log is `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` §8.1.1. The defaults shown are retained unless the register says otherwise.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| OD-R1 | The repo is not yet a git repository; M6 deletes the only copy of the C# reference tree. | **Locked 2026-10-01 (NUD-05=C): delete the C# tree outright** — no git tag/archive, no `docs/port-history/` copy; keep it in place only until plans 00/01 stop referencing it, then delete at M6. | locked |
| OD-R2 | Godot renderer method for the whole port (plans 16–18 follow it): `forward_plus` vs `gl_compatibility`/`mobile`. | **Locked 2026-10-01 (NUD-06=B): `mobile` on all platforms** (desktop + Android); `gl_compatibility` is the documented emergency fallback. RenderingDevice-only paths (e.g. max-blend) do not apply. | locked |
| OD-R3 | How the standard (non-mono) Godot 4.7 build is obtained for the preferred host and CI. | **Locked 2026-10-01 (NUD-07); host updated to WSL:** standard Godot 4.7.2 is on PATH as `godot4` in WSL Ubuntu; mono (`godot4-mono`) is fallback only. Pin the same standard build in CI. | locked |
| OD-R4 | `spacetimedb-sdk` is async/tokio; `mind-core` is tokio-free. | `mind-stdb` owns a single dedicated worker thread + channels and pumps `frame_tick()` once per Godot frame from `mind-gdext`; no tokio type crosses the crate boundary. | no |
| OD-R5 | `bevy_ecs` version drift (0.19 stable; 0.20 rc). | Exact pin `=0.19.1`; upgrades are deliberate PRs that re-run the determinism goldens. | no |
| OD-R6 | gdext API level feature name (`api-4-7`) for Godot 4.7. | Use `features = ["api-4-7"]` on `godot 0.5.5`; if the feature name differs at M1, fall back to `api-custom` with a dumped 4.7 `extension_api.json` and record it here. | no |
| OD-R7 | Generated Rust bindings directory handling (HLP §12 C3). | **Checked in** at `client/rust/mind-stdb/src/module_bindings/`; `server/build.sh --check` is the drift gate. The `stdb` feature gate is retained only for optional builds. | locked |
| OD-R8 | Event dispatch order must be deterministic across peers. | Registration-order `Vec` dispatch; re-registration during a fire is a debug assert; plan 05 queues per-tick events to slot boundaries. | no |
| OD-R9 | Data-directory location (Godot `user://` vs OS app-data vs repo-local). | `~/.local/share/Mindustry-Godot/` (Linux/XDG default, honors `$XDG_DATA_HOME`) in both hosts; engine `user://` maps to `~/.local/share/godot/app_userdata/Mindustry-Godot/`; `--data-dir`/env override for test isolation. | no |
| OD-R10 | Input mapping: Godot `[input]` actions vs raw keycodes read in Rust. | Rust handles raw `InputEvent` keycodes/mouse at P0 (D1); the full `Binding`/keybind port is plan 15’s. | no |
| OD-R11 | Goldens are machine-dependent (float formatting, iteration). | Checksums use integer/canonical streams only (no float hashing at P0); dumps use structs/BTreeMap; goldens recorded at M3 and diffed byte-for-byte in CI. | no |
| OD-R12 | Retained third-party addons (`blastbullets2d`, `phantom_camera`) are unused at P0 and will be evaluated by plans 10/15. | Keep installed but not autoloaded; adoption recorded in `THIRD_PARTY_NOTICES.md` only when actually used. | no |
| OD-R13 | MCP bridge on the standard vs mono host. | MCP addon is GDScript-only and verified on standard 4.7.2 (`godot4`); `godot.sh` records the binary/port; the mono fallback is a binary-path change. | no |
| OD-R14 | `spine_determinism` command generator could mask real divergence if it reads wall-clock. | Generator derives entirely from the scenario seed via `JavaRandom`; no wall-clock, no `HashMap` iteration in sim code. | no |
| OD-R15 | `mind-core`/`mind-gdext` split may tempt shortcuts as systems land. | CI boundary greps (§3.3) fail the build if `mind-core` gains Godot/tokio imports; `mind-gdext` rule reviewed per PR. | no |

## 9. References

Repo-local: `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §1 port table, §2.1–2.4, §4 template, §5 P0 gate, §6 conventions, §7 verification, §8 addons, §10 OD1–OD9); `mindustry-godot/PRELIMINARY_PLAN.md`; `mindustry-godot/client/project.godot`; `mindustry-godot/client/Scripts/Components/README.md`; `mindustry-godot/client/sstdbsdk/{AGENTS.md,README.md,pointers.md,DatabaseConnector.cs,TableSubscriber.cs,TableBinderComponent.cs,DatabaseConnector.tscn}`.

Mindustry: `AGENTS.md`; `core/AGENTS.md`; `core/src/mindustry/AGENTS.md`; `core/src/mindustry/core/AGENTS.md`; `desktop/AGENTS.md`; `tests/AGENTS.md`; `tests/src/test/java/**`; `core/src/mindustry/Vars.java`; `core/src/mindustry/ClientLauncher.java`; `core/src/mindustry/core/Logic.java`; `core/src/mindustry/core/GameState.java`; `core/src/mindustry/core/World.java`; `core/src/mindustry/wiki`-adjacent paths cited in §4 (via the AGENTS indexes); `LICENSE`.

Tooling/reference projects: `/mnt/c/Users/Clinton/g/main/AGENTS.md`; `main/server/AGENTS.md`; `main/server/build.sh`; `main/server/spacetime.json`; `main/server/spacetimedb/Cargo.toml`; `/mnt/c/Users/Clinton/g/.opencode/skills/godot-compositor-testing/SKILL.md`; `/mnt/c/Users/Clinton/g/.opencode/skills/playtest/SKILL.md`; `godot4`/`godot4-mono` on PATH in WSL Ubuntu; `/mnt/c/Users/Clinton/g/code_examples/spacetimedb/licenses/` (license inventory); godot-rust releases (`godot 0.5.5`, `api-4-7`); crates.io pins (`bevy_ecs 0.19.1`, `spacetimedb 2.10.1`, `spacetimedb-sdk 2.10.1`).

## Changelog

> Append entries here when execution starts. Every “done” claim carries evidence (command, dump path, screenshot path, checksum).

### M0 — Workspace scaffolding and docs (2026-10-01) — complete

Created: `rust-toolchain.toml` (1.98.1, rustfmt+clippy, minimal), root `.gitignore`, `README.md`, `LICENSE` (GPL-3.0 text + derivative note), `THIRD_PARTY_NOTICES.md`, `client/rust/Cargo.toml` (workspace, resolver 3; `bevy_ecs =0.19.1`, `godot 0.5.5` + `api-4-7`, `spacetimedb-sdk =2.10.1`, plus `clap`/`anyhow`/`tokio` per §3.3; lints `unsafe_code` deny with a `mind-gdext` allow override, clippy `all`/`unwrap_used`/`expect_used` deny), and the four crates `mind-core` / `mind-headless` / `mind-gdext` / `mind-stdb` (SPDX headers, minimal compiling skeletons). `git init -b main` (no add/commit).

Evidence:

- `cargo metadata --manifest-path client/rust/Cargo.toml --no-deps --format-version 1` → `packages` = `['mind-core', 'mind-headless', 'mind-gdext', 'mind-stdb']`; default members `['mind-core', 'mind-headless', 'mind-stdb']`.
- `cargo check --manifest-path client/rust/Cargo.toml -p mind-core -p mind-headless -p mind-stdb` → `Finished dev profile [unoptimized + debuginfo] target(s) in 53.34s` (316 packages locked; `godot 0.5.5` with `api-4-7` resolves; `spacetimedb-sdk 2.10.1`; `bevy_ecs 0.19.1`).
- `cargo build --manifest-path client/rust/Cargo.toml -p mind-gdext` → `Finished dev profile ... in 3m 35s`; `client/rust/target/debug/libmind_gdext.so` (4,463,344 bytes) produced without Godot headers.
- `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets` clean.
- `git rev-parse --is-inside-work-tree` → `true` (branch `main`); `rg --version` → `ripgrep 15.2.0` (installed via `cargo install ripgrep --locked`).
- All new files LF/UTF-8 with trailing newline. Nothing under `client/` outside the new `client/rust/` tree was touched (`project.godot`, `client/Scripts/`, `client/sstdbsdk/` unchanged); no M1+ artifacts created.

Notes:

- Host prerequisite (no repo change): WSL Ubuntu 26.04 lacks `pkg-config`/`libssl-dev` and sudo needs a password, but `spacetimedb-sdk`'s `native-tls` requires OpenSSL headers. `libssl-dev` 3.5.5 was extracted user-locally to `~/opt/openssl-dev` (`apt-get download` + `dpkg-deb -x`, plus the `opensslconf.h`/`configuration.h` multiarch symlinks); cargo runs export `OPENSSL_INCLUDE_DIR`/`OPENSSL_LIB_DIR`. Install `pkg-config libssl-dev` system-wide on dev/CI hosts so plain `cargo` works.
- `spacetimedb-sdk`/`spacetimedb` 2.10.1 are **BSL-1.1** (Change Date 2031-09-08 → AGPL-3.0 with linking exception), not Apache-2.0; recorded in `THIRD_PARTY_NOTICES.md`.
- Follow-up (orchestrator, same day): the OpenSSL workaround was made durable with a user-level `~/.cargo/config.toml` (`[env] OPENSSL_INCLUDE_DIR`/`OPENSSL_LIB_DIR`/`PKG_CONFIG_PATH` → `~/opt/openssl-dev`), so plain `cargo check -p mind-stdb` / `cargo build -p mind-gdext` work without manual exports (verified). CI/dev hosts should install `pkg-config libssl-dev ripgrep` instead.

### M1 — Godot reset + "hello, gdext" (2026-10-01) — complete

Created `client/mind.gdextension` (`entry_symbol = gdext_rust_init`, `compatibility_minimum = 4.7`, reloadable, win/linux/macos paths under `res://bin/rust/{debug,release}/`), `client/rust/mind-gdext/src/{hello.rs,log_bridge.rs}` (`MindHello` base `Node`, `#[func] crate_version`; `[D]/[I]/[W]/[E]` console/log bridge installed once at `InitStage::Scene`; panic hook), `client/scenes/hello.tscn` (tscn-first; `MindHello` declared as a node in the scene, no code instantiation), `tools/build.{sh,ps1}` (`cargo build -p mind-gdext -p mind-headless --target-dir client/bin/rust`, calls `sync_scenarios.*` when present), `tools/godot.{sh,ps1}` (`$GODOT_BIN` → `godot4` → `godot4-mono` fallback + install hint). `client/project.godot` reset per §3.4: `[dotnet]`/`assembly_name`/d3d12 removed, `config/features=PackedStringArray("4.7")`, `renderer/rendering_method="mobile"`, display stretch, Jolt physics; MCP autoload + editor plugin/settings kept; `run/main_scene` temporarily `res://scenes/hello.tscn`. `client/.godot/` deleted and regenerated. Added `client/rust/.gdignore` + `client/bin/.gdignore` so the editor scan skips the cargo trees over WSL 9p (plan 22 must confirm exports still pack the raw `.so`/`.dll`).

Evidence:

- `tools/build.sh` → `client/bin/rust/debug/libmind_gdext.so`; `tools/godot.sh --headless --quit-after 120` → `[I] mind-gdext loaded (mind-gdext 0.1.0)`, exit 0.
- `godot4 --headless --editor --quit --path client` → exit 0, zero SCRIPT/SHADER/Parse errors; `extension_list.cfg` lists `res://mind.gdextension`; `grep -R "dotnet\|DotNet\|C#" client/project.godot` empty.
- Deferrals: `user://last_log.txt` half of the log bridge → M4 (§3.6); a cold `.godot/` needs one `--headless --editor --quit` import before runtime runs (M7 CI ordering); phantom_camera autoload/editor-plugin entry removed per OD-R12 (addon stays installed).

### M2 — `mind-core` spine + portable tests (2026-10-01) — complete

Implemented `version`, `config`, `content` (placeholder registry `air=0`, `stone-wall=1`, append-only asserted), `game`, `command`, `random` (`JavaRandom` 48-bit LCG + `Rand`), `time` (`delta`/run queue parity subset), `event` (`define_events!` + registration-order `Events`), `log` (`MindLogger`, `[D]/[I]/[W]/[E]`), `world::WorldGrid`, `ecs`, `schedule::SimSet` (reserved `Logic.updateEntities()` order; only commands + Buildings populated at P0), `sim::{Sim, FixedStepRunner, dump}` and the scenario/command-log module.

Evidence:

- `cargo test -p mind-core` → **29 passed / 0 failed**, including `time::tests::{timer_runs_after_two_updates, many_timers_same_update, long_timers_catch_up}`, `world::tests::resize_and_fill`, `content::tests::spine_registry_non_empty`, `sim::tests::{same_seed_same_checksums, command_log_replay_matches_direct_run}`, `sim::dump::tests::json_roundtrip`, `event::tests::listeners_fire_in_registration_order` (plus JavaRandom parity vectors, stable-sort, fixed-step tests).
- Boundary clean: `cargo tree -p mind-core --prefix none | grep -E '^(godot|tokio)( |$)'` empty; `rg "use godot|use tokio" client/rust/mind-core` empty; fmt/`clippy -D warnings` clean for the crate.
- Notes: upstream Arc `Rand` in this revision is xorshift128+; the plan pinned `JavaRandom` LCG so that is what shipped (parity vectors pinned in tests). The checksum entity stream is documented in `Sim::checksum` (`id u64 · kind u8=0 · block u16 · team u8 · rot u8 · x i16 · y i16`); tile stream matches §6.4. Scenario schema extensions are append-only (`command_generator`, `emit_per_tick`, `expect.bench`).

### M3 — `mind-headless` harness + goldens (2026-10-01) — complete

CLI (`list/run/sim/replay/dump/bench`), lib+bin split (HLP §12 C8), Rust scenario registry, canonical `scenarios/*.json` (§6.1), `tools/sync_scenarios.{sh,ps1}` (mirror → `client/scenarios/`, gitignored), minimal `tools/ci.{sh,ps1}` with the §3.3 boundary greps. Exit codes 0/1/2 honored.

Evidence (goldens recorded in the scenario files):

- `spine_place_break` (seed 1, 32×32, 60 steps) → checksum **`e53c9277bb8c28d1`**, `"pass": true`, `in_process_stable: true`, `commands_applied: 4`; two processes byte-identical; `replay` of the emitted JSONL identical.
- `spine_determinism` (seed 2, 64×64, 200 seeded ops, 600 steps) → **`e435247bbe23afb1`**; two processes + all 600 per-tick checksums + replay identical.
- `spine_many_commands` (seed 3, 32×32, 10 000 ops) → **`faec40ccbe6ff9d8`**, 84 ms release (< 1 s).
- `bench --ticks 100000` (64×64, 256 blocks) → p50 101 ns / p99 240 ns (budget 100/250 µs); `spine_determinism` release wall 100–120 ms (≤ 300 ms); all-tiles 64×64 dump 2.1–2.9 ms (≤ 5 ms); `bench_baseline.json` carries `cb1b0594f3f4285e` + `p50_us: 1` / `p99_us: 1` (ns truth in the bench report).
- `tools/ci.sh` / `tools/ci.ps1` green; `sync_scenarios.sh/.ps1` produce the 4-file mirror.

### M4 — In-engine spine (D4 rig) (2026-10-01) — complete

`MindSimHost` (fixed 60 Hz `FixedStepRunner`, `_input` place/break, every §3.5 `#[func]` incl. `place_block`/`break_block`/`get_state_json`/`get_checksum`/`get_tick`/`is_paused`/`set_paused`/`step`/`load_scenario`/`selected_block`/`select_block`/`capture`; `state_changed(tick, checksum)` + `world_changed` signals; `-- --capture <path>`), `MindCamera2D` (`screen_to_tile`/`tile_to_screen` [tile center in viewport coords]/`center_on_tile`, WASD+edge pan, zoom 0.25–4.0), `MindTileGrid` (`_draw` grid + one quad per non-air block, `set_host`/`redraw_requested`/`tile_blocks`), `client/scenes/spine.tscn` (§3.5 tree, all static nodes tscn-declared; zero code-instantiated nodes), GDScript inspector `client/ui/state_inspector.gd` + `client/scenes/ui/state_inspector.tscn` (250 ms poll + `state_changed`, read-only), the deferred `user://last_log.txt` half of the log bridge, minimal `user://settings.json` (§6.5; plan 04 replaces).

Evidence (orchestrator re-verified at the join, pid-stamped):

- `tools/build.sh` ok; `cargo fmt/clippy -p mind-gdext` clean; `godot4 --headless --editor --quit --path client` exit 0, zero SCRIPT/SHADER/Parse errors; headless run logs `[I] MindSimHost ready (32x32 seed 1 selected 'stone-wall', mind-core 0.1.0)`.
- MCP §7c: identity `.../mindustry-godot/client`, scene `res://scenes/spine.tscn`; eval `load_scenario` + `step(60)` while `Playing` → `tick=60`, **`checksum=e53c9277bb8c28d1` / `golden_match=true`** (orchestrator run, game pid 84560); `place_block(3,5,"stone-wall")`/`break_block(3,5)` → `true`; inspector label contains `tick` + `checksum`; `godot_screenshot game` 800×450 / 33 148 B; `godot_log errors` clean; `godot_game stop` ok. Windowed `-- --capture` writes a 1152×648 PNG; headless capture fails on the dummy renderer (exit 1) by design.
- Fixed during M4: `state_inspector.tscn` was missing its `script = ExtResource(...)` line (caught by MCP); corrected and re-verified.
- §7c corrected above (pause changes the checksum because `phase` is hashed): order is load → step(60) in `Playing` → pause → API/input steps. M7's `tools/mcp-smoke.sh` must follow this order.

### M5 — STDB + `mind-stdb` skeleton + publish workflow (2026-10-01) — complete

`server/spacetimedb` crate (module `mindustry_godot`: `player` table with `identity` pk, `init` seed marker, `client_connected`/`client_disconnected`, `set_username` ≤ 40 chars per `Vars.maxNameLength`), `server/spacetime.json`, `server/build.{sh,ps1}` (publish + generate + `--check`/`-Check` drift mode, `--db`/`--server` overrides), `mind-stdb` tree per §3.7 (`config`/`tokens`/`waves`/`binder`/`conn`, live ops `MindDbError::NotImplemented` per OD-R4) plus checked-in `src/module_bindings/{mod,player_table,player_type,set_username_reducer}.rs` generated by CLI 2.10.1, and the CI additions (`cargo check -p mind-stdb`, server `--tests` typecheck, drift gate).

Evidence:

- `cargo check -p mind-stdb` and `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` clean; `tools/ci.sh` green end-to-end including the new checks.
- `server/build.sh` publishes the module and regenerates bindings; `spacetime describe mindustry --json` lists table `player` (pk `identity`, unique index) and reducer `set_username` (`ClientCallable`); `server/build.sh --check` drift-clean; a deliberate 2-byte corruption made `--check` exit 1 with a diff, then was restored.
- **Final naming (user decision, recorded in plan 01 + 23):** STDB 2.10.1 rejects underscores in *database* names (`^[a-z0-9]+(-[a-z0-9]+)*$`), so NUD-08's literal db `mindustry_godot` cannot be published. Crate/module stays `mindustry_godot`; local db is **`mindustry`** (integration `mindustry-it`), overridable via `--db`.
- 2.2.0→2.10.1 deltas recorded for plan 01 (accessor-trait imports required for `ctx.db.*`; `--delete-data=always`; `spacetime describe --json`; `spacetime generate` needs the `wasm32-unknown-unknown` target; token layout `<data-dir>/identity/<key>.token.json` vs plan 01 §6.6's single-file store — reconcile in 01 M2).
- M5 addendum (orchestrator): wired the deferred `-- --db [--db-host H] [--db-name N]` flag into `MindSimHost` (constructs the `MindDb` facade, pumps `frame_tick()` exactly once per frame independent of pause, `disconnect()` on exit; offline-safe) and corrected `mind-stdb` `DEFAULT_DB_NAME` to the final `mindustry`; fmt/clippy clean; headless `-- --db` logs `[I] mind-stdb facade created for mindustry (state offline)`.

### M6 — C# deletion and migration (2026-10-01) — complete

Captured the legacy semantics into `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` (new `## Legacy C# migration notes…` section: token key/persistence + replace chain, subscription-wave lists/timing, binder row semantics, frame pump, reconnect/backoff, component-framework→Bevy-ECS map, and the M5 2.10.1 deltas), then deleted per NUD-05=C with no archive: `client/Scripts/` (19 files incl. `Components/README.md`) and `client/sstdbsdk/` (11 files incl. `DatabaseConnector.tscn` + docs). `client/Scripts/Entities/` was already absent from this copy.

Evidence: `find client -name '*.cs' -not -path 'client/addons/*' | wc -l` → `0` (addons keep 13 `.cs`); `grep -c 'dotnet\|DotNet' client/project.godot` → `0`; no `Scripts/`/`sstdbsdk/` references in `client/scenes`, `client/ui`, `client/project.godot`; `godot4 --headless --editor --quit --path client` → exit 0.

### M7 — CI, docs, repo playtest skill (2026-10-01) — complete

`tools/ci.sh` extended to the full §3.8 set (workspace fmt/clippy/check, `cargo test -p mind-core` 29/29, the three golden scenarios, scenario-mirror diff, ns-honest bench gate 101/240 ns with +20 % warn / +50 % fail, Godot build + headless import check, boundary greps, `mind-stdb` + server `--tests` checks, bindings drift gate); `tools/ci.ps1` verified. `.github/workflows/ci.yml` added (rust ubuntu+windows, spacetimedb incl. `wasm32` target + drift, godot-import with pinned 4.7.2 standard; no secrets; no live publish). `tools/mcp_smoke.py` (stdlib-only stdio MCP driver) + `tools/mcp-smoke.{sh,ps1}` automate §7c steps 1–6/9 with pid-stamps and golden assertions. Docs: root `AGENTS.md` + `README.md` moved to executed state; `client/AGENTS.md` + `server/AGENTS.md` stubs; repo skill `.opencode/skills/playtest/SKILL.md` with `.kimi-code` mirror.

Evidence:

- M8 final run: `tools/ci.sh` → `CI_RC=0`; `tools/mcp-smoke.sh` → all steps `ok`, `load_scenario + step(60) → checksum=e53c9277bb8c28d1`; `.ps1` twins green; skill frontmatter parses and mirrors are byte-identical; workflow YAML parses (`yaml.safe_load`).
- Hardened `tools/mcp_smoke.py` after one transient failure: the headless editor import can briefly hold/release the bridge, so the health probe now retries up to 20 s; re-run green.
- MCP call shape documented for the skill/driver: tools take `(action, params)` with arguments nested under `params`; identity preflight uses `godot_editor_read state` before play plus the runtime `res://` check inside the pid-stamp eval after play.

### M8 — Foundation exit gate (2026-10-01) — P0 complete

All §7e boxes are checked with the evidence in the entries above. Two recorded amendments: (1) STDB db name is `mindustry` (crate/module stays `mindustry_godot`; integration `mindustry-it`); (2) git commits are pending user go-ahead — the repo is `git init -b main` with **zero commits** and 39 untracked paths, so every artifact above exists in the working tree and is reproducible with the recorded commands.

**P0 gate (`HIGH_LEVEL_PLAN.md` §5):** headless harness passes; in-engine spine runs (camera + grid + place/break); MCP can drive it end-to-end; CI green. **Unblocks F1** (plans `01` + `02`, plus the `23` harness lane) per §5.1.



