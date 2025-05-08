// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_package_alt::{
    dependency::{self, DependencySet, Pinned, PinnedDependencyInfo, Unpinned},
    errors::PackageResult,
    flavor::MoveFlavor,
    package::PackageName,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename = "kebab-case")]
pub struct OnChainDependency {
    on_chain: bool,
}

#[derive(Debug)]
pub struct SuiFlavor;

impl MoveFlavor for SuiFlavor {
    type FlavorDependency<P: ?Sized> = OnChainDependency;

    fn name() -> String {
        "sui move 2025".to_string()
    }

    fn pin(
        &self,
        deps: DependencySet<Self::FlavorDependency<Unpinned>>,
    ) -> PackageResult<DependencySet<Self::FlavorDependency<Pinned>>> {
        todo!()
    }

    fn fetch(
        &self,
        deps: DependencySet<Self::FlavorDependency<Pinned>>,
    ) -> PackageResult<DependencySet<std::path::PathBuf>> {
        todo!()
    }

    type PublishedMetadata = (); // TODO

    type EnvironmentID = String; // TODO

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn implicit_deps(
        &self,
        environments: impl Iterator<Item = Self::EnvironmentID>,
    ) -> DependencySet<PinnedDependencyInfo<Self>> {
        todo!()
    }
}
