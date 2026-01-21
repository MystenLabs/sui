// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bytecode execution counters for profile-guided optimization.
//!
//! This module provides atomic counters for tracking bytecode execution frequency.
//! The counters are designed for minimal overhead when profiling is enabled.
//!
//! Bytecode statistics are exposed through the telemetry infrastructure via
//! `MoveRuntimeTelemetry::bytecode_stats`.

use move_binary_format::file_format_common::Opcodes;
use std::sync::atomic::{AtomicU64, Ordering};

/// Number of bytecode variants to track.
/// This should cover all opcodes defined in `Opcodes`.
const BYTECODE_COUNT: usize = 128;

/// Global bytecode counters instance.
/// This is a global static to avoid passing counters through the call stack.
pub static BYTECODE_COUNTERS: BytecodeCounters = BytecodeCounters::new();

/// Per-instruction execution counters.
/// Array indices map to `Opcodes` discriminant values.
pub struct BytecodeCounters {
    counts: [AtomicU64; BYTECODE_COUNT],
}

impl BytecodeCounters {
    /// Create a new set of bytecode counters, all initialized to zero.
    pub const fn new() -> Self {
        // Initialize all counters to 0 using const initialization
        const ZERO: AtomicU64 = AtomicU64::new(0);
        Self {
            counts: [ZERO; BYTECODE_COUNT],
        }
    }

    /// Increment the counter for a specific opcode.
    ///
    /// Uses `Relaxed` ordering for minimal overhead - we don't need
    /// strict ordering guarantees for profiling data.
    #[inline(always)]
    pub fn increment(&self, opcode: Opcodes) {
        let idx = opcode as usize;
        if idx < BYTECODE_COUNT {
            self.counts[idx].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Take a snapshot of all counters.
    ///
    /// This reads all counters atomically (individually, not as a group)
    /// and returns a copyable snapshot for analysis.
    pub fn snapshot(&self) -> BytecodeSnapshot {
        let mut counts = [0u64; BYTECODE_COUNT];
        for (i, counter) in self.counts.iter().enumerate() {
            counts[i] = counter.load(Ordering::Relaxed);
        }
        BytecodeSnapshot { counts }
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        for counter in &self.counts {
            counter.store(0, Ordering::Relaxed);
        }
    }

    /// Get the count for a specific opcode.
    #[inline]
    pub fn get(&self, opcode: Opcodes) -> u64 {
        let idx = opcode as usize;
        if idx < BYTECODE_COUNT {
            self.counts[idx].load(Ordering::Relaxed)
        } else {
            0
        }
    }
}

/// A point-in-time snapshot of bytecode execution counts.
#[derive(Clone, Debug)]
pub struct BytecodeSnapshot {
    counts: [u64; BYTECODE_COUNT],
}

impl BytecodeSnapshot {
    /// Get the count for a specific opcode.
    pub fn get(&self, opcode: Opcodes) -> u64 {
        let idx = opcode as usize;
        if idx < BYTECODE_COUNT {
            self.counts[idx]
        } else {
            0
        }
    }

    /// Get total instruction count across all opcodes.
    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }

    /// Returns an iterator over all opcodes with non-zero counts.
    /// Yields `(Opcodes, count)` pairs in opcode order (not sorted by frequency).
    pub fn iter(&self) -> impl Iterator<Item = (Opcodes, u64)> {
        self.counts.iter().enumerate().filter_map(|(idx, &count)| {
            if count > 0 {
                Opcodes::from_u8(idx as u8).map(|op| (op, count))
            } else {
                None
            }
        })
    }

    /// Format as a human-readable report.
    pub fn format_report(&self) -> String {
        let total = self.total();
        if total == 0 {
            return "No bytecode executions recorded.".to_string();
        }

        let mut entries: Vec<_> = self.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        let mut report = String::new();
        report.push_str(&format!("Total instructions: {}\n\n", total));
        report.push_str("Opcode                          Count         %\n");
        report.push_str("------------------------------------------------\n");

        for (opcode, count) in entries {
            let pct = (count as f64 / total as f64) * 100.0;
            report.push_str(&format!(
                "{:<30} {:>12} {:>8.2}%\n",
                format!("{:?}", opcode),
                count,
                pct
            ));
        }

        report
    }
}

impl Default for BytecodeSnapshot {
    fn default() -> Self {
        Self {
            counts: [0u64; BYTECODE_COUNT],
        }
    }
}

impl BytecodeSnapshot {
    /// Format as CSV with header row.
    /// Format: opcode,count,percentage
    pub fn format_csv(&self) -> String {
        let total = self.total();
        let mut entries: Vec<_> = self.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        let mut csv = String::new();
        csv.push_str("opcode,count,percentage\n");

        for (opcode, count) in entries {
            let pct = if total > 0 {
                (count as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            csv.push_str(&format!("{:?},{},{:.4}\n", opcode, count, pct));
        }

        csv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_increment() {
        let counters = BytecodeCounters::new();
        assert_eq!(counters.get(Opcodes::ADD), 0);

        counters.increment(Opcodes::ADD);
        assert_eq!(counters.get(Opcodes::ADD), 1);

        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::ADD);
        assert_eq!(counters.get(Opcodes::ADD), 3);
    }

    #[test]
    fn test_snapshot() {
        let counters = BytecodeCounters::new();
        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::SUB);

        let snapshot = counters.snapshot();
        assert_eq!(snapshot.get(Opcodes::ADD), 1);
        assert_eq!(snapshot.get(Opcodes::SUB), 2);
        assert_eq!(snapshot.total(), 3);
    }

    #[test]
    fn test_reset() {
        let counters = BytecodeCounters::new();
        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::SUB);

        counters.reset();

        assert_eq!(counters.get(Opcodes::ADD), 0);
        assert_eq!(counters.get(Opcodes::SUB), 0);
    }

    #[test]
    fn test_iter() {
        let counters = BytecodeCounters::new();
        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::MUL);
        counters.increment(Opcodes::MUL);
        counters.increment(Opcodes::MUL);

        let snapshot = counters.snapshot();
        let entries: Vec<_> = snapshot.iter().collect();

        // Verify all three opcodes are present
        assert_eq!(entries.len(), 3);

        // Find each opcode and verify count
        let add_entry = entries
            .iter()
            .find(|(op, _)| *op as u8 == Opcodes::ADD as u8);
        let sub_entry = entries
            .iter()
            .find(|(op, _)| *op as u8 == Opcodes::SUB as u8);
        let mul_entry = entries
            .iter()
            .find(|(op, _)| *op as u8 == Opcodes::MUL as u8);

        assert_eq!(add_entry.unwrap().1, 1);
        assert_eq!(sub_entry.unwrap().1, 2);
        assert_eq!(mul_entry.unwrap().1, 3);
    }

    #[test]
    fn test_iter_sorted_by_frequency() {
        let counters = BytecodeCounters::new();
        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::MUL);
        counters.increment(Opcodes::MUL);
        counters.increment(Opcodes::MUL);

        let snapshot = counters.snapshot();
        let mut sorted: Vec<_> = snapshot.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        // Compare using `as u8` since Opcodes doesn't implement PartialEq
        assert_eq!(sorted[0].0 as u8, Opcodes::MUL as u8);
        assert_eq!(sorted[0].1, 3);
        assert_eq!(sorted[1].0 as u8, Opcodes::SUB as u8);
        assert_eq!(sorted[1].1, 2);
        assert_eq!(sorted[2].0 as u8, Opcodes::ADD as u8);
        assert_eq!(sorted[2].1, 1);
    }

    #[test]
    fn test_format_csv() {
        let counters = BytecodeCounters::new();
        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::MUL);
        counters.increment(Opcodes::MUL);
        counters.increment(Opcodes::MUL);

        let snapshot = counters.snapshot();
        let csv = snapshot.format_csv();

        // Verify header
        assert!(csv.starts_with("opcode,count,percentage\n"));

        // Verify each line has proper format: opcode,count,percentage
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() >= 4); // header + 3 opcodes

        // Verify MUL appears first (highest count)
        assert!(lines[1].starts_with("MUL,3,"));

        // Verify SUB appears second
        assert!(lines[2].starts_with("SUB,2,"));

        // Verify ADD appears third
        assert!(lines[3].starts_with("ADD,1,"));

        // Verify percentages are present - check they start with expected values
        // (exact precision may vary slightly)
        assert!(lines[1].contains("50."));
        assert!(lines[2].contains("33."));
        assert!(lines[3].contains("16."));
    }

    #[test]
    fn test_format_csv_empty() {
        let counters = BytecodeCounters::new();
        let snapshot = counters.snapshot();
        let csv = snapshot.format_csv();

        // Should only have header when no counts
        assert_eq!(csv, "opcode,count,percentage\n");
    }
}
