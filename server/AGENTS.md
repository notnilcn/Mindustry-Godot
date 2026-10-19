# AGENTS.md — server/ (SpacetimeDB)

The SpacetimeDB side of Mindustry-Godot: the `mindustry_godot` module plus the tooling that publishes it and regenerates the Rust client bindings. The server owns authoritative persistent rows, relays ordered client commands, and performs cheap validation only; the determinism-critical simulation runs on the clients. Read the root [`AGENTS.md`](../AGENTS.md) first. Tables, reducers, views and schema ABI are owned by [`server/spacetimedb/AGENTS.md`](spacetimedb/AGENTS.md); client binders and subscription waves by [`client/rust/mind-stdb/AGENTS.md`](../client/rust/mind-stdb/AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `spacetimedb/` | The module crate (`cdylib`, `edition = "2024"`, `spacetimedb = "=2.10.1"`). Declares its own `[workspace]`, so it is NOT a member of `client/rust/`. |
| `spacetime.json` | Project defaults: `server: local`, `module-path: ./spacetimedb`, Rust generate target `../client/rust/mind-stdb/src/module_bindings`. |
| `build.sh` | Canonical publish script: `spacetime generate` into the bindings dir, then `spacetime publish --delete-data=always`. |
| `gen_content_seed.sh` | Regenerates `spacetimedb/src/main/content_seed.rs`; invoked by `build.sh` when `GEN_CONTENT_SEED=1`. |

## Publish flow

- `bash server/build.sh` regenerates the checked-in bindings and publishes DB `mindustry` to `--server local`; `--db NAME` and `--server HOST` override those defaults. Publishing always passes `--delete-data=always`, so local data is discarded and seeds re-run from code. The script is checked in non-executable, so invoke it through `bash`.
- `bash server/build.sh --check` regenerates into a temporary directory and diffs against `client/rust/mind-stdb/src/module_bindings` without publishing; it exits non-zero on drift.
- Generated bindings are checked in and never hand-edited.
- `spacetime generate` requires the `wasm32-unknown-unknown` Rust target; the CLI, crate and bindings are pinned to `2.10.1`.
- Database names match `^[a-z0-9]+(-[a-z0-9]+)*$` (no underscores): the local DB is `mindustry`, integration tests use `mindustry-it`, and the module/crate keeps `mindustry_godot`.
- No secrets or tokens are committed; local STDB identities live outside the repo, and `client/**/identity/` is gitignored.

## Commands

```bash
bash server/build.sh                              # publish mindustry + regenerate bindings
bash server/build.sh --check                      # bindings drift gate (no publish)
bash server/build.sh --db my-db --server local    # alternate database
GEN_CONTENT_SEED=1 bash server/build.sh           # also regenerate content_seed.rs
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests
spacetime describe mindustry --server local --json  # tables/reducers/views
spacetime logs mindustry --server local      # module logs
spacetime sql mindustry --server local "SELECT * FROM player LIMIT 10"
```

Runtime client/server verification runs through the Godot MCP tools rather than CLI reducer calls; use the `playtest` skill for the launch flow and MCP recipes.
