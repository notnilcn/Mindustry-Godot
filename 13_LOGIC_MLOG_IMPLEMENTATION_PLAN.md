# 13 — Logic (mlog), VM & Logic Blocks Implementation Plan

> Inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9 (conventions). Where this file conflicts with `HIGH_LEVEL_PLAN.md`, the high-level plan wins.
> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → `core/src/mindustry/logic/AGENTS.md` → `world/blocks/AGENTS.md` (logic family) → `annotations/AGENTS.md` → `net/AGENTS.md`.

---

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Planned — not started (2026-10-01) |
| **Phase** | P5 — Logic, UI, input (`HIGH_LEVEL_PLAN.md` §5) |
| **Depends on** | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (tile/grid APIs, `WorldHooks`, tile events, `check_map_area`), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (`BuildingBehavior`, `Building` component, config/`ConfigValue`, building IO + revisions, `DrawBlock` contract), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (`LogicAI`, `CommandAI`/`UnitGroup`, `UnitType` runtime, `TeamData` unit/building queries), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Rules` fields, `Teams`, `MapMarkers`, `MapObjectives`, `Gamemode`) — **12 is not on disk yet; interfaces here are written against plan 05 §3.12's minimal `Rules` surface and the upstream types, and must be reconciled at 12 kickoff (§8 R1)** |
| **Blocks** | `14_UI_IMPLEMENTATION_PLAN.md` (logic editor dialog/canvas + every `build(Table)` half), `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (processor selection/monitor; RTS command interplay), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (editor processors dialog, `LogicFilter` options UI), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (`sync`/`clientdata` relay, configure-command relay, checksum/desync), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (golden suite) |
| **Sources** | `core/src/mindustry/logic/*.java` (all files; inventory in §4), `world/blocks/logic/{LogicBlock,LogicDisplay,MemoryBlock,MessageBlock,SwitchBlock,CanvasBlock,TileableLogicDisplay}.java`, `annotations/src/main/java/mindustry/annotations/misc/LogicStatementProcessor.java` (replaced by hand-written Rust text IO), `entities/comp/BuildingComp.java` §2100–2227 (`sense`/`senseObject`/`control`/`setProp`), `entities/comp/UnitComp.java` §267–427, `ai/types/LogicAI.java`, `maps/filters/LogicFilter.java`, `content/Blocks.java` §6887–7032 (logic block definitions), `game/Rules.java` (logic rules), `tests/src/test/java/LogicTests.java` (full), `tests/src/test/java/ApplicationTests.java` (bootstrap pattern), `core/assets/bundles/bundle.properties` (`instruction.*`, `lst.*`, `lenum.*`, `lcategory.*`, `lunitcontrol.*`, `action.*`) |
| **AGENTS read in full** | `core/src/mindustry/logic/AGENTS.md`, `core/src/mindustry/world/blocks/AGENTS.md`, `core/src/mindustry/world/AGENTS.md`, `annotations/AGENTS.md`, `core/src/mindustry/net/AGENTS.md`, `entities/AGENTS.md`, `maps/AGENTS.md`, `content/AGENTS.md`, `tests/AGENTS.md`, `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md` |
| **Extends spine** | `mind-core` gains the `logic` plugin (registered by `LogicPlugin`); `mind-headless` gains `logic assemble\|run\|dump\|bench` subcommands and `logic_*` scenarios; `MindSimHost` gains the add-only `logic_*` `#[func]`s (§3.12); the state inspector gains a `Logic` tab fed by new `buildings[].logic` dump fields; `parity/logic/` goldens; `tests/golden/logic/*`. |

**Locked inputs treated as constants:** GPL-3.0 (D6); pure Rust/GDExtension + `bevy_ecs` (D1); STDB = relay + cheap validation, no authoritative sim yet (D2); fixed 60 Hz (D8); `logicids.dat` intentionally skipped, logic content lookups resolve by name (`HIGH_LEVEL_PLAN.md` §9) — this plan ports the *absent-file* behavior (see §8 R2).

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Text language.** `LParser` port: CR/CRLF normalization, the 16-token line grammar, `#` comments, string literals with `\n \" \\ \uXXXX` escapes (validated, preserved verbatim), `;` separators, jump labels (max 500), legacy op renames (`atan2→angle`, `dst→len`), `status` effect prefixing, `@configure→@config`/`configure→config` rewrites, the 1000-line parse cap, privileged-statement filtering, and `LAssembler.customParsers`.
2. **Statement model.** All 53 `@RegisterStatement` classes in `LStatements.java` + their exact serialized field order, `afterRead()` behavior, `category()`, `privileged()`, `useWrapping()`, `hidden()`, the `build(LAssembler)` compilation half, and `sanitize`. The 53rd registered entry is `noop` (`InvalidStatement`); `CommentStatement` is **not** registered upstream (`//@RegisterStatement("#")`, TODO broken) and therefore writes nothing — preserve that.
3. **Value & variable model.** `LVar` (`isobj`/`constant`/`numval`/`objval`/`syncTime`), number conversions (`num`, `numOrNan`, `numfWorld`, `bool`, `set`, hidden numeric/string constants `___…`), and `LAssembler`'s variable table, numeric parsing (decimal/scientific/hex/binary with 64-bit wrap rules), `%rrggbb[aa]`/`%[name]` colors, and string unescaping.
4. **Assembler.** `LAssembler.assemble/read/write`, `var/putVar/putConst/getVar`, the `@counter`/`@unit`/`@this` bootstrap variables, `@links`/`@ipt`/`@queries` injection.
5. **VM.** `LExecutor` in full: `load`, `runOnce`, `@counter` clamping, `yield`/`stop`, instruction budget/`maxInstructionScale`, `ipt`, all `LInstruction` implementations, links (`getlink`, `linkIds`), unit binding (`ubind`), unit control (`ucontrol`), unit locate (`ulocate`), radar (`radar`/`uradar`), sensor/control/setprop, query/fetch, locate/getblock/setblock, spawn/bullet/status/weather/spawnwave/setrule, print/format/draw buffers, message/cutscene/effect/explosion, sync/clientdata, flags, markers, playsound/playmusic/localeprint, `end`/`noop`/`stop`/`wait`.
6. **Global variables.** `GlobalVars.init/update` (time/tick/second/minute/wave/waveTime/mapw/maph/server/client/*, content/team/sound/color/LAccess/align constants, `@ctrlProcessor/@ctrlPlayer/@ctrlCommand`), `waitVar`, privileged names, docs entries (`VarEntry`) for the plan-14 dialog, `remove` for data-patch reset, and name-based content constants. `logicids.dat` is not read.
7. **Capabilities & `LAccess`.** `LAccess` enum (exact order/params/`isObj`/privileged), derived `senseable`/`senseablePrivileged`/`controls`/`settable` tables, and the `Senseable`/`Settable`/`Controllable`/`Ranged`/`LReadable`/`LWritable`/`LPrintable`/`LDrawable` contracts (definitions in 13, implementations across 07/08/10/11/12).
8. **Logic blocks.** `LogicBlock`/`LogicBuild` (executor ownership, tick budget, links, code/vars/waits save format, config compression, privilege rules, sync), `MemoryBlock`, `MessageBlock`, `SwitchBlock`, `LogicDisplay`, `TileableLogicDisplay`, `CanvasBlock`; world processor variants (`world-processor`, `world-cell`, `world-message`, `world-switch`) and the `state.rules.*` privilege gates.
9. **Persistence.** Code `deflate` blob (version 1) and its `relative` decoding, building revision 5 write/read for processors, per-kind revisions for the other logic blocks, `TypeIO` object values for variables.
10. **World-level helpers.** `LogicFx` table, `LogicRule` enum + apply, `LogicFilter`/`runLogicScript` runner (privileged, capped), `GlobalVars.rand` stream, `unitTimeouts` reset behavior.

### 2.2 Definition of done

- `cargo test -p mind-core` passes the ported `LogicTests` groups plus the new §7a tests with **no Godot, no network, no JVM**.
- `mind-headless logic run logic_arith --json` and the other §7b scenarios pass against committed goldens; the scripted draw scenario produces byte-identical `DisplayCmd` vectors across runs.
- A processor placed in-game executes a program, drives a linked memory cell and a linked display; the display output is visible on screen; save/reload preserves code, links, non-null vars, wait timers, `ipt`, tag and display transform (MCP §7c).
- The performance budgets in §7d are met and recorded.
- Every statement round-trips through the hand-written text codec with the exact upstream field order; `parity/golden_logicio.txt` (one-off JVM dump) shows zero differences.

### 2.3 Explicit boundaries (who owns what)

| Area | Plan 13 owns | Deferred to |
|---|---|---|
| mlog lexer/parser, statement enum, field-order table, text (de)serialization, statement registry, `sanitize`, `copy` | Everything | — |
| VM (`LExecutor`), `LVar`, op/condition evaluation, buffers, budget, links, `GlobalVars`, `LogicRule`, `LogicFx`, `runLogicScript` | Everything | — |
| Logic editor dialog + `LCanvas`/`StatementElem`/jump curves, all `build(Table)` UI halves, `GlobalVarsDialog`, `CanvasEditDialog`, variable view, add dialog | Statement metadata sufficient to build them (`StatementField` descriptors, category, keys, defaults) | `14_UI_IMPLEMENTATION_PLAN.md` |
| Logic content metadata (names, requirements, categories, build visibility, `BlockDef` fields) | Behavior + revision codecs + `BlockKindData::Logic*` variants | 02 (metadata) + 07 (`BlockKindData`, behavior registration) |
| `LogicAI` movement/timeout body, `LogicAiState` component, unit control reset | Instruction-side setters (`check_logic_ai`, `control_timer` refresh) and `LUnitControl` enum | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| `LAccess` enum, dispatch trait definitions, building default table | `sense`/`sense_object`/`set_prop`/`control` implementations on: buildings (07 base, 08 payloads, 10 turrets, 12 cores), units (11) | 07/08/10/11/12 |
| Logic display **packing + sim state** (command queue, `operations`, transform, linked-display arena) | GPU `FrameBuffer`, `processCommands`, draw execution, font metrics | 16 (`LDrawable` rendering), 03/16 (`Fonts.logic` metrics) |
| Message/announce/toast instruction semantics + `@wait` blocking state | Actual dialog/HUD/toast presentation | 14 (+ 18 for music/sfx) |
| Marker instructions (`setmarker`/`makemarker`) | `MapMarkers`, `ObjectiveMarker`, `MapObjectives.markerNameToType` | 12 |
| `setrule` instruction | `Rules` fields + team rules storage + JSON | 12 |
| Effects: `LogicFx` entries (name → effect ID + size/rotate/color/data flags/bounds) | Effect bodies, `EffectId` registry | 17 |
| `getblock`/`setblock`/`spawn`/`bullet`/`status`/`explosion`/`weatherset`/`spawnwave` host ops | World tile mutation (06), bullet/turret factories (10), unit spawn (11), weather state (12/16), spawner groups (11) | 06/10/11/12 |
| Save **format** (compressed blob, building chunks) | Codec registration with 04 through 07's `BuildingBehavior::write/read/version` | 04 + 07 |
| `sync`/`clientdata` events and local-apply semantics | STDB relay rows, ordering, late-join | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| `LogicFilter` logic execution hook | Filter class/registry/JSON class tags | 06 |
| Editor "processors" dialog, map-level processor editing | All editor UI | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| Monitor/selection of a processor, logic cutscene camera flags | Input handling, camera | 15 |
| Component/revision metadata plumbing | `FieldMeta`, `EntityDefs!` entries for logic components | 05 + 04 |

### 2.4 Deliberate deviations (reason stated)

| # | Deviation | Reason |
|---|---|---|
| 1 | **Statements and instructions are Rust enums, not class hierarchies.** One `Statement` variant per registered class, one `Instruction` variant per `LInstruction` implementation; `UnitRadarStatement` shares a `RadarFields` struct. Same observable behavior, no downcasts; text IO order stays an explicit table. | Rust idiom; keeps behavior dispatch exhaustive and testable. |
| 2 | **`VarRef` is `Local(u32) | Global(u32)`** instead of Java object references. Global constants must be shared/mutable (`@time`, `@tick`, …) across executors, so they cannot be copied per processor. | Preserves `GlobalVars.update` visibility from every executor. |
| 3 | **Query results are `LogicValue::Query(QueryId)` into an executor-owned arena**, not `Seq<Object>` objects. | `Rc<RefCell<…>>` is not `Send`; an arena is deterministic and cheap. |
| 4 | **`sync` throttling and `unitTimeouts` use sim time (ticks), not `Time.millis`.** 20/s becomes every 3 ticks. | D8/sim determinism; wall clock must not affect sim state. |
| 5 | **`FlushMessageI`'s `@wait` blocking reads a sim-visible `MessageState`** (`has_announcement`, `has_toast`, mission text) updated by plan 14 on show/expire; headless defaults to no message present. | Keeps execution deterministic while preserving blocking semantics; upstream reads UI state directly. |
| 6 | **`LogicScript`'s millisecond timeout becomes an instruction cap** for the editor/debug class; `run_logic_script` already takes an explicit instruction cap (upstream `LogicFilter` passes 6 250 000), so map generation is unchanged. | No wall clock in sim. |
| 7 | **Draw packing runs behind a `skip_draw_pack` flag** (default true when `Platform::headless`, mirroring upstream's early return) that the harness/`MindSimHost` can disable for verification. With the flag on, `operations` still increments on `drawflush` with an empty buffer exactly like upstream headless. | Upstream headless produces no buffer, so headless draw could not be tested; the flag is test-visible only. |
| 8 | **Hand-written text IO instead of an annotation processor**, with a committed field-order table per statement and a Java parity dump tool. | D1: no build-time Java processing. |
| 9 | **`logicids.dat` absent-file behavior**: `lookup` returns null, `lookupLogicId` returns -1, `@<type>Count` constants are not registered. Content is addressed by name constants (`@copper`, `@dagger`). | `HIGH_LEVEL_PLAN.md` §9 locked; the file is gitignored/Anuken-only upstream. |
| 10 | **Link names stored as UTF-8 (not Java modified-UTF) in the compressed blob.** | Link names are ASCII block names; native format is ours (OD2). Names are length-prefixed identically. |
| 11 | **`static best/bestValue` in `RadarI` move onto the instruction instance.** | Upstream statics are a latent cross-instruction race; per-instance is behavior-identical (reset at the start of every refresh). |
| 12 | **`LogicDisplay.displays` becomes a `LogicDisplays` resource** with the same swap-remove index semantics, cleared on `ResetEvent`. `FrameBuffer`/`processCommands` are 16. | No globals; keeps command-image indexing deterministic. |

---

## 3. Target design

All names below are final unless marked otherwise. `mind-core` is Godot-free and tokio-free; no `HashMap` iteration on sim paths (lookup-only `hashbrown::HashMap` allowed), ordered maps use `IndexMap`, sorted iteration uses `BTreeMap`.

### 3.1 Module layout (`client/rust/mind-core/src/logic/`)

```
logic/
  mod.rs              # LogicPlugin: register system sets, block behaviors, reset listeners, re-exports
  value.rs            # LogicValue, LogicObject, LVar, VarId, VarRef, VarArena, conversions/formatting
  parser.rs           # lexer + LParser port (text -> Vec<Statement>, ParseError)
  statement.rs        # Statement enum, registered name, category/privileged/use_wrapping, StatementField
  textio.rs           # LogicIO equivalent: write_statement/read_statement + STATEMENT_FIELDS table
  assembler.rs        # LAssembler port (vars, put_var/put_const, var(), assemble/read/write)
  globals.rs          # GlobalVars: LogicVars resource, constants, docs entries, update(), lookup
  access.rs           # LAccess, capability traits, sense/control/set_prop dispatch, privilege tables
  enums.rs            # FetchType/QueryType/QueryShape/RadarSort/RadarTarget/LLocate/TileLayer/
                      # MessageType/CutsceneAction/LMarkerControl/LCategory/LUnitControl + choice tables
  ops.rs              # LogicOp/ConditionOp symbols, semantics, legacy name map
  fx.rs               # LogicFx table (name -> LogicEffectEntry)
  rules.rs            # LogicRule enum + apply_to_rules
  script.rs           # run_logic_script, LogicFilter hook, LogicScript-equivalent cap
  statements/
    mod.rs            # per-category build() dispatch, statement constructors
    io.rs             # read/write/draw/drawflush/print/printchar/format/printflush/getlink
    block.rs          # control/sensor/radar/getblock/setblock/setprop
    operation.rs      # set/op/select/wait/stop/lookup/packcolor/unpackcolor/end/jump/setrate
    unit.rs           # ubind/ucontrol/uradar/ulocate
    world.rs          # query/spawn/bullet/status/weather*/spawnwave/setrule/message/cutscene/
                      # effect/explosion/fetch/sync/clientdata/getflag/setflag/playsound/
                      # playmusic/setmarker/makemarker/localeprint
  executor/
    mod.rs            # Executor: load, run_once, variable helpers, tick budget driver, unit timeouts
    instr.rs          # Instruction enum + run dispatch
    unit_control.rs   # UnitControlI port, check_logic_ai, LUnitControl setters, transfer delays
    radar.rs          # RadarI incl. timer caching + target/sort filters
    draw.rs           # DrawI packing, DisplayCmd, buffer caps, PrintI::to_string
  blocks/
    mod.rs            # LogicBlocksPlugin: register_behavior for the 11 logic BlockKinds
    logic_block.rs    # LogicBlock metadata + LogicBuild behavior, links, budget, config/save
    memory.rs         # MemoryBlock + slot model + revision 1
    message.rs        # MessageBlock + message text + revision 0
    switch.rs         # SwitchBlock + enabled config + revision 1
    display.rs        # LogicDisplay + LogicDisplays arena + TileableLogicDisplay linking
    canvas.rs         # CanvasBlock bit-packed pixels + revision 0
```

### 3.2 Value & variable model (`value.rs`)

```rust
pub type VarId = u32;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum VarRef { Local(VarId), Global(VarId) }        // deviation 2

#[derive(Clone, Debug, PartialEq)]
pub enum LogicValue { Null, Num(f64), Obj(LogicObject) }

#[derive(Clone, Debug, PartialEq)]
pub enum LogicObject {
    Building(Entity),            // Building / LogicBuild / any Building entity
    Unit(Entity),
    Content(ContentRef),         // Item/Liquid/Block/UnitType/Weather/StatusEffect/LAccess/...
    Team(TeamId),
    Str(SmolStr),                // Java String object values (print, flags, channels, ...)
    Enum(&'static str),          // Java enum object values (LUnitControl/idle, GraphicsType, ...)
    Align(i32),                  // @topLeft etc. from LStatement.nameToAlign
    Query(QueryId),              // @queries result arena (deviation 3)
    EffectName(&'static str),    // reserved for effect object values (none in vanilla mcode)
}

pub struct LVar {
    pub name: SmolStr,
    pub id: i32,                 // index in Executor::vars, -1 while assembling
    pub value: LogicValue,
    pub constant: bool,
    pub synced_at: f64,          // sim ms; SyncI throttle (deviation 4)
}

pub struct VarArena { cells: Vec<LVar>, by_name: IndexMap<SmolStr, VarId> }
impl VarArena {
    pub fn put_var(&mut self, name: &str) -> VarId;                 // null object by default
    pub fn put_const(&mut self, name: &str, v: LogicValue) -> VarId;
    pub fn put_num_const(&mut self, name: &str, v: f64) -> VarId;   // "___<value>" hidden const
    pub fn get(&self, id: VarId) -> &LVar;
    pub fn get_mut(&mut self, id: VarId) -> &mut LVar;
}
```

- **Conversions (exact `LVar` port):** `num()` = object ? (non-null ? 1 : 0) : (invalid ? 0 : numval); `num_or_nan()` = null object or invalid → `f64::NAN`; `numf()`/`numf_or_nan()` cast; `numf_world()` = `World::unconv`; `numi()` truncates toward zero; `bool()` = object ? non-null : `numval.abs() >= 0.00001`; `set(other)` copies `isobj` and value (`@counter` object assignment keeps only the numeric field when the source is non-object); `setnum(v)` with NaN/Inf → null object; `setobj`, `setconst`; `setlink(Some)` sets `constant=true`, `setlink(None)` demotes to a regular variable.
- **Invalid detection** = `f64::is_nan() || f64::is_infinite()`.
- **Equality (`Structs.eq` parity):** `Null == Null`; `Num` by `==`; `Str` by value; `Content` by `ContentRef`; `Building`/`Unit` by ECS `Entity`; `Team` by id; `Enum`/`Align` by value; `Query` by id. Used by `op equal/strictEqual` and `ConditionOp::test`.
- **Numeric parsing (`LAssembler.parseDouble`):** fail fast when the first char is not `[0-9.+-%]`; `0b/+0b/-0b` and `0x/+0x/-0x` with leading-zero skip, per-digit validation, >64 significant bits → NaN, 64-bit wrapping via unsigned accumulators; `%rrggbb`/`%rrggbbaa` → `Color::to_double_bits`; `%[name]` → Arc named-color table; otherwise `Strings.parseDouble` semantics (`NaN`/`Infinity` spelled names are **not** numbers; `e10` is not a number).
- **Unescape (`LAssembler.unescape`):** `\n → 0x0A`, `\" → "`, `\\ → \`, `\uXXXX` (4 hex accepted only), unknown escapes pass through (parser keeps them raw; only `var()` decodes string literals).

### 3.3 Statement model, registry, text IO (`statement.rs`, `textio.rs`, `statements/`)

```rust
pub enum Statement {
    Invalid, Read(ReadStatement), Write(WriteStatement), Draw(DrawStatement),
    /* ... one variant per registered statement, 53 total ... */
    LocalePrint(LocalePrintStatement),
}

pub struct StatementField {
    pub name: &'static str,
    pub kind: FieldKind,       // Str, Int, Bool, Enum(&'static [&'static str]), Char
    pub default: &'static str, // upstream default literal for UI (plan 14)
}
pub struct StatementMeta {
    pub registered_name: &'static str,   // "noop", "read", ...
    pub rust_variant: &'static str,
    pub fields: &'static [StatementField],  // exact serialization order, inherited fields last
    pub category: LCategory,
    pub privileged: bool,
    pub use_wrapping: bool,
    pub hidden: bool,
    pub name_key: &'static str,          // "instruction.<statementKey>"
    pub tooltip_key: &'static str,       // "lst.<statementKey>"
}
// on Statement:
pub fn meta(&self) -> &'static StatementMeta;
pub fn build(&self, asm: &mut Assembler) -> Option<Instruction>;   // None -> compiled away
```

- **Registry.** `const STATEMENTS: &[StatementMeta]` in `textio.rs`, one row per `@RegisterStatement` (53 rows; `noop` = `InvalidStatement`). Order = upstream generated `allStatements` order (declaration order) for the plan-14 add dialog; lookup by `registered_name`.
- **Text IO (generated-`LogicIO` port):** `write_statement(stmt, out)` appends `name` then for every field (table order) `' ' + value.encoded()` where enums use Java enum `name()` and bools use `true`/`false`. `read_statement(tokens) -> Result<Statement, ParseError>` constructs the default, assigns `tokens[i+1]` to field `i` **only when `tokens.len() > i + 1`**, then calls `after_read()`. Extra tokens are ignored; unknown name returns `None` (parser falls back to custom parsers / `Invalid`).
- **`after_read()` ports:** `DrawStatement` (color alpha `"0"` → `"255"`; un-prefixed align names get `@`), `EffectStatement` (`LogicFx` lookup deferred to build), `SensorStatement`/`SetPropStatement` (selection index only, transient). `JumpStatement` maps label names to `destIndex` at parse time (labels) or reads the serialized `destIndex` (numeric).
- **`LStatement.sanitize`** is ported verbatim with its 20-case table (§7a); `copy()` = `write` then `read(privileged=true)`, `None` when the round-trip yields zero statements.
- **Privileged filter:** `Parser::parse(text, privileged)` turns privileged statements into `Invalid` when `!privileged`; custom-parser output is checked the same way.
- **Plan-14 UI halves:** each variant exposes `meta()`; plan 14 implements `build_ui` generically from (`kind`, defaults, enum choices, align select, color picker, image picker). The `LCanvas` statement graph, `StatementElem`, jump buttons/curves, `setupUI`/`saveUI` (jump destination resolution by index) are plan 14. `LCanvas::save` = `Assembler::write`; `LCanvas::load` truncates at `Executor::MAX_INSTRUCTIONS`.

### 3.4 Assembler (`assembler.rs`)

```rust
pub struct Assembler {
    pub privileged: bool,
    pub vars: VarArena,
    pub instructions: Vec<Instruction>,
}
impl Assembler {
    pub fn assemble(text: &str, privileged: bool) -> Result<Assembler, ParseError>;
    pub fn read(text: &str, privileged: bool) -> Result<Vec<Statement>, ParseError>;
    pub fn write(statements: &[Statement]) -> String;
    pub fn var(&mut self, symbol: &str) -> VarRef;   // global const -> string const -> num const -> var
    pub fn put_var(&mut self, name: &str) -> VarId;
    pub fn put_const(&mut self, name: &str, v: LogicValue) -> VarId;
    pub fn get_var(&self, name: &str) -> Option<VarId>;
    pub fn custom_parsers: &'static mut IndexMap<SmolStr, CustomParserFn>, // 20 registers
}
```

- Bootstrap (`LAssembler` ctor): `@counter` = local numeric variable; `@unit` = local null const; `@this` = local null const.
- `assemble` builds one instruction per statement, dropping `None` (comments, `clientdata` when `!rules.allow_logic_data`, unparseable custom output), then `retain_all(|i| i.is_some())`.
- `var(symbol)` resolution order exactly upstream: `GlobalVars.get(symbol, privileged)` → quoted string literal (`put_const("___\"…\"", Str(unescape(..)))`) → numeric literal (`put_num_const("___<value>", v)`, ±Inf → 0) → `put_var(symbol)`. Unknown names become null variables (no validation).

### 3.5 VM (`executor/`)

```rust
pub struct Executor {
    pub arena: VarArena,             // moved from the Assembler on load
    pub instructions: Vec<Instruction>,
    pub vars: Vec<LVar>,             // visible non-constant vars + link constants (ids 0..n)
    pub counter: VarId, pub unit: VarId, pub thisv: VarId, pub ipt: VarId,
    pub query_result: Option<VarId>,
    pub binds: Vec<u32>,             // per UnitType id binding cursor
    pub links: Vec<Entity>,
    pub link_ids: IndexSet<i32>,
    pub team: TeamId,
    pub privileged: bool,
    pub build: Option<Entity>,
    pub graphics_buffer: Vec<u64>,   // cap 256
    pub text_buffer: String,         // cap 400
    pub queries: Vec<Vec<LogicValue>>,
    pub yielded: bool, pub stopped: bool,
    pub name_map: Option<IndexMap<SmolStr, u32>>,
}
impl Executor {
    pub fn initialized(&self) -> bool;                            // !instructions.is_empty()
    pub fn load(&mut self, asm: Assembler);                       // resets buffers/stop, ids, @ipt, @queries
    pub fn run_once(&mut self, sim: &mut Sim);                    // counter clamp + one instruction
    pub fn optional_var(&mut self, name: &str) -> Option<VarRef>; // lazily builds name_map
    pub fn optional_var_id(&self, i: i32) -> Option<VarRef>;
}
```

- **`load` exact semantics:** `stop=false`, buffers cleared, `name_map=None`; `vars` = assembler cells retained by `!constant || (name[0] != '_' && name[0] != '@')` (link constants survive); ids assigned in order; `counter/unit/thisv` looked up; `ipt = put_const("@ipt", build.ipt or 0)`; when privileged, `query_result = put_const("@queries", Query(0))`.
- **`run_once`:** if `counter >= len || counter < 0` → `counter = 0`; mark counter numeric; run `instructions[counter++]`.
- **Instruction budget driver** (called by `LogicBlockBehavior::update_tile`, §3.6): clamp `accumulator <= max_instruction_scale * ipt`; `while accumulator >= 1 { run_once; if yielded { yielded=false; break } accumulator -= 1 }`; then `accumulator += edelta * ipt` (after the loop, per upstream comment — keeps `WaitI` accumulation in sync).
- **Instruction enum** has one variant per upstream `LInstruction` class, with fields as `VarRef` plus per-instance caches (`RadarI` timer/last target, `UnitLocateI` cache, `WaitI::cur_time`, `UnitControlI` plan, `DrawI` byte type). Instruction → module mapping is §4.
- **`unitTimeouts`** becomes a `LogicTimeouts` resource (`IndexMap<UnitId, f64>`), cleared on `ResetEvent`, with `timeout_done(unit, delay)`/`update_timeout(unit)` helpers used by `ucontrol` transfers and `SyncI` throttling.
- **Determinism:** `RadarI` iterates `Teams::present` in slot order and unit groups in slot order (05 contract); `FetchI`/`QueryI` iterate `TeamData` lists and quadtrees with the query order defined by 05/11; no hash iteration.
- **`GlobalVars.rand`** is `RngStream::Logic` (11's bit-exact Arc `Rand` port); its state joins `Sim::checksum()` (reconcile with 05/23, §8 R3).

### 3.6 Logic block integration (`blocks/logic_block.rs`)

Block metadata (owned by 02/07; 13 supplies the exact values transcribed from `content/Blocks.java` §6887–7032):

| Block | kind data | behavior |
|---|---|---|
| `micro-processor` | `ipt=2, range=80 (8*10), size=1` | `LogicBlockBehavior` |
| `logic-processor` | `ipt=8, range=176 (8*22), size=2` | `LogicBlockBehavior` |
| `hyper-processor` | `ipt=25, range=336 (8*42), size=3`, consumes `cryofluid 0.08` | `LogicBlockBehavior` |
| `world-processor` | `ipt=8, max_ipt=1000, range=f32::MAX, privileged, force_dark, targetable=false, can_overdrive=false, size=1` | `LogicBlockBehavior` (privileged) |
| `memory-cell`/`memory-bank` | `capacity=64`/`512` | `MemoryBehavior` |
| `message`/`reinforced-message` | `max_text=400, max_newlines=24`; reinforced health 100 + crush-fragile | `MessageBehavior` |
| `switch` | `click_sound=click`, `auto_reset_enabled=false` | `SwitchBehavior` |
| `logic-display`/`large-logic-display` | `display_size=80`/`176, scale_factor=1` | `DisplayBehavior` |
| `tile-logic-display` | `display_size=32, frame_size=6, max_display_dimensions=16`, rectangle placement | `TileableDisplayBehavior` |
| `canvas`/`large-canvas` | `canvas_size=12`/`24, padding=3.5`, palette (large = Woodspark 16 colors) | `CanvasBehavior` |
| `world-cell`/`world-message`/`world-switch` | privileged variants (`capacity=512`, `targetable=false`, `force_dark`) | respective behaviors |

Components (07 component + this plan's state):

```rust
#[derive(Component)] pub struct LogicBlockState {
    pub code: String,
    pub executor: Executor,
    pub accumulator: f32,
    pub links: Vec<LogicLink>,
    pub link_map: Option<IndexMap<SmolStr, usize>>,
    pub checked_duplicates: bool,
    pub ipt: i32,
    pub tag: Option<String>,       // world processors
    pub icon_tag: char,
    pub links_var: Option<VarRef>,
    pub load_pending: bool,        // deferred variable/wait application after save load
}
pub struct LogicLink { pub valid: bool, pub x: i32, pub y: i32, pub name: SmolStr,
                       pub last_build: Option<Entity>, pub logic_var: Option<VarRef> }
```

**`LogicBlockBehavior` (07 trait impl) responsibilities** (exact ports):

- `created()`: `executor.privileged = block.privileged`; `executor.build = Some(e)`.
- `configured(config)` (07 config dispatch):
  - `Bytes` (compressed code): reject when `!accessible()`; `read_compressed(data, relative=true)`.
  - `String`: only privileged; `len < MAX_NAME_LENGTH (32)` → `tag`.
  - `Char`: only privileged; `icon_tag`.
  - `Point2`/`Building` link toggle: valid-link check; remove existing links to the same build (clearing variables, re-enabling `auto_reset_enabled` targets disabled by this build); else append a `LogicLink` with `find_link_name`; then `update_links`.
- `update_tile()` order (upstream `updateTile`):
  1. `check_read_code()` (flush deferred save load);
  2. `executor.team = team`;
  3. one-time duplicate-link removal by building id;
  4. link-validity fixpoint loop (`valid_link`, `last_build` change, variable clear/reassign, dedup, `updates=true; break`) → `update_links()` when changed;
  5. `if rules.disable_world_processors && privileged { return }`;
  6. `if enabled && executor.initialized() { budget loop; accumulator += edelta * ipt }`.
- `update_links()`: rebuild `executor.links`/`link_ids` from valid links; write `links_var`.
- `readable/writable/read/write`: variable-by-name reads/writes through `executor.optional_var`; numeric read = link index; `optional_link` name→`last_build` (with the trailing-digit fast path); `readable` requires `is_valid && (privileged || team == exec.team && !block.privileged)`.
- `valid_link(other)`: upstream predicate including `!(privileged && !rules.world_processor_player_link && other.team == default_team)`, no `ConstructBuild`, range + half-size for regular processors.
- **Privilege surface:** `accessible()` (= `!privileged || rules.editor || playtesting_map.is_some() || rules.allow_edit_world_processors`), `can_break`, `check_force_dark`, `collide`/`damage` no-op for privileged unless destructible, `can_pickup = false`, cursor override. `world_processor_player_link` defaults `true`.
- **Persistence:** `version() -> 5`; `write`/`read` per §6.5. `config()` returns `compress(code, relative_connections())` (link coordinates relative to the processor tile).
- **Reset:** on `ResetEvent` clear `LogicTimeouts` and `LogicDisplays`; on building removal release the display index and clear the root of linked tileable displays.

### 3.7 Capability traits and `LAccess` dispatch (`access.rs`)

```rust
#[repr(u16)]
pub enum LAccess { TotalItems = 0, FirstItem, /* ... exact upstream order ... */ Color }
// LAccess::ALL, params(), is_obj(), privileged(), SETTABLE, SENSEABLE, SENSEABLE_PRIVILEGED, CONTROLS

pub trait LogicSense { fn sense(&self, w: &World, e: Entity, a: LAccess) -> f64;
                       fn sense_object(&self, w: &World, e: Entity, a: LAccess) -> Option<LogicObject>;
                       fn sense_content(&self, w: &World, e: Entity, c: ContentRef) -> f64; }
pub trait LogicSettle { fn set_prop_num(&mut self, w: &mut World, e: Entity, a: LAccess, v: f64);
                        fn set_prop_obj(&mut self, w: &mut World, e: Entity, a: LAccess, v: LogicObject);
                        fn set_prop_content(&mut self, w: &mut World, e: Entity, c: ContentRef, v: f64); }
pub trait LogicControl { fn control_num(&mut self, w: &mut World, e: Entity, a: LAccess, p1: f64, p2: f64, p3: f64, p4: f64);
                         fn control_obj(&mut self, w: &mut World, e: Entity, a: LAccess, p1: LogicObject, p2: f64, p3: f64, p4: f64);
                         fn logic_team(&self, w: &World, e: Entity) -> TeamId; }
pub trait LogicRanged { fn range(&self, w: &World, e: Entity) -> f32; }
```

- **Definitions live here; implementations land with the owning plan.** 07's `BuildingBehavior` gains `sense_object`/`set_prop`/`control` extension hooks exactly where upstream `BuildingComp` switches (07 §8 R12); 10 adds turret/`ControlBlock` entries; 11 implements the unit table; 12 supplies core/team-sensitive entries; 08 supplies payload entries. The default building table and unit table are quoted in §6.7.
- `sensor` instruction: null target + `@dead` → 1; content sense; `sense_object` sentinel (`noSensed`) → numeric; strings/`Query` support `size`/`bufferSize`; otherwise null object.
- `control`: target must be a building and privileged or a valid link of `exec.build`; `enabled` handles `no_sleep`/`last_disabler` before delegating.
- `setprop`: `LAccess` → numeric/object; `UnlockableContent` → `set_prop_content` (item/liquid/status).
- `Ranged` is required by `radar`/`ulocate`: logic processors, turrets, `ControlBlock` proxies (10), and units (11) implement it.

### 3.8 LogicAI bridge (`executor/unit_control.rs` + plan 11)

- `check_logic_ai(exec, unit_obj, control)` port: unit valid; `exec.unit.obj() == unit`; `unit.team == exec.team || exec.privileged`; `unit.controller().is_logic_controllable()`. Reuse an existing `LogicAI`; otherwise install one when `control`, setting `controller = exec.thisv.building()` and clearing `mine_tile`/build plans. Returns `None` when the unit is not logic-controllable.
- `LUnitControl` variants and setters (11's `LogicAiState`): `idle`, `stop`, `move`, `approach`, `pathfind`, `autoPathfind`, `boost`, `target`, `targetp`, `itemDrop`, `itemTake`, `payDrop`, `payTake`, `payEnter`, `mine`, `flag`, `build`, `deconstruct`, `getBlock`, `within`, `unbind`. Transfer delay `90 ticks`; control timeout `600 ticks` (both refresh `ai.control_timer`); `unbind` calls `unit.reset_controller()`.
- **Gating:** `!privileged && !rules.logic_unit_control` disables `ubind`/`ulocate`/`ucontrol`; `logic_unit_build` gates `build`; `logic_unit_deconstruct` gates `deconstruct`; `uradar` only requires a controllable base (upstream does not consult the rule). Unit build additionally requires `block.can_be_built() && (block.unlocked_now() || unit.team.is_ai())`.
- 11 owns `LogicAI::update_movement`, `check_target_timer` (radar cache), `exec_cache`, `plan`, timeout/reset; 13 never edits controller bodies (11 §8 R8).

### 3.9 Logic blocks: displays, memory, message, switch, canvas

- **`LogicDisplays` resource:** `Vec<Option<Entity>>` with upstream add/remove swap semantics (`index` field updated on remove); cleared on `ResetEvent`; `get(index)` used by `commandImage`.
- **`LogicDisplayState`:** `root_display: Entity`, `color: f32` (float bits), `stroke: f32`, `commands: VecDeque<u64>` (cap `MAX_DISPLAY_BUFFER = 1024`), `transform: Option<[f32; 16]>` (saved), `operations: u64`, `index: i32`, `processing: bool`. `LDrawable::draw(buffer)` appends `min(buffer.len, 1024 - commands.len())` commands and increments `operations`. `displayWidth/displayHeight` → `display_size`; `bufferSize` → `root.commands.len()`; `operations` → `root.operations`. `version() -> 1`; `write/read` = optional transform matrix.
- **Display commands (`executor/draw.rs`):** `DisplayCmd { type: u8 (4 bits), x, y, p1, p2, p3, p4: i32 (10 bits each) }` packed into `u64` with `pack`/`pack_sign`; `commandColorPack`/`commandPrint` are virtual and expanded in `DrawI` (print iterates `text_buffer`, packs glyph positions using 16's `LogicFontMetrics`, clears the buffer when full); `commandImage` packs `(content_id << 5) | content_type` or `(display_index << 5) | 30`, split across `p1`/`p4`; `commandScale` divides by `scale_step = 0.05`.
- **Tileable display:** `TileableDisplayState { tiles_width, tiles_height, origin_x, origin_y, bits, needs_update, rectangular, root_display }`; `link_displays(start)` BFS over `proximity` (root = lowest x then lowest y; non-rectangular when `w*h != count`); `on_proximity_added/removed` set `needs_update` on self/neighbours; `displayWidth/Height = tiles*32 - frame_size*2`; `bits` 8-neighbour mask; `draw(buffer)` routes to root. FrameBuffer merging/rendering is 16.
- **`MemoryBlockState`:** `slots: Vec<MemSlot>` where `MemSlot::Num(f64)` / `MemSlot::Obj(LogicValue)` (sentinel parity); `read` out-of-range → null; `write` ignores out-of-range; `sense memoryCapacity`; `readable/writable` team rule; revision 1 codec (`i32 count` + per-slot TypeIO tag; `doubleType` fast path; revision 0 reads raw `f64`s); `after_read_all` unboxes.
- **`MessageBlockState`:** `message: String`; config `String` clamped/trimmed at 400 chars / 24 newlines; `read` char (NaN out of range); `print` truncates to 400; `sense bufferSize`; `write`/`read` raw string.
- **`SwitchBlockState`:** `enabled: bool` via `Boolean` config; `config_tapped` toggles + click sound; revision 1 read/write.
- **`CanvasBlockState`:** `data: Vec<u8>` (`ceil(size²*bpp/8)`), `blending: u8`, palette/`bits_per_pixel` from block data; `get_pixel` (NaN out of range), `set_pixel` (bounds/palette validation); config `Bytes`; revision 0 (`i32 len` + bytes; skip when size differs); `displayWidth/Height = canvas_size`. Texture/Pixmap is 16.

### 3.10 Global variables and world-level helpers (`globals.rs`, `rules.rs`, `fx.rs`, `script.rs`)

```rust
#[derive(Resource)]
pub struct LogicVars {
    pub cells: Vec<LVar>,                 // Global VarIds (deviation 2)
    pub by_name: IndexMap<SmolStr, VarId>,
    pub privileged_names: IndexSet<SmolStr>,
    pub entries: Vec<VarEntry>,           // docs for plan 14
    // direct handles for the hot update:
    time: VarId, tick: VarId, second: VarId, minute: VarId, wave: VarId, wave_time: VarId,
    map_w: VarId, map_h: VarId, server: VarId, client: VarId,
    client_locale: VarId, client_unit: VarId, client_name: VarId, client_team: VarId,
    client_mobile: VarId, client_music_playing: VarId, client_current_music: VarId,
    wait: VarId,
}
```

- `init(content)` runs at content-init time (02 §3.4 reserved this hook): section entries; `@this/@thisx/@thisy/@links/@ipt` (per-executor locals, docs only); `the end`/`false`/`true`/`null`; math constants (`@pi`, `π`, `@e`, `@degToRad`, `@radToDeg`); time variables; network/client variables; `@ctrlProcessor/@ctrlPlayer/@ctrlCommand`; `@sfx-*` sound ids (only when audio assets are loaded, via plan 18's `SoundId` table); teams/items/liquids/blocks/units/weathers/statuses (`@status-<name>`); `@color<Name>` (lowercase initial only); all `LAccess` constants; align constants; then `put_entry_only` rows. Duplicate puts log + keep the first (`#6910` behavior).
- `update()` runs in plan 05 `TickSet::LogicVars`: `@time = state.tick/60*1000`, `@tick = state.tick`, `@second = tick/60`, `@minute = tick/3600`, `@wave`, `@waveTime = wavetime/60`, `@mapw/@maph`, `@server`, `@client`, client locale/unit/name/team/mobile/music variables (`None` when no player). `@time` is tick-derived, never system time.
- **Lookup:** `lookup_content(type, id)` → teams by index (0..255), otherwise `None` (no `logicids.dat`); `lookup_logic_id` → `-1`. Therefore the `lookup` instruction always outputs null and `@<type>Count` constants are absent (deviation 9).
- `remove(lvar)`/`remove(name)` support plan 20 data-patch reset.
- `LogicRule` enum (28 variants, exact order) with `apply_to_rules(rule, value, p1..p4)` ported from `SetRuleI`: tile→world ×8 conversions for `enemyCoreBuildRadius`/`dropZoneRadius`, `×60` for `waveSpacing`, `Mathf.clamp` ranges (`buildSpeed 0.001..50`, `unitHealth >= 0.001`, `unitBuildSpeed 0..50`, `unitMineSpeed/unitCost/unitDamage >= 0`, `blockHealth >= 0.001`, `blockDamage >= 0`), `mapArea` via 06's `check_map_area`, `ban`/`unban` block/unit lists, `ambientLight.from_double`, `mission` (message), and the `@Remote` server-only paths becoming host-gated events.
- `LogicFx`: ordered table of the 35 vanilla entries with flags (`size`, `rotate`, `color`, `data=Block` for `blockFall`, `bounds=100`), `get(name)`, `all()`, `add` for mods (20). `EffectId` resolution is 17's registry by name.
- `script.rs`: `run_logic_script(code, max_instructions, loop)` port (privileged executor, standard variables null, loop/range guard). The instruction cap replaces the upstream millisecond timeout only for `LogicScript`-style callers (deviation 6); `LogicFilter` calls with `500*500*25 = 6_250_000` exactly.

### 3.11 Schedule, determinism & reset

| Slot | System added here |
|---|---|
| `TickSet::LogicVars` (05 step 12) | `globals::update` (`@time`/`@tick`/wave/client vars) |
| `EntitySet::UpdateBuildings` (07) | no new system; `LogicBlockBehavior::update_tile` drives its own executor in `Groups.build` slot order (matches upstream `Groups.build.update()`) |
| building add/remove hooks (07 lifecycle) | `LogicDisplays::add/remove`, tileable `needs_update` flags |
| `ResetEvent` listener | clear `LogicTimeouts`, `LogicDisplays`, transient executor buffers; mutable global cells are refreshed by the next `update()` |
| `TickSet::AfterGameUpdate` | none (logic is fully inside build update) |

Determinism rules (HLP §2.4): no `HashMap` iteration in the VM; links ordered by insertion; radar/query/fetch iterate group slots; `GlobalVars.rand` is a sim stream included in `Sim::checksum()`; `sync`/`unitTimeouts` use tick-derived time; wall-clock never enters the VM.

### 3.12 Godot surfaces (`mind-gdext`)

Add-only `MindSimHost` methods (plan 00 stable test API), thin wrappers over `SimCommand`/sim accessors:

| Method | Behavior |
|---|---|
| `logic_place(kind: String, x: i64, y: i64) -> bool` | place one of the 11 logic blocks (dev only) |
| `logic_set_code(x: i64, y: i64, code: String) -> bool` | apply `SimCommand::Configure` with the compressed blob |
| `logic_get_state(x: i64, y: i64) -> Dictionary` | `{code, ipt, accumulator, privileged, links: [{name,x,y,valid}], vars: {name: value}, waits: [{index,cur_time}]}` |
| `logic_display_commands(x: i64, y: i64) -> PackedInt64Array` | packed `DisplayCmd` queue + `operations` |
| `logic_run(code: String, ticks: i64) -> Dictionary` | dev: temporary privileged executor, run N ticks, dump vars |
| `logic_bench(name: String) -> Dictionary` | runs a registered bench case (§7d) |
| `logic_set_rule(name: String, value: Variant) -> bool` | dev `setrule` probe |

Inspector: the `Logic` tab (plan 00 `state_inspector.tscn`) lists processors with `code` line count, `ipt`, `operations`, link count, and the first N non-null variables; `get_state_json()` `buildings[]` gains `"logic": { "code_lines", "ipt", "accumulator", "operations", "links", "vars_nonnull" }` (append-only, plan 07 §6.4 schema).

### 3.13 STDB surfaces (plan 21 handshake)

No tables/reducers/views are defined here (D2). This plan contributes:

1. `SimCommand::Configure { value: ConfigValue::Bytes(compressed_blob) }` (07 §6.3) is the code/configure transport; plan 21 relays it in command order.
2. `LogicSyncEvent { building_pos: i32, var_name: SmolStr, var_id: i32, value: LogicValue }` fired by `SyncI` every 3 ticks per variable; applied locally immediately on host/offline; plan 21 packages `sync variable` relay rows and applies remote events to `Executor::optional_var_id`. Never serialized into saves.
3. `ClientLogicDataEvent { channel: String, value: LogicValue, reliable: bool }` (plans 18/21 consume; gated by `rules.allow_logic_data`).
4. Rule/flag mutations (`setrule`, `setflag`, `setblock`, `spawnwave`) are host-only; plan 21 relays them as commands. Markers/weather go through plan 12/16 relay paths.
5. Checksum coverage: `LogicBlockState::{code, ipt, accumulator, links}`, non-constant executor variables, the `Logic` RNG stream, and optionally display command queues; checksum-version bump handled by 05/23.

### 3.14 Cross-plan interface ledger (reconcile by filename)

| Sibling plan/file | Interface required | Status at plan-write time |
|---|---|---|
| `02_CONTENT` `content/registries/blocks/logic.rs` | `BlockDef` records for the 11 logic blocks with metadata fields (`privileged`, `force_dark`, `targetable`, `can_overdrive`, `ignore_resize_config`, `schematic_priority=5`, `build_visibility=worldProcessorOnly` for world variants, `category=logic`, `env=any`, requirements); `ContentType` for `lookup`; `GlobalVars.init` content-constant registration hook (02 §3.4 reserved) | Present (framework); this plan supplies exact field values + the `GlobalVars` hook body |
| `02` `UnitType.logic_controllable`/`internal`/`commands`/`stances` | runtime flags used by `ubind`/`spawn`/`getblock`; `UnitCommand`/`UnitStance` lookup by id | Present |
| `03_ASSETS` bundle + fonts | `instruction.*`/`lst.*`/`lenum.*`/`lcategory.*`/`lunitcontrol.*` keys; logic glyph metrics (`spaceXadvance`, `lineHeight`, glyph coverage) | 03 owns loading; plan 16 supplies `LogicFontMetrics`; fallback constants in §8 R4 |
| `04_IO` `TypeIO` | `TypeValue` tags identical to upstream (incl. `l_access(13)`, `byte_array(14)`, `object_array(22)`, `unit_command(23)`); `write_object/read_object` for processor variables and memory slots; `BuildingCodec` registration seam | Present (04 §3.4/§4) |
| `04`/`07` building revisions | `LogicBuild=5`, `MemoryBuild=1`, `MessageBuild=0`, `SwitchBuild=1`, `LogicDisplayBuild=1`, `TileableLogicDisplayBuild=0`, `CanvasBuild=0`; manifests under `mind-core/revisions/buildings/` | 07 §6.2 contract; this plan supplies the per-kind field lists |
| `05_SIM_CORE` | `TickSet::LogicVars`, `EntitySet::UpdateBuildings` ordering, `EventBus`/`ResetEvent`, `SimCommand`, `Sim::checksum`, `RngStream` (add `Logic`), `SimClock`, `Teams::present` stable order | Present; requires adding `RngStream::Logic` + checksum inclusion (§8 R3) |
| `06_WORLD` | `WorldGrid`/`Tile`/`set_block`/`set_floor`/`set_overlay` (for `setblock`), `WorldHooks`, `check_map_area`, tile events for link invalidation, `LogicFilter` hook (`apply_filters` calls `run_logic_script`) | Present (06 §3.2–3.4, §3.8); 06 already declares the `logic` filter hook |
| `07_BLOCKS` | `BuildingBehavior` (+ `sense`), `ConfigValue::{Bytes,String,Number,Bool,Building,Point2}`, `register_behavior`, building lifecycle hooks, `Build.valid_break`, `WorldHooks::new_building`, `BuildingWriter/Reader` aliases, `BlockKindData` addition | Present; this plan appends `Logic*` variants to `BlockKindData` and adds `sense_object`/`set_prop`/`control` extension hooks (07 §8 R12) |
| `08_LOGISTICS` | payload calls used by `ucontrol` (`picked_unit_payload`, `picked_build_payload`, `payload_dropped`, `unit_building_control_select`, `transfer_item_to`, `take_items`) as host functions | 08 on disk; route through its public API, no edits to 08 unless a call is missing |
| `10_COMBAT` | `Senseable`/`Settable`/`ControlBlock` functions for turrets (10 §3.11/§3.14); `BulletType::create` full-signature entry for `bullet`; `Damage::damage` for `explosion` | Present; 10 froze its API in §3.14 |
| `11_UNITS` | `LogicAI` + `LogicAiState` fields, `UnitController::is_logic_controllable`, `unit.reset_controller`, `UnitType::spawn`, `Units::can_create`, `TeamData` caches/queries, `Pathfinder`/`ControlPathfinder` (`autoPathfind`), `UnitGroup` | Present (11 §3.5/§3.6/§3.8, §8 R8); 11 declared `LogicAI` state public to 13 and `check_target_timer` exposed |
| `12_CAMPAIGN` (not on disk) | `Rules` fields (`logic_unit_control`, `logic_unit_build`, `logic_unit_deconstruct`, `disable_world_processors`, `allow_edit_world_processors`, `world_processor_player_link`, `allow_logic_data`, `editor`, `mission`, `objective_flags`, `banned_blocks/units`, `limit_map_area*`, `music_volume`, `unit_light`, `solar_multiplier`, `drag_multiplier`, `ambient_light`, `pause_disabled`, `can_game_over`, `lighting`, `wave*`, `enemy_core_build_radius`, `drop_zone_radius`, `unit_cap`, `spawns`, `default_team`, `wave_team`), `TeamRules` multipliers, `MapMarkers`/`ObjectiveMarker`/`MapObjectives.markerNameToType`, `TeamData` | **Not written**; this plan stubs behind `LogicRulesApi`/`MarkerApi` traits with upstream defaults (05 §3.12 minimal `Rules`), replaced in 12 without API churn (§8 R1) |
| `14_UI` | logic dialog/canvas/variables/globals UI consuming `StatementMeta`+`StatementField`; `MessageState` writer; `LCanvas::load/save` semantics | Not written; this plan ships the metadata + message-state seam |
| `15_INPUT_RTS` | processor selection/monitor hooks, `logicCutscene` camera flags, `MessageState` toggles | Not written; `ClientHooks` seam provided |
| `16_RENDER` | `LDrawable` execution (`processCommands`, `FrameBuffer`, `commandImage`/print glyph rendering), `LogicFontMetrics`, display ownership/`drawDisplays` | Not written; command queue + packing frozen here so 16 only renders |
| `17_FX`/`18_AUDIO` | `LogicFx` name → `EffectId`; `@sfx-*` sound ids; `playsound`/`playmusic` sinks | Not written; table + events frozen here |
| `19_EDITOR` | processor editor dialog; `LogicFilter` options UI; editor processor placement | Not written; the `LogicFilter` runner hook is here |
| `21_MULTIPLAYER` | relay of `Configure`/`LogicSyncEvent`/`ClientLogicDataEvent`/host rule ops; authority flag (`SimAuthority`) consulted by all `net.client()` guards | Not written (HLP §10 OD3); local/offline default = authority true |
| `23_PARITY` | scenario/bench registration, `tests/golden/logic/*`, `parity/logic/*` goldens | Continuous |

### 3.15 Boundaries & invariants

1. `mind-core` (and `logic`) must not depend on `godot`, `tokio`, or platform types; CI boundary check from plan 00.
2. The VM never blocks the tick: per-instruction work is bounded; `WaitI`/`StopI` yield; `run_logic_script` is capped by instruction count.
3. Constants are read-only (`set*` silently no-ops); `@counter` stays numeric; unknown variables become null; out-of-range ids return null.
4. Serialization order is ABI: statement fields, `LogicBuild` revision 5 layout, `DisplayCmd` bit layout, and `LAccess`/`LogicRule`/enum ordinals are append-only and never reordered.
5. No `HashMap` iteration, no wall-clock reads, no allocation in steady-state instruction execution (buffers reused; `String`/`SmolStr` allocations only where upstream allocates).
6. Every ported file carries the GPL header; no upstream generated file is written.

---

## 4. Port map

### 4.1 `core/src/mindustry/logic/*.java`

| Mindustry source | Rust target | Notes |
|---|---|---|
| `LAssembler.java` | `logic/assembler.rs` | vars table, numeric/color parsing, unescape, bootstrap vars, custom-parser registry. |
| `LParser.java` | `logic/parser.rs` | lexer + label fixups + privileged filtering; exact error messages for the ported tests. |
| `LStatements.java` (53 registered classes) | `logic/statement.rs` + `logic/statements/{io,block,operation,unit,world}.rs` + `logic/textio.rs` | One `Statement` enum; UI halves → `StatementMeta` + plan 14. `CommentStatement` ported but unregistered. |
| `LStatement.java` | `logic/statement.rs` | `sanitize`, `copy`, `name`/`type_name`/`statement_key`, `use_wrapping`, `hidden`, `privileged`, `non_privileged`; UI helpers (`field`, `col`, `showSelect`, `param`, `tooltip`, `bundle`) → 14. |
| `LExecutor.java` | `logic/executor/{mod,instr,unit_control,radar,draw}.rs` | All instruction implementations; `runLogicScript`; `syncVariable` → `LogicSyncEvent`; `setMapArea`/`setFlag`/`logicExplosion`/`createMarker` remotes → host commands/events. |
| `LVar.java` | `logic/value.rs` | `LogicValue`/`LogicObject`, conversions, `setlink`, invalid handling. |
| `LCategory.java` | `logic/enums.rs` | 7 categories with color/icon-name data for 14. |
| `LogicOp.java` | `logic/ops.rs` | Symbol table, unary/binary/object functions, `rand` via `RngStream::Logic`. |
| `ConditionOp.java` | `logic/ops.rs` | `test(LVar, LVar)` incl. the `strictEqual` special case. |
| `LAccess.java` | `logic/access.rs` | Exact enum order, params, `isObj`, privileged set, derived tables. |
| `LUnitControl.java` | `logic/enums.rs` | 21 variants + params; `lunitcontrol.label.*` keys. |
| `GlobalVars.java` | `logic/globals.rs` | Constants, update, docs entries, lookup (absent `logicids.dat`). |
| `GlobalVarsDialog.java` | **14** (UI) | Consumes `LogicVars::entries`. |
| `LCanvas.java` | **14** (UI) + `logic/textio.rs` (save/load text) | `save()` = `Assembler::write`; `load()` truncates at `MAX_INSTRUCTIONS`. |
| `LogicDialog.java` | **14** (UI) | `show(code, executor, privileged, modified)` contract documented for 14. |
| `LogicFx.java` | `logic/fx.rs` | Ordered effect table + flags; `EffectId` resolution in 17. |
| `LogicRule.java` | `logic/rules.rs` | Enum + `apply_to_rules`. |
| `FetchType.java` | `logic/enums.rs` | 8 variants + `all`. |
| `QueryType.java` | `logic/enums.rs` | 3 variants + `queryable` (bullet disabled upstream). |
| `QueryShape.java` | `logic/enums.rs` | 2 variants. |
| `RadarSort.java` | `logic/enums.rs` + `logic/executor/radar.rs` | 5 sort functions. |
| `RadarTarget.java` | `logic/enums.rs` | 8 filters, incl. `enemy` excludes derelict. |
| `LLocate.java` | `logic/enums.rs` | 4 variants. |
| `TileLayer.java` | `logic/enums.rs` | 4 variants + `settable` (3). |
| `MessageType.java` | `logic/enums.rs` | 4 variants. |
| `CutsceneAction.java` | `logic/enums.rs` | 7 variants. |
| `LMarkerControl.java` | `logic/enums.rs` | 26 variants + params. |
| `Senseable.java` | `logic/access.rs` | Trait + `noSensed` sentinel. |
| `Settable.java` | `logic/access.rs` | Trait (3 methods). |
| `Controllable.java` | `logic/access.rs` | Trait + `team`. |
| `Ranged.java` | `logic/access.rs` | Trait used by radar/locate. |
| `LReadable.java`/`LWritable.java`/`LPrintable.java`/`LDrawable.java` | `logic/access.rs` + `logic/blocks/*` | Implemented by processor-adjacent building behaviors. |
| `LogicScript.java` | `logic/script.rs` | Dead upstream (`TODO not used`); only `run_logic_script` + instruction-cap semantics ported. |
| `annotations/.../misc/LogicStatementProcessor.java` | `logic/textio.rs` | Hand-written field-order table + writer/reader; parity dump tool `parity/java/DumpLogicIO.java`. |

### 4.2 `core/src/mindustry/world/blocks/logic/*.java` and adjacent integrations

| Mindustry source | Rust target | Notes |
|---|---|---|
| `LogicBlock.java` | `logic/blocks/logic_block.rs` | `LogicBuild` behavior, `LogicLink`, `compress`/`readCompressed`, revision 5 codec, privilege surface; metadata fields land in 02/07. |
| `LogicDisplay.java` | `logic/blocks/display.rs` | `GraphicsType`, command bytes, `DisplayCmd`, `LogicDisplayState`, `LogicDisplays` arena, revision 1 transform. Rendering → 16. |
| `TileableLogicDisplay.java` | `logic/blocks/display.rs` (`TileableDisplayBehavior`) | `link_displays`, `bits`, `tiles_width/height/origin`, sense overrides; FrameBuffer merge → 16. |
| `MemoryBlock.java` | `logic/blocks/memory.rs` | Slot sentinel semantics, revision 1, sense. |
| `MessageBlock.java` | `logic/blocks/message.rs` | config/print limits, read/sense, write/read. |
| `SwitchBlock.java` | `logic/blocks/switch.rs` | enabled config, revision 1, config tap. |
| `CanvasBlock.java` | `logic/blocks/canvas.rs` | bit-packed pixels, palette, blending, revision 0. |
| `content/Blocks.java` §logic region | 02 (`BlockDef`) + 07 (`BlockKindData::Logic*`) + `logic/blocks/mod.rs` | 13 registers behaviors; values transcribed in §3.6. |
| `entities/comp/BuildingComp.java` sense/control/setProp | `logic/access.rs` (traits) + 07 behavior | 07 owns the base switch body per its §3.4 note; 13 defines `LAccess` + tables + dispatch. |
| `entities/comp/UnitComp.java` sense/control/setProp | 11 components + `logic/access.rs` | 11 owns bodies; 13 defines the mapping table. |
| `ai/types/LogicAI.java` | `ai/types/logic.rs` (11) | 13 calls setters. |
| `maps/filters/LogicFilter.java` | `logic/script.rs` (`run_logic_script`) + 06 filter hook | `maxInstructionsExecution = 6_250_000`; loop flag. |
| `tests/src/test/java/LogicTests.java` | `logic::parser::tests`, `logic::value::tests`, `logic::textio::tests` (§7a) | All 15 groups. |

---

## 5. Milestones & task breakdown

Each milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, and the named harness command; evidence goes in the Changelog. GPL header on every ported file.

- **M0 — Value model + lexer/parser + three statements (smallest vertical slice).**
  `value.rs` (LogicValue/LVar/VarArena/conversions/parsing/unescape), `parser.rs` (tokens, strings, labels, errors), `statement.rs`/`textio.rs` for `set`/`op`/`jump`, `assembler.rs` bootstrap. Port all `LogicTests` parse/sanitize groups.
  *Verify:* `cargo test -p mind-core logic::parser` (escape/sanitize/parse/number/color/CRLF tables); `mind-headless logic run logic_arith` (set/op/jump only).
- **M1 — All 53 statements: text IO + build halves.**
  Remaining statements + instruction construction; full field-order table; privileged filtering; `@configure`/`configure` rewrites; legacy op renames; `ClientDataStatement` compile-away without `allow_logic_data`; `LogicFx`/enums.
  *Verify:* `logic::textio::tests::all_statements_roundtrip` (53 × generated corpus); `parity/golden_logicio.txt` diff empty; `logic_io_roundtrip` scenario.
- **M2 — Executor core (single-processor VM).**
  `executor/mod.rs` + `instr.rs`: load/run_once, budget driver, set/op/select/jump/end/noop/stop/wait, print/printchar/format/printflush, packcolor/unpackcolor, `@counter`/`@ipt`/`@unit`/`@this`, read/write over memory stubs, lookup null, `setrate`.
  *Verify:* `logic::executor` unit tests + `logic_arith`, `logic_strings`, `logic_budget` scenarios.
- **M3 — Logic blocks + persistence.**
  `blocks/logic_block.rs` (links, updateTile fixpoint, config, revision 5, privilege), `memory.rs`, `switch.rs`; `TypeIO` variable codec through 04; `LogicTimeouts`.
  *Verify:* `logic_save_load`, `logic_link_sensor`; `cargo test -p mind-core logic::blocks`.
- **M4 — Displays, message, canvas + draw packing.**
  `blocks/{display,message,canvas}.rs`, `executor/draw.rs`, `LogicDisplays`, `LogicFontMetrics` seam.
  *Verify:* `logic_draw` scenario (golden `DisplayCmd` vectors), `logic_draw_headless` (skip-flag parity), MCP display scenario.
- **M5 — Capabilities: sensor/control/setprop/radar/units.**
  `access.rs` + dispatch; `executor/radar.rs`; `unit_control.rs` + `LogicAI` bridge; `ulocate`; gating rules.
  *Verify:* `logic_sensor_access` (07/10/11 harness fixtures), `logic_unit_control_gating`, `logic_radar_filters`.
- **M6 — Privileged world instructions.**
  query/fetch/getblock/setblock/spawn/bullet/status/weathersense/weatherset/spawnwave/setrule/message/cutscene/effect/explosion/getflag/setflag/setmarker/makemarker/playsound/playmusic/localeprint/clientdata/sync; host gating; `MessageState`; marker/rules stubs.
  *Verify:* `logic_privileged_world`, `logic_markers_smoke` (stub), `logic_sync_event`.
- **M7 — GlobalVars + script runner + content integration.**
  `globals.rs` full constant set + update; `script.rs` + 06 filter hook; content-constant registration at content init; plan-20 `remove` reset hooks.
  *Verify:* `logic_globals`, `logic_script_filter`.
- **M8 — Goldens, perf, MCP, plan-14 metadata handoff.**
  inspector `Logic` tab, `MindSimHost` methods, benches, `parity/logic/*`, hand-off doc for 14 (`StatementMeta`/`StatementField`/`LCanvas` contract).
  *Verify:* §7c MCP run + budgets + checklist.

Dependency-safe ordering: M0–M2 need only plan 02's registry stub (a fake 2-item/1-unit registry is enough); M3 swaps in 04/07; M4 uses 16 stubs; M5 uses 07/10/11 fixtures; M6 uses 12 trait stubs.

---

## 6. Data & formats

### 6.1 Text language rules (grammar card)

| Rule | Value |
|---|---|
| Line endings | CR and CRLF normalized to LF before parsing; a lone CR acts as a line ending. |
| Token separators | `\n`, `;`, space, tab; a `#` comment runs to EOL; max 16 tokens/line ("Line too long; may only contain 16 tokens"). |
| Statement separator | newline or `;`; only lines with ≥1 token are processed. |
| Strings | `"…"`; escapes `\n`, `\"`, `\\`, `\uXXXX` (4 hex validated); escapes preserved raw in statement fields and decoded only by `var()`; >65535 UTF-16 bytes or missing quote before EOL/EOF errors ("Missing closing quote \" before end of line/file.", "String value too long.", "Invalid \u escape; expected 4 hex digits."). |
| Labels | a single token ending with `:` defines a jump location; max 500 jumps; duplicates/undefined labels error ("Jump label already defined: \"…\".", "Undefined jump location: \"…\". …"). |
| Jump pre-substitution | `jump <label> …` becomes `jump -1 …` and is patched after the pass. |
| Legacy rewrites | `op` operands `atan2→angle`, `dst→len`; `status <effect>` → `@status-<effect>` when the effect exists; tokens `@configure→@config`, `configure→config`. |
| Parse cap | at most `Executor::MAX_INSTRUCTIONS = 1000` statement lines. |
| Privileged | a privileged statement parsed by a non-privileged assembler becomes `Invalid` (→ `NoopI`). |
| Comments | `#`-prefixed lines produce nothing; `CommentStatement` is unregistered and serializes to nothing. |

### 6.2 Registered statements and exact serialized field order (53 rows)

Order below is the `LogicIO.write`/`read` order (Java declaration order; inherited fields last). Types: `S` = String, `I` = int, `B` = bool, `E(enum)`. All fields are written separated by single spaces after the statement name; reads tolerate missing trailing tokens by keeping defaults.

| # | reg name | variant | fields in order |
|---|---|---|---|
| 1 | `noop` | InvalidStatement | — |
| 2 | `read` | Read | `S output`, `S target`, `S address` |
| 3 | `write` | Write | `S input`, `S target`, `S address` |
| 4 | `draw` | Draw | `E(GraphicsType) type`, `S x`, `S y`, `S p1`, `S p2`, `S p3`, `S p4` |
| 5 | `print` | Print | `S value` |
| 6 | `printchar` | PrintChar | `S value` |
| 7 | `format` | Format | `S value` |
| 8 | `drawflush` | DrawFlush | `S target` |
| 9 | `printflush` | PrintFlush | `S target` |
| 10 | `getlink` | GetLink | `S output`, `S address` |
| 11 | `control` | Control | `E(LAccess) type`, `S target`, `S p1`, `S p2`, `S p3`, `S p4` |
| 12 | `radar` | Radar | `E(RadarTarget) target1`, `target2`, `target3`, `E(RadarSort) sort`, `S radar`, `S sortOrder`, `S output` |
| 13 | `sensor` | Sensor | `S to`, `S from`, `S type` |
| 14 | `set` | Set | `S to`, `S from` |
| 15 | `op` | Operation | `E(LogicOp) op`, `S dest`, `S a`, `S b` |
| 16 | `select` | Select | `S result`, `E(ConditionOp) op`, `S comp0`, `S comp1`, `S a`, `S b` |
| 17 | `wait` | Wait | `S value` |
| 18 | `stop` | Stop | — |
| 19 | `lookup` | Lookup | `E(ContentType) type`, `S result`, `S id` |
| 20 | `packcolor` | PackColor | `S result`, `S r`, `S g`, `S b`, `S a` |
| 21 | `unpackcolor` | UnpackColor | `S r`, `S g`, `S b`, `S a`, `S value` |
| 22 | `end` | End | — |
| 23 | `jump` | Jump | `I destIndex`, `E(ConditionOp) op`, `S value`, `S compare` |
| 24 | `ubind` | UnitBind | `S type` |
| 25 | `ucontrol` | UnitControl | `E(LUnitControl) type`, `S p1`, `S p2`, `S p3`, `S p4`, `S p5` |
| 26 | `uradar` | UnitRadar (shared RadarFields) | `E target1`, `target2`, `target3`, `E sort`, `S radar`, `S sortOrder`, `S output` |
| 27 | `ulocate` | UnitLocate | `E(LLocate) locate`, `E(BlockFlag) flag`, `S enemy`, `S ore`, `S outX`, `S outY`, `S outFound`, `S outBuild` |
| 28 | `query` | Query | `E(QueryShape) shape`, `E(QueryType) type`, `S team`, `S x`, `S y`, `S w`, `S h` |
| 29 | `getblock` | GetBlock | `E(TileLayer) layer`, `S result`, `S x`, `S y` |
| 30 | `setblock` | SetBlock | `E(TileLayer) layer`, `S block`, `S x`, `S y`, `S team`, `S rotation` |
| 31 | `spawn` | SpawnUnit | `S type`, `S x`, `S y`, `S rotation`, `S team`, `S result`, `S effect` |
| 32 | `bullet` | SpawnBullet | `S result`, `S from`, `S index`, `S x`, `S y`, `S rotation`, `S team`, `S owner`, `S damage`, `S velocityScl`, `S lifeScl`, `S aimX`, `S aimY` |
| 33 | `status` | ApplyStatus | `B clear`, `S effect`, `S unit`, `S duration` |
| 34 | `weathersense` | WeatherSense | `S to`, `S weather` |
| 35 | `weatherset` | WeatherSet | `S weather`, `S state` |
| 36 | `spawnwave` | SpawnWave | `S x`, `S y`, `S natural` |
| 37 | `setrule` | SetRule | `E(LogicRule) rule`, `S value`, `S p1`, `S p2`, `S p3`, `S p4` |
| 38 | `message` | FlushMessage | `E(MessageType) type`, `S duration`, `S outSuccess` |
| 39 | `cutscene` | Cutscene | `E(CutsceneAction) action`, `S p1`, `S p2`, `S p3`, `S p4` |
| 40 | `effect` | Effect | `S type`, `S x`, `S y`, `S sizerot`, `S color`, `S data` |
| 41 | `explosion` | Explosion | `S team`, `S x`, `S y`, `S radius`, `S damage`, `S air`, `S ground`, `S pierce`, `S effect` |
| 42 | `setrate` | SetRate | `S amount` |
| 43 | `fetch` | Fetch | `E(FetchType) type`, `S result`, `S team`, `S index`, `S extra` |
| 44 | `sync` | Sync | `S variable` |
| 45 | `clientdata` | ClientData | `S channel`, `S value`, `S reliable` |
| 46 | `getflag` | GetFlag | `S result`, `S flag` |
| 47 | `setflag` | SetFlag | `S flag`, `S value` |
| 48 | `setprop` | SetProp | `S type`, `S of`, `S value` |
| 49 | `playsound` | PlaySound | `B positional`, `S id`, `S volume`, `S pitch`, `S pan`, `S x`, `S y`, `S limit` |
| 50 | `playmusic` | PlayMusic | `S name`, `S interrupt` |
| 51 | `setmarker` | SetMarker | `E(LMarkerControl) type`, `S id`, `S p1`, `S p2`, `S p3` |
| 52 | `makemarker` | MakeMarker | `S type`, `S id`, `S x`, `S y`, `S replace` |
| 53 | `localeprint` | LocalePrint | `S value` |

Defaults are those in the extracted declaration table: `radar enemy any any distance turret1 1 result`, `control enabled block1 0 0 0 0`, `sensor result block1 @copper`, `set result 0`, `op add result a b`, `select result notEqual x false a b`, `lookup item result 0`, `getblock block result 0 0`, `setblock block @air 0 0 @derelict 0`, `spawn @dagger 10 10 90 @sharded result true`, `bullet result @dagger 0 x y angle null null -1 1 1 -1 -1`, `status @status-wet unit 10` (`clear` default false), `query circle unit null 0 0 10 10`, `setrule waveSpacing 10 0 0 100 100`, `message announce 3 @wait`, `cutscene pan 100 100 0.06 0`, `effect warn 0 0 2 %ffaaff ""`, `explosion @crux 0 0 5 50 true true false true`, `fetch unit result @sharded 0 @conveyor`, `playsound @sfx-shoot 1 1 0 @thisx @thisy true` (`positional` default false), `playmusic "game1" true`, `setmarker pos 0 0 0 0`, `makemarker shape 0 0 0 true`, `localeprint "name"`. `JumpStatement.destIndex` is the **first** field (numeric jumps) and is replaced by labels at parse time; `setupUI`/`saveUI` re-derivation is plan 14.

### 6.3 Instruction semantics quick table (VM)

| Instruction | Key semantics |
|---|---|
| `SetI` | no-op when destination constant; copies `isobj`/value. |
| `OpI` | `strictEqual` special case (type + `Structs.eq`); unary ops; object-vs-object uses the object function for `equal/notEqual`, numeric otherwise. |
| `SelectI` | `result.set(test ? a : b)`; no-op when constant. |
| `JumpI` | `address != -1 && op.test(value, compare)` → `counter = address`. |
| `EndI` | `counter = instructions.len`; `NoopI` nothing. |
| `WaitI` | `value <= 0` → yield once (`cur_time = 0`); `cur_time >= value` → reset; else `counter--`, yield, `cur_time += delta/60` (fixed 1/60 per tick). |
| `StopI` | `counter--`; `yield = true; stop = true`. |
| `ReadI` | `LReadable` target; else string char (NaN OOB), `Query` index (null OOB), else null. |
| `WriteI` | `LWritable` + writable check. |
| `SenseI` | null + `dead` → 1; content → `sense(content)`; LAccess privileged check; `senseObject` sentinel → numeric; string/query `size`/`bufferSize`; else null. |
| `ControlI` | building + (privileged or valid link); `enabled` noSleep/lastDisabler; object/numeric dispatch. |
| `RadarI` | base must be `Ranged` + team/privileged; buildings refresh every 30 ticks; units per `LogicAI::check_target_timer`; enemy/ally/any loops over `Teams::present`; sort multiplier by `sortOrder.bool()`; cached output between refreshes. |
| `UnitBindI` | gated by `logic_unit_control`; rebind cursor per unit type; specific-unit binding requires team/privileged + `logic_controllable`; null otherwise. |
| `UnitControlI` | gating; `check_logic_ai`; all 21 commands per §3.8; coordinates unconverted; build/deconstruct rules; `getBlock` range = max(unit.range, type.build_range); item take/drop range `logicItemTransferRange` + half block size; transfers throttled 90 ticks. |
| `UnitLocateI` | gated; per-instruction cache + `check_target_timer`; ore → `indexer.find_closest_ore`; building → closest flagged/enemy; spawn → `spawner.get_spawns`; damaged → `Units::find_damaged_tile`; outputs converted to tile coords; build output only when within max(unit.range, building_range) or own team. |
| `DrawI` | skipped when `skip_draw_pack`; `col` unpacks packed double color; `print` expands text via font metrics and clears the text buffer; other commands packed with `pack_sign`; cap 256. |
| `DrawFlushI` | `LDrawable` append (cap 1024 total, `operations++`), then clear buffer. |
| `PrintI`/`PrintCharI`/`FormatI` | 400-char cap; integer rendering when within 1e-5; object to-string (`null`, string, `MappableContent.name`, building block name, unit type name, enum name, team name, `[object]`); `printchar` content → emoji char; `format` replaces the lowest `{N}` placeholder. |
| `PrintFlushI` | `LPrintable` print, then clear text buffer. |
| `GetLinkI` | index into `links` (bounds → null). |
| `LookupI` | `logicVars.lookup_content` — null without `logicids.dat`. |
| `PackColorI`/`UnpackColorI` | clamp 0..1 → `Color::to_double_bits`; unpack from double bits. |
| `SetRateI` | `ipt = clamp(value, 1, privileged ? max_ipt : ipt_base)`; writes the `@ipt` LVar. |
| `FetchI` | team `TeamData` lists/unitCache/cores/buildings/getBuildings; counts; null/0 OOB. |
| `QueryI` | privileged; `@queries` arena; world-coord rect/circle; bullet type disabled; team filter or all present teams; circle trims by `within(radius + hit_size/2)`. |
| `GetBlockI`/`SetBlockI` | `TileLayer` switch; setblock host-only (authority check replacing `net.client()`); floor/ore/block only. |
| `SpawnUnitI` | host-only; team + non-internal + `Units::can_create`; random ±0.01 world offset; `result` set; effect bool decides `spawn_effect` vs `unloaded()+UnitSpawnEvent`. |
| `SpawnBulletI` | resolves the bullet from UnitType weapon index / turret ammo / `shootType`; `type.create(owner, team, x, y, rot, damage, velocity_scl, life_scl, aim_x, aim_y)`. |
| `ApplyEffectI` | host-only; apply/clear status (`duration * 60`). |
| `SenseWeatherI`/`SetWeatherI` | weather active flag; create/fade via the weather host call. |
| `SetRuleI` | 28 rules per §3.10; the team branch requires a team. |
| `FlushMessageI` | success default 1; headless clears unless `mission`; `@wait` blocks while the corresponding `MessageState` flag is active; text `@key` bundle lookup; `mission` → `rules.mission`. |
| `EffectI` | `LogicFx` lookup at build; rotation clamped unless `rotate`; data object. |
| `ExplosionI`/`logicExplosion` | host-only; radius cap 100; damage ≥ 0; effect shockwave/explosion. |
| `SyncI` | non-constant variable; throttle 3 ticks; fires `LogicSyncEvent`. |
| `ClientDataI` | string channel; reliable/unreliable event; only built when `allow_logic_data`. |
| `GetFlagI`/`SetFlagI` | `rules.objective_flags` contains/change; set only when state changes. |
| `SpawnWaveI` | host-only; natural → `logic.skip_wave()`; else per `SpawnGroup` matching packed x/y, `get_spawned(wave-1)` units with ±2-tile spread. |
| `SetPropI` | `Settable` target; LAccess (obj/num) or content key. |
| `PlaySoundI` | positional vs 2D; volume clamp 2; `@sfx-*` id table. |
| `PlayMusicI` | name lookup (null stops); interrupt flag. |
| `SetMarkerI` | remove/flushText/texture/control dispatch on `MapMarkers`; texture fetch flag uses the text buffer. |
| `MakeMarkerI` | `markerNameToType` lookup; cap 20000; replace flag. |
| `LocalePrintI` | map-locale property (`<name>.mobile` preferred on mobile), raw append (no cap truncation). |

Executor constants: `MAX_INSTRUCTIONS = 1000`, `MAX_GRAPHICS_BUFFER = 256`, `MAX_DISPLAY_BUFFER = 1024`, `MAX_TEXT_BUFFER = 400`, `MAKE_MARKER_MAX = 20000`, `SYNC_INTERVAL_TICKS = 3`, `UNIT_TIMEOUT_KEEP = 600`, `TRANSFER_DELAY = 90`, `MAX_INSTRUCTION_SCALE = 5` (per-block default).

### 6.4 Op / condition tables

`LogicOp` (37 entries, symbol order): `+ - * / // % %% ^ == not and < <= > >= === << >> >>> or b-and xor flip max min angle anglediff len noise abs sign log logn log10 floor ceil round sqrt rand sin cos tan asin acos atan`. Unary: `flip abs sign log log10 floor ceil round sqrt rand sin cos tan asin acos atan`. Functional/labeled: `max min angle anglediff len noise`. Object function: `equal`, `notEqual` (Java `Structs.eq`, incl. number-vs-number value equality). Trig uses `doubleDegRad`/`doubleRadDeg`; `noise` is Simplex raw2d; `rand` uses the `Logic` RNG stream. `strictEqual` is handled specially (type + `Structs.eq` for objects, `==` for numbers).

`ConditionOp` (8): `== not < <= > >= === always`; equality is `|a − b| < 1e-6` numeric, `Structs.eq` object (strict: type + exact value / identity equality). `ConditionOp::test(LVar, LVar)` is the single implementation shared by `jump`/`select`.

### 6.5 Compressed code blob + `LogicBuild` revision 5

`compress(code, links)` — zlib (`DeflaterOutputStream` default) wrapping big-endian `DataOutputStream`:

```
u8    version = 1
i32   byte_len
bytes code (UTF-8)
i32   link_count (reader clamps to 6000)
per link:
  u16 name_byte_len + name bytes   (upstream writeUTF; ASCII link names in practice)
  i16 x, i16 y
```

`read_compressed(data, relative)` adds the processor tile origin when `relative`; version 0 = links only (`i32` positions ignored); unknown/invalid builds drop links; a mismatched block type reassigns the link name via `find_link_name`; duplicate building ids drop later links; then `update_code`. Size caps: `MAX_BYTE_LEN = 102400` code bytes, `MAX_COMPRESSED_LEN = 16000`, `MAX_LINKS = 6000`, `MAX_NAME_LENGTH = 32`.

`LogicBuild::version() == 5`; `write` order (after 07's `write_base`):

```
i32 compressed_len; bytes compressed
i32 var_count                       // non-null executor vars + @unit
  if unit present: str "@unit"; TypeIO.writeObject(unit)
  per var: str name; TypeIO.writeObject(value or numval)   // null objects skipped
i32 0                               // legacy memory count
if privileged: i16 clamp(ipt,1,max_ipt) else i16 (ipt==base ? 0 : clamp(ipt,1,base))
str tag; i16 iconTag
i16 wait_count; per wait: i16 instruction_index, f32 cur_time
f32 accumulator
```

`read` mirrors with `revision >= 1..5` gates; waits/vars are applied in a deferred `load_block` after `update_code` (variables only when the target var exists and is non-constant or `@unit`; `Boxed` values unboxed); `@unit` is restored separately.

### 6.6 Block revisions and display packing

| Building kind | revision | fields after base |
|---|---|---|
| `LogicBuild` | 5 | §6.5 |
| `MemoryBuild` | 1 | `i32 count` + per slot TypeIO tag/value; rev 0 = raw f64 list; `after_read_all` unboxes |
| `MessageBuild` | 0 | `str message` |
| `SwitchBuild` | 1 | `bool enabled` (rev 0 reads nothing) |
| `LogicDisplayBuild` | 1 | `bool has_transform` + 16×f32 matrix |
| `TileableLogicDisplayBuild` | 0 | none beyond base/display |
| `CanvasBuild` | 0 | `i32 len` + bytes (skip when length differs) |

`DisplayCmd`: `u64 = type(4b) | x(10b) | y(10b) | p1(10b) | p2(10b) | p3(10b) | p4(10b)`; each value is `pack_sign` (10-bit magnitude + sign bit) except `col` color components (`pack`, 10-bit unsigned) and `print` (`p1` = char code, `p1` = align bits for the print command). Command bytes: `0 clear, 1 color, 2 colorpack (virtual), 3 stroke, 4 line, 5 rect, 6 lineRect, 7 poly, 8 linePoly, 9 triangle, 10 image, 11 print, 12 translate, 13 scale, 14 rotate, 15 reset`. Display content id `30`; `scale_step = 0.05`; `max_sides = 25`.

### 6.7 Tables owned here (frozen ordinals)

- **`LAccess`** — exact upstream order (`totalItems … color`) with parameterized entries `enabled("to")`, `shoot("x","y","shoot")`, `shootp(true,"unit","shoot")`, `config(true,"to")`, `color("to")`; `privileged = {cameraX, cameraY, cameraWidth, cameraHeight}`; `settable = {x, y, velocityX, velocityY, rotation, speed, armor, health, shield, team, flag, totalPower, payloadType, bulletTime, bulletLifetime}`; `senseable` = params ≤ 1 && !privileged; `senseablePrivileged` = params ≤ 1; `controls` = params > 0.
- **Building base table** (compiled from `BuildingComp.java:2100-2227`): numeric senses `x/y` (tile→`World.conv`), `color` (team color double bits), `dead`, `solid`, `team`, `health/maxHealth`, `efficiency`, `timescale`, `range` (Ranged, /8), `rotation`, `totalItems`, `totalLiquids`, `totalPower` (buffered ? status·capacity : 1), capacity senses, `powerNetIn/Out/Stored/Capacity`, `armor`, `enabled`, `controlled` (`ControlBlock` → ctrlPlayer), `payloadCount`, `size`, camera senses via a `ControlBlock` proxy; object senses `type`, `firstItem`, `config` (only `config_senseable`), `payloadType`; content senses item/liquid/payload counts; `control` handles `enabled` and object `config` (only `logic_configurable`, excluding `LogicBuild`); `setProp` numeric `health/team/totalPower`, object `team`, content item/liquid amounts with accept/remove rules.
- **Unit table** (compiled from `UnitComp.java:267-427`): numeric senses `totalItems`, `itemCapacity`, `rotation`, `health`, `shield`, `maxHealth`, `flying`, `x/y`, `velocityX/Y`, `dead`, `team`, `shooting`, `boosting`, `range`, `shootX/Y`, camera senses via `Player`, `mining/mineX/mineY`, `buildX/buildY`, `armor` (override), `flag`, `speed`, `controlled` (`LogicAI` → ctrlProcessor, `Player` → ctrlPlayer, `CommandAI` with command → ctrlCommand), `payloadCount/totalPayload/payloadCapacity`, `size` (`hitSize/8`), `color`, `selectedRotation`, `pingX/Y`; object senses `type`, `name`, `firstItem`, `controller` (`LogicAI.controller`), `payloadType`, `building`/`breaking`, `selectedBlock`, `pingText`; content senses stack item, payload unit/block counts, status duration; `setProp` numeric `health/shield/x/y/velocityX/Y/rotation/team/flag/speed/armor`, object `team`/`payloadType`, content item/status.
- **`GlobalVars` init/update surface** is enumerated in §3.10; sound constants require plan 18's id table, color constants require the Arc named-color table (16/00 supplies `mind_core::math::colors`; fallback list is committed, §8 R5), and `@sfx-*`/`@color*` entries are skipped when their source table is absent (upstream `Core.assets == null` behavior).
- **`LogicRule`** exact order: `currentWaveTime, waveTimer, waves, wave, waveSpacing, waveSending, attackMode, enemyCoreBuildRadius, dropZoneRadius, unitCap, mapArea, lighting, canGameOver, ambientLight, unitLight, solarMultiplier, dragMultiplier, ban, unban, pauseDisabled, musicVolume, buildSpeed, unitHealth, unitBuildSpeed, unitMineSpeed, unitCost, unitDamage, blockHealth, blockDamage, rtsMinWeight, rtsMinSquad` (31 values; the team-specific tail requires `p1.team()`).
- **`LogicFx`** 35 entries in order: `warn, cross, blockFall, placeBlock, placeBlockSpark, breakBlock, spawn, trail, breakProp, smokeCloud, vapor, hit, hitSquare, shootSmall, shootBig, smokeSmall, smokeBig, smokeColor, smokeSquare, smokeSquareBig, spark, sparkBig, sparkShoot, sparkShootBig, drill, drillBig, lightBlock, explosion, smokePuff, sparkExplosion, crossExplosion, wave, bubble` with the size/rotate/color/data/bounds flags from `LogicFx.java`.
- **`MessageState`** (deviation 5): `{ has_announcement: bool, has_toast: bool, mission: Option<String> }`; plan 14 sets flags on presentation/expiry; headless defaults false.

### 6.8 Files

- Runtime: `client/rust/mind-core/src/logic/**` per §3.1.
- Goldens/fixtures: `tests/golden/logic/{statement_roundtrip.txt, displaycmd_vectors.bin, globals_init.json, la_access_table.json, logic_arith.checksums, logic_draw.checksums}`; `scenarios/logic_*.json` mirrored to `client/scenarios/`.
- Parity: `parity/java/DumpLogicIO.java` (standalone; prints every registered statement's name + declared field order from the upstream `:core` classpath) → `parity/golden_logicio.txt`; committed. It never writes into the Mindustry checkout.
- Bench: `bench/baselines.json` entries `logic_assemble`, `logic_vm_mixed`, `logic_idle_processors`, `logic_draw_pack` (§7d).
- Revision manifests: `client/rust/mind-core/revisions/buildings/{LogicBuild,MemoryBuild,SwitchBuild,LogicDisplayBuild}.json` per §6.6.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (`Mindustry/tests/src/test/java/LogicTests.java` → `cargo test -p mind-core`)

All upstream tests run through `ApplicationTests.launchApplication()` (full content + Logic bootstrap). The Rust harness boots a minimal `mind-core` registry fixture (`logic_test_rig`) with items `copper` + unit `dagger` + status `wet` needed by the parse tests, so no Godot/JVM is required.

| Upstream test group (parameterized) | Rust test | Cases |
|---|---|---|
| `parsesStringEscapes` | `logic::parser::tests::string_escapes` | 12 cases: plain, `\n`, `\"`, `\\`, `\\n` non-collapse, unknown `\t` passthrough, combined, empty string, trailing backslash, `\u0041`, `\uf8ff` pair, mixed `\u`+escapes. |
| `plainNumberIsNotTreatedAsAString` | `logic::parser::tests::plain_number_not_a_string` | `set result 5` → `from.isobj == false`, `numval == 5.0`. |
| `sanitizesInput` | `logic::statement::tests::sanitize` | 20 cases incl. bare `"`, `;`, space, `#`, quoted escapes, malformed `\u`, unquoted replacements. |
| `sanitizedQuotedValuesRoundTripThroughTheParser` | `logic::statement::tests::sanitize_roundtrip` | 10 cases; each asserts decoded value and `read(write(stmt)) == stmt`. |
| `parseVarValues` | `logic::value::tests::parse_var_values` | 36 cases: decimal/scientific, hex (`0x0…Long.MIN`), binary wrap, `null`. |
| `parseColorValues` | `logic::value::tests::parse_colors` | `%ffffff`, `%ff000080`, `%[red]` exact double bits. |
| `parseInvalidNumbers` | `logic::value::tests::parse_invalid_numbers` | 15 cases stay variables (empty, `e10`, `NaN`, `0x`, `0b2`, overflow > 64 bits, `%fff`, `%[nosuchcolor]`). |
| `unterminatedStringsThrow` | `logic::parser::tests::unterminated_strings_throw` | 8 cases incl. `\u12zz`, `\uhh23`, escaped-quote terminator, EOF, newline-in-string. |
| `varWithProperlyQuotedEmptyString` | `logic::value::tests::empty_string_const` | `""` → object, empty. |
| `quoteInVariableNameThrows` / `quoteAtEndOfVariableNameThrows` | `logic::parser::tests::quote_in_token_throws` | 2 cases. |
| `crlfAfterUnquotedTokenDoesNotCorruptTheToken` | `logic::parser::tests::crlf_token_identity` | same `bar` var object for CRLF/LF. |
| `crlfAfterQuotedStringParsesCleanly` | `logic::parser::tests::crlf_quoted_string` | value `bar`. |
| `loneCarriageReturnActsAsALineEnding` | `logic::parser::tests::lone_cr` | two statements. |
| `crlfLabelsResolveToTheSameJumpLocationAsLfLabels` | `logic::parser::tests::crlf_labels` | no error. |

New Rust-only tests (no upstream counterpart; they lock this plan's formats):

| Test | Oracle |
|---|---|
| `logic::textio::tests::all_statements_roundtrip` | for all 53 rows: build a max-field statement, `write → read → write`, byte-equal; missing trailing fields keep defaults. |
| `logic::textio::tests::field_order_matches_golden` | `STATEMENTS` field lists vs `parity/golden_logicio.txt` (skip when the golden is absent + `MIND_JAVA_PARITY=0`, else fail). |
| `logic::parser::tests::privileged_filtering` | `setblock`/`spawn`/`setrule`/`sync` parse to `Invalid` in a non-privileged assembler, to real statements when privileged. |
| `logic::parser::tests::legacy_rewrites` | `op atan2`, `op dst`, `status wet`, `sensor … @configure`, `configure` tokens. |
| `logic::parser::tests::jump_limits_and_errors` | 500-label limit, duplicate label, undefined label messages. |
| `logic::executor::tests::ops_table` | every `LogicOp`/`ConditionOp` symbol case incl. unary, `strictEqual`, object equality, angle/len semantics, `rand` stream reproducibility. |
| `logic::executor::tests::budget_clamping` | `ipt=2` + 10× `edelta` → ≤ `5*2` instructions; `setrate` clamp to `ipt_base`/`max_ipt`. |
| `logic::executor::tests::wait_and_stop` | `wait 0.5` yields 30 ticks then proceeds; `wait 0` yields once; `stop` halts with stop flag. |
| `logic::executor::tests::print_format` | integer vs fractional rendering, object names, 400 cap, `format {0}/{1}` substitution order. |
| `logic::executor::tests::draw_pack_vectors` | `DisplayCmd` bit vectors vs `tests/golden/logic/displaycmd_vectors.bin` (incl. `col`, `scale /0.05`, `image` content/display packing, print cursor advance). |
| `logic::blocks::tests::logic_build_io_roundtrip` | revision-5 write/read with code+links+vars+waits+accumulator+tag+iconTag; checksum equality. |
| `logic::blocks::tests::link_fixpoint` | invalid→valid link rename/`find_link_name`, duplicate link removal, `last_disabler` re-enable on removal. |
| `logic::blocks::tests::memory_sentinel` | number/object slots, OOB null, revision 0/1 read. |
| `logic::blocks::tests::canvas_pixels` | bit packing for 8/16-color canvases, OOB NaN. |
| `logic::globals::tests::globals_init` | constants table vs `globals_init.json`; `@time/@tick` update math; privileged `null` fallback. |
| `logic::access::tests::la_access_table` | enum order/params/privileged/settable vs golden; building/unit table spot checks. |
| `logic::script::tests::script_cap` | `run_logic_script` respects the instruction cap and loop flag. |
| `logic::tests::logic_ids_absent` | `lookup` → null; `lookup_logic_id` → -1; no `@<type>Count` constants. |

### 7b. Headless harness scenarios (`mind-headless`)

| Command | Behavior | Assertions |
|---|---|---|
| `mind-headless logic assemble <file> --out parsed.txt` | parse + write a program text | byte-equal output; incompatible privileged lines become `noop`. |
| `mind-headless logic run logic_arith --ticks 1 --json` | `set`/`op`/`jump` loop computing a known expression | final vars match; checksum golden. |
| `mind-headless logic run logic_strings --ticks 5 --json` | print/format/printchar/jump/select/wait/stop program | exact text buffer; yield tick counts. |
| `mind-headless logic run logic_link_sensor --ticks 10 --json` | place memory-cell + switch + processor, write program, assert memory/switch state | memory values and `sensor` reads match; `@links` count. |
| `mind-headless logic run logic_draw --ticks 3 --json --capture-draw` | display + draw program (`clear/col/stroke/line/rect/poly/print/rotate/scale`) | `DisplayCmd` vector equals golden; `operations` counter; 256 buffer cap. |
| `mind-headless logic run logic_draw_headless --ticks 3 --json` | same program without `--capture-draw` | buffer stays empty, `operations` still increments (upstream headless parity). |
| `mind-headless logic run logic_budget --ticks 600 --json` | `setrate`/hot loop across micro/logic/hyper/world processors | executed-instruction totals obey `5*ipt`; `@ipt` synced. |
| `mind-headless logic run logic_save_load --ticks 20 --out tmp/logic.msav --roundtrip` | run, wait at instruction 7, save, reset, load, run | checksum + wait timers + vars equal; code/tag/icon/transform preserved. |
| `mind-headless logic run logic_unit_control_gating --ticks 300 --json` | `ubind`/`ucontrol move` with `rules.logic_unit_control=false` then `true` | no controller installed / movement + 600-tick timeout; `logic_unit_build/deconstruct` gates. |
| `mind-headless logic run logic_privileged_world --ticks 10 --json` | world-processor program touching `getblock/setblock/spawn/bullet/status/setrule/getflag/effect/explosion/query/fetch` | rule/tile/unit side effects asserted via dump; non-privileged twin is all `noop`. |
| `mind-headless logic run logic_sync_event --ticks 12 --json` | `sync` on a variable | exactly 4 events over 12 ticks (every 3 ticks), value/id correct. |
| `mind-headless logic run logic_globals --ticks 120 --json` | dump `@time/@tick/@second/@minute/@wave/@waveTime/@mapw/@maph/@server/@client` | values match tick math. |
| `mind-headless logic run logic_script_filter --seed 7` | `LogicFilter`-style `run_logic_script` that mutates tiles, 6 250 000 cap | tile result equals golden; cap does not hang. |
| `mind-headless logic bench --suite logic_vm_mixed --json` | see §7d | budgets recorded. |

Scenario names follow plan 00 `{system}_{case}`; every scenario registers a golden checksum after the first accepted run.

### 7c. MCP playtest scenario (concrete, open-godot-mcp)

Preconditions: plan-00 spine running (`res://scenes/spine.tscn` with `SimHost` at `/root/Spine/SimHost`; adapt to the actual node if plan 00 differs), `MindSimHost.logic_*` methods present (M8), plan 06 grid + plan 07 buildings live.

1. `godot_health check` → `{ok:true}`.
2. `godot_game play` (plan-00 spine scene); wait for `godot_log` "content loaded" and tick > 0.
3. `godot_exec call`: `$"/root/Spine/SimHost".logic_place("logic-processor", 40, 40)` → `true`; same for `"memory-cell"` at (42, 40) and `"logic-display"` at (40, 43).
4. `godot_exec call`: build the mlog source (processor writes to the cell, then draws to the display):
   - `"write 123 cell1 0\nwrite 1 cell1 1\nread result cell1 0\nprint \"value: \"\nprint result\ndrawflush display1\n"`.
   - Call `logic_set_code(40, 40, code)` → `true`; verify `logic_get_state(40, 40).code_lines == 6`.
5. `godot_exec eval`: `print($"/root/Spine/SimHost".logic_display_commands(40, 43))` → a non-empty `PackedInt64Array`; `logic_get_state(40,40).vars.result == 123`.
6. `godot_runtime_state watch` node `/root/Spine/SimHost` property `tick` for 500 ms; then `godot_exec eval` on `logic_get_state(42,40).memory[0] == 123` and `.memory[1] == 1` (memory read via the same state API).
7. `godot_input` dev action or second `godot_exec call`: `logic_set_rule("waves", false)` to keep the state stable; re-run one more `logic_set_code` with a `draw rect 0 0 20 20` line and confirm the display command count grows after `drawflush`.
8. `godot_screenshot` with `region` around the display (screen-space rect from the display's world position captured via `MindTileGrid`/camera math; simplest path: full-window screenshot at 1280×720 then read it) → attached as evidence the display shows drawn content; assert non-blank by pixel sample in GDScript via `get_image()` on the screenshot (or by eye in the evidence file).
9. `godot_log errors` → no `ParseError`/logic panics.
10. `godot_game stop`.

Fallback if `logic_display_commands` cannot expose texture pixels: the screenshot + command-queue vector are the acceptance artifacts; the headless `logic_draw` scenario carries exact rendering-independent assertions.

### 7d. Performance budget & measurement

Measurements: `mind-headless logic bench --suite <name> --json` (release, median of 20 runs) and `MindSimHost.logic_bench(name)`. Baseline fixture: 64×64 flat map, 100 processors, no bullets/units unless stated. Counters come from `TickReport` (`logic_instructions_run`, per-set duration) and `std::time::Instant` around `run_once` batches.

| Metric | Budget | Method |
|---|---|---|
| Instruction throughput, mixed stream (`op`/`set`/`jump`/`sensor`/`print`) | ≤ 25 ns/instruction median; ≤ 40 ns P95 | `logic_vm_mixed`: 1M instructions on one privileged executor |
| Arithmetic-only stream | ≤ 10 ns/instruction median | `logic_vm_mixed --pure-arith` |
| World-processor worst case (1000 executed instructions/tick) | ≤ 30 µs/tick, no allocations after warmup | `logic_vm_mixed --world` |
| 100 idle processors (link check + accumulator + 0 instructions) | ≤ 0.2 ms/tick total | `logic_idle_processors` |
| 100 hyper-processors at 25 ipt (125 instr/tick with burst) | ≤ 1.5 ms/tick total | `logic_idle_processors --active 100` |
| Assemble + parse 1000-line program | ≤ 2 ms median | `logic_assemble` |
| Text write/read round-trip one statement | ≤ 1 µs median, ≤ 20 allocations | `logic_assemble --roundtrip 100k` |
| Draw packing | ≤ 40 ns/`DisplayCmd`; `print` ≤ 60 ns/glyph | `logic_draw_pack` |
| Per-processor memory (executor + vars + links, 64 vars, 16 links) | ≤ 16 KiB steady state; no growth over 10 000 ticks | harness allocator counter on `logic_idle_processors` |

Regressions block the P5 gate (HLP §7.4); plan 23 owns the CI hard gate. The global sim budget remains “≤ 4 ms/tick mid-game”; logic’s share is the sum of the rows above for the configured processor count.

### 7e. Exit criteria checklist

- [ ] All 15 `LogicTests` groups ported and green with no Godot/JVM.
- [ ] `all_statements_roundtrip` covers all 53 registered names; `parity/golden_logicio.txt` diff empty (one-off JVM run recorded in the Changelog).
- [ ] `sanitize`/`copy`/privileged-filter/legacy-rewrite/label-limit tests green.
- [ ] Executor semantics tests green: ops, budget clamp, wait/stop, print/format, pack/unpack, read/write, getlink, lookup-null.
- [ ] `logic_link_sensor`, `logic_save_load`, `logic_draw`, `logic_draw_headless`, `logic_budget`, `logic_unit_control_gating`, `logic_privileged_world`, `logic_sync_event`, `logic_globals`, `logic_script_filter` scenarios pass with goldens.
- [ ] MCP §7c passes with a display screenshot and command-queue evidence; `godot_log errors` empty.
- [ ] `LAccess`/building/unit tables match golden spot checks; `sensor`/`control`/`setprop` work through 07/10/11 hooks.
- [ ] `LogicBuild` revision 5 round-trips with waits/vars/links/accumulator/tag/transform; memory/switch/display/canvas revisions round-trip.
- [ ] `logicids.dat` absent-file behavior verified (`lookup` null, no `@<type>Count`).
- [ ] Budgets from §7d measured and recorded; steady-state allocation audit clean.
- [ ] Plan-14 hand-off note (statement metadata + `LCanvas` contract + `MessageState`) delivered; plan-21 event shapes (`LogicSyncEvent`, `ClientLogicDataEvent`) documented in the plan-21 interface.
- [ ] `cargo clippy -p mind-core -- -D warnings` clean; no `unwrap()` on runtime data in sim paths.
- [ ] `cargo tree -p mind-core` shows no `godot`/`tokio`; GPL headers on every ported file.
- [ ] This plan's Changelog updated with evidence paths.

---

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Flag |
|---|---|---|---|
| R1 | Plan 12 (Rules/Teams/Markers) is not on disk; the logic instruction set and blocks depend on many `Rules` fields and `MapMarkers`. | Implement `LogicRulesApi`/`MarkerApi` traits in 13 with upstream defaults on plan 05's minimal `Rules`; at 12 kickoff, replace the trait impls with direct `Rules` access without changing the VM API. `MapMarkers`/`MapObjectives` stub returns empty. | **NEEDS USER DECISION (process)** — whether to wait for 12 or ship the stubs as above; default chosen, work continues. Orchestrator reconcile at 12 kickoff. |
| R2 | `logicids.dat` skip removes `lookup`/`@<type>Count` behavior. | Locked by HLP §9: absent-file behavior; tests assert null/-1. If numeric lookup parity is later wanted, add an optional committed `logicids.toml` name table (append-only) behind a flag. | Locked; follow-up optional |
| R3 | `GlobalVars.rand` must join the sim checksum, which changes `CHECKSUM_VERSION` in plan 05. | Add `RngStream::Logic`; fold its state into `Sim::checksum()` in a version bump coordinated with 05/23. Until 05 ships the stream, use a local stream and mark the checksum entry reserved. | Orchestrator reconcile with 05/23 |
| R4 | `DrawI` print packing needs upstream font metrics (`spaceXadvance`, `lineHeight`, glyph coverage) before plan 16 exists. | `LogicFontMetrics` trait with committed fallback constants captured from the upstream logic font; plan 16 replaces with real atlas metrics. Headless/`capture_draw` tests use the fallback (golden committed from it). | Flagged (parity risk if upstream font changes) |
| R5 | Named-color table (`%[red]`, `@colorRed`) lives in Arc. | Commit `mind_core::math::colors` with the Arc named-color list; plan 16 verifies against the atlas/palette. Missing table skips `@sfx-*`/`@color*` constants (upstream `Core.assets == null` behavior). | Cross-plan note (16/18) |
| R6 | `FlushMessageI` `@wait` blocking depends on client UI timing (nondeterministic across peers). | `MessageState` is sim-visible but client-written; documented as a local client divergence (upstream has the same property). Headless treats messages as absent. | Flagged |
| R7 | `LCanvas` jump curves/statement graph are substantial UI work (14). | 13 ships metadata + text save/load; 14 implements the graph. The contract (`show(code, executor, privileged, modified)`, `saveUI` index semantics) is frozen in §3.3. | Reconcile with 14 |
| R8 | `RadarI`/`UnitLocateI`/`QueryI` rely on `TeamData` trees/queries whose exact iteration order is 11's contract; a mismatch changes who gets targeted first. | Use 11's `QuadTree`/`TeamData` iteration helpers only; add a cross-plan test asserting order with a fixed fixture. | Reconcile with 11 |
| R9 | `ucontrol` payload/item calls are plan 08 host functions; some may not exist yet. | Route through 08's public API; if a call is missing, add a ledger row to 08 rather than duplicating payload logic. | Reconcile with 08 |
| R10 | `setrule`/`setflag`/`setblock`/`spawnwave` become host commands under D2; a peer running without relay could diverge. | Authority flag from 21 (`offline/host = true`, remote client = false); default true. All call sites are already `net.client()`-guarded upstream, so the replacement is mechanical. | Reconcile with 21 |
| R11 | `LogicDisplay` view/render split could leak GPU state into sim. | Command queue, `operations`, transform and linked-display arena are sim state; `FrameBuffer`/`processCommands`/glyph rendering are strictly 16 and never feed back except through `sense`/`operations` reads. | Resolved by design |
| R12 | `LogicScript`'s wall-clock timeout cannot be ported exactly. | Instruction cap only (deviation 6); `LogicFilter`'s explicit cap is upstream-correct and unaffected. | Resolved by design |
| R13 | Upstream field order is inferred from Java declaration order; javac `getEnclosedElements` order is not spec-guaranteed. | `parity/java/DumpLogicIO.java` captures the actual generated order from a real upstream build; the committed `golden_logicio.txt` is the oracle. Until it is generated, use declaration order (observed) and keep the table single-sourced. | Flagged (needs one JVM run, same policy as 02 R1) |
| R14 | Enum ordinal ABI: `LAccess`, `LogicRule`, content `ContentType`, and choice enums are serialized as names in text IO but as ordinals in dumps/checksums. | Text IO uses names (upstream); dumps/checksums use ordinals with the plan's exact order; changes are append-only. | Resolved by design |

---

## 9. References

### Mindustry sources read

- `core/src/mindustry/logic/AGENTS.md`
- `core/src/mindustry/logic/`: `LAssembler.java`, `LParser.java`, `LStatement.java`, `LStatements.java`, `LExecutor.java`, `LVar.java`, `LCategory.java`, `LogicOp.java`, `ConditionOp.java`, `LAccess.java`, `LUnitControl.java`, `GlobalVars.java`, `GlobalVarsDialog.java`, `LogicDialog.java`, `LCanvas.java` (API + save/load), `LogicFx.java`, `LogicRule.java`, `FetchType.java`, `QueryType.java`, `QueryShape.java`, `RadarSort.java`, `RadarTarget.java`, `LLocate.java`, `TileLayer.java`, `MessageType.java`, `CutsceneAction.java`, `LMarkerControl.java`, `Senseable.java`, `Settable.java`, `Controllable.java`, `Ranged.java`, `LReadable.java`, `LWritable.java`, `LPrintable.java`, `LDrawable.java`, `LogicScript.java`
- `core/src/mindustry/world/blocks/logic/`: `LogicBlock.java`, `LogicDisplay.java`, `TileableLogicDisplay.java`, `MemoryBlock.java`, `MessageBlock.java`, `SwitchBlock.java`, `CanvasBlock.java`
- `core/src/mindustry/world/blocks/AGENTS.md`, `world/AGENTS.md`, `annotations/AGENTS.md`, `net/AGENTS.md`, `entities/AGENTS.md`, `maps/AGENTS.md`, `content/AGENTS.md`, `tests/AGENTS.md`, `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`
- `annotations/src/main/java/mindustry/annotations/misc/LogicStatementProcessor.java`, `annotations/src/main/java/mindustry/annotations/util/Stype.java`, `BaseProcessor.java`
- `entities/comp/BuildingComp.java` §2099–2227, `entities/comp/UnitComp.java` §266–427
- `ai/types/LogicAI.java`, `ai/AGENTS.md`
- `maps/filters/LogicFilter.java`
- `content/Blocks.java` §6885–7034 (`//region logic`)
- `game/Rules.java` (logic rules §101–111, `allowLogicData` §251)
- `tests/src/test/java/LogicTests.java`, `tests/src/test/java/ApplicationTests.java` (bootstrap), `tests/AGENTS.md`
- `core/assets/bundles/bundle.properties` (`instruction.*`, `lst.*`, `lenum.*`, `lcategory.*`, `lunitcontrol.*`)

### Project plans read

- `HIGH_LEVEL_PLAN.md` (§0, §2, §3, §4, §5, §6–§9, §10)
- `PRELIMINARY_PLAN.md`
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (spine contracts §3.5, §3.10, harness CLI)
- `02_CONTENT_IMPLEMENTATION_PLAN.md` (§3.4 `ContentRegistry` init hook)
- `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (§3.4 `TypeIO`, §3.5 entity codecs/revisions)
- `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.4 schedule, §3.6 groups, §3.8 time, §6.4 `SimCommand`, §6.5 checksum)
- `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§3.2–3.4 tile ops/events, §3.8 filters/`logic` hook)
- `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (§3.4 `BuildingBehavior`, §3.9 config, §3.10 IO, §6.1 `BlockKindData`, §6.4 dump, §8 R12)
- `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (§3.11 status, §3.14 cross-plan ledger)
- `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (§3.5 controllers, §3.6 kinds, §3.7 pathfinding, §6.3 controller codec, §6.5 `SimCommand`, §8 R8)
- Sibling plans referenced but not on disk at authoring time: `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `14_UI_IMPLEMENTATION_PLAN.md`, `15_INPUT_RTS_IMPLEMENTATION_PLAN.md`, `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md`, `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md`, `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (interfaces written against HIGH_LEVEL_PLAN descriptions and plan 10/11 ledgers)

## Changelog

- 2026-10-01 — Plan authored. Locked: 53 registered statements with exact field order; `VarRef` local/global model; enum-based statements/instructions; absent `logicids.dat`; sim-time `sync`/timeouts; `MessageState` + `LogicFontMetrics` seams; `parity/golden_logicio.txt` oracle. No milestones executed.
- 2026-10-03 — **M0 complete (`lane/13-logic`).** `mind-core::logic` created (Godot-free): `value.rs` (`LVar` exact port: `num`/`numOrNan`/`bool`/`set`/`setlink`/invalid), `parser.rs` (CR/CRLF normalization, 16-token grammar, raw strings + validated `\uXXXX`, comments, `;`, jump labels/max-500, legacy `atan2→angle`/`dst→len`, privileged filter, 1000-line cap), `statement.rs` (all 53 registered statements with exact serialized field order; `read`/`write`/`afterRead`/`sanitize`/`copy`), `assembler.rs` (`var`/`putVar`/`putConst`, decimal/scientific/hex/binary 64-bit wrap, `%rrggbb[aa]`/`%[name]`, unescape), `ops.rs` (37 `LogicOp` + 8 `ConditionOp`, exact `Structs.eq`), `enums.rs`, `access.rs` (77-entry `LAccess` + derived tables + capability traits), `globals.rs` (static constants, control ids, `LAccess`/align constants, Arc named colors), `executor/` (`load`/`runOnce`/budget driver + Set/Op/Select/Jump/End/Stop/Wait/Print/PrintChar/Format/Lookup/Pack/Unpack), `textio.rs`. **All 15 ported `LogicTests` groups pass** (`logic::tests` + module tests, 60 tests) with no Godot/JVM. Evidence: `cargo test -p mind-core` **1121 passed / 2 ignored**; fmt + workspace clippy `-D warnings` clean; `mind-headless` 65 passed. Deferred: `Global` mutable vars (`@time`/`@tick`), full `GlobalVars.init` content/audio constants, M3+ block/unit/world instruction bodies (see below).
- 2026-10-03 — **M1 + M2 partial (`lane/13-logic`).** `fx.rs` (`LogicFx` 33-entry ordered table + flags), `rules.rs` (`LogicRule::apply_to_rules` for the stateless/team fields; state/world/content/color rules return unapplied pending 05/06/12), `script.rs` (`run_logic_script` instruction-cap semantics; `LOGIC_FILTER_MAX_INSTRUCTIONS = 6_250_000`). Headless `mind-headless logic assemble|run|dump` with scenarios `logic_arith`, `logic_strings`, `logic_budget`, `logic_globals` and committed checksum goldens (`13f8b1eccdfff995`, `7458e26c1c008d74`, `2eb20edbd2660fe4`, `21d7c51ee221bb5f`). Evidence: `mind-headless` **65 passed** (incl. `scenario_goldens_match`). Still open: full 53-instruction build/run bodies needing world/units/ECS (M3–M6), per-block executor ownership + links (M3), displays/draw packing (M4), unit control (M5), the complete `GlobalVars.update`/content registration (M7), and the gdext/inspector surface (M8).



- 2026-10-03 — **M3 complete (lane/13-logic).** Logic blocks + persistence. logic/blocks/{mod,io,logic_block,memory,switch}.rs: LogicBlockState/LogicBlockBehavior (links, updateTile fixpoint with rename/dedup/last_build, @this/@thisx/@thisy/@links/@ipt injection, indLinkName, elativeConnections, zlib compress/eadCompressed, revision-5 write/read with vars+waits+accumulator+tag+iconTag, LogicRulesApi/ccessible), MemoryBlockState/MemoryBehavior (sentinel slots, revision 0/1), SwitchBehavior (Boolean config on Building.enabled, revision 1), LogicTimeouts, and the TypeIO variable codec (CellValue↔TypeValue, doubleType fast path, building tile-pack refs). Executor gains 	eam/uild_ipt/links/link_ids and Read/Write/GetLink; Instruction::run + un/un_once/un_budget now take &mut World. **Latent M2 bug fixed:** WaitI.cur_time was lost because instructions were cloned on run; it now mutates the live instruction (wait 1.5 advances Time.delta/60/tick). **Plan-07 integration (13 §3.14/§8 R12):** BlockKindData::{Logic,Memory,Switch} + BlockFamily::Logic + vanilla knobs; BuildingKind::{LOGIC=5, MEMORY=1, MESSAGE=0, SWITCH=1, DISPLAY=1, TILEABLE_DISPLAY=0, CANVAS=0}; default_behavior logic arms. **Verify:** cargo test -p mind-core logic::blocks 9/9 (link write/read, link-name increment, revision-5 round-trip, memory rev0/1, compress); full mind-core 1215 passed/2 ignored; mind-headless 74 passed with new goldens logic_link_sensor ef79a8557eadddb4 and logic_save_load 8f4d87c924126091; the four prior logic_* goldens are unchanged. Re-recorded plan-07 	ests/golden/blocks_families.json (other 181→172, new logic 9). **Deferred:** M4 displays/message/canvas + draw packing; M5 capabilities; M6 privileged world instrs; M7 GlobalVars/script; M8 gdext/inspector/MCP + plan-14 handoff. MCP steps DEFERRED (single-editor mutex).

- 2026-10-03 — **M4 complete (`lane/13-logic`).** Displays, message, canvas + draw packing. `logic/executor/draw.rs` (`DisplayCmd` 4+6x10-bit layout, `pack`/`packSign`/`unpackSign`, `DrawI` colorPack/print/image/scale expansion, `LogicFontMetrics` seam + fallback metrics, command byte constants) and new `Draw`/`DrawFlush`/`PrintFlush` instructions. `logic/blocks/{display,message,canvas}.rs` (`LogicDisplayState` + `LogicDisplays` arena + revision-1 transform; `MessageBlockState` clamp/read/print + revision 0; `CanvasBlockState` bit-packed pixels + revision 0) with `flush_draw`/`flush_print`. Plan-07 `BlockKindData::{Message,Display,TileableDisplay,Canvas}` + `building_kind`/`default_behavior` arms + `blocks_families.json` re-record (`logic` 17 / `other` 164). Align globals switched to numeric to match `DrawI`. **Verify:** `cargo test -p mind-core logic::blocks` 12/12; full `mind-core` 1220 passed / 2 ignored; `mind-headless` 74 passed with new goldens `logic_draw` `82572433be79fab9`, `logic_draw_headless` `89cd31291d2aefa4` (prior `logic_*` goldens unchanged); fmt + workspace clippy `-D warnings` clean. **Deferred:** tileable BFS linking + `commandImage` display-index resolution (16), M5 capabilities, M6 world instrs, M7 globals/script, M8 gdext/MCP.
