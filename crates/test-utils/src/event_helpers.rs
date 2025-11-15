// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helper utilities for testing event emission in Sui

use std::collections::HashMap;

/// Event test utilities for validating event emission
pub struct EventValidator {
    expected_events: Vec<String>,
    actual_events: Vec<String>,
}

impl EventValidator {
    /// Create a new event validator
    pub fn new() -> Self {
        Self {
            expected_events: Vec::new(),
            actual_events: Vec::new(),
        }
    }

    /// Add an expected event type
    pub fn expect_event(&mut self, event_type: impl Into<String>) -> &mut Self {
        self.expected_events.push(event_type.into());
        self
    }

    /// Add multiple expected events
    pub fn expect_events(&mut self, event_types: Vec<String>) -> &mut Self {
        self.expected_events.extend(event_types);
        self
    }

    /// Record an actual event
    pub fn record_event(&mut self, event_type: impl Into<String>) {
        self.actual_events.push(event_type.into());
    }

    /// Check if all expected events were emitted
    pub fn validate(&self) -> Result<(), String> {
        for expected in &self.expected_events {
            if !self.actual_events.contains(expected) {
                return Err(format!("Expected event not found: {}", expected));
            }
        }
        Ok(())
    }

    /// Check if events were emitted in order
    pub fn validate_order(&self) -> Result<(), String> {
        let mut actual_iter = self.actual_events.iter();

        for expected in &self.expected_events {
            match actual_iter.find(|e| *e == expected) {
                Some(_) => continue,
                None => return Err(format!("Expected event not found in order: {}", expected)),
            }
        }

        Ok(())
    }

    /// Get count of specific event type
    pub fn count_event(&self, event_type: &str) -> usize {
        self.actual_events
            .iter()
            .filter(|e| e.as_str() == event_type)
            .count()
    }

    /// Reset the validator
    pub fn reset(&mut self) {
        self.expected_events.clear();
        self.actual_events.clear();
    }
}

impl Default for EventValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Event counter for tracking event frequencies
pub struct EventCounter {
    counts: HashMap<String, usize>,
}

impl EventCounter {
    /// Create a new event counter
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// Increment count for an event type
    pub fn increment(&mut self, event_type: impl Into<String>) {
        let event = event_type.into();
        *self.counts.entry(event).or_insert(0) += 1;
    }

    /// Get count for an event type
    pub fn get(&self, event_type: &str) -> usize {
        self.counts.get(event_type).copied().unwrap_or(0)
    }

    /// Get all event counts
    pub fn all_counts(&self) -> &HashMap<String, usize> {
        &self.counts
    }

    /// Get total event count
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// Reset all counts
    pub fn reset(&mut self) {
        self.counts.clear();
    }

    /// Assert that an event occurred at least N times
    pub fn assert_min_count(&self, event_type: &str, min: usize) {
        let count = self.get(event_type);
        assert!(
            count >= min,
            "Event {} occurred {} times, expected at least {}",
            event_type,
            count,
            min
        );
    }

    /// Assert that an event occurred exactly N times
    pub fn assert_exact_count(&self, event_type: &str, expected: usize) {
        let count = self.get(event_type);
        assert_eq!(
            count, expected,
            "Event {} occurred {} times, expected {}",
            event_type, count, expected
        );
    }
}

impl Default for EventCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_validator() {
        let mut validator = EventValidator::new();
        validator.expect_event("Transfer");
        validator.expect_event("Mint");

        validator.record_event("Transfer");
        validator.record_event("Mint");

        assert!(validator.validate().is_ok());
    }

    #[test]
    fn test_event_validator_order() {
        let mut validator = EventValidator::new();
        validator.expect_event("Create");
        validator.expect_event("Transfer");

        validator.record_event("Create");
        validator.record_event("Mint");
        validator.record_event("Transfer");

        assert!(validator.validate_order().is_ok());
    }

    #[test]
    fn test_event_counter() {
        let mut counter = EventCounter::new();

        counter.increment("Transfer");
        counter.increment("Transfer");
        counter.increment("Mint");

        assert_eq!(counter.get("Transfer"), 2);
        assert_eq!(counter.get("Mint"), 1);
        assert_eq!(counter.total(), 3);
    }

    #[test]
    fn test_event_counter_assertions() {
        let mut counter = EventCounter::new();

        counter.increment("Transfer");
        counter.increment("Transfer");

        counter.assert_min_count("Transfer", 1);
        counter.assert_exact_count("Transfer", 2);
    }
}
