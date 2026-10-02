# AGENTS.md — Mindustry-Godot

A full 1:1 port of **Mindustry** (`../Mindustry/`, Java + Arc, GPL-3.0) to a **pure-Rust Godot 4.7 client + SpacetimeDB module**. The P0 foundation (plan 00) has landed: the headless oracle, the in-engine spine (`res://scenes/spine.tscn`), the STDB skeleton, CI, and the repo playtest skill all exist. The legacy C# tree (`client/Scripts/`, `client/sstdbsdk/`) is deleted — no C# anywhere.

> Upstream policy: Mindustry does not accept AI-written PRs. This is a local port; never submit generated code or plans upstream.

## Start here

1. **[`HIGH_LEVEL_PLAN.md`](HIGH_LEVEL_PLAN.md)** — locked decisions (§0), architecture (§2), the 24-plan set (§3), the mandatory plan template (§4), conventions (§6), verification tooling (§7), parity ledger (§9), open decisions (§10), reconciliation log (§12). **This file wins on any conflict.**
2. The target system’s **`{nn}_{SYSTEM}_IMPLEMENTATION_PLAN.md`** — execution order is the numbering; `Depends on` gates are enforced. `00_FOUNDATION_IMPLEMENTATION_PLAN.md` defines the spine and its extension contract (§3.10).
3. The Mindustry source docs named by that plan (every `Mindustry/**/AGENTS.md`).
4. Repo-local skills: **`.opencode/skills/playtest/SKILL.md`** (drive the running client + STDB CLI), plus the global `godot-compositor-testing` skill for Godot parse checks.

## Locked decisions (2026-10-01, full text in HIGH_LEVEL_PLAN §0)

- **Pure Rust client**: `godot-rust` GDExtension + `bevy_ecs` as a library. No C#. The legacy `sstdbsdk` is rewritten as the `mind-stdb` crate.
- **SpacetimeDB = persistent state + command relay + cheap validation**; a server-authoritative match sim is deferred but must not be precluded (D2).
- **Full parity** with Mindustry; strict dependency order; the foundation spine + `mind-headless` harness came first (D3/D4).
- Every plan carries **Oracle & verification** (ported tests + headless scenarios + MCP scenario + perf budget) — D5.
- **GPL-3.0**; `LICENSE` + `THIRD_PARTY_NOTICES.md` + per-file ported headers (D6).
- Fixed **60 Hz** sim; no Godot physics for sim entities (D8).
- **Godot conventions**: `.tscn`-first nodes/scenes, editor-navigable; code-instantiated nodes require a justified `# code-instantiated: <reason>` comment (HIGH_LEVEL_PLAN §6.6).

## Layout

```
client/                  # Godot 4.7 project (scenes/GDScript UI only) — see client/AGENTS.md
  scenes/ ui/ addons/    # spine.tscn, inspector overlay, open_godot_mcp bridge
  rust/                  # Cargo workspace: mind-core, mind-headless, mind-gdext, mind-stdb
  bin/ .godot/ scenarios/  # gitignored build output, editor cache, scenario mirror
server/                  # STDB module crate + publish scripts — see server/AGENTS.md
scenarios/               # canonical headless scenarios (goldens live here)
tools/                   # build/godot/ci/sync_scenarios + mcp-smoke (sh + ps1 twins)
.github/workflows/ci.yml # rust (ubuntu+windows), spacetimedb, godot-import
{nn}_{SYSTEM}_IMPLEMENTATION_PLAN.md   # 00..23, root level
```

`mind-core` is **Godot-free and tokio-free** — all sim/content/world/io/campaign logic lives there and must stay testable with plain `cargo test`. CI fails if the boundary is broken.

## Folders to treat carefully

- `client/rust/mind-stdb/src/module_bindings/` — generated STDB client bindings. **Never read or hand-edit**; regenerate with `server/build.sh`; CI drift gate is `server/build.sh --check` (`.ps1` twin uses `-Check`).
- `client/.godot/`, `client/bin/`, `client/rust/target/`, `server/spacetimedb/target/`, `client/scenarios/` — caches / build output / generated mirror.
- `Mindustry/**/gen/**` in the source repo does not exist (generated); never look for it.

## Build & run (WSL2 Ubuntu-first)

All commands run in **WSL2 Ubuntu** (login shell — `godot4`, `cargo`, `spacetime` on PATH). From a Windows terminal, prefix `wsl -d Ubuntu -e bash -lc '<cmd>'`. Repo path in WSL: `/mnt/c/Users/Clinton/g/code_examples/mindustry-godot`.

- Rust checks: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p mind-core` (all with `--manifest-path client/rust/Cargo.toml`).
- Headless oracle: `cargo run -p mind-headless -- run <scenario> --json` / `sim` / `replay` / `dump` / `bench` (goldens: `spine_place_break` → `2033eb5b4ec1206d`).
- Build + engine: `tools/build.sh`; `godot4 --path client` runs `res://scenes/spine.tscn`.
- Server: `server/build.sh` (publish + generate; `--check` drift mode); local DB is `mindustry`, module `mindustry_godot`.
- Full local gate: **`tools/ci.sh`** (`.ps1` twin); in-engine smoke: **`tools/mcp-smoke.sh`** (`.ps1` twin, needs the editor running).

## Verification

Automated checks stop at compilation + headless tests: `cargo test -p mind-core` and `mind-headless` scenarios are the primary oracle. In-engine verification goes through **open-godot-mcp** (`godot_health` → `godot_game play` → `godot_input`/`godot_exec`/`godot_screenshot`/`godot_log`) with **pid-stamped** evals — follow the repo skill [`.opencode/skills/playtest/SKILL.md`](.opencode/skills/playtest/SKILL.md); always run `res://scenes/spine.tscn` and verify identity first. GDScript/scene parse checks use the global `godot-compositor-testing` skill. Server inspection only via the `spacetime` CLI; never drive gameplay through CLI reducer calls when an MCP path exists.

## Godot conventions (workflow rule)

Godot-first and editor-navigable. Prefer **nodes and scenes declared in `.tscn` files** over trees built in code; a human should be able to open the project in the Godot editor and navigate/inspect the game. Static UI, world scaffolding, autoloads and debug views belong in scenes. When code must instantiate nodes (spawner-produced units/buildings/bullets, pooled FX, data-driven list entries), add a comment at the instantiation site:

```gdscript
var unit = UNIT_SCENE.instantiate()  # code-instantiated: spawned by WaveSpawner at runtime; count/position are data-driven
```

The comment must give the **specific** reason (“spawned at runtime”, “pooled/high-churn”, “count driven by server rows”) — “easier in code” is not a justification. Rust owns behavior, not scene construction, for static content. Full rule: `HIGH_LEVEL_PLAN.md` §6.6.

## Rust naming

- Hand-written Rust follows the standard API guidelines: types/traits/enums/variants **UpperCamelCase**, crates/modules/functions/methods/variables/fields **snake_case**, constants/statics **SCREAMING_SNAKE_CASE**, short lowercase lifetimes, acronyms as words (`MindDb`, `JsonIo`, `MindUi`); generated `module_bindings/` and frozen Godot/JSON/STDB ABI names are exempt.

## Rules for agents

- Execute plans in gate order (`Depends on`), one milestone at a time; run the plan’s **Oracle & verification** before checking a box. With concurrent subagents, schedule only the ready groups in `HIGH_LEVEL_PLAN.md` §5.1 and honor the shared-resource mutexes in §5.2.
- Plan docs are the source of truth: if code must diverge, update the plan first; if a locked decision (HIGH_LEVEL_PLAN §0) is challenged, stop and ask the user.
- Decision items were all resolved on 2026-10-01; the authoritative answers live in `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` §8.1.1. Don’t silently flip a default; record any change in the owning plan and the register.
- Content IDs are append-only; content names, sprite region names and bundle keys are the parity/mod ABI — never rename them.
- Cite ported sources in comments (`// Ported from core/src/mindustry/...`), keep new files LF/UTF-8, no secrets.
