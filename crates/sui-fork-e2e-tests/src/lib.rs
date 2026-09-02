// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared harness for the `sui-fork` end-to-end tests.
//!
//! The test targets in this crate all need a source network that a fork can be taken from, so
//! that harness lives in this library rather than under `tests/`. A module under `tests/` is
//! compiled once per test target, and each target would then warn about the items the others use.

pub mod source;

pub use source::SourceNetwork;
