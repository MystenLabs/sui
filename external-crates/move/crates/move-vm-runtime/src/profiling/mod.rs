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
//! The module (types, constants, formatting helpers) is always available.
//! The per-runtime counter storage and the increment in the interpreter hot
//! loop are gated behind the `tracing` feature, so there is zero runtime
//! overhead when it is disabled.
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

pub use crate::shared::constants::{MOVE_VM_DUMP_PROFILE_FILE_ENV, MOVE_VM_PROFILE_MODE_ENV};
pub use counters::{BytecodeCounters, BytecodeSnapshot};
