// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Input, placement & RTS (plan 15).
//!
//! The pure input state machine, placement algorithms, plan mirror and RTS
//! selection live here (Godot-free/tokio-free, deviation I1). Godot event
//! translation, node plumbing and settings persistence live in
//! `mind-gdext::input`. None of this registers an ECS system and none of it
//! enters the checksum (I2/I3).

pub mod binding;
pub mod caps;
pub mod focus;
pub mod input_log;
pub mod place_mode;

pub use binding::{
    BINDS, BindingDefault, BindingState, BindingValue, Category, KeyBind, KeyBindId, KeyBindTable,
    KeyKind, ids,
};
pub use caps::{InputCaps, TestCaps};
pub use focus::{FocusGuards, FocusState, InputLocks, LockId};
pub use input_log::{
    INPUT_LOG_FORMAT, InputHeader, InputLog, InputLogError, InputRecord, InputReplay, RawEvent,
};
pub use place_mode::{MobileMode, PlaceMode};
