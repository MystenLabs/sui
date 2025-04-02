// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Defines the [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports no
//! flavor-specific resolvers and stores no additional metadata in the lockfile.

use std::{
    collections::{self, BTreeMap},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    dependency::{Pinned, PinnedDependency, Unpinned},
    errors::PackageResult,
    package::PackageName,
};

use super::MoveFlavor;

/// The [Vanilla] implementation of the [MoveFlavor] trait. This implementation supports git,
/// local, and externally resolved dependencies, and stores no additional metadata in the lockfile.
pub struct Vanilla;

impl MoveFlavor for Vanilla {
    // TODO
    type PublishedMetadata = ();

    type EnvironmentID = String;

    fn implicit_dependencies(&self, id: Self::EnvironmentID) -> Vec<PinnedDependency<Self>> {
        vec![]
    }

    // TODO: should be !, but that's not supported; instead
    // should be some type that always gives an error during
    // deserialization
    type FlavorDependency<P: ?Sized> = ();

    fn pin(
        &self,
        deps: BTreeMap<PackageName, Self::FlavorDependency<Unpinned>>,
    ) -> PackageResult<BTreeMap<PackageName, Self::FlavorDependency<Pinned>>> {
        todo!()
    }

    fn fetch(
        &self,
        deps: BTreeMap<PackageName, Self::FlavorDependency<Pinned>>,
    ) -> PackageResult<BTreeMap<PackageName, PathBuf>> {
        todo!()
    }
}
