// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_package_alt::{
    dependency::{self, DependencySet, PinnedDependencyInfo},
    errors::PackageResult,
    flavor::MoveFlavor,
    package::PackageName,
};
use serde::{Deserialize, Serialize};
use sui_sdk::types::base_types::ObjectID;

#[derive(Debug, Serialize, Deserialize)]
pub struct SuiFlavor;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SuiMetadata {
    pub upgrade_cap: Option<ObjectID>,
    pub version: Option<u64>,
}

impl MoveFlavor for SuiFlavor {
    fn name() -> String {
        "sui".to_string()
    }

    type PublishedMetadata = SuiMetadata;

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
