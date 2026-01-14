// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bytecode execution counters for profile-guided optimization.
//!
//! This module provides atomic counters for tracking bytecode execution frequency.
//! The counters are designed for minimal overhead when profiling is enabled.

use crate::shared::constants::{DEFAULT_PROFILE_FILE, SUI_PROFILE_FILE_ENV};
use move_binary_format::file_format_common::Opcodes;
use std::sync::atomic::{AtomicU64, Ordering};

/// Convert a u8 opcode value back to Opcodes enum.
/// Returns None for invalid/unrecognized opcode values.
fn opcode_from_u8(val: u8) -> Option<Opcodes> {
    // Match on known opcode values from file_format_common.rs
    match val {
        0x01 => Some(Opcodes::POP),
        0x02 => Some(Opcodes::RET),
        0x03 => Some(Opcodes::BR_TRUE),
        0x04 => Some(Opcodes::BR_FALSE),
        0x05 => Some(Opcodes::BRANCH),
        0x06 => Some(Opcodes::LD_U64),
        0x07 => Some(Opcodes::LD_CONST),
        0x08 => Some(Opcodes::LD_TRUE),
        0x09 => Some(Opcodes::LD_FALSE),
        0x0A => Some(Opcodes::COPY_LOC),
        0x0B => Some(Opcodes::MOVE_LOC),
        0x0C => Some(Opcodes::ST_LOC),
        0x0D => Some(Opcodes::MUT_BORROW_LOC),
        0x0E => Some(Opcodes::IMM_BORROW_LOC),
        0x0F => Some(Opcodes::MUT_BORROW_FIELD),
        0x10 => Some(Opcodes::IMM_BORROW_FIELD),
        0x11 => Some(Opcodes::CALL),
        0x12 => Some(Opcodes::PACK),
        0x13 => Some(Opcodes::UNPACK),
        0x14 => Some(Opcodes::READ_REF),
        0x15 => Some(Opcodes::WRITE_REF),
        0x16 => Some(Opcodes::ADD),
        0x17 => Some(Opcodes::SUB),
        0x18 => Some(Opcodes::MUL),
        0x19 => Some(Opcodes::MOD),
        0x1A => Some(Opcodes::DIV),
        0x1B => Some(Opcodes::BIT_OR),
        0x1C => Some(Opcodes::BIT_AND),
        0x1D => Some(Opcodes::XOR),
        0x1E => Some(Opcodes::OR),
        0x1F => Some(Opcodes::AND),
        0x20 => Some(Opcodes::NOT),
        0x21 => Some(Opcodes::EQ),
        0x22 => Some(Opcodes::NEQ),
        0x23 => Some(Opcodes::LT),
        0x24 => Some(Opcodes::GT),
        0x25 => Some(Opcodes::LE),
        0x26 => Some(Opcodes::GE),
        0x27 => Some(Opcodes::ABORT),
        0x28 => Some(Opcodes::NOP),
        0x2E => Some(Opcodes::FREEZE_REF),
        0x2F => Some(Opcodes::SHL),
        0x30 => Some(Opcodes::SHR),
        0x31 => Some(Opcodes::LD_U8),
        0x32 => Some(Opcodes::LD_U128),
        0x33 => Some(Opcodes::CAST_U8),
        0x34 => Some(Opcodes::CAST_U64),
        0x35 => Some(Opcodes::CAST_U128),
        0x36 => Some(Opcodes::MUT_BORROW_FIELD_GENERIC),
        0x37 => Some(Opcodes::IMM_BORROW_FIELD_GENERIC),
        0x38 => Some(Opcodes::CALL_GENERIC),
        0x39 => Some(Opcodes::PACK_GENERIC),
        0x3A => Some(Opcodes::UNPACK_GENERIC),
        0x40 => Some(Opcodes::VEC_PACK),
        0x41 => Some(Opcodes::VEC_LEN),
        0x42 => Some(Opcodes::VEC_IMM_BORROW),
        0x43 => Some(Opcodes::VEC_MUT_BORROW),
        0x44 => Some(Opcodes::VEC_PUSH_BACK),
        0x45 => Some(Opcodes::VEC_POP_BACK),
        0x46 => Some(Opcodes::VEC_UNPACK),
        0x47 => Some(Opcodes::VEC_SWAP),
        0x48 => Some(Opcodes::LD_U16),
        0x49 => Some(Opcodes::LD_U32),
        0x4A => Some(Opcodes::LD_U256),
        0x4B => Some(Opcodes::CAST_U16),
        0x4C => Some(Opcodes::CAST_U32),
        0x4D => Some(Opcodes::CAST_U256),
        0x4E => Some(Opcodes::PACK_VARIANT),
        0x4F => Some(Opcodes::PACK_VARIANT_GENERIC),
        0x50 => Some(Opcodes::UNPACK_VARIANT),
        0x51 => Some(Opcodes::UNPACK_VARIANT_IMM_REF),
        0x52 => Some(Opcodes::UNPACK_VARIANT_MUT_REF),
        0x53 => Some(Opcodes::UNPACK_VARIANT_GENERIC),
        0x54 => Some(Opcodes::UNPACK_VARIANT_GENERIC_IMM_REF),
        0x55 => Some(Opcodes::UNPACK_VARIANT_GENERIC_MUT_REF),
        0x56 => Some(Opcodes::VARIANT_SWITCH),
        _ => None,
    }
}

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
                opcode_from_u8(idx as u8).map(|op| (op, count))
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

/// Dump profile information to a file.
///
/// Takes a snapshot of the current bytecode counters and writes them to a CSV file.
/// The file path is determined by:
/// 1. The `SUI_PROFILE_FILE` environment variable if set
/// 2. Otherwise, defaults to "sui-profile.profraw"
///
/// Returns `Ok(())` if the file was written successfully, or an error message.
pub fn dump_profile_info() -> Result<(), String> {
    let file_path =
        std::env::var(SUI_PROFILE_FILE_ENV).unwrap_or_else(|_| DEFAULT_PROFILE_FILE.to_string());
    dump_profile_info_to_file(&file_path)
}

/// Dump profile information to a specified file path.
///
/// Takes a snapshot of the current bytecode counters and writes them to a CSV file
/// at the specified path.
///
/// Returns `Ok(())` if the file was written successfully, or an error message.
pub fn dump_profile_info_to_file(file_path: &str) -> Result<(), String> {
    let snapshot = BYTECODE_COUNTERS.snapshot();
    let csv_content = snapshot.format_csv();

    std::fs::write(file_path, csv_content)
        .map_err(|e| format!("Failed to write profile to {}: {}", file_path, e))
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

    #[test]
    fn test_dump_profile_info_to_file() {
        use std::fs;

        // Use a unique temp file for this test
        let test_file = "/tmp/test_dump_profile_info.profraw";

        // Reset counters and add some data
        BYTECODE_COUNTERS.reset();
        BYTECODE_COUNTERS.increment(Opcodes::CALL);
        BYTECODE_COUNTERS.increment(Opcodes::CALL);
        BYTECODE_COUNTERS.increment(Opcodes::RET);

        // Dump to file using the explicit path function
        let result = dump_profile_info_to_file(test_file);
        assert!(
            result.is_ok(),
            "dump_profile_info_to_file failed: {:?}",
            result
        );

        // Read and verify file contents
        let contents = fs::read_to_string(test_file).expect("Failed to read profile file");

        // Verify header
        assert!(contents.starts_with("opcode,count,percentage\n"));

        // Verify CALL and RET are present
        assert!(contents.contains("CALL,2,"));
        assert!(contents.contains("RET,1,"));

        // Cleanup
        let _ = fs::remove_file(test_file);
    }
}
