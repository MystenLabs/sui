// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

/// A moving window that maintains the last N values and calculates their average.
///
/// This provides a true arithmetic mean of recent values, with all values in the window
/// having equal weight. When the window is full and a new value is added, the oldest
/// value is removed to maintain the window size.
///
/// ## Choosing Window Sizes for Different Behaviors
///
/// **Smaller window size (e.g., 10-20):**
/// - More responsive to recent changes
/// - Better for detecting sudden performance shifts
/// - Less stable, more susceptible to short-term fluctuations
///
/// **Larger window size (e.g., 50-100):**
/// - More stable averages that smooth out temporary spikes
/// - Better representation of sustained performance levels
/// - Less responsive to recent changes, takes longer to adapt
///
/// **Memory usage:** O(window_size) per instance
/// **Update complexity:** O(1) amortized
/// **Get complexity:** O(1)
#[derive(Debug, Clone)]
pub struct MovingWindow {
    values: VecDeque<f64>,
    max_size: usize,
    sum: f64,
}

impl MovingWindow {
    /// Create a new MovingWindow with the specified maximum size.
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of values to keep in the window. Must be > 0.
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "Window size must be greater than 0");
        Self {
            values: VecDeque::with_capacity(max_size),
            max_size,
            sum: 0.0,
        }
    }

    /// Create a new MovingWindow with an initial value.
    ///
    /// # Arguments
    /// * `init_value` - The initial value to populate the window with
    /// * `max_size` - Maximum number of values to keep in the window. Must be > 0.
    pub fn with_initial_value(init_value: f64, max_size: usize) -> Self {
        let mut window = Self::new(max_size);
        window.add_value(init_value);
        window
    }

    /// Add a new value to the window.
    ///
    /// If the window is at capacity, the oldest value is removed before adding the new value.
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

    /// Get the current average of all values in the window.
    ///
    /// Returns 0.0 if the window is empty.
    pub fn get_average(&self) -> f64 {
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

    /// Check if the window is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Check if the window is at maximum capacity.
    pub fn is_full(&self) -> bool {
        self.values.len() == self.max_size
    }

    /// Get the maximum capacity of the window.
    pub fn capacity(&self) -> usize {
        self.max_size
    }

    /// Clear all values from the window.
    pub fn clear(&mut self) {
        self.values.clear();
        self.sum = 0.0;
    }

    /// Get the most recent value added to the window.
    ///
    /// Returns None if the window is empty.
    pub fn latest_value(&self) -> Option<f64> {
        self.values.back().copied()
    }

    /// Get the oldest value in the window.
    ///
    /// Returns None if the window is empty.
    pub fn oldest_value(&self) -> Option<f64> {
        self.values.front().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_window() {
        let window = MovingWindow::new(5);
        assert_eq!(window.len(), 0);
        assert_eq!(window.capacity(), 5);
        assert!(window.is_empty());
        assert!(!window.is_full());
        assert_eq!(window.get_average(), 0.0);
    }

    #[test]
    fn test_with_initial_value() {
        let window = MovingWindow::with_initial_value(10.0, 3);
        assert_eq!(window.len(), 1);
        assert_eq!(window.get_average(), 10.0);
        assert!(!window.is_empty());
        assert!(!window.is_full());
    }

    #[test]
    fn test_add_values_within_capacity() {
        let mut window = MovingWindow::new(3);

        window.add_value(1.0);
        assert_eq!(window.get_average(), 1.0);
        assert_eq!(window.len(), 1);

        window.add_value(2.0);
        assert_eq!(window.get_average(), 1.5);
        assert_eq!(window.len(), 2);

        window.add_value(3.0);
        assert_eq!(window.get_average(), 2.0);
        assert_eq!(window.len(), 3);
        assert!(window.is_full());
    }

    #[test]
    fn test_add_values_exceeding_capacity() {
        let mut window = MovingWindow::new(3);

        // Fill the window
        window.add_value(1.0);
        window.add_value(2.0);
        window.add_value(3.0);
        assert_eq!(window.get_average(), 2.0);

        // Add fourth value, should remove first
        window.add_value(4.0);
        assert_eq!(window.get_average(), 3.0); // (2 + 3 + 4) / 3
        assert_eq!(window.len(), 3);
        assert!(window.is_full());

        // Add fifth value, should remove second
        window.add_value(5.0);
        assert_eq!(window.get_average(), 4.0); // (3 + 4 + 5) / 3
    }

    #[test]
    fn test_latest_and_oldest_values() {
        let mut window = MovingWindow::new(3);

        assert_eq!(window.latest_value(), None);
        assert_eq!(window.oldest_value(), None);

        window.add_value(1.0);
        assert_eq!(window.latest_value(), Some(1.0));
        assert_eq!(window.oldest_value(), Some(1.0));

        window.add_value(2.0);
        window.add_value(3.0);
        assert_eq!(window.latest_value(), Some(3.0));
        assert_eq!(window.oldest_value(), Some(1.0));

        // Add value that causes oldest to be removed
        window.add_value(4.0);
        assert_eq!(window.latest_value(), Some(4.0));
        assert_eq!(window.oldest_value(), Some(2.0));
    }

    #[test]
    fn test_clear() {
        let mut window = MovingWindow::new(3);
        window.add_value(1.0);
        window.add_value(2.0);

        window.clear();
        assert!(window.is_empty());
        assert_eq!(window.len(), 0);
        assert_eq!(window.get_average(), 0.0);
        assert_eq!(window.latest_value(), None);
    }

    #[test]
    #[should_panic(expected = "Window size must be greater than 0")]
    fn test_zero_size_panics() {
        MovingWindow::new(0);
    }
}
