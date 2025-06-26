// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This library defines the package management system for Move.
//!
//! TODO: major modules, etc

#![allow(unused)]
pub mod cli;
pub mod dependency;
pub mod errors;
pub mod flavor;
pub mod git;
pub mod graph;
pub mod package;
pub mod schema;
