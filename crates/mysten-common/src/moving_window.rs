// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

/// A moving window that maintains the last N values and calculates their average.
/// This provides a true arithmetic mean of recent values, with all values in the window
/// having equal weight. When the window is full and a new value is added, the oldest
/// value is removed to maintain the window size.
#[derive(Debug, Clone)]
pub struct MovingWindow {
    values: VecDeque<f64>,
    max_size: usize,
    sum: f64,
}

impl MovingWindow {
    /// Create a new MovingWindow with the specified maximum size and an `init_value`. The provided `max_size` must be greater than 0.
    pub fn new(init_value: f64, max_size: usize) -> Self {
        assert!(max_size > 0, "Window size must be greater than 0");
        let mut window = Self {
            values: VecDeque::with_capacity(max_size),
            max_size,
            sum: 0.0,
        };
        window.add_value(init_value);
        window
    }

    /// Add a new value to the window. If the window is at capacity, the oldest value is removed before adding the new value.
    pub fn add_value(&mut self, value: f64) {
        if self.values.len() == self.max_size {
            // Remove oldest value
            if let Some(old_value) = self.values.pop_front() {
                self.sum -= old_value;
            }
        }

        // Add new value
        self.values.push_back(value);
        self.sum += value;
    }

    /// Get the current average of all values in the window. Returns 0.0 if the window is empty.
    pub fn get(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f64
        }
    }

    /// Get the number of values currently in the window.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_initial_value() {
        let window = MovingWindow::new(10.0, 3);
        assert_eq!(window.len(), 1);
        assert_eq!(window.get(), 10.0);
    }

    #[test]
    fn test_add_values_within_capacity() {
        let mut window = MovingWindow::new(0.0, 3);

        window.add_value(1.0);
        assert_eq!(window.get(), 0.5);
        assert_eq!(window.len(), 2);

        window.add_value(2.0);
        assert_eq!(window.get(), 1.0);
        assert_eq!(window.len(), 3);

        window.add_value(3.0);
        assert_eq!(window.get(), 2.0);
        assert_eq!(window.len(), 3);
    }

    #[test]
    fn test_add_values_exceeding_capacity() {
        let mut window = MovingWindow::new(0.0, 3);

        // Fill the window
        window.add_value(1.0);
        window.add_value(2.0);
        window.add_value(3.0);
        assert_eq!(window.get(), 2.0);

        // Add fourth value, should remove first
        window.add_value(4.0);
        assert_eq!(window.get(), 3.0); // (2 + 3 + 4) / 3
        assert_eq!(window.len(), 3);

        // Add fifth value, should remove second
        window.add_value(5.0);
        assert_eq!(window.get(), 4.0); // (3 + 4 + 5) / 3
    }

    #[test]
    #[should_panic(expected = "Window size must be greater than 0")]
    fn test_zero_size_panics() {
        MovingWindow::new(0.0, 0);
    }
}
