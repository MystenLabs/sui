// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::summary;
use move_core_types::account_address::AccountAddress;
use move_package::BuildConfig;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use sui_types::{
    base_types::ObjectID,
    move_package::{TypeOrigin, UpgradeInfo},
};

const SUI_DERIVE_ADDRESS_SET: &[AccountAddress] = &[AccountAddress::ZERO];

#[derive(Parser)]
#[group(id = "sui-move-summary")]
pub struct Summary {
    #[clap(flatten)]
    pub summary: summary::Summary,
    /// The object ID to summarize if `package-id` is present. The `--path` will be ignored if this field is used.
    #[clap(long = "package-id", value_parser = ObjectID::from_hex_literal)]
    pub package_id: Option<ObjectID>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct PackageSummaryMetadata {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub root_package_id: Option<ObjectID>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub root_package_original_id: Option<ObjectID>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub root_package_version: Option<u64>,
    // Mapping of original package ID to path to the package relative to the summary directory
    // root.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dependencies: Option<BTreeMap<ObjectID, PathBuf>>,
    // Mapping of original package ID to upgraded (on-chain) package ID.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub linkage: Option<BTreeMap<ObjectID, UpgradeInfo>>,
    // Mapping of original package ID to type origins for that package for each package.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub type_origins: Option<BTreeMap<ObjectID, Vec<TypeOrigin>>>,
}

impl Summary {
    pub fn execute(
        self,
        path: Option<&Path>,
        build_config: BuildConfig,
        sui_package_metadata: PackageSummaryMetadata,
    ) -> anyhow::Result<()> {
        let derive_address_set = BTreeSet::from_iter(SUI_DERIVE_ADDRESS_SET.iter().cloned());
        self.summary.execute(
            path,
            build_config,
            Some(&sui_package_metadata),
            Some(&derive_address_set),
        )
    }
}
