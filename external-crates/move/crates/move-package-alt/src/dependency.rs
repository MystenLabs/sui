// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines several reusable dependency types, but not an aggreated type (like
//! `Dependency`). The entire set of dependency types may differ from flavor to flavor, so they
//! are defined by the associated types in the [crate::flavor::MoveFlavor] trait. Implementations
//! of that trait will typically define an enumeration whose variants wrap the types defined in
//! this module. See [crate::flavor::Vanilla] for an example.

mod external;
mod git;
mod local;

pub use external::ExternalDependency;
pub use git::GitDependency;
pub use local::LocalDependency;
