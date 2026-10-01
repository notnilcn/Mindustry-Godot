# Mindustry-Godot

A full 1:1 parity port of [Mindustry](https://github.com/Anuken/Mindustry) (Java + Arc,
GPL-3.0) to a **pure-Rust Godot 4.7 client** (`godot-rust` GDExtension + `bevy_ecs` as a
library) with a **SpacetimeDB** backend. No C# anywhere. The P0 foundation is landed: a
deterministic headless oracle, the in-engine spine (camera + tile grid + place/break +
inspector), an STDB skeleton, CI, and the repo playtest skill. The full plan set is
`00_FOUNDATION_IMPLEMENTATION_PLAN.md` … `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`;
[`HIGH_LEVEL_PLAN.md`](HIGH_LEVEL_PLAN.md) is the constitution, [`AGENTS.md`](AGENTS.md)
the agent guide.

## Layout

| Path | Contents |
|---|---|
| `client/` | Godot 4.7 project — scenes/GDScript UI only (see `client/AGENTS.md`). |
| `client/rust/` | Cargo workspace: `mind-core` (Godot-free sim), `mind-headless` (oracle), `mind-gdext` (GDExtension), `mind-stdb` (STDB client). |
| `server/` | SpacetimeDB module crate + publish scripts (see `server/AGENTS.md`). |
| `scenarios/` | Canonical headless scenarios; goldens live here. |
| `tools/` | `build`, `godot`, `ci`, `sync_scenarios`, `mcp-smoke` (bash + PowerShell twins). |

## Build, run, verify (WSL2 Ubuntu, from the repo root)

```bash
# Rust checks (mind-core must stay Godot-free/tokio-free)
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path client/rust/Cargo.toml -p mind-core

# Headless oracle (goldens: spine_place_break -> e53c9277bb8c28d1)
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- run spine_place_break --json
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- bench --ticks 100000

# Engine
tools/build.sh                 # mind-gdext + mind-headless -> client/bin/rust, syncs scenarios
godot4 --path client           # opens res://scenes/spine.tscn

# Server (local publish wipes dev data; --check is the bindings drift gate)
server/build.sh
server/build.sh --check
spacetime describe mindustry --server local

# Gates
tools/ci.sh                    # full local gate (fmt/clippy/tests/goldens/bench/godot/STDB)
tools/mcp-smoke.sh             # in-engine MCP smoke (needs the editor running)
```

From a Windows terminal prefix WSL commands with `wsl -d Ubuntu -e bash -lc '<command>'`;
the `.ps1` twins are Windows parity. In-engine workflows (launch, node map, pid-stamped
evals) are in [`.opencode/skills/playtest/SKILL.md`](.opencode/skills/playtest/SKILL.md);
`.github/workflows/ci.yml` runs the Rust/STDB/Godot-import jobs on push/PR to `main`.

## License

GPL-3.0 (derivative of Mindustry). See `LICENSE` and `THIRD_PARTY_NOTICES.md`.
