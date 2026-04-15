// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bytecode execution counters for profile-guided optimization.
//!
//! Provides per-VM atomic counters for tracking bytecode execution frequency.
//! Counters live inside `TelemetryContext` (one per `MoveRuntime`) so
//! concurrent VM instances do not contaminate each other's counts.
//!
//! Snapshots are exposed through the telemetry infrastructure via
//! `MoveRuntimeTelemetry::bytecode_stats`, and can be rendered as:
//!
//! - Human-readable report (`BytecodeSnapshot::format_report`)
//! - CSV for spreadsheets (`BytecodeSnapshot::format_csv`)
//! - JSON for analysis tools (`BytecodeSnapshot::format_json`)
//!
//! See `profiling/README.md` for a usage guide.

use move_binary_format::file_format_common::Opcodes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// All opcode variants, populated once at `BytecodeCounters::new`.
///
/// Kept in one place so adding a new opcode variant is a single-site change
/// (no hidden coupling to an array length).
const ALL_OPCODES: &[Opcodes] = &[
    Opcodes::POP,
    Opcodes::RET,
    Opcodes::BR_TRUE,
    Opcodes::BR_FALSE,
    Opcodes::BRANCH,
    Opcodes::LD_U64,
    Opcodes::LD_CONST,
    Opcodes::LD_TRUE,
    Opcodes::LD_FALSE,
    Opcodes::COPY_LOC,
    Opcodes::MOVE_LOC,
    Opcodes::ST_LOC,
    Opcodes::MUT_BORROW_LOC,
    Opcodes::IMM_BORROW_LOC,
    Opcodes::MUT_BORROW_FIELD,
    Opcodes::IMM_BORROW_FIELD,
    Opcodes::CALL,
    Opcodes::PACK,
    Opcodes::UNPACK,
    Opcodes::READ_REF,
    Opcodes::WRITE_REF,
    Opcodes::ADD,
    Opcodes::SUB,
    Opcodes::MUL,
    Opcodes::MOD,
    Opcodes::DIV,
    Opcodes::BIT_OR,
    Opcodes::BIT_AND,
    Opcodes::XOR,
    Opcodes::OR,
    Opcodes::AND,
    Opcodes::NOT,
    Opcodes::EQ,
    Opcodes::NEQ,
    Opcodes::LT,
    Opcodes::GT,
    Opcodes::LE,
    Opcodes::GE,
    Opcodes::ABORT,
    Opcodes::NOP,
    Opcodes::EXISTS_DEPRECATED,
    Opcodes::MUT_BORROW_GLOBAL_DEPRECATED,
    Opcodes::IMM_BORROW_GLOBAL_DEPRECATED,
    Opcodes::MOVE_FROM_DEPRECATED,
    Opcodes::MOVE_TO_DEPRECATED,
    Opcodes::FREEZE_REF,
    Opcodes::SHL,
    Opcodes::SHR,
    Opcodes::LD_U8,
    Opcodes::LD_U128,
    Opcodes::CAST_U8,
    Opcodes::CAST_U64,
    Opcodes::CAST_U128,
    Opcodes::MUT_BORROW_FIELD_GENERIC,
    Opcodes::IMM_BORROW_FIELD_GENERIC,
    Opcodes::CALL_GENERIC,
    Opcodes::PACK_GENERIC,
    Opcodes::UNPACK_GENERIC,
    Opcodes::EXISTS_GENERIC_DEPRECATED,
    Opcodes::MUT_BORROW_GLOBAL_GENERIC_DEPRECATED,
    Opcodes::IMM_BORROW_GLOBAL_GENERIC_DEPRECATED,
    Opcodes::MOVE_FROM_GENERIC_DEPRECATED,
    Opcodes::MOVE_TO_GENERIC_DEPRECATED,
    Opcodes::VEC_PACK,
    Opcodes::VEC_LEN,
    Opcodes::VEC_IMM_BORROW,
    Opcodes::VEC_MUT_BORROW,
    Opcodes::VEC_PUSH_BACK,
    Opcodes::VEC_POP_BACK,
    Opcodes::VEC_UNPACK,
    Opcodes::VEC_SWAP,
    Opcodes::LD_U16,
    Opcodes::LD_U32,
    Opcodes::LD_U256,
    Opcodes::CAST_U16,
    Opcodes::CAST_U32,
    Opcodes::CAST_U256,
    Opcodes::PACK_VARIANT,
    Opcodes::PACK_VARIANT_GENERIC,
    Opcodes::UNPACK_VARIANT,
    Opcodes::UNPACK_VARIANT_IMM_REF,
    Opcodes::UNPACK_VARIANT_MUT_REF,
    Opcodes::UNPACK_VARIANT_GENERIC,
    Opcodes::UNPACK_VARIANT_GENERIC_IMM_REF,
    Opcodes::UNPACK_VARIANT_GENERIC_MUT_REF,
    Opcodes::VARIANT_SWITCH,
];

/// Per-VM bytecode execution counters.
///
/// Each `MoveRuntime` owns one of these (held via `Arc` inside
/// `TelemetryContext`). Multiple runtimes running concurrently in the same
/// process therefore see independent counts.
#[derive(Debug)]
pub struct BytecodeCounters {
    counts: HashMap<Opcodes, AtomicU64>,
}

impl BytecodeCounters {
    /// Create a new set of bytecode counters, all initialized to zero.
    pub fn new() -> Self {
        let counts = ALL_OPCODES.iter().map(|op| (*op, AtomicU64::new(0))).collect();
        Self { counts }
    }

    /// Increment the counter for a specific opcode.
    ///
    /// Uses `Relaxed` ordering for minimal overhead — profiling data does not
    /// need strict ordering guarantees.
    #[inline(always)]
    pub fn increment(&self, opcode: Opcodes) {
        if let Some(counter) = self.counts.get(&opcode) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Take a snapshot of all counters.
    ///
    /// Each counter is read atomically (individually, not as a group).
    pub fn snapshot(&self) -> BytecodeSnapshot {
        let counts = self
            .counts
            .iter()
            .map(|(op, counter)| (*op, counter.load(Ordering::Relaxed)))
            .collect();
        BytecodeSnapshot { counts }
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        for counter in self.counts.values() {
            counter.store(0, Ordering::Relaxed);
        }
    }

    /// Get the count for a specific opcode.
    #[inline]
    pub fn get(&self, opcode: Opcodes) -> u64 {
        self.counts
            .get(&opcode)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }
}

impl Default for BytecodeCounters {
    fn default() -> Self {
        Self::new()
    }
}

/// A point-in-time snapshot of bytecode execution counts.
#[derive(Clone, Debug, Default)]
pub struct BytecodeSnapshot {
    counts: HashMap<Opcodes, u64>,
}

impl BytecodeSnapshot {
    /// Get the count for a specific opcode.
    pub fn get(&self, opcode: Opcodes) -> u64 {
        self.counts.get(&opcode).copied().unwrap_or(0)
    }

    /// Total instruction count across all opcodes.
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Iterator over opcodes with non-zero counts, yielding `(Opcodes, count)` pairs.
    /// Order is unspecified (HashMap iteration order).
    pub fn iter(&self) -> impl Iterator<Item = (Opcodes, u64)> + '_ {
        self.counts
            .iter()
            .filter_map(|(op, count)| if *count > 0 { Some((*op, *count)) } else { None })
    }

    /// Format as a human-readable report, sorted by count descending.
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

    /// Format as CSV with header row. Columns: opcode, count, percentage.
    /// Rows are sorted by count descending.
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

    /// Format as JSON for consumption by analysis tools.
    ///
    /// Shape:
    /// ```json
    /// {
    ///   "total": 1234,
    ///   "opcodes": [
    ///     { "opcode": "ADD", "count": 500, "percentage": 40.5186 },
    ///     { "opcode": "LT",  "count": 300, "percentage": 24.3112 }
    ///   ]
    /// }
    /// ```
    ///
    /// Rows are sorted by count descending. Zero-count opcodes are omitted.
    /// Hand-rolled (no serde dependency) because this crate is
    /// infrastructure-level and serde would pull in a significant
    /// transitive surface.
    pub fn format_json(&self) -> String {
        let total = self.total();
        let mut entries: Vec<_> = self.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"total\": {},\n", total));
        out.push_str("  \"opcodes\": [\n");

        for (i, (opcode, count)) in entries.iter().enumerate() {
            let pct = if total > 0 {
                (*count as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            let comma = if i + 1 < entries.len() { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"opcode\": \"{:?}\", \"count\": {}, \"percentage\": {:.4} }}{}\n",
                opcode, count, pct, comma
            ));
        }

        out.push_str("  ]\n");
        out.push('}');
        out
    }

    /// If `MOVE_VM_DUMP_PROFILE_FILE` is set to a path, write this snapshot's
    /// JSON representation there. Otherwise a no-op.
    ///
    /// Errors (bad path, I/O failure) are logged via `tracing::warn!` rather
    /// than propagated — a failed profile dump should not fail execution.
    pub fn maybe_dump_to_env_file(&self) {
        let Ok(path) = std::env::var("MOVE_VM_DUMP_PROFILE_FILE") else {
            return;
        };
        match std::fs::write(&path, self.format_json()) {
            Ok(()) => tracing::debug!(%path, "wrote bytecode profile"),
            Err(e) => tracing::warn!(%path, error = %e, "failed to write bytecode profile"),
        }
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
        assert_eq!(entries.len(), 3);

        let add_entry = entries.iter().find(|(op, _)| *op == Opcodes::ADD);
        let sub_entry = entries.iter().find(|(op, _)| *op == Opcodes::SUB);
        let mul_entry = entries.iter().find(|(op, _)| *op == Opcodes::MUL);

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

        assert_eq!(sorted[0].0, Opcodes::MUL);
        assert_eq!(sorted[0].1, 3);
        assert_eq!(sorted[1].0, Opcodes::SUB);
        assert_eq!(sorted[1].1, 2);
        assert_eq!(sorted[2].0, Opcodes::ADD);
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

        assert!(csv.starts_with("opcode,count,percentage\n"));

        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() >= 4);
        assert!(lines[1].starts_with("MUL,3,"));
        assert!(lines[2].starts_with("SUB,2,"));
        assert!(lines[3].starts_with("ADD,1,"));
        assert!(lines[1].contains("50."));
        assert!(lines[2].contains("33."));
        assert!(lines[3].contains("16."));
    }

    #[test]
    fn test_format_csv_empty() {
        let counters = BytecodeCounters::new();
        let snapshot = counters.snapshot();
        let csv = snapshot.format_csv();
        assert_eq!(csv, "opcode,count,percentage\n");
    }

    #[test]
    fn test_counters_are_independent() {
        // Two independent counter sets should not see each other's increments.
        let a = BytecodeCounters::new();
        let b = BytecodeCounters::new();

        a.increment(Opcodes::ADD);
        a.increment(Opcodes::ADD);
        b.increment(Opcodes::ADD);

        assert_eq!(a.get(Opcodes::ADD), 2);
        assert_eq!(b.get(Opcodes::ADD), 1);
    }

    #[test]
    fn test_format_json() {
        let counters = BytecodeCounters::new();
        counters.increment(Opcodes::ADD);
        counters.increment(Opcodes::SUB);
        counters.increment(Opcodes::SUB);

        let json = counters.snapshot().format_json();
        assert!(json.contains("\"total\": 3"));
        assert!(json.contains("\"opcode\": \"SUB\""));
        assert!(json.contains("\"count\": 2"));
        assert!(json.contains("\"opcode\": \"ADD\""));
        assert!(json.contains("\"count\": 1"));
        // SUB appears before ADD (sorted by count descending).
        assert!(json.find("SUB").unwrap() < json.find("ADD").unwrap());
    }

    #[test]
    fn test_format_json_empty() {
        let snapshot = BytecodeCounters::new().snapshot();
        let json = snapshot.format_json();
        assert!(json.contains("\"total\": 0"));
        assert!(json.contains("\"opcodes\": ["));
    }

}
