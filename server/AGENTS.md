# AGENTS.md — `server/` (SpacetimeDB)

The SpacetimeDB module for Mindustry-Godot: persistent state, command relay and cheap validation (D2). A server-authoritative match simulation is deferred but must not be precluded. The client-side SDK port lives separately in `client/rust/mind-stdb/`.

## Layout

| Path | Contents |
|---|---|
| `spacetimedb/` | Module crate (NOT a member of the client Rust workspace), module name `mindustry_godot`. P0 skeleton: `player` table + `init`, `client_connected`/`client_disconnected`, `set_username`. |
| `spacetime.json` | Local defaults + generate target (`../client/rust/mind-stdb/src/module_bindings`). |
| `build.sh` / `build.ps1` | Publish + regenerate bindings; `--check` / `-Check` is the drift gate (regenerates to a temp dir and diffs, never publishes). |

## Rules

- **Read [`main/server/AGENTS.md`](../../main/server/AGENTS.md) for the SpacetimeDB Rust rules** before writing any Rust here (reducer determinism, `ctx.sender()`, views vs tables, index naming, spread updates). Section 6.3 of [`HIGH_LEVEL_PLAN.md`](../HIGH_LEVEL_PLAN.md) summarizes: reducers never return data (clients read rows), cheap validation only, seeds live in code.
- Every publish runs `--delete-data=always` and wipes local dev data; connected clients drop. Seeds re-run from module code.
- Generated Rust client bindings (`client/rust/mind-stdb/src/module_bindings/`) are **checked in and never hand-edited**; regenerate via `build.sh` and let the drift gate verify.
- Database names match `^[a-z0-9]+(-[a-z0-9]+)*$` (STDB 2.10.1): the crate/module is `mindustry_godot`, the local DB is **`mindustry`** and integration tests use **`mindustry-it`**. `--db` overrides.
- `spacetime generate` needs the `wasm32-unknown-unknown` Rust target; CLI/crates/bindings are pinned to 2.10.1.
- No secrets/tokens committed; STDB token/identity files are gitignored.

## Commands (WSL2 Ubuntu, login shell)

```bash
server/build.sh                    # publish mindustry + regenerate bindings
server/build.sh --check            # bindings drift gate
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests
spacetime describe mindustry --server local # tables/reducers
spacetime logs mindustry --server local     # module logs
spacetime sql mindustry --server local "SELECT * FROM player LIMIT 10"
```

Runtime verification of client/server behavior goes through the Godot MCP tools, **not** CLI reducer calls — see [`.opencode/skills/playtest/SKILL.md`](../.opencode/skills/playtest/SKILL.md) Part 2. Plans: [`01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md`](../01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md), [`21_MULTIPLAYER_IMPLEMENTATION_PLAN.md`](../21_MULTIPLAYER_IMPLEMENTATION_PLAN.md).
