// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Non-entity reuse pools (plan 05 §3.9).
//!
//! Ported from Arc `arc.util.Pool`/`Pools`. Used for per-tick scratch
//! collections; entity pooling lives in [`crate::entities::lifecycle`].

/// A pool of reusable `Vec<T>` buffers.
#[derive(Debug)]
pub struct VecPool<T> {
    free: Vec<Vec<T>>,
    max_free: usize,
}

impl<T> Default for VecPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> VecPool<T> {
    /// Creates a pool that retains up to 16 free buffers.
    pub fn new() -> Self {
        Self::with_max_free(16)
    }

    /// Creates a pool with a custom free-list cap.
    pub fn with_max_free(max_free: usize) -> Self {
        Self {
            free: Vec::new(),
            max_free,
        }
    }

    /// Number of retained buffers.
    pub fn free_len(&self) -> usize {
        self.free.len()
    }

    /// Takes a cleared buffer from the pool (or allocates one).
    pub fn get(&mut self) -> Vec<T> {
        let mut buffer = self.free.pop().unwrap_or_default();
        buffer.clear();
        buffer
    }

    /// Returns a buffer to the pool (dropped when the cap is reached).
    pub fn put(&mut self, mut buffer: Vec<T>) {
        if self.free.len() < self.max_free {
            buffer.clear();
            self.free.push(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_pool_reuses_and_caps() {
        let mut pool = VecPool::<u32>::new();
        let mut buffer = pool.get();
        buffer.push(5);
        pool.put(buffer);
        assert_eq!(pool.free_len(), 1);
        let reused = pool.get();
        assert!(reused.is_empty());

        let mut small = VecPool::<u32>::with_max_free(1);
        small.put(Vec::new());
        small.put(Vec::new());
        assert_eq!(small.free_len(), 1);
    }
}
