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
/// ## Choosing Decay Factors for Different Behaviors
///
/// **Lower decay factor (closer to 0.0):**
/// - Adapts quickly to new values, making the average more responsive to recent changes
/// - Outliers are "forgotten" faster, providing better tolerance to temporary spikes or anomalies
/// - Useful for tracking recent trends where you want to react quickly to changes
/// - Example: `0.1` - heavily weights recent values, good for responsive latency tracking
///
/// **Higher decay factor (closer to 1.0):**
/// - Changes slowly and retains more historical information
/// - Outliers have a longer-lasting impact on the average, making them more visible
/// - Provides more stable tracking that's less sensitive to temporary fluctuations
/// - Example: `0.9` - heavily weights historical values, good for stable baseline tracking
///
/// When using this to track moving average of latency, it is important that
/// there should be a cap on the maximum value that can be stored.
#[derive(Debug, Clone)]
pub struct DecayMovingAverage {
    value: f64,
    decay_factor: f64,
}

impl DecayMovingAverage {
    /// Create a new DecayMovingAverage with an initial value and decay factor.
    ///
    /// # Arguments
    /// * `init_value` - The initial value for the moving average
    /// * `decay_factor` - A value between 0.0 and 1.0 that controls how much historical data is retained.
    ///   Lower values (e.g., 0.1) make the average more responsive to recent values and better at
    ///   forgetting outliers. Higher values (e.g., 0.9) make the average more stable but outliers
    ///   will have a longer-lasting impact.
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

    /// Override the moving average with a new value, bypassing the decay calculation.
    ///
    /// Unlike `update_moving_average()`, this method immediately sets the average to the new value
    /// rather than blending it with the previous value using the decay factor.
    ///
    /// This is particularly useful for implementing patterns like "decay moving max":
    /// - Track the maximum value seen recently, but let it decay over time if no new maxima occur
    /// - When a new maximum is encountered, immediately jump to that value using `override_moving_average()`
    /// - For regular updates below the maximum, use `update_moving_average()` to let the value decay naturally
    ///
    /// # Example: Decay Moving Max
    /// ```
    /// fn update_moving_max(new_value: f64) {
    ///     use mysten_common::decay_moving_average::DecayMovingAverage;
    ///     let mut decay_max = DecayMovingAverage::new(0.0, 0.9);
    ///
    ///     // New maximum encountered - immediately jump to it
    ///     if new_value > decay_max.get() {
    ///         decay_max.override_moving_average(new_value);
    ///     } else {
    ///         // Let the maximum decay naturally toward the current value
    ///         decay_max.update_moving_average(new_value);
    ///     }
    /// }
    /// ```
    pub fn override_moving_average(&mut self, value: f64) {
        self.value = value;
    }

    /// Get the current value of the moving average.
    pub fn get(&self) -> f64 {
        self.value
    }
}
