// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bytecode execution profiling for the Move VM interpreter.
//!
//! Counts how many times each opcode is dispatched so callers can identify
//! hot opcodes, validate gas calibration, or compare workload distributions.
//!
//! See `README.md` in this directory for a full usage guide.
//!
//! # Scope
//!
//! Counters are **per `MoveRuntime`**. A process with two runtimes has two
//! independent counter sets. This prevents concurrent replay sessions from
//! contaminating each other's counts.
//!
//! # Feature-gating
//!
//! Only compiled in when the `tracing` feature is enabled. When disabled, the
//! increment in the interpreter hot loop is removed by the compiler, so there
//! is zero runtime overhead.
//!
//! # Example
//!
//! Pulling a snapshot through the telemetry API:
//!
//! ```ignore
//! use move_vm_runtime::runtime::MoveRuntime;
//!
//! # fn make_runtime() -> MoveRuntime { unimplemented!() }
//! let runtime = make_runtime();
//! // ... execute transactions ...
//!
//! let report = runtime.get_telemetry_report();
//! let stats = &report.bytecode_stats;
//! println!("total instructions: {}", stats.total());
//! println!("{}", stats.format_report());
//! ```
//!
//! Emitting via tracing and, optionally, dumping JSON to a file:
//!
//! ```ignore
//! // If MOVE_VM_DUMP_PROFILE_FILE is set, the snapshot is written there.
//! runtime.emit_bytecode_profile();
//! ```

pub mod counters;

pub use counters::{BytecodeCounters, BytecodeSnapshot};
