// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared harness for the `sui-fork` end-to-end tests.
//!
//! The test targets in this crate all need a source network that a fork can be taken from and
//! the binaries under test, so those pieces live in this library rather than under `tests/`. A
//! module under `tests/` is compiled once per test target, and each target would then warn about
//! the items the others use.

pub mod binaries;
pub mod source;

pub use binaries::Binaries;
pub use source::SourceNetwork;
