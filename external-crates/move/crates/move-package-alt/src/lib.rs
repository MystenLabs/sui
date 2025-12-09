// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This library defines the package management system for Move.
//!
//! TODO: major modules, etc

pub mod cli;
pub mod compatibility;
pub mod dependency;
pub mod errors;
pub mod flavor;
pub mod git;
pub mod graph;
pub mod package;
pub mod schema;
pub mod test_utils;

pub use package::package_impl::cache_package;
pub use package::paths::read_name_from_manifest;
