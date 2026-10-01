// SPDX-License-Identifier: GPL-3.0-only

//! Scenario files (§6.1) and command-log JSONL (§6.2).
//!
//! Rust-registered fixtures live in `mind-headless`; this module owns the Godot-free
//! schema so `mind-gdext` can `load_scenario` the mirrored files in plan M4.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::command::{Command, CommandRecord, sort_records};
use crate::content::{BlockId, Blocks};
use crate::random::JavaRandom;
use crate::version::SCENARIO_FORMAT;

/// A complete scenario definition (`scenarios/*.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    /// Always `version::SCENARIO_FORMAT`.
    pub format: u32,
    /// Registered scenario name.
    pub name: String,
    /// Deterministic seed.
    pub seed: u64,
    /// World generation parameters.
    pub world: ScenarioWorld,
    /// Rule flags (P0: recorded, not simulated).
    #[serde(default)]
    pub rules: ScenarioRules,
    /// Explicit tick-stamped commands.
    #[serde(default)]
    pub commands: Vec<CommandSpecRecord>,
    /// Optional deterministic command generator (append-only extension; §6.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_generator: Option<CommandGenerator>,
    /// Number of ticks to run.
    pub steps: u64,
    /// Golden expectations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<ScenarioExpect>,
    /// Emit a per-tick checksum list in `run --json` reports (determinism scenarios).
    #[serde(default)]
    pub emit_per_tick: bool,
}

/// World parameters. `generator` variants are append-only (`flat` at P0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioWorld {
    /// Generator name (`flat`).
    pub generator: String,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Floor block name (retained for plan 06; no floor content exists at P0).
    pub floor: String,
    /// Wall block name.
    pub wall: String,
}

/// Rule flags (`ApplicationTests`/`Rules` subset, recorded only at P0).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioRules {
    /// Whether waves spawn.
    #[serde(default)]
    pub waves: bool,
    /// Whether resources are infinite.
    #[serde(default)]
    pub infinite_resources: bool,
}

/// A tick-stamped command in JSON form (block references are names, the parity ABI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpecRecord {
    /// Simulation tick.
    pub tick: u64,
    /// Command op.
    pub op: CommandSpec,
}

/// JSON command op (§6.1/§6.2): `{ "type": "place", "x": 4, "y": 4, "block": "stone-wall" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandSpec {
    /// Place a block.
    Place {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Block name.
        block: String,
    },
    /// Break a block.
    Break {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
    },
    /// Select a block.
    SelectBlock {
        /// Block name.
        block: String,
    },
}

impl CommandSpec {
    /// Resolves names through the content registry.
    pub fn resolve(&self, blocks: &Blocks) -> Result<Command, ScenarioError> {
        match self {
            CommandSpec::Place { x, y, block } => Ok(Command::Place {
                x: *x,
                y: *y,
                block: resolve_block(blocks, block)?,
            }),
            CommandSpec::Break { x, y } => Ok(Command::Break { x: *x, y: *y }),
            CommandSpec::SelectBlock { block } => Ok(Command::SelectBlock {
                block: resolve_block(blocks, block)?,
            }),
        }
    }

    /// Converts a resolved command back to its JSON form.
    pub fn from_command(blocks: &Blocks, command: &Command) -> Result<Self, ScenarioError> {
        match command {
            Command::Place { x, y, block } => Ok(CommandSpec::Place {
                x: *x,
                y: *y,
                block: block_name(blocks, *block)?,
            }),
            Command::Break { x, y } => Ok(CommandSpec::Break { x: *x, y: *y }),
            Command::SelectBlock { block } => Ok(CommandSpec::SelectBlock {
                block: block_name(blocks, *block)?,
            }),
        }
    }
}

/// Deterministic command generator variants (append-only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandGenerator {
    /// Pseudo-random ops derived only from the scenario seed via `JavaRandom` (OD-R14).
    SeededOps {
        /// Number of ops to generate.
        count: usize,
        /// Tick span; each op is stamped in `[0, steps)`.
        steps: u64,
        /// Op mix.
        #[serde(default)]
        mode: OpMode,
    },
    /// Places a block on every tile of a rectangle at tick 0 (bench fixtures).
    Fill {
        /// Left tile.
        x: i16,
        /// Top tile.
        y: i16,
        /// Rectangle width in tiles.
        width: i16,
        /// Rectangle height in tiles.
        height: i16,
    },
}

/// Op mix for [`CommandGenerator::SeededOps`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpMode {
    /// Random place/break mix.
    #[default]
    Mixed,
    /// Only placements.
    PlaceOnly,
}

/// Golden expectations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScenarioExpect {
    /// Final checksum (16 lowercase hex digits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// Sparse tile assertions.
    #[serde(default)]
    pub tiles: Vec<ExpectTile>,
    /// Committed benchmark numbers (§7d).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bench: Option<BenchExpect>,
}

/// One sparse tile assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectTile {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Expected block name.
    pub block: String,
}

/// Committed benchmark baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchExpect {
    /// Median microseconds per tick.
    pub p50_us: u64,
    /// 99th percentile microseconds per tick.
    pub p99_us: u64,
}

/// Errors from scenario/command-log parsing.
#[derive(thiserror::Error, Debug)]
pub enum ScenarioError {
    /// File IO failed.
    #[error("failed to access `{path}`: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// Scenario JSON was invalid.
    #[error("invalid scenario JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// One JSONL line was invalid.
    #[error("invalid command-log line {line}: {source}")]
    JsonLine {
        /// 1-based line number.
        line: usize,
        /// Underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// The scenario format is not supported by this build.
    #[error("unsupported scenario format {found} (expected {})", SCENARIO_FORMAT)]
    UnsupportedFormat {
        /// Format found in the file.
        found: u32,
    },
    /// A content name was unknown.
    #[error("unknown content name `{0}` in scenario/command log")]
    UnknownContent(String),
    /// The world generator is unknown.
    #[error("unknown world generator `{0}`")]
    UnknownGenerator(String),
    /// The world dimensions are invalid.
    #[error("scenario world dimensions must be positive and fit in i16 (got {width}x{height})")]
    InvalidWorldSize {
        /// Requested width.
        width: i32,
        /// Requested height.
        height: i32,
    },
    /// The command generator settings are invalid.
    #[error("scenario command generator `seeded_ops` requires count > 0 and 0 < steps <= i32::MAX")]
    InvalidGenerator,
}

/// Resolves a block name through the registry.
pub fn resolve_block(blocks: &Blocks, name: &str) -> Result<BlockId, ScenarioError> {
    blocks
        .id(name)
        .map_err(|_| ScenarioError::UnknownContent(name.to_owned()))
}

/// Block name for a resolved id.
pub fn block_name(blocks: &Blocks, id: BlockId) -> Result<String, ScenarioError> {
    blocks
        .name(id)
        .map(str::to_owned)
        .map_err(|_| ScenarioError::UnknownContent(format!("block id {}", id.raw())))
}

impl Scenario {
    /// Parses and validates scenario JSON.
    pub fn parse(json: &str) -> Result<Scenario, ScenarioError> {
        let scenario: Scenario = serde_json::from_str(json)?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// Reads and validates a scenario file.
    pub fn read(path: impl AsRef<Path>) -> Result<Scenario, ScenarioError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| ScenarioError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Scenario::parse(&text)
    }

    /// Writes the scenario as pretty JSON.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), ScenarioError> {
        let path = path.as_ref();
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, format!("{text}\n")).map_err(|source| ScenarioError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Validates format/world/generator fields.
    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.format != SCENARIO_FORMAT {
            return Err(ScenarioError::UnsupportedFormat { found: self.format });
        }
        if self.world.width <= 0
            || self.world.height <= 0
            || self.world.width > i16::MAX as i32
            || self.world.height > i16::MAX as i32
        {
            return Err(ScenarioError::InvalidWorldSize {
                width: self.world.width,
                height: self.world.height,
            });
        }
        if let Some(CommandGenerator::SeededOps { count, steps, .. }) = &self.command_generator
            && (*count == 0 || *steps == 0 || i32::try_from(*steps).is_err())
        {
            return Err(ScenarioError::InvalidGenerator);
        }
        if let Some(CommandGenerator::Fill { width, height, .. }) = &self.command_generator
            && (*width <= 0 || *height <= 0)
        {
            return Err(ScenarioError::InvalidGenerator);
        }
        Ok(())
    }

    /// Builds the tick-stamped command list from the explicit `commands` array or
    /// the deterministic generator; always sorted stably by tick.
    pub fn resolve_commands(&self, blocks: &Blocks) -> Result<Vec<CommandRecord>, ScenarioError> {
        let mut records: Vec<CommandRecord> = if let Some(generator) = &self.command_generator {
            self.generate_commands(blocks, generator)?
        } else {
            let mut records = Vec::with_capacity(self.commands.len());
            for record in &self.commands {
                records.push(CommandRecord::new(record.tick, record.op.resolve(blocks)?));
            }
            records
        };
        sort_records(&mut records);
        Ok(records)
    }

    /// Deterministic command generator.
    ///
    /// `seeded_ops` fixed draw order per op: `tick`, `x`, `y`, then (for
    /// [`OpMode::Mixed`] only) one boolean draw choosing place vs break. Nothing
    /// depends on wall-clock or hash iteration order (OD-R14).
    pub fn generate_commands(
        &self,
        blocks: &Blocks,
        generator: &CommandGenerator,
    ) -> Result<Vec<CommandRecord>, ScenarioError> {
        let block = resolve_block(blocks, "stone-wall")?;
        let mut records = match generator {
            CommandGenerator::SeededOps { count, steps, mode } => {
                let steps = i32::try_from(*steps).map_err(|_| ScenarioError::InvalidGenerator)?;
                if *count == 0 || steps == 0 {
                    return Err(ScenarioError::InvalidGenerator);
                }
                let width = self.world.width;
                let height = self.world.height;
                let mut rng = JavaRandom::new(self.seed);
                let mut records = Vec::with_capacity(*count);
                for _ in 0..*count {
                    let tick = rng.next_int_bound(steps) as u64;
                    let x = rng.next_int_bound(width) as i16;
                    let y = rng.next_int_bound(height) as i16;
                    let command = match mode {
                        OpMode::PlaceOnly => Command::Place { x, y, block },
                        OpMode::Mixed => {
                            if rng.next_boolean() {
                                Command::Place { x, y, block }
                            } else {
                                Command::Break { x, y }
                            }
                        }
                    };
                    records.push(CommandRecord::new(tick, command));
                }
                records
            }
            CommandGenerator::Fill {
                x,
                y,
                width,
                height,
            } => {
                if *width <= 0 || *height <= 0 {
                    return Err(ScenarioError::InvalidGenerator);
                }
                let mut records = Vec::with_capacity(*width as usize * *height as usize);
                for dy in 0..*height {
                    for dx in 0..*width {
                        records.push(CommandRecord::new(
                            0,
                            Command::Place {
                                x: x.saturating_add(dx),
                                y: y.saturating_add(dy),
                                block,
                            },
                        ));
                    }
                }
                records
            }
        };
        sort_records(&mut records);
        Ok(records)
    }
}

/// Drives a scenario's commands against a `Sim` tick by tick.
///
/// Commands stamped with tick `t` are applied immediately before the tick `t`
/// executes (§6.1); equal ticks preserve the sorted (stable) file order.
#[derive(Debug, Clone)]
pub struct ScenarioPlayer {
    records: Vec<CommandRecord>,
    next: usize,
    total_steps: u64,
    tick: u64,
}

impl ScenarioPlayer {
    /// Builds a player from a scenario and the content registry.
    pub fn new(scenario: &Scenario, blocks: &Blocks) -> Result<Self, ScenarioError> {
        Ok(Self {
            records: scenario.resolve_commands(blocks)?,
            next: 0,
            total_steps: scenario.steps,
            tick: 0,
        })
    }

    /// Next tick index to execute.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Total configured steps.
    pub fn total_steps(&self) -> u64 {
        self.total_steps
    }

    /// Number of commands already applied.
    pub fn applied(&self) -> usize {
        self.next
    }

    /// Total commands (applied + pending).
    pub fn total_commands(&self) -> usize {
        self.records.len()
    }

    /// Applies all commands stamped for the current tick, then ticks the sim.
    /// Returns `false` once `steps` ticks have run.
    pub fn step(&mut self, sim: &mut crate::sim::Sim) -> Result<bool, crate::sim::SimError> {
        if self.tick >= self.total_steps {
            return Ok(false);
        }
        while self.next < self.records.len() && self.records[self.next].tick <= self.tick {
            sim.apply(self.records[self.next].command)?;
            self.next += 1;
        }
        sim.tick()?;
        self.tick += 1;
        Ok(true)
    }

    /// Runs the scenario to completion.
    pub fn run(&mut self, sim: &mut crate::sim::Sim) -> Result<(), crate::sim::SimError> {
        while self.step(sim)? {}
        Ok(())
    }
}

/// Serializes commands as JSONL (`{"tick":..,"op":{..}}` per line).
pub fn command_log_to_string(
    blocks: &Blocks,
    records: &[CommandRecord],
) -> Result<String, ScenarioError> {
    let mut out = String::new();
    for record in records {
        let spec = CommandSpecRecord {
            tick: record.tick,
            op: CommandSpec::from_command(blocks, &record.command)?,
        };
        out.push_str(&serde_json::to_string(&spec)?);
        out.push('\n');
    }
    Ok(out)
}

/// Parses JSONL commands; blank lines are skipped. Resolution uses `blocks`.
pub fn parse_command_log(blocks: &Blocks, text: &str) -> Result<Vec<CommandRecord>, ScenarioError> {
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let spec: CommandSpecRecord =
            serde_json::from_str(line).map_err(|source| ScenarioError::JsonLine {
                line: index + 1,
                source,
            })?;
        records.push(CommandRecord::new(spec.tick, spec.op.resolve(blocks)?));
    }
    Ok(records)
}

/// Reads a `*.jsonl` command log.
pub fn read_command_log(
    blocks: &Blocks,
    path: impl AsRef<Path>,
) -> Result<Vec<CommandRecord>, ScenarioError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| ScenarioError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_command_log(blocks, &text)
}

/// Writes a `*.jsonl` command log.
pub fn write_command_log(
    blocks: &Blocks,
    path: impl AsRef<Path>,
    records: &[CommandRecord],
) -> Result<(), ScenarioError> {
    let path = path.as_ref();
    let text = command_log_to_string(blocks, records)?;
    std::fs::write(path, text).map_err(|source| ScenarioError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACE_BREAK: &str = r#"{
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
    }"#;

    #[test]
    fn parses_place_break_example() {
        let scenario = Scenario::parse(PLACE_BREAK).unwrap();
        assert_eq!(scenario.format, 1);
        assert_eq!(scenario.seed, 1);
        assert_eq!((scenario.world.width, scenario.world.height), (32, 32));
        assert_eq!(scenario.world.floor, "stone");
        assert_eq!(scenario.steps, 60);

        let blocks = Blocks::new();
        let records = scenario.resolve_commands(&blocks).unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].tick, 0);
        assert_eq!(
            records[1].command,
            Command::Place {
                x: 4,
                y: 4,
                block: BlockId::STONE_WALL
            }
        );
        assert_eq!(records[3].command, Command::Break { x: 4, y: 4 });

        // Round-trips through serde.
        let json = serde_json::to_string(&scenario).unwrap();
        let reparsed = Scenario::parse(&json).unwrap();
        assert_eq!(scenario, reparsed);
    }

    #[test]
    fn generated_commands_are_seed_stable() {
        let scenario = Scenario::parse(
            r#"{
              "format": 1,
              "name": "unit",
              "seed": 2,
              "world": { "generator": "flat", "width": 64, "height": 64, "floor": "stone", "wall": "air" },
              "commands": [],
              "command_generator": { "type": "seeded_ops", "count": 200, "steps": 600 },
              "steps": 600
            }"#,
        )
        .unwrap();
        let blocks = Blocks::new();
        let first = scenario.resolve_commands(&blocks).unwrap();
        let second = scenario.resolve_commands(&blocks).unwrap();
        assert_eq!(first.len(), 200);
        assert_eq!(first, second);
        assert!(first.iter().all(|record| record.tick < 600));
        // Stable tick sort.
        let ticks: Vec<u64> = first.iter().map(|record| record.tick).collect();
        let mut sorted = ticks.clone();
        sorted.sort_unstable();
        assert_eq!(ticks, sorted);
    }

    #[test]
    fn command_log_roundtrip() {
        let blocks = Blocks::new();
        let records = vec![
            CommandRecord::new(
                0,
                Command::SelectBlock {
                    block: BlockId::STONE_WALL,
                },
            ),
            CommandRecord::new(
                3,
                Command::Place {
                    x: 1,
                    y: 2,
                    block: BlockId::STONE_WALL,
                },
            ),
            CommandRecord::new(9, Command::Break { x: 1, y: 2 }),
        ];
        let text = command_log_to_string(&blocks, &records).unwrap();
        assert_eq!(text.lines().count(), 3);
        let parsed = parse_command_log(&blocks, &text).unwrap();
        assert_eq!(parsed, records);
    }

    #[test]
    fn rejects_bad_format_and_unknown_names() {
        let bad = Scenario::parse(
            r#"{ "format": 2, "name": "x", "seed": 0, "world": { "generator": "flat", "width": 1, "height": 1, "floor": "stone", "wall": "air" }, "steps": 1 }"#,
        );
        assert!(matches!(
            bad,
            Err(ScenarioError::UnsupportedFormat { found: 2 })
        ));

        let unknown = Scenario::parse(
            r#"{
              "format": 1, "name": "x", "seed": 0,
              "world": { "generator": "flat", "width": 4, "height": 4, "floor": "stone", "wall": "air" },
              "commands": [ { "tick": 0, "op": { "type": "place", "x": 0, "y": 0, "block": "nope" } } ],
              "steps": 1
            }"#,
        )
        .unwrap();
        let blocks = Blocks::new();
        assert!(matches!(
            unknown.resolve_commands(&blocks),
            Err(ScenarioError::UnknownContent(name)) if name == "nope"
        ));
    }
}
