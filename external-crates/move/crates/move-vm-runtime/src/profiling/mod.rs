// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Profile data collection for the Move VM interpreter.
//!
//! This module provides infrastructure for collecting bytecode execution statistics
//! to enable profile-guided optimization of the interpreter dispatch loop.
//!
//! Bytecode statistics are exposed through the telemetry infrastructure via
//! `MoveRuntimeTelemetry::bytecode_stats` when the `tracing` feature is enabled.
//!
//! # Usage
//!
//! Enable the `tracing` feature to collect bytecode frequency data:
//!
//! ```bash
//! cargo build --features move-vm-runtime/tracing
//! ```

pub mod counters;

pub use counters::{BYTECODE_COUNTERS, BytecodeCounters, BytecodeSnapshot};
