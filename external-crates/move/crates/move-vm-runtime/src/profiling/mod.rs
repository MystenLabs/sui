// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Profile data collection for the Move VM interpreter.
//!
//! This module provides infrastructure for collecting bytecode execution statistics
//! to enable profile-guided optimization of the interpreter dispatch loop.
//!
//! # Usage
//!
//! Enable the `tracing` feature to collect bytecode frequency data:
//!
//! ```bash
//! cargo build --features move-vm-runtime/tracing
//! ```

pub mod counters;

pub use crate::shared::constants::{DEFAULT_PROFILE_FILE, SUI_PROFILE_FILE_ENV};
pub use counters::{
    BYTECODE_COUNTERS, BytecodeCounters, BytecodeSnapshot, dump_profile_info,
    dump_profile_info_to_file,
};
