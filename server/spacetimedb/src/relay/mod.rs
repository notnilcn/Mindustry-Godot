// SPDX-License-Identifier: GPL-3.0-only

//! Command relay foundation (plan 01 §3.7/§3.8): matches, membership and the
//! append-only ordered command log.

pub mod checksum;
pub mod methods;
pub mod plans;
pub mod player_state;
pub mod reducers;
pub mod snapshot;
pub mod sweep;
pub mod tables;
pub mod ui_events;
pub mod views;
