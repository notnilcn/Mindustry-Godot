// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Tmp` / `TempVec` scratch discipline (plan 05 §3.9).
//!
//! Ported from Arc `arc.util.Tmp` (single-threaded static scratch). Here the
//! scratch is thread-local; worker threads get their own instances. Scratch is
//! reset-on-borrow and must never be held across systems.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};

/// A reusable scratch vector. Clears on drop so no element is observed twice.
#[derive(Debug, Default)]
pub struct TempVec<T> {
    inner: Vec<T>,
}

impl<T> TempVec<T> {
    /// Creates an empty scratch vector.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Creates a scratch vector with reserved capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    /// Current length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T> Deref for TempVec<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for TempVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Drop for TempVec<T> {
    fn drop(&mut self) {
        self.inner.clear();
    }
}

/// Runs `f` with a fresh scratch `Vec<T>` (allocation-free after warmup when a
/// caller-supplied [`crate::util::pools::VecPool`] is used instead).
pub fn with_temp_vec<T, R>(f: impl FnOnce(&mut TempVec<T>) -> R) -> R {
    let mut scratch = TempVec::new();
    f(&mut scratch)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TmpBuffers {
    v2: [f32; 2],
    length: f32,
}

thread_local! {
    static TMP: RefCell<TmpBuffers> = const { RefCell::new(TmpBuffers { v2: [0.0, 0.0], length: 0.0 }) };
}

/// Single-threaded scratch handles (Arc `Tmp`).
#[derive(Debug, Clone, Copy)]
pub struct Tmp;

impl Tmp {
    /// Returns the shared 2-float scratch value.
    pub fn v2() -> [f32; 2] {
        TMP.with(|tmp| tmp.borrow().v2)
    }

    /// Sets the shared 2-float scratch and returns the previous value.
    pub fn set_v2(x: f32, y: f32) -> [f32; 2] {
        TMP.with(|tmp| {
            let mut tmp = tmp.borrow_mut();
            let previous = tmp.v2;
            tmp.v2 = [x, y];
            previous
        })
    }

    /// Returns the shared scalar scratch value.
    pub fn length() -> f32 {
        TMP.with(|tmp| tmp.borrow().length)
    }

    /// Sets the shared scalar scratch.
    pub fn set_length(value: f32) {
        TMP.with(|tmp| tmp.borrow_mut().length = value);
    }

    /// Resets every scratch slot.
    pub fn clear() {
        TMP.with(|tmp| {
            *tmp.borrow_mut() = TmpBuffers {
                v2: [0.0, 0.0],
                length: 0.0,
            };
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_vec_clears_on_drop_and_derefs() {
        {
            let mut scratch = TempVec::with_capacity(4);
            scratch.push(1u32);
            scratch.push(2);
            assert_eq!(scratch.len(), 2);
        }
        with_temp_vec(|scratch| {
            scratch.push(7u32);
            assert_eq!(scratch.as_slice(), [7]);
        });
    }

    #[test]
    fn tmp_slots_roundtrip() {
        Tmp::clear();
        assert_eq!(Tmp::v2(), [0.0, 0.0]);
        let previous = Tmp::set_v2(1.5, -2.5);
        assert_eq!(previous, [0.0, 0.0]);
        assert_eq!(Tmp::v2(), [1.5, -2.5]);
        Tmp::set_length(3.0);
        assert_eq!(Tmp::length(), 3.0);
        Tmp::clear();
        assert_eq!(Tmp::length(), 0.0);
    }
}
