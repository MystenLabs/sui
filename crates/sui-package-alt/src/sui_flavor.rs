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

#[derive(Debug)]
pub struct SuiFlavor;

impl MoveFlavor for SuiFlavor {
    fn name() -> String {
        "sui move 2025".to_string()
    }

    type PublishedMetadata = (); // TODO

    type EnvironmentID = String; // TODO

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn implicit_deps(
        &self,
        environments: impl Iterator<Item = Self::EnvironmentID>,
    ) -> DependencySet<PinnedDependencyInfo> {
        todo!()
    }
}
