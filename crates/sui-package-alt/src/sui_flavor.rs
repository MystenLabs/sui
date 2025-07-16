// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_package_alt::{
    dependency::{self, DependencySet, PinnedDependencyInfo},
    errors::PackageResult,
    flavor::MoveFlavor,
    schema::{EnvironmentID, EnvironmentName, PackageName, ReplacementDependency},
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct SuiFlavor;

impl MoveFlavor for SuiFlavor {
    fn name() -> String {
        "sui".to_string()
    }

    type PublishedMetadata = (); // TODO

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn default_environments() -> BTreeMap<EnvironmentName, EnvironmentID> {
        todo!()
    }

    fn implicit_deps(
        &self,
        environment: EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        todo!()
    }
}
