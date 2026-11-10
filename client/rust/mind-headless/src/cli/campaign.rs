// SPDX-License-Identifier: GPL-3.0-only

//! `campaign` subcommands (plan 12 §7b).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `campaign` subcommands (plan 12 §7b).
#[derive(Debug, Subcommand)]
pub enum CampaignCommand {
    /// `campaign_rules_roundtrip`: JSON → Rules → TypeIO → JSON equality,
    /// `mode()`/checksum and one `RulesLoadEvent` (`from_save=false`).
    Rules {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_tech_unlock_gating`: locked→unlock gating, item spend,
    /// `req-` persistence, auto-unlocks and MP `Rules.researched`.
    Tech {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_sector_cycle`: planet/sector runtime, campaign rules + one
    /// production turn for a vanilla sector.
    Sector {
        /// Planet content name.
        #[arg(long, default_value = "serpulo")]
        planet: String,
        /// Sector preset name.
        #[arg(long, default_value = "groundZero")]
        sector: String,
        /// Fixed ticks to advance before the turn.
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_turn`: deterministic multi-turn production/export means.
    Turn {
        /// Number of turns to run.
        #[arg(long, default_value_t = 10)]
        turns: u32,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_schematic`: `.msch`/base64 round-trip, loadout decode and
    /// rotation of the vanilla starting schematics.
    Schematic {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_fog`: static exploration vs dynamic visibility + RLE chunk
    /// round-trip and attack-indicator lifecycle.
    Fog {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `objectives_completion`: all 13 in-map objective types complete under a
    /// scripted env, flags apply, and map markers add/control/round-trip.
    Objectives {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_play`: launch a sector, run waves, capture, and exercise the
    /// game-over/lose variants + the `set_rules` campaign guard.
    Play {
        /// Planet content name.
        #[arg(long, default_value = "serpulo")]
        planet: String,
        /// Sector preset name.
        #[arg(long, default_value = "groundZero")]
        sector: String,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_map_mode`: the `Control.playMap`/`start_map_mode` world half
    /// on one map file — `World.FilterContext` (`World.loadMap`) applied with
    /// `seed`, reporting the generated ore/boulder counts.
    MapMode {
        /// Map/save container file path.
        #[arg(long, default_value = "assets/maps/default/caldera.msav")]
        map: PathBuf,
        /// Generation seed (`FilterContext`; `GenerateFilter.randomize`).
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },

    /// `campaign_bench` (plan 12 §7d): per-system timings + deterministic
    /// checksums for the campaign budget profiles.
    Bench {
        /// Comma-separated profiles (`rules,teams,fog,turn,objectives,schematic`).
        #[arg(long, default_value = "rules,teams,fog,turn,objectives,schematic")]
        profile: String,
        /// Fixed ticks per profile (proportional scaling inside each).
        #[arg(long, default_value_t = 3600)]
        ticks: u32,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the canonical golden dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
}
