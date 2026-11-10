// SPDX-License-Identifier: GPL-3.0-only

//! `meta` subcommands (plan 05).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `meta` subcommands (plan 05 M5 §6.2).
#[derive(Debug, Subcommand)]
pub enum MetaCommand {
    /// Dump the entity/component metadata JSON (`entitymeta.json` shape).
    Entities {
        /// Write the stable JSON here (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit the JSON report on stdout even when `--out` is set.
        #[arg(long)]
        json: bool,
    },
}
