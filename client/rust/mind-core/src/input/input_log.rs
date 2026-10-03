// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic input-event log (`format:1`, plan 15 §6.2).
//!
//! The JSONL form is the headless replay oracle: events are applied in order
//! before the sim tick with the same number; emitted commands enter the command
//! log (plan 05/23). This substitutes for Mindustry's missing `mindustry.input`
//! unit tests (§7a) and is also the MCP regression format.

use serde::{Deserialize, Serialize};

use super::focus::FocusState;

/// Format version of the input log.
pub const INPUT_LOG_FORMAT: u32 = 1;

/// A raw, platform-neutral input event (Godot/MCP events are translated to this).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum RawEvent {
    /// Key pressed (`code` is a `KeyCode` name, e.g. `a`, `shiftLeft`).
    KeyDown {
        /// Key-code name.
        code: String,
    },
    /// Key released.
    KeyUp {
        /// Key-code name.
        code: String,
    },
    /// Pointer moved (screen pixels).
    MouseMove {
        /// X.
        x: f32,
        /// Y.
        y: f32,
    },
    /// Mouse button pressed/released.
    MouseButton {
        /// Button name (`left`/`right`/`middle`/`wheel`/...).
        button: String,
        /// Down (`true`) or up (`false`).
        down: bool,
        /// X at the event (optional in the wire format).
        #[serde(default)]
        x: f32,
        /// Y at the event (optional in the wire format).
        #[serde(default)]
        y: f32,
    },
    /// Scroll wheel delta.
    Scroll {
        /// Horizontal delta.
        x: f32,
        /// Vertical delta.
        y: f32,
    },
    /// Touch began.
    TouchDown {
        /// Pointer index.
        pointer: i32,
        /// X.
        x: f32,
        /// Y.
        y: f32,
    },
    /// Touch ended.
    TouchUp {
        /// Pointer index.
        pointer: i32,
        /// X.
        x: f32,
        /// Y.
        y: f32,
    },
    /// Touch moved.
    TouchMove {
        /// Pointer index.
        pointer: i32,
        /// X.
        x: f32,
        /// Y.
        y: f32,
    },
    /// Magnify/pinch gesture.
    Magnify {
        /// Scale factor.
        factor: f32,
    },
    /// Viewport resized.
    Resize {
        /// Width in pixels.
        width: i32,
        /// Height in pixels.
        height: i32,
    },
    /// UI focus flags changed (plan 14 push).
    UiFocus(FocusState),
    /// Raw semantic action for UI-driven events (`select_block`, `rotate`, ...).
    Action {
        /// Action name.
        action: String,
        /// Optional numeric argument (delta, rotation, ...).
        #[serde(default)]
        value: f32,
    },
}

impl RawEvent {
    /// Convenience constructor for a key-down event.
    pub fn key_down(code: impl Into<String>) -> Self {
        RawEvent::KeyDown { code: code.into() }
    }

    /// Convenience constructor for a key-up event.
    pub fn key_up(code: impl Into<String>) -> Self {
        RawEvent::KeyUp { code: code.into() }
    }

    /// Whether this event is a key edge for `code`.
    pub fn key(&self, code: &str) -> Option<bool> {
        match self {
            RawEvent::KeyDown { code: name } if name == code => Some(true),
            RawEvent::KeyUp { code: name } if name == code => Some(false),
            _ => None,
        }
    }
}

/// Header of an input log (`format:1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputHeader {
    /// Build tag.
    pub build: String,
    /// Map name.
    pub map: String,
    /// Sim seed.
    pub seed: u64,
    /// Whether the replay targets the mobile handler.
    pub mobile: bool,
    /// Viewport width.
    pub width: i32,
    /// Viewport height.
    pub height: i32,
}

impl Default for InputHeader {
    fn default() -> Self {
        Self {
            build: crate::version::MIND_VERSION.to_owned(),
            map: "flat".to_owned(),
            seed: 0,
            mobile: false,
            width: 1920,
            height: 1080,
        }
    }
}

/// A tick-stamped input event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputRecord {
    /// Absolute sim tick the event is applied before.
    pub tick: u64,
    /// The raw event.
    pub ev: RawEvent,
}

/// The first JSONL line envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct HeaderEnvelope {
    format: u32,
    header: InputHeader,
}

/// An ordered, tick-stamped input-event log.
#[derive(Debug, Clone, PartialEq)]
pub struct InputLog {
    /// Header.
    pub header: InputHeader,
    /// Events in application order.
    pub records: Vec<InputRecord>,
}

impl InputLog {
    /// Empty log with the given header.
    pub fn new(header: InputHeader) -> Self {
        Self {
            header,
            records: Vec::new(),
        }
    }

    /// Appends an event at `tick`.
    pub fn push(&mut self, tick: u64, ev: RawEvent) {
        self.records.push(InputRecord { tick, ev });
    }

    /// Number of events.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the log has no events.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Serializes to the `format:1` JSONL text.
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        if let Ok(line) = serde_json::to_string(&HeaderEnvelope {
            format: INPUT_LOG_FORMAT,
            header: self.header.clone(),
        }) {
            out.push_str(&line);
            out.push('\n');
        }
        for record in &self.records {
            if let Ok(line) = serde_json::to_string(record) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Parses `format:1` JSONL; unknown future versions are rejected.
    pub fn from_jsonl(text: &str) -> Result<Self, InputLogError> {
        let mut lines = text.lines().filter(|line| !line.trim().is_empty());
        let first = lines.next().ok_or(InputLogError::Empty)?;
        let envelope: HeaderEnvelope = serde_json::from_str(first).map_err(InputLogError::Json)?;
        if envelope.format != INPUT_LOG_FORMAT {
            return Err(InputLogError::UnsupportedFormat {
                found: envelope.format,
            });
        }
        let mut records = Vec::new();
        for line in lines {
            let record: InputRecord = serde_json::from_str(line).map_err(InputLogError::Json)?;
            records.push(record);
        }
        Ok(Self {
            header: envelope.header,
            records,
        })
    }
}

/// Errors decoding an input log.
#[derive(Debug, thiserror::Error)]
pub enum InputLogError {
    /// No header line.
    #[error("empty input log")]
    Empty,
    /// The format version is not supported.
    #[error("unsupported input log format {found} (expected 1)")]
    UnsupportedFormat {
        /// Version found.
        found: u32,
    },
    /// A line failed to parse.
    #[error("invalid input log JSON: {0}")]
    Json(serde_json::Error),
}

/// Deterministic replay cursor over an [`InputLog`].
#[derive(Debug, Clone)]
pub struct InputReplay {
    /// The source log.
    pub log: InputLog,
    /// Record index of the next event to apply.
    next: usize,
}

impl InputReplay {
    /// Wraps a log.
    pub fn new(log: InputLog) -> Self {
        Self { log, next: 0 }
    }

    /// Total events.
    pub fn len(&self) -> usize {
        self.log.records.len()
    }

    /// Whether every event has been consumed.
    pub fn is_empty(&self) -> bool {
        self.next >= self.log.records.len()
    }

    /// Drains every event stamped exactly `tick`, in order.
    pub fn drain_tick(&mut self, tick: u64) -> impl Iterator<Item = RawEvent> + '_ {
        let mut out = Vec::new();
        while self.next < self.log.records.len() && self.log.records[self.next].tick == tick {
            out.push(self.log.records[self.next].ev.clone());
            self.next += 1;
        }
        out.into_iter()
    }

    /// The last stamped tick (or 0).
    pub fn last_tick(&self) -> u64 {
        self.log
            .records
            .last()
            .map(|record| record.tick)
            .unwrap_or(0)
    }

    /// Resets the cursor.
    pub fn reset(&mut self) {
        self.next = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_round_trips_through_jsonl() {
        let mut log = InputLog::new(InputHeader {
            map: "flat-64".to_owned(),
            seed: 123,
            ..InputHeader::default()
        });
        log.push(
            7,
            RawEvent::KeyDown {
                code: "a".to_owned(),
            },
        );
        log.push(7, RawEvent::MouseMove { x: 640.0, y: 360.0 });
        log.push(
            51,
            RawEvent::MouseButton {
                button: "left".to_owned(),
                down: true,
                x: 10.0,
                y: 20.0,
            },
        );
        log.push(80, RawEvent::Scroll { x: 0.0, y: 1.0 });
        log.push(
            95,
            RawEvent::TouchDown {
                pointer: 0,
                x: 100.0,
                y: 200.0,
            },
        );
        log.push(120, RawEvent::Magnify { factor: 1.1 });

        let text = log.to_jsonl();
        assert!(text.starts_with("{\"format\":1,"));
        let decoded = InputLog::from_jsonl(&text).expect("decode");
        assert_eq!(decoded, log);
    }

    #[test]
    fn replay_drains_by_tick() {
        let mut log = InputLog::new(InputHeader::default());
        log.push(1, RawEvent::key_down("a"));
        log.push(1, RawEvent::key_up("a"));
        log.push(3, RawEvent::key_down("b"));
        let mut replay = InputReplay::new(log);
        let first: Vec<RawEvent> = replay.drain_tick(1).collect();
        assert_eq!(first.len(), 2);
        assert!(replay.drain_tick(2).next().is_none());
        let third: Vec<RawEvent> = replay.drain_tick(3).collect();
        assert_eq!(third.len(), 1);
        assert!(replay.is_empty());
        assert!(InputReplay::new(InputLog::new(InputHeader::default())).is_empty());
    }

    #[test]
    fn rejects_bad_format() {
        assert!(matches!(
            InputLog::from_jsonl("").unwrap_err(),
            InputLogError::Empty
        ));
        let bad = "{\"format\":9,\"header\":{\"build\":\"x\",\"map\":\"y\",\"seed\":0,\"mobile\":false,\"width\":1,\"height\":1}}\n";
        assert!(matches!(
            InputLog::from_jsonl(bad).unwrap_err(),
            InputLogError::UnsupportedFormat { found: 9 }
        ));
    }
}
