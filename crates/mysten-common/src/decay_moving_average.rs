// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A moving average that decays over time so that the average value
/// skews towards the newer values over time.
///
/// The decay factor is a value between 0 and 1 that determines how much of the previous value
/// is kept when updating the average.
///
/// A decay factor of 0 means that the average is completely replaced with the new value every time,
/// and a decay factor of 1 means that the average never changes (keeps the old value).
///
/// When using this to track moving average of latency, it is important that
/// there should be a cap on the maximum value that can be stored.
#[derive(Debug, Clone)]
pub struct DecayMovingAverage {
    value: f64,
    decay_factor: f64,
}

impl DecayMovingAverage {
    pub fn new(init_value: f64, decay_factor: f64) -> Self {
        assert!(
            decay_factor > 0.0 && decay_factor < 1.0,
            "Decay factor must be between 0 and 1"
        );
        Self {
            value: init_value,
            decay_factor,
        }
    }

    /// Update the moving average with a new value.
    ///
    /// The new value is weighted by (1 - decay_factor), and the previous value
    /// is weighted by decay_factor, so that the average value skews towards
    /// the newer values over time.
    pub fn update_moving_average(&mut self, value: f64) {
        self.value = self.value * self.decay_factor + value * (1.0 - self.decay_factor);
    }

    /// Override the moving average with a new value.
    /// This can be useful when we want to reset the initial value of the moving average.
    pub fn override_moving_average(&mut self, value: f64) {
        self.value = value;
    }

    /// Get the current value of the moving average.
    pub fn get(&self) -> f64 {
        self.value
    }
}
