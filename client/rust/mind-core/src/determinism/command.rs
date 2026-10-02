// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SimCommand` / `CommandLog` — the transport-free command interface
//! (plan 05 §6.4).
//!
//! Plan 21 turns STDB `command_event` rows into `SimCommand`s in relay order;
//! plan 23 replays `.simlog` files. The text form is JSON Lines: one header line
//! then one command per line.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::command::Command as P0Command;
use crate::content::BlockId;

/// A configuration value payload (`@Configure` intent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigValue {
    /// No value.
    None,
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i32),
    /// Float.
    Float(f32),
    /// Content reference by name (parity ABI).
    Content(String),
    /// Raw byte payload (mod extension).
    Bytes(Vec<u8>),
}

/// A single simulation command (plan 05 §6.4).
///
/// `SetRules` carries a JSON blob until plan 12's typed `Rules` lands
/// (documented deferral); plan 21/12 replace it in place without changing the
/// other variants.
#[derive(Debug, Clone, PartialEq)]
pub enum SimCommand {
    /// Place a block.
    Place {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Block content id.
        block: u16,
        /// Rotation.
        rotation: i8,
        /// Team id.
        team: u8,
        /// Issuing player, when known.
        player: Option<i32>,
    },
    /// Break a block.
    Break {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Issuing player, when known.
        player: Option<i32>,
    },
    /// Configure a building.
    Configure {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Configuration value.
        value: ConfigValue,
    },
    /// Issue a unit command.
    UnitCommand {
        /// Unit entity ids.
        units: SmallVec<[i32; 32]>,
        /// Command content id.
        command: u16,
        /// Target x.
        x: f32,
        /// Target y.
        y: f32,
    },
    /// Replace the match rules (plan 12 owns the typed payload).
    SetRules {
        /// Serialized rules JSON.
        json: String,
    },
    /// Spawn a unit (server/harness only).
    SpawnUnit {
        /// Unit content id.
        unit: u16,
        /// X.
        x: f32,
        /// Y.
        y: f32,
        /// Team id.
        team: u8,
    },
    /// Mod/data extension.
    Custom {
        /// Extension kind.
        kind: u16,
        /// Extension data.
        data: SmallVec<[u8; 32]>,
    },
}

impl SimCommand {
    /// Parity op name.
    pub const fn op_name(&self) -> &'static str {
        match self {
            SimCommand::Place { .. } => "place",
            SimCommand::Break { .. } => "break",
            SimCommand::Configure { .. } => "configure",
            SimCommand::UnitCommand { .. } => "unit_command",
            SimCommand::SetRules { .. } => "set_rules",
            SimCommand::SpawnUnit { .. } => "spawn_unit",
            SimCommand::Custom { .. } => "custom",
        }
    }

    /// Converts the P0 subset commands into `SimCommand`s.
    pub fn from_p0(command: P0Command) -> Self {
        match command {
            P0Command::Place { x, y, block } => SimCommand::Place {
                x,
                y,
                block: block.raw(),
                rotation: 0,
                team: 0,
                player: None,
            },
            P0Command::Break { x, y } => SimCommand::Break { x, y, player: None },
            P0Command::SelectBlock { block } => SimCommand::Configure {
                x: 0,
                y: 0,
                value: ConfigValue::Content(format!("{}", block.raw())),
            },
        }
    }

    /// Converts back to the P0 subset when representable.
    pub fn to_p0(&self) -> Option<P0Command> {
        match self {
            SimCommand::Place { x, y, block, .. } => Some(P0Command::Place {
                x: *x,
                y: *y,
                block: BlockId::new(*block),
            }),
            SimCommand::Break { x, y, .. } => Some(P0Command::Break { x: *x, y: *y }),
            _ => None,
        }
    }
}

/// Log header (plan 05 §6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogHeader {
    /// Format version.
    pub format: u32,
    /// Master seed.
    pub seed: u64,
    /// Map name.
    pub map: String,
    /// Map content hash.
    pub map_hash: u64,
    /// Build tag.
    pub build: String,
    /// Content registry hash.
    pub content_hash: u64,
}

impl LogHeader {
    /// Current `.simlog` format version.
    pub const FORMAT: u32 = 1;

    /// Creates a header for a build/seed/map.
    pub fn new(seed: u64, map: impl Into<String>) -> Self {
        Self {
            format: Self::FORMAT,
            seed,
            map: map.into(),
            map_hash: 0,
            build: crate::version::MIND_VERSION.to_owned(),
            content_hash: 0,
        }
    }
}

/// The recorded command stream.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandLog {
    /// Header.
    pub header: LogHeader,
    /// Tick-stamped commands in application order.
    pub entries: Vec<(u64, SimCommand)>,
}

impl CommandLog {
    /// Creates an empty log.
    pub fn new(header: LogHeader) -> Self {
        Self {
            header,
            entries: Vec::new(),
        }
    }

    /// Appends a command at `tick`.
    pub fn push(&mut self, tick: u64, command: SimCommand) {
        self.entries.push((tick, command));
    }

    /// Number of commands.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log has no commands.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Commands for exactly `tick`, in insertion order.
    pub fn commands_at(&self, tick: u64) -> impl Iterator<Item = &SimCommand> {
        self.entries
            .iter()
            .filter(move |(entry_tick, _)| *entry_tick == tick)
            .map(|(_, command)| command)
    }
}

/// `.simlog` binary magic (`MGSM`). Text logs are JSON Lines; this is the
/// compact form consumed by plan 21/23 (plan 05 §6.4).
pub const SIMLOG_MAGIC: [u8; 4] = *b"MGSM";
/// Binary `.simlog` format version (append-only; never renumber).
pub const SIMLOG_FORMAT: u32 = 1;

/// Errors from decoding a binary `.simlog`.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CommandLogError {
    /// The stream does not start with [`SIMLOG_MAGIC`].
    #[error("not a `.simlog` (bad magic)")]
    BadMagic,
    /// The binary format version is not supported by this build.
    #[error("unsupported `.simlog` format {found} (expected {SIMLOG_FORMAT})")]
    UnsupportedFormat {
        /// Version found in the stream.
        found: u32,
    },
    /// The stream ended before a complete value was read.
    #[error("truncated `.simlog`")]
    Truncated,
    /// A length-prefixed UTF-8 string was not valid UTF-8.
    #[error("invalid UTF-8 string in `.simlog`")]
    InvalidString,
    /// An unknown command op tag was encountered.
    #[error("unknown `.simlog` op tag {0}")]
    UnknownOp(u8),
    /// A string/array length exceeded the decoder's sanity cap.
    #[error("`.simlog` length {0} exceeds the cap {1}")]
    LengthOverflow(u32, u32),
}

/// Decoder sanity caps (untrusted input; mirrors plan 04's safe-read policy).
const MAX_SIMLOG_BYTES: u32 = 64 * 1024 * 1024;
const MAX_SIMLOG_STRING: u32 = 1 << 20;
const MAX_SIMLOG_UNITS: u32 = 100_000;

impl CommandLog {
    /// Encodes the log in the compact binary form (`MGSM` + version + header +
    /// tick-stamped commands). Deterministic: fields are written in a fixed
    /// order with little-endian integers.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.entries.len() * 24);
        out.extend_from_slice(&SIMLOG_MAGIC);
        put_u32(&mut out, SIMLOG_FORMAT);
        put_u64(&mut out, self.header.seed);
        put_str(&mut out, &self.header.map);
        put_u64(&mut out, self.header.map_hash);
        put_str(&mut out, &self.header.build);
        put_u64(&mut out, self.header.content_hash);
        put_u32(&mut out, self.entries.len() as u32);
        for (tick, command) in &self.entries {
            put_u64(&mut out, *tick);
            encode_command(&mut out, command);
        }
        out
    }

    /// Whether `bytes` starts with the `.simlog` magic.
    pub fn is_binary(bytes: &[u8]) -> bool {
        bytes.starts_with(&SIMLOG_MAGIC)
    }

    /// Decodes a binary `.simlog` written by [`CommandLog::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CommandLogError> {
        if !Self::is_binary(bytes) {
            return Err(CommandLogError::BadMagic);
        }
        let mut cur = Cursor {
            bytes,
            pos: SIMLOG_MAGIC.len(),
        };
        let format = cur.u32()?;
        if format != SIMLOG_FORMAT {
            return Err(CommandLogError::UnsupportedFormat { found: format });
        }
        let seed = cur.u64()?;
        let map = cur.string()?;
        let map_hash = cur.u64()?;
        let build = cur.string()?;
        let content_hash = cur.u64()?;
        let count = cur.u32()?;
        if count > MAX_SIMLOG_UNITS {
            return Err(CommandLogError::LengthOverflow(count, MAX_SIMLOG_UNITS));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let tick = cur.u64()?;
            let command = decode_command(&mut cur)?;
            entries.push((tick, command));
        }
        Ok(Self {
            header: LogHeader {
                format: LogHeader::FORMAT,
                seed,
                map,
                map_hash,
                build,
                content_hash,
            },
            entries,
        })
    }
}

fn put_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}
fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_bits().to_le_bytes());
}
fn put_str(out: &mut Vec<u8>, value: &str) {
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}
fn put_bytes(out: &mut Vec<u8>, value: &[u8]) {
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value);
}

fn encode_command(out: &mut Vec<u8>, command: &SimCommand) {
    match command {
        SimCommand::Place {
            x,
            y,
            block,
            rotation,
            team,
            player,
        } => {
            put_u8(out, 0);
            put_i16(out, *x);
            put_i16(out, *y);
            put_u16(out, *block);
            put_u8(out, *rotation as u8);
            put_u8(out, *team);
            match player {
                Some(id) => {
                    put_u8(out, 1);
                    put_i32(out, *id);
                }
                None => put_u8(out, 0),
            }
        }
        SimCommand::Break { x, y, player } => {
            put_u8(out, 1);
            put_i16(out, *x);
            put_i16(out, *y);
            match player {
                Some(id) => {
                    put_u8(out, 1);
                    put_i32(out, *id);
                }
                None => put_u8(out, 0),
            }
        }
        SimCommand::Configure { x, y, value } => {
            put_u8(out, 2);
            put_i16(out, *x);
            put_i16(out, *y);
            encode_config(out, value);
        }
        SimCommand::UnitCommand {
            units,
            command,
            x,
            y,
        } => {
            put_u8(out, 3);
            put_u32(out, units.len() as u32);
            for id in units {
                put_i32(out, *id);
            }
            put_u16(out, *command);
            put_f32(out, *x);
            put_f32(out, *y);
        }
        SimCommand::SetRules { json } => {
            put_u8(out, 4);
            put_str(out, json);
        }
        SimCommand::SpawnUnit { unit, x, y, team } => {
            put_u8(out, 5);
            put_u16(out, *unit);
            put_f32(out, *x);
            put_f32(out, *y);
            put_u8(out, *team);
        }
        SimCommand::Custom { kind, data } => {
            put_u8(out, 6);
            put_u16(out, *kind);
            put_bytes(out, data);
        }
    }
}

fn encode_config(out: &mut Vec<u8>, value: &ConfigValue) {
    match value {
        ConfigValue::None => put_u8(out, 0),
        ConfigValue::Bool(value) => {
            put_u8(out, 1);
            put_u8(out, u8::from(*value));
        }
        ConfigValue::Int(value) => {
            put_u8(out, 2);
            put_i32(out, *value);
        }
        ConfigValue::Float(value) => {
            put_u8(out, 3);
            put_f32(out, *value);
        }
        ConfigValue::Content(value) => {
            put_u8(out, 4);
            put_str(out, value);
        }
        ConfigValue::Bytes(value) => {
            put_u8(out, 5);
            put_bytes(out, value);
        }
    }
}

/// Bounds-checked little-endian reader.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], CommandLogError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(CommandLogError::Truncated)?;
        if end > self.bytes.len() {
            return Err(CommandLogError::Truncated);
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, CommandLogError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, CommandLogError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().unwrap_or([0; 2]),
        ))
    }
    fn i16(&mut self) -> Result<i16, CommandLogError> {
        Ok(i16::from_le_bytes(
            self.take(2)?.try_into().unwrap_or([0; 2]),
        ))
    }
    fn u32(&mut self) -> Result<u32, CommandLogError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or([0; 4]),
        ))
    }
    fn i32(&mut self) -> Result<i32, CommandLogError> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or([0; 4]),
        ))
    }
    fn u64(&mut self) -> Result<u64, CommandLogError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().unwrap_or([0; 8]),
        ))
    }
    fn f32(&mut self) -> Result<f32, CommandLogError> {
        Ok(f32::from_bits(u32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or([0; 4]),
        )))
    }
    fn string(&mut self) -> Result<String, CommandLogError> {
        let len = self.u32()?;
        if len > MAX_SIMLOG_STRING {
            return Err(CommandLogError::LengthOverflow(len, MAX_SIMLOG_STRING));
        }
        let bytes = self.take(len as usize)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| CommandLogError::InvalidString)
    }
    fn bytes(&mut self) -> Result<Vec<u8>, CommandLogError> {
        let len = self.u32()?;
        if len > MAX_SIMLOG_BYTES {
            return Err(CommandLogError::LengthOverflow(len, MAX_SIMLOG_BYTES));
        }
        Ok(self.take(len as usize)?.to_vec())
    }
    fn unit_list(&mut self) -> Result<SmallVec<[i32; 32]>, CommandLogError> {
        let count = self.u32()?;
        if count > MAX_SIMLOG_UNITS {
            return Err(CommandLogError::LengthOverflow(count, MAX_SIMLOG_UNITS));
        }
        let mut units = SmallVec::new();
        for _ in 0..count {
            units.push(self.i32()?);
        }
        Ok(units)
    }
}

fn decode_command(cur: &mut Cursor<'_>) -> Result<SimCommand, CommandLogError> {
    match cur.u8()? {
        0 => {
            let x = cur.i16()?;
            let y = cur.i16()?;
            let block = cur.u16()?;
            let rotation = cur.u8()? as i8;
            let team = cur.u8()?;
            let player = read_optional_player(cur)?;
            Ok(SimCommand::Place {
                x,
                y,
                block,
                rotation,
                team,
                player,
            })
        }
        1 => {
            let x = cur.i16()?;
            let y = cur.i16()?;
            let player = read_optional_player(cur)?;
            Ok(SimCommand::Break { x, y, player })
        }
        2 => {
            let x = cur.i16()?;
            let y = cur.i16()?;
            let value = decode_config(cur)?;
            Ok(SimCommand::Configure { x, y, value })
        }
        3 => {
            let units = cur.unit_list()?;
            let command = cur.u16()?;
            let x = cur.f32()?;
            let y = cur.f32()?;
            Ok(SimCommand::UnitCommand {
                units,
                command,
                x,
                y,
            })
        }
        4 => Ok(SimCommand::SetRules {
            json: cur.string()?,
        }),
        5 => {
            let unit = cur.u16()?;
            let x = cur.f32()?;
            let y = cur.f32()?;
            let team = cur.u8()?;
            Ok(SimCommand::SpawnUnit { unit, x, y, team })
        }
        6 => {
            let kind = cur.u16()?;
            let data = cur.bytes()?;
            Ok(SimCommand::Custom {
                kind,
                data: SmallVec::from_vec(data),
            })
        }
        other => Err(CommandLogError::UnknownOp(other)),
    }
}

fn read_optional_player(cur: &mut Cursor<'_>) -> Result<Option<i32>, CommandLogError> {
    match cur.u8()? {
        0 => Ok(None),
        _ => Ok(Some(cur.i32()?)),
    }
}

fn decode_config(cur: &mut Cursor<'_>) -> Result<ConfigValue, CommandLogError> {
    match cur.u8()? {
        0 => Ok(ConfigValue::None),
        1 => Ok(ConfigValue::Bool(cur.u8()? != 0)),
        2 => Ok(ConfigValue::Int(cur.i32()?)),
        3 => Ok(ConfigValue::Float(cur.f32()?)),
        4 => Ok(ConfigValue::Content(cur.string()?)),
        5 => Ok(ConfigValue::Bytes(cur.bytes()?)),
        other => Err(CommandLogError::UnknownOp(other)),
    }
}

/// Command application errors (structured; never panics).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// The command is not representable in this build.
    #[error("unsupported command `{0}`")]
    Unsupported(&'static str),
    /// The command referenced an unknown block/content id.
    #[error("unknown content id {0}")]
    UnknownContent(u16),
    /// The command target was invalid.
    #[error("invalid command target")]
    InvalidTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p0_roundtrip_for_place_and_break() {
        let place = SimCommand::from_p0(P0Command::Place {
            x: 3,
            y: 4,
            block: BlockId::STONE_WALL,
        });
        assert_eq!(place.op_name(), "place");
        assert_eq!(
            place.to_p0(),
            Some(P0Command::Place {
                x: 3,
                y: 4,
                block: BlockId::STONE_WALL
            })
        );
        let brk = SimCommand::from_p0(P0Command::Break { x: 1, y: 2 });
        assert_eq!(brk.to_p0(), Some(P0Command::Break { x: 1, y: 2 }));
        assert!(
            SimCommand::SetRules {
                json: "{}".to_owned()
            }
            .to_p0()
            .is_none()
        );
    }

    #[test]
    fn command_log_orders_by_tick() {
        let mut log = CommandLog::new(LogHeader::new(7, "flat"));
        log.push(
            2,
            SimCommand::Break {
                x: 0,
                y: 0,
                player: None,
            },
        );
        log.push(
            1,
            SimCommand::Break {
                x: 1,
                y: 1,
                player: None,
            },
        );
        log.push(
            2,
            SimCommand::Break {
                x: 2,
                y: 2,
                player: None,
            },
        );
        assert_eq!(log.len(), 3);
        let at_two: Vec<&SimCommand> = log.commands_at(2).collect();
        assert_eq!(at_two.len(), 2);
        assert_eq!(log.header.build, crate::version::MIND_VERSION);
    }

    fn full_log() -> CommandLog {
        let mut log = CommandLog::new(LogHeader::new(0xdead_beef, "flat-64"));
        log.header.map_hash = 0x1122_3344_5566_7788;
        log.header.content_hash = 0x99aa_bbcc_ddee_ff00;
        log.push(
            0,
            SimCommand::Place {
                x: -3,
                y: 7,
                block: 447,
                rotation: 2,
                team: 3,
                player: Some(42),
            },
        );
        log.push(
            1,
            SimCommand::Break {
                x: 1,
                y: -1,
                player: None,
            },
        );
        log.push(
            2,
            SimCommand::Configure {
                x: 4,
                y: 4,
                value: ConfigValue::Content("router".to_owned()),
            },
        );
        log.push(
            3,
            SimCommand::Configure {
                x: 5,
                y: 5,
                value: ConfigValue::Bytes(vec![1, 2, 3, 254]),
            },
        );
        log.push(
            4,
            SimCommand::UnitCommand {
                units: smallvec::smallvec![-1, 2, 3],
                command: 9,
                x: 1.5,
                y: -2.25,
            },
        );
        log.push(
            5,
            SimCommand::SetRules {
                json: "{\"waves\":true}".to_owned(),
            },
        );
        log.push(
            6,
            SimCommand::SpawnUnit {
                unit: 12,
                x: 0.5,
                y: 0.25,
                team: 2,
            },
        );
        log.push(
            7,
            SimCommand::Custom {
                kind: 77,
                data: smallvec::smallvec![9, 8, 7],
            },
        );
        log.push(
            8,
            SimCommand::Configure {
                x: 0,
                y: 0,
                value: ConfigValue::Float(1.25),
            },
        );
        log.push(
            9,
            SimCommand::Configure {
                x: 0,
                y: 0,
                value: ConfigValue::Bool(true),
            },
        );
        log.push(
            10,
            SimCommand::Configure {
                x: 0,
                y: 0,
                value: ConfigValue::Int(-7),
            },
        );
        log.push(
            11,
            SimCommand::Configure {
                x: 0,
                y: 0,
                value: ConfigValue::None,
            },
        );
        log
    }

    #[test]
    fn binary_simlog_roundtrip_all_ops() {
        let log = full_log();
        let bytes = log.to_bytes();
        assert!(CommandLog::is_binary(&bytes));
        let decoded = CommandLog::from_bytes(&bytes).expect("decode");
        assert_eq!(decoded, log);
    }

    #[test]
    fn binary_simlog_is_deterministic_and_rejects_bad_input() {
        let log = full_log();
        assert_eq!(log.to_bytes(), log.to_bytes());
        assert_eq!(
            CommandLog::from_bytes(b"not-a-simlog").unwrap_err(),
            CommandLogError::BadMagic
        );
        // Truncated stream.
        let mut bytes = log.to_bytes();
        bytes.truncate(bytes.len() - 1);
        assert_eq!(
            CommandLog::from_bytes(&bytes).unwrap_err(),
            CommandLogError::Truncated
        );
        // Wrong format version.
        let mut bytes = log.to_bytes();
        bytes[4] = 9;
        assert_eq!(
            CommandLog::from_bytes(&bytes).unwrap_err(),
            CommandLogError::UnsupportedFormat { found: 9 }
        );
    }
}
