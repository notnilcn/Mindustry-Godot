# AGENTS.md — mind-core/src/logic (mlog logic system)

`logic/` is the Godot-free mlog subsystem of `mind-core`: the text language and parser, the typed
statement model, the assembler that lowers statements to instructions, the shared variable/global
arena, the instruction op set, rule application, the world blocks that host processors/displays/
memory/messages/switches/canvases, the view-only FX seam, and the deterministic, bounded-per-tick
`Executor` VM. It ports upstream `core/src/mindustry/logic/*` and
`core/src/mindustry/world/blocks/logic/*`; the executor drives the logic blocks through
`crate::world::behavior::BehaviorRegistry`. Read the root [`AGENTS.md`](../../../../../AGENTS.md)
first; the crate map is `mind-core/AGENTS.md` (`../../AGENTS.md`).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Module root and re-exports (`Assembler`, `Executor`, `Instruction`, `GlobalVars`, `Statement`, `LVar`, `LogicObject`, `LAccess`, ...). |
| `value.rs` | Value model: `LVar`, `LogicObject`, `VarRef`, `VarArena`, `VarId`, `invalid`/`conv`/`unconv`, `structs_eq`. |
| `parser.rs` | `LParser` port: CR/CRLF normalization, the 16-token line grammar, comments, string/`\uXXXX` handling, jump labels, op/`status` rewrites, the 1000-statement cap. |
| `statement.rs` | `Statement` enum (all 53 registered statements) via `define_statements!`, plus `StatementMeta`/`StatementField`, `LogicRule`, `name_to_align`. |
| `enums.rs` | Option enums (`GraphicsType`, `FetchType`, `QueryType`, `QueryShape`, `RadarSort`, `RadarTarget`, `TileLayer`, `LUnitControl`, `BlockFlag`, `LCategory`, ...) and the `logic_enum!`/`MlogField` text codec. |
| `assembler.rs` | `LAssembler` port: `assemble`/`read`/`write`, `var`/`putVar`/`putConst`/`getVar`, numeric/hex/binary/color parsing, string unescaping. |
| `textio.rs` | Generated-`LogicIO` equivalent: statement `write`/`read` in exact field order and canvas save/load. |
| `ops.rs` | `LogicOp`/`ConditionOp` ports with `eval` (including the `ArcRand` `rand` op), `symbol`, `test`. |
| `globals.rs` | `GlobalVars` constant/variable table, `GlobalUpdate`, content/`@sfx-*` initialization, `install_world`, per-tick `update`. |
| `access.rs` | `LAccess` enum plus `sense`, `sense_content`, `set_prop_num`/`set_prop_obj`/`set_prop_content`, `control` and the `LogicSense`/`LogicControl`/... traits. |
| `rules.rs` | `setrule` application (`apply_to_state`, `apply_to_rules`) over `Rules` and `LogicWorldState`. |
| `fx.rs` | `LogicFx` effect table (`EffectEntry`, `EFFECTS`, `get`, `all`) resolved by the `effect` instruction. |
| `script.rs` | `LogicScript`/`LogicFilter` runners (`run_logic_script`, `run_logic_script_in`, `run_logic_filter`, `run_standalone`). |
| `world.rs` | Privileged world-instruction state and sinks: `LogicWorldState`, `LogicWorldEvent`, `LogicContentIndex`, `MessageState`, `run_query`/`run_fetch`/`run_set_rule`/`run_effect`. |
| `executor/` | The `Executor` VM, `Instruction`, `build_statement`, the instruction-budget driver, and the `draw.rs`/`radar.rs`/`unit_control.rs` submodules. |
| `blocks/` | Logic-block behaviors and shared state (`logic_block`, `memory`, `display`, `canvas`, `message`, `switch`, `io`) plus the `LogicRulesApi` rules seam. |
| `tests.rs` | Ported `LogicTests.java` groups over a minimal registry fixture. |

## Key types

- Values: `LVar` is a named cell that is either numeric or object-valued;
  `LogicObject::{Building, Unit, Content, Team, Str, Enum, Align, Query}` models Java `Object`
  structurally; `VarRef::{Local, Global}` indexes the executor arena or the shared global arena;
  `VarArena` keeps an insertion-ordered `Vec<LVar>` and a name→id `IndexMap`.
- Language: `Parser`/`ParseError`, the `Statement` enum with `StatementMeta`/`StatementField`, and
  `LogicRule` for `setrule` targets.
- Compile/run: `Assembler`, `Instruction`, `Executor`, and the limits `MAX_INSTRUCTIONS`
  (`1000`), `MAX_GRAPHICS_BUFFER` (`256`), `MAX_DISPLAY_BUFFER` (`1024`), `MAX_TEXT_BUFFER` (`400`)
  and `MAX_INSTRUCTION_SCALE` (`5.0`).
- Globals/access: `GlobalVars`/`GlobalUpdate`, `LAccess`, `Sensed`, and the rules seam
  `LogicRulesApi`/`LogicRulesRes`/`rules_ref` (`DefaultLogicRules` supplies upstream defaults).
- Blocks: `LogicBlockState`, `LogicLink`, `MemoryBlockState`, `LogicDisplayState`,
  `CanvasBlockState`, `MessageBlockState`, `SwitchBehavior`, `LogicTimeouts`, `LogicDisplays`.

## Invariants

- **Godot-free and tokio-free.** No source under `mind-core` may `use godot`/`use tokio`; CI checks
  the dependency tree and greps the sources.
- **Deterministic and bounded per tick.** `Executor::run_once` executes exactly one instruction;
  `Executor::run` takes an explicit max; `Executor::run_budget` clamps its accumulator to
  `MAX_INSTRUCTION_SCALE * ipt` and breaks on `yielded`; programs stop at `MAX_INSTRUCTIONS`.
- **Deterministic randomness.** `Executor::rng` and `GlobalVars::rand` are `crate::math::ArcRand`
  streams seeded from simulation state; there is no wall clock or OS entropy on the logic paths.
- **Checksum where it mutates sim.** Privileged side effects are recorded as `LogicWorldEvent`s on
  `LogicWorldState`; world mutations flow through `WorldGrid`'s `ChecksumPart`; lowered
  `VarRef::Global` mirror cells are marked constant so they stay out of `var_ids` and state dumps.
- **Text-IO is ABI.** `Statement` field order, `logic_enum!` `name()`/`ordinal()` values and
  `LAccess` ordinals are frozen and append-only; `Assembler::write` round-trips through
  `read_statement` (checked by `textio` tests).
- **Privilege gating.** A privileged statement parsed without privilege becomes `Statement::Invalid`;
  world-processor effects are host-gated through `LogicRulesApi`/`block.privileged`.
- Invalid floats (`NaN`/infinite) normalize to a null object, and constants ignore assignment.

## Rules

- The pipeline is always text → `Parser`/`Statement` → `Assembler::assemble` → `Instruction` →
  `Executor`; keep the stages separate and data-driven.
- Add a statement through `define_statements!` in `statement.rs` with its exact field order, and keep
  the `textio` round-trip test green.
- Add or change a logic block through `logic/blocks/` and register it in `BehaviorRegistry`; keep
  per-tick work in the behavior update path fed by the executor budget driver.
- Cite the ported upstream class in the file header; files are UTF-8/LF with
  `SPDX-License-Identifier: GPL-3.0-only`. No `unwrap`/`expect` on runtime data.

## Verification

```bash
cargo test -p mind-core logic
cargo test -p mind-core
cargo clippy -p mind-core
```

Run from `client/rust/`, or add `--manifest-path client/rust/Cargo.toml` from the repo root.
