// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Godot-free MSUI builder (plan 14 §3.6).
//!
//! 1:1 port of `ui/builder/*`: typed nodes/entries, the wire codec, the DSL
//! parser/writer, `MenuBuilder`/`MenuResult` and style lookup. No Godot, no
//! network, no filesystem — the Godot sink lives in `mind-gdext`.

pub mod dsl;
pub mod dsl_factory;
pub mod dsl_writer;
pub mod hot_reload;
pub mod menu_builder;
pub mod menu_host;
pub mod menu_result;
pub mod style_lookup;
pub mod tree_builder;
pub mod ui_key;
pub mod ui_node;
pub mod ui_relay;
