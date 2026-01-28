// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This library defines the package management system for Move.
//!
//! TODO: major modules, etc

mod compatibility;
mod dependency;
mod errors;
mod flavor;
mod graph;
mod logging;
mod package;

pub mod git;
pub mod schema;
pub mod test_utils;

pub use package::paths::read_name_from_manifest;

// TODO: maybe put Vanilla into test_utils
// TODO: maybe put SourcePackageLayout, NamedAddress into schema

pub use errors::{PackageError, PackageResult};
pub use flavor::{MoveFlavor, Vanilla};
pub use graph::{NamedAddress, PackageInfo};
pub use package::layout::SourcePackageLayout;
pub use package::{
    RootPackage, package_impl::cache_package, package_loader::PackageLoader, paths::PackagePath,
};
