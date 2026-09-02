// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared harness for the `sui-fork` end-to-end tests.
//!
//! The test targets in this crate all need a source network that a fork can be taken from, the
//! binaries under test, and a way to run a script without leaking its background processes, so
//! those pieces live in this library rather than under `tests/`. A module under `tests/` is
//! compiled once per test target, and each target would then warn about the items the others use.

pub mod binaries;
pub mod script;
pub mod source;

pub use binaries::Binaries;
pub use script::ScriptOutput;
pub use script::forward_termination_signals;
pub use script::run_script;
pub use source::SourceNetwork;
