// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::{
    collections::{self, BTreeMap},
    iter::empty,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::dependency::{DependencySet, PinnedDependencyInfo};
use crate::{
    dependency::{Pinned, Unpinned},
    errors::PackageResult,
    package::PackageName,
};

use super::MoveFlavor;

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
/// flavor-specific resolvers and stores no additional metadata in the lockfile.
pub struct Vanilla;

impl MoveFlavor for Vanilla {
    type PublishedMetadata = ();
    type PackageMetadata = ();
    type EnvironmentID = String;
    type AddressInfo = ();

    fn implicit_deps(
        &self,
        environments: impl Iterator<Item = Self::EnvironmentID>,
    ) -> DependencySet<PinnedDependencyInfo<Self>> {
        empty().collect()
    }

    // TODO: should be !, but that's not supported; instead
    // should be some type that always gives an error during
    // deserialization
    type FlavorDependency<P: ?Sized> = ();

    fn pin(
        &self,
        deps: DependencySet<Self::FlavorDependency<Unpinned>>,
    ) -> PackageResult<DependencySet<Self::FlavorDependency<Pinned>>> {
        // always an error
        todo!()
    }

    fn fetch(
        &self,
        deps: DependencySet<Self::FlavorDependency<Pinned>>,
    ) -> PackageResult<DependencySet<PathBuf>> {
        // always an error
        todo!()
    }
}
