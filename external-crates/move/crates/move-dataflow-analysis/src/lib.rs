// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Dataflow analysis framework for Move stackless bytecode.
//!
//! Provides a generic fixpoint solver that works with any abstract domain and
//! transfer functions, supporting both forward and backward analyses.

pub mod analyses;
pub mod analysis;
pub mod domains;
