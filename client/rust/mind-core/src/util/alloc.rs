// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Allocation audit (plan 05 M8 §7.4).
//!
//! With the `alloc-audit` feature enabled, `mind-core` installs a counting
//! global allocator so `mind-headless bench --assert-alloc 0` can prove that
//! steady-state ticks do not allocate after warmup (the `Tmp`/`Pools` discipline
//! from §3.9). Without the feature these functions are zero-cost no-ops.
//!
//! The counters are process-global atomics; the audit only checks *deltas*
//! across a measured region.

// A counting global allocator must implement the unsafe `GlobalAlloc` trait.
#![allow(unsafe_code)]

/// Number of allocations observed so far (0 without `alloc-audit`).
pub fn alloc_count() -> u64 {
    #[cfg(feature = "alloc-audit")]
    {
        counting::ALLOC_COUNT.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(feature = "alloc-audit"))]
    {
        0
    }
}

/// Number of bytes requested so far (0 without `alloc-audit`).
pub fn alloc_bytes() -> u64 {
    #[cfg(feature = "alloc-audit")]
    {
        counting::ALLOC_BYTES.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(feature = "alloc-audit"))]
    {
        0
    }
}

/// Whether the counting global allocator is installed.
pub const fn enabled() -> bool {
    cfg!(feature = "alloc-audit")
}

#[cfg(feature = "alloc-audit")]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Count of allocation calls.
    pub static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
    /// Bytes requested by allocations.
    pub static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

    /// Wraps the system allocator, counting every `alloc`/`realloc`.
    struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            // SAFETY: forwarded to the system allocator with the same layout.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: forwarded to the system allocator with the same layout.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            // SAFETY: forwarded to the system allocator with the same arguments.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_are_monotonic() {
        let before = alloc_count();
        let value = Box::new([0u8; 32]);
        assert_eq!(value.len(), 32);
        let after = alloc_count();
        if enabled() {
            assert!(after >= before);
        } else {
            assert_eq!(before, after);
        }
    }

    /// Probe which tick in the steady-state region allocates (alloc-audit only).
    #[cfg(feature = "alloc-audit")]
    #[test]
    fn steady_state_tick_allocations_are_isolated() {
        use crate::content::BlockId;
        use crate::sim::Sim;
        let mut sim = Sim::new(1, 256, 256, BlockId::AIR, BlockId::AIR);
        let mut placed = 0;
        'place: for y in 0..256i16 {
            for x in 0..256i16 {
                if placed >= 600 {
                    break 'place;
                }
                sim.apply(crate::command::Command::Place {
                    x,
                    y,
                    block: BlockId::STONE_WALL,
                })
                .expect("place");
                placed += 1;
            }
        }
        for _ in 0..600 {
            sim.tick().expect("warmup");
        }
        let mut prev = alloc_count();
        let mut offenders = Vec::new();
        for tick in 0..2000u32 {
            sim.tick().expect("tick");
            let now = alloc_count();
            if now > prev {
                offenders.push((tick, now - prev));
            }
            prev = now;
        }
        eprintln!("alloc offenders: {offenders:?}");
        assert!(
            offenders.is_empty(),
            "steady-state ticks allocated: {offenders:?}"
        );
    }
}
