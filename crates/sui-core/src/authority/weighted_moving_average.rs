// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::fmt;

#[derive(Clone)]
struct Sample {
    value: u64,
    weight: u64,
    insertion_total: u64, // cumulative weight at insertion time
}

impl fmt::Debug for Sample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.2}, {:.2})", self.value, self.weight)
    }
}

impl fmt::Display for Sample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.2}, {:.2})", self.value, self.weight)
    }
}

/// Weighted moving average of a stream of samples.
///
/// This is not only computing a weighted average, it is also attempting to prevent
/// low weight samples from evicting higher weight samples. The idea is that we do
/// not want the measurement to quickly become skewed due to the arrival of several
/// low weight samples.
#[derive(Debug, Clone)]
pub struct WeightedMovingAverage {
    queue: VecDeque<Sample>,
    total_weight: u64, // running cumulative weight of all added samples
    max_size: usize,   // maximum number of samples to store
    default: u64,
}

impl WeightedMovingAverage {
    /// Create a new WeightedMovingAverage with a fixed maximum number of samples.
    pub fn new(default: u64, max_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            total_weight: 0,
            max_size,
            default,
        }
    }

    fn is_full(&self) -> bool {
        self.queue.len() >= self.max_size
    }

    pub fn add_sample(&mut self, value: u64, weight: u64) {
        self.total_weight += weight;
        let sample = Sample {
            value,
            weight,
            insertion_total: self.total_weight,
        };

        // See if there are old samples to evict.
        while self.is_full() && self.try_evict_oldest_sample().is_some() {}

        // If we are still full, find a sample with weight less than the new sample
        // and replace it with the new sample.
        if self.is_full() {
            // Find the old sample with weight less than the new sample
            if let Some(min_idx) = self
                .queue
                .iter()
                .enumerate()
                .find(|(_, s)| s.weight < sample.weight)
                .map(|(i, _)| i)
            {
                self.queue.remove(min_idx);
                self.queue.push_back(sample);
            }

            // The sample is dropped, but it is has still incremented the total weight
            // so eventually the oldest sample will become old enough to be evicted and
            // make room for new measurements.
        } else {
            self.queue.push_back(sample);
        }
    }

    // If the oldest sample is old enough, we evict it. "old enough" means that
    // the total weight added since its insertion is max_size * weight.
    // The intuition is that this preserves the property that if all samples had equal weight,
    // you would have to add max_size samples to the queue before the oldest sample is evicted.
    //
    // For example, for a queue of size 10, and a sample of weight 10, you must add a total
    // of 100 weight to the queue before that sample becomes old enough to be evicted.
    fn try_evict_oldest_sample(&mut self) -> Option<Sample> {
        let old_sample = self.queue.front()?;

        let weight_since_insertion = self.total_weight - old_sample.insertion_total;
        let threshold = old_sample.weight * self.max_size as u64;

        if weight_since_insertion >= threshold {
            self.queue.pop_front()
        } else {
            None
        }
    }

    /// Compute the weighted average over the samples currently in the window.
    pub fn get_weighted_average(&self) -> u64 {
        if self.queue.is_empty() {
            return self.default;
        }
        let (weighted_sum, total_w) = self.queue.iter().fold((0u64, 0u64), |(sum, w_sum), s| {
            (sum + s.value * s.weight, w_sum + s.weight)
        });
        weighted_sum / total_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_moving_average_equal_weights() {
        let mut wma = WeightedMovingAverage::new(0, 10);
        wma.add_sample(100, 100);
        assert_eq!(wma.get_weighted_average(), 100);

        // Add 9 samples with value 1.0 and weight 100.0
        for _ in 0..9 {
            wma.add_sample(1, 100);
        }
        assert_eq!(wma.get_weighted_average(), 10);

        // one more sample with weight 100 pushes the oldest sample out
        wma.add_sample(1, 100);
        assert_eq!(wma.get_weighted_average(), 1);
    }

    #[test]
    fn test_old_sample_is_evicted() {
        let mut wma = WeightedMovingAverage::new(0, 10);
        wma.add_sample(10, 10);
        assert_eq!(wma.get_weighted_average(), 10);

        // verify that after adding 99 samples of value 1, weight 1,
        // the original sample is still in the average. Not enough
        // weight has been added to evict it.
        for _ in 0..99 {
            wma.add_sample(1, 1);
        }
        assert_eq!(wma.get_weighted_average(), 5);

        // One more sample now means that there has been 10 x the capacity
        // in weight added, so the oldest sample is evicted and the moving
        // average is now 1.
        wma.add_sample(1, 1);
        assert_eq!(wma.get_weighted_average(), 1);
    }

    #[test]
    fn test_lowest_sample_is_replaced() {
        let mut wma = WeightedMovingAverage::new(0, 10);
        wma.add_sample(10, 10);
        assert_eq!(wma.get_weighted_average(), 10);

        for _ in 0..10 {
            wma.add_sample(1, 1);
        }
        assert_eq!(wma.get_weighted_average(), 5);

        // A weight 5 sample is enough to evict one of the weight 1 samples,
        // which moves the average up to 6.
        wma.add_sample(10, 5);
        assert_eq!(wma.get_weighted_average(), 6);
    }
}
