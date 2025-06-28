// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::iter::empty;

use serde::{Deserialize, Serialize};

use crate::dependency::{DependencySet, PinnedDependencyInfo};

use super::MoveFlavor;

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
/// flavor-specific resolvers and stores no additional metadata in the lockfile.
#[derive(Debug)]
pub struct Vanilla;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum VanillaDep {}

impl MoveFlavor for Vanilla {
    type PublishedMetadata = ();
    type PackageMetadata = ();
    type EnvironmentID = String;
    type AddressInfo = ();

    fn name() -> String {
        "vanilla".to_string()
    }

    fn implicit_deps(
        &self,
        environments: impl Iterator<Item = Self::EnvironmentID>,
    ) -> DependencySet<PinnedDependencyInfo> {
        empty().collect()
    }
}
