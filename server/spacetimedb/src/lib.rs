// SPDX-License-Identifier: GPL-3.0-only

//! Mindustry-Godot SpacetimeDB module — P0 skeleton.
//!
//! `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` grows this into the identity/session
//! skeleton and the command relay. Rules carried from `main/server/AGENTS.md`:
//! reducers are deterministic and return no data, `ctx.sender()` is the only
//! principal, updates spread the existing row (`..row`), index names are unique
//! module-wide.

#![allow(special_module_name)]
#![allow(non_snake_case)]

mod main;
