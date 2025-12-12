// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Profile data collection for the Move VM interpreter.
//!
//! This module provides infrastructure for collecting bytecode execution statistics
//! to enable profile-guided optimization of the interpreter dispatch loop.
//!
//! # Usage
//!
//! Enable the `profiling` feature to collect bytecode frequency data:
//!
//! ```bash
//! cargo build --features move-vm-runtime/profiling
//! ```

pub mod counters;

pub use counters::{
    dump_profile_info, dump_profile_info_to_file, BytecodeCounters, BytecodeSnapshot,
    DEFAULT_PROFILE_FILE, SUI_PROFILE_FILE_ENV, BYTECODE_COUNTERS,
};
