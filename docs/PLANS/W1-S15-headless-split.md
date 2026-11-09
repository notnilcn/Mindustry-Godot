<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S15 — Split `mind-headless` `exec.rs` (5967), `cli.rs` (2291), `server/commands.rs` (2709)

**Wave:** W1 (pure mechanical split, RULES §3).
**Owns:** `client/rust/mind-headless/src/exec.rs` → `exec/**`, `cli.rs` → `cli/**`, `server/commands.rs` → `server/commands/**`.
**Effort:** medium.

Read `client/rust/mind-headless/AGENTS.md` first. mind-headless has a crate-level
`cfg_attr(test, allow(unwrap_used, expect_used))` (lib.rs:11).

## `exec.rs`: all CLI command bodies (no tests; the header has SPDX only)

`git mv exec.rs exec/mod.rs`. `mod.rs` keeps 1–92 (consts, the quiet flag, `run`, logging), the
`dispatch` match (93–449, **one match; leave it whole**), and the shared helpers (`print_report`
~2635, `is_quiet`, `SuiteCase`/`SuiteReport`/`run_catalog_entries`/`sample_checksums`, which are
`pub(crate)`). Move each helper to the file it belongs to by topic. Command bodies go to:

| File | Lines at `291e30a8` | Commands |
|---|---|---|
| `exec/assets.rs` | 450–874 | assets |
| `exec/io.rs` | 875–1480 | io |
| `exec/meta.rs` | 1481–1553 | meta/trace |
| `exec/mods.rs` | 1554–2417 | mods (uses `DataPatcher` ~1772, ~2166) |
| `exec/scenario.rs` | 2418–2899 | scenarios/run/suite (`run_suite`, used by `parity/mod.rs:117`; `load_scenario` and `run_scenario`, used by `parity/determinism.rs:161,177`) |
| `exec/bench.rs` | 2900–3425 | sim/replay/bench/dump |
| `exec/content.rs` | 3426–3723 and 5921–end (`content_bench`) | content |
| `exec/world.rs` | 3724–4163 and 5828–5920 | world |
| `exec/maps.rs` | 4164–4220 and 5329–5827 | maps |
| `exec/editor.rs` | 4221–5328 | editor |

**Paths to keep.**
- `crate::exec::run` (lib.rs:49) and `crate::exec::is_quiet` (`stdb_scenarios.rs:234,363,458`,
  `mp_scenarios.rs:120`): leave them in `mod.rs`.
- `run_suite`, `load_scenario` and `run_scenario` move into `scenario.rs`, so add
  `pub(crate) use scenario::{run_suite, load_scenario, run_scenario};` (matching each item's
  visibility) to `mod.rs`.

Add the `//! Upstream: none (port-only CLI; …)` header line; the oracle CLI has no Java counterpart.
Where a command ports a Java server command, cite it.

## `cli.rs`: clap derive definitions only (no fns)

`git mv cli.rs cli/mod.rs`. `mod.rs` keeps `Cli` (16) and `Command` (36–402). Move each subcommand
enum, with its `#[derive]`s and doc comments, to its own file, and `pub use` it back so
`crate::cli::X` keeps working (16 importer files):

| File | Enum (start line) |
|---|---|
| `cli/ui.rs` | `UiCommand` 403 |
| `cli/input.rs` | `InputCommand` 573 |
| `cli/parity.rs` | `ParityCommand` 644–909 |
| `cli/network.rs` | `NetworkCommand` 910 |
| `cli/render.rs` | `RenderCommand` 948 |
| `cli/campaign.rs` | `CampaignCommand` 1004 |
| `cli/fx.rs` | `FxCommand` 1148 |
| `cli/world.rs` | `WorldCommand` 1256 |
| `cli/maps.rs` | `MapsCommand` 1381 |
| `cli/editor.rs` | `EditorCommand` 1456 |
| `cli/blocks.rs` | `BlocksCommand` 1577 |
| `cli/combat.rs` | `CombatCommand` 1614 |
| `cli/units.rs` | `UnitsCommand` 1667 |
| `cli/logic.rs` | `LogicCommand` 1742 |
| `cli/audio.rs` | `AudioCommand` 1782 |
| `cli/mods.rs` | `ModsCommand` 1882 |
| `cli/trace.rs` | `TraceCommand` 2001 |
| `cli/meta.rs` | `MetaCommand` 2019 |
| `cli/assets.rs` | `AssetsCommand` 2033 |
| `cli/io.rs` | `IoCommand` 2112 |
| `cli/content.rs` | `ContentCommand` 2221 |

You may combine enums under about 80 lines into one file (e.g. `cli/small.rs`); record it.
**Do not** introduce `#[command(flatten)] JsonOut` for the 128 `json: bool` fields. It would change
every match pattern (out of scope).

The `--help` output must be byte-identical. Check it: build the bin, then
`run --cargo-run -q -p mind-headless -- --help > /tmp/S15-help-after.txt`, and diff against the
same command run before your edits. Also spot-check two subcommands (`… -- parity --help`,
`… -- campaign --help`).

## `server/commands.rs`: dedicated-server console and chat commands

`git mv server/commands.rs server/commands/mod.rs`, then:

| File | Lines | Contents | Upstream |
|---|---|---|---|
| `commands/console.rs` | 1–1242 | `CommandSpec`/`COMMANDS` 18–250, `execute`/`dispatch` 263–368, `cmd_*` 421–1229 | `server/src/mindustry/server/ServerControl.java` (repo-relative; `registerCommands`) |
| `commands/chat.rs` | 1243–2173 | consts, `CHAT_COMMANDS`, `fix_name`, `parse_chat_command` (1243–1580), `ChatConfig`/`ChatEngine` incl. votekick 1865–2056 (1581–2083), `apply_chat_actions`, `tick_chat`, `impl ServerState` (2084–2173) | `core/NetServer.java` (`registerCommands`) |
| `commands/tests.rs` | 2174–2709 | tests | – |

`mod.rs` holds the header, the `mod` lines and the re-exports. Importers: `server/mod.rs`
(`execute`, `ChatEngine`, `tick_chat` at 124, 285, 466, 509, 529, 586, 601, 789–790) and
`server/net_host.rs:32` (`fix_name`).

## Verification (RULES §6)

```bash
B=$(git merge-base main HEAD); H=client/rust/mind-headless/src
python3 tools/refactor/move_check.py --base $B --old $H/exec.rs --new $H/exec
python3 tools/refactor/move_check.py --base $B --old $H/cli.rs --new $H/cli
python3 tools/refactor/move_check.py --base $B --old $H/server/commands.rs --new $H/server/commands
run --cargo-check -p mind-headless --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-headless --all-targets -- -D warnings
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- run-all --tier T0
```

Also do the `--help` diffs described above.
