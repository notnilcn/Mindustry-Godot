// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Rolling mean window (`arc/src/arc/math/WindowedMean.java`).
//!
//! Ported exactly from Arc's `WindowedMean` (plan 09 §6.3): a fixed-size ring of
//! samples whose `mean()` is only meaningful once the window has filled; the
//! oldest sample is replaced by the newest. Plan 09 owns this single
//! implementation (HLP C10) and plan 08's `ItemModule` flow window consumes it.

/// A fixed-size windowed mean (`arc.math.WindowedMean`).
#[derive(Debug, Clone, PartialEq)]
pub struct WindowedMean {
    values: Vec<f32>,
    added_values: usize,
    last_value: usize,
    mean: f32,
    dirty: bool,
}

impl WindowedMean {
    /// Creates a window of `window_size` samples.
    pub fn new(window_size: usize) -> Self {
        Self {
            values: vec![0.0; window_size],
            added_values: 0,
            last_value: 0,
            mean: 0.0,
            dirty: true,
        }
    }

    /// Clears the window but keeps the allocated capacity (`reset`).
    pub fn reset(&mut self) {
        self.added_values = 0;
        self.last_value = 0;
        self.mean = 0.0;
        self.dirty = true;
    }

    /// Clears the window and zeroes every sample (`clear`).
    pub fn clear(&mut self) {
        self.added_values = 0;
        self.last_value = 0;
        self.values.fill(0.0);
        self.dirty = true;
    }

    /// Fills every sample with `value` (`fill`).
    pub fn fill(&mut self, value: f32) {
        self.values.fill(value);
        self.added_values = self.values.len();
        self.last_value = 0;
        self.dirty = true;
    }

    /// Adds a sample, replacing the oldest when full (`add`).
    pub fn add(&mut self, value: f32) {
        if self.added_values < self.values.len() {
            self.added_values += 1;
        }
        if let Some(slot) = self.values.get_mut(self.last_value) {
            *slot = value;
        }
        self.last_value += 1;
        if self.last_value > self.values.len().saturating_sub(1) {
            self.last_value = 0;
        }
        self.dirty = true;
    }

    /// Whether enough samples have been added for [`mean`](Self::mean) to be
    /// meaningful (`hasEnoughData`).
    pub fn has_enough_data(&self) -> bool {
        self.added_values >= self.values.len()
    }

    /// The mean over the full window (`0` before it fills).
    pub fn mean(&mut self) -> f32 {
        if self.has_enough_data() {
            if self.dirty {
                let sum: f32 = self.values.iter().sum();
                self.mean = sum / self.values.len() as f32;
                self.dirty = false;
            }
            self.mean
        } else {
            0.0
        }
    }

    /// The mean over inserted samples, usable before the window fills
    /// (`rawMean`).
    pub fn raw_mean(&self) -> f32 {
        if self.has_enough_data() {
            let sum: f32 = self.values.iter().sum();
            sum / self.values.len() as f32
        } else if self.added_values == 0 {
            0.0
        } else {
            let sum: f32 = self.values.iter().take(self.last_value).sum();
            sum / self.added_values as f32
        }
    }

    /// Sample `index` relative to the newest (`get`).
    pub fn get(&self, index: usize) -> f32 {
        if self.values.is_empty() {
            return 0.0;
        }
        self.values[(index + self.last_value) % self.values.len()]
    }

    /// The oldest sample in the window (`oldest`).
    pub fn oldest(&self) -> f32 {
        if self.added_values < self.values.len() {
            self.values.first().copied().unwrap_or(0.0)
        } else {
            self.values.get(self.last_value).copied().unwrap_or(0.0)
        }
    }

    /// The most recently added sample (`latest`).
    pub fn latest(&self) -> f32 {
        if self.values.is_empty() {
            return 0.0;
        }
        let index = if self.last_value == 0 {
            self.values.len() - 1
        } else {
            self.last_value - 1
        };
        self.values[index]
    }

    /// Number of samples added so far (`getCount`).
    pub fn get_count(&self) -> usize {
        self.added_values
    }

    /// The window size (`getWindowSize`).
    pub fn get_window_size(&self) -> usize {
        self.values.len()
    }

    /// Samples oldest-to-newest, length `addedValues` (`getWindowValues`).
    pub fn get_window_values(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.added_values);
        if self.has_enough_data() {
            for i in 0..self.values.len() {
                out.push(self.values[(i + self.last_value) % self.values.len()]);
            }
        } else {
            out.extend_from_slice(&self.values[..self.added_values.min(self.values.len())]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_full_window_for_mean() {
        let mut window = WindowedMean::new(4);
        assert!(!window.has_enough_data());
        assert_eq!(window.mean(), 0.0);
        window.add(1.0);
        window.add(2.0);
        assert_eq!(window.raw_mean(), 1.5);
        window.add(3.0);
        window.add(4.0);
        assert!(window.has_enough_data());
        assert_eq!(window.mean(), 2.5);
        // Fifth sample replaces the oldest (1.0).
        window.add(8.0);
        assert_eq!(window.mean(), (2.0 + 3.0 + 4.0 + 8.0) / 4.0);
    }

    #[test]
    fn raw_mean_matches_arc_before_fill() {
        let mut window = WindowedMean::new(6);
        assert_eq!(window.raw_mean(), 0.0);
        for value in [2.0, 4.0, 6.0] {
            window.add(value);
        }
        assert_eq!(window.raw_mean(), 4.0);
        assert_eq!(window.get_count(), 3);
        assert_eq!(window.get_window_size(), 6);
    }

    #[test]
    fn fill_and_clear_are_exact() {
        let mut window = WindowedMean::new(3);
        window.fill(5.0);
        assert!(window.has_enough_data());
        assert_eq!(window.mean(), 5.0);
        window.clear();
        assert!(!window.has_enough_data());
        assert_eq!(window.mean(), 0.0);
        assert_eq!(window.raw_mean(), 0.0);
    }

    #[test]
    fn window_values_are_oldest_first() {
        let mut window = WindowedMean::new(3);
        window.add(1.0);
        window.add(2.0);
        assert_eq!(window.get_window_values(), vec![1.0, 2.0]);
        window.add(3.0);
        window.add(4.0);
        assert_eq!(window.get_window_values(), vec![2.0, 3.0, 4.0]);
        assert_eq!(window.latest(), 4.0);
        assert_eq!(window.oldest(), 2.0);
    }
}
