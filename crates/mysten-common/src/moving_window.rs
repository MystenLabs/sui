// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, fmt::Debug, time::Duration};

/// A moving window that maintains the last N values of type `T` and calculates their arithmetic
/// mean. All values in the window have equal weight and the oldest value is dropped when the
/// window exceeds its configured capacity.
#[derive(Debug, Clone)]
pub struct MovingWindow<T: MovingWindowValue> {
    values: VecDeque<T>,
    max_size: usize,
    sum: T,
}

impl<T: MovingWindowValue> MovingWindow<T> {
    /// Creates a new `MovingWindow` with the specified maximum size and an `init_value`.
    /// The provided `max_size` must be greater than 0.
    pub fn new(init_value: T, max_size: usize) -> Self {
        assert!(max_size > 0, "Window size must be greater than 0");
        let mut window = Self {
            values: VecDeque::with_capacity(max_size),
            max_size,
            sum: T::zero(),
        };
        window.add_value(init_value);
        window
    }

    /// Adds a new value to the window. If the window is at capacity, the oldest value is removed
    /// before adding the new value.
    pub fn add_value(&mut self, value: T) {
        if self.values.len() == self.max_size
            && let Some(old_value) = self.values.pop_front()
        {
            T::sub_assign(&mut self.sum, old_value);
        }

        self.values.push_back(value);
        T::add_assign(&mut self.sum, value);
    }

    /// Get the current average of all values in the window. Returns the value's zero if the
    /// window is empty.
    pub fn get(&self) -> T {
        if self.values.is_empty() {
            T::zero()
        } else {
            T::average(self.sum, self.values.len())
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

pub trait MovingWindowValue: Copy + Debug {
    fn zero() -> Self;
    fn add_assign(target: &mut Self, value: Self);
    fn sub_assign(target: &mut Self, value: Self);
    fn average(total: Self, divisor: usize) -> Self;
}

impl MovingWindowValue for Duration {
    fn zero() -> Self {
        Duration::ZERO
    }

    fn add_assign(target: &mut Self, value: Self) {
        *target += value;
    }

    fn sub_assign(target: &mut Self, value: Self) {
        *target -= value;
    }

    fn average(total: Self, divisor: usize) -> Self {
        let divisor = u32::try_from(divisor).expect("window size too large for Duration average");
        total / divisor
    }
}

impl MovingWindowValue for f64 {
    fn zero() -> Self {
        0.0
    }

    fn add_assign(target: &mut Self, value: Self) {
        *target += value;
    }

    fn sub_assign(target: &mut Self, value: Self) {
        *target -= value;
    }

    fn average(total: Self, divisor: usize) -> Self {
        total / divisor as f64
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
    fn test_duration_window() {
        let mut window = MovingWindow::new(Duration::ZERO, 3);
        assert_eq!(window.get(), Duration::ZERO);

        // Adding value within the window size.
        window.add_value(Duration::from_millis(100));
        assert_eq!(window.get(), Duration::from_millis(50));
        assert_eq!(window.len(), 2);

        // Adding value within the window size.
        window.add_value(Duration::from_millis(200));
        assert_eq!(window.get(), Duration::from_millis(100));
        assert_eq!(window.len(), 3);

        // Adding values exceeding the window size.
        window.add_value(Duration::from_millis(300));
        assert_eq!(window.get(), Duration::from_millis(200));
        assert_eq!(window.len(), 3);

        // Adding values exceeding the window size.
        window.add_value(Duration::from_millis(400));
        assert_eq!(window.get(), Duration::from_millis(300));
        assert_eq!(window.len(), 3);
    }

    #[test]
    fn test_float_window() {
        let mut window = MovingWindow::new(0.0_f64, 3);
        assert_eq!(window.get(), 0.0);

        // Adding value within the window size.
        window.add_value(1.0);
        assert_eq!(window.get(), 0.5);
        assert_eq!(window.len(), 2);

        // Adding value within the window size.
        window.add_value(2.0);
        assert_eq!(window.get(), 1.0);
        assert_eq!(window.len(), 3);

        // Adding value exceeding the window size.
        window.add_value(3.0);
        assert_eq!(window.get(), 2.0);
        assert_eq!(window.len(), 3);

        // Adding value exceeding the window size.
        window.add_value(4.0);
        assert_eq!(window.get(), 3.0);
        assert_eq!(window.len(), 3);
    }

    #[test]
    #[should_panic(expected = "Window size must be greater than 0")]
    fn test_zero_size_panics() {
        let _window = MovingWindow::new(0.0_f64, 0);
    }
}
