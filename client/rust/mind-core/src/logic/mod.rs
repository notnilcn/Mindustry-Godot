// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logic (mlog) system: text language, statement model, assembler and VM.
//!
//! Ported from `core/src/mindustry/logic/*.java` and
//! `core/src/mindustry/world/blocks/logic/*.java`. This module is Godot-free and
//! tokio-free; instruction execution is deterministic and bounded per tick
//! (`HIGH_LEVEL_PLAN` §2.4, D8).

pub mod access;
pub mod assembler;
pub mod enums;
pub mod executor;
pub mod fx;
pub mod globals;
pub mod ops;
pub mod parser;
pub mod rules;
pub mod script;
pub mod statement;
pub mod textio;

pub use access::LAccess;
pub use assembler::Assembler;
pub use executor::{Executor, Instruction};
pub use globals::GlobalVars;
pub use statement::{LogicRule, Statement, StatementField, StatementMeta};
pub use value::{LVar, LogicObject, VarArena, VarId, VarRef};

pub mod value;

#[cfg(test)]
mod tests;
