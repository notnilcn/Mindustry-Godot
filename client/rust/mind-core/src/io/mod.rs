// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Persistence layer for `mind-core` (plan 04): the native `MGRS` save container,
//! `TypeIO` object codec, entity revision IO, `JsonIO`, settings, save slots and
//! map headers. Package path parity with `mindustry.io`.
//!
//! Ported from `core/src/mindustry/io/` (`SaveIO`, `SaveVersion`, `SaveFileReader`,
//! `TypeIO`, `JsonIO`, `MapIO`), `core/src/mindustry/game/Saves.java` and Arc
//! `Settings`/`Fi`. The legacy upstream `MSAV` format is import-only behind the
//! default-off `msav-import` feature (OD2 / NUD-02).
//!
//! Boundaries (plan 04 §3.12): no Godot/tokio, no `unwrap`/`expect` on runtime
//! data, ordered maps only on serialized paths, append-only format versions.

pub mod entity;
pub mod error;
pub mod fs;
pub mod json;
pub mod legacy;
pub mod map;
pub mod save;
pub mod settings;
pub mod sim_io;
pub mod typeio;
pub mod wire;

pub use error::IoError;
pub use fs::{FileSystem, MockFs, NativeFs, Paths};
pub use save::{SaveIo, SaveMeta, SaveOptions, SaveReadState, WorldContext};
pub use settings::{SettingValue, SettingsStore};
pub use sim_io::SimIoHandler;
pub use wire::{WireReader, WireWriter};

/// Ordered string map — the Arc `StringMap` equivalent used for meta tags and
/// map tags. Insertion order is the serialized order (determinism, §3.12.6).
pub type StringMap = indexmap::IndexMap<String, String>;

/// Result alias for IO operations.
pub type IoResult<T> = Result<T, IoError>;
