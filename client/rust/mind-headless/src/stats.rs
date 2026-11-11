// SPDX-License-Identifier: GPL-3.0-only

//! Report statistics shared by the harness modules.
//! Upstream: none (port-only; the per-module percentile copies of the old oracle)

/// The `percent`-th sample of an ascending `samples` slice by index
/// (`len * percent / 100`, clamped to the last sample); `0` when empty.
///
/// The index form is the one `exec/bench.rs`, `stdb_scenarios.rs` and
/// `input_scenarios.rs` used; `mp_scenarios.rs` keeps its nearest-rank
/// (`div_ceil`) copy because its rank formula differs.
pub fn percentile(samples: &[u64], percent: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let index = (samples.len() * percent / 100).min(samples.len() - 1);
    samples[index]
}
