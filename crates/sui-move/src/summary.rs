// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::summary;
use move_core_types::account_address::AccountAddress;
use move_package::{BuildConfig, resolution::resolution_graph::ResolvedGraph};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use sui_types::{
    base_types::ObjectID,
    move_package::{TypeOrigin, UpgradeInfo},
};

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
        self.summary.execute(
            path,
            build_config,
            Some(&sui_package_metadata),
            Some(Self::derive_ids),
        )
    }

    fn derive_ids(resolved_graph: &mut ResolvedGraph) -> anyhow::Result<()> {
        let root_pkg = resolved_graph
            .package_table
            .get_mut(&resolved_graph.root_package())
            .unwrap();
        let in_use_addrs = root_pkg
            .resolved_table
            .values()
            .cloned()
            .collect::<BTreeSet<_>>();
        // Assign a unique address to each named address in the package deterministically.
        // So start at 42 and increment until we find an address that is not in use (this should be
        // always immediate in expectation).
        let mut i = 42;
        for (_, old_addr) in root_pkg.resolved_table.iter_mut() {
            // If the named address is unset (0x0) then derive unique address for it.
            if *old_addr == AccountAddress::ZERO {
                loop {
                    let random_addr = AccountAddress::from_suffix(i);
                    i += 1;
                    if !in_use_addrs.contains(&random_addr) {
                        *old_addr = random_addr;
                        break;
                    }
                }
            }
        }

        let new_address_assignment = root_pkg.resolved_table.clone();

        // NB: The root package is the has a global resolution of all named addresses so we are
        // guaranteed to have all addresses in the `new_address_mapping`. If we can't find it
        // that's an error.
        let root_renaming = resolved_graph.root_renaming();
        for (pkg_name, pkg) in resolved_graph.package_table.iter_mut() {
            let package_root_renaming =
                root_renaming.get(pkg_name).expect("Will always be present");
            for (local_name, old_addr) in pkg.resolved_table.iter_mut() {
                let root_name = package_root_renaming
                    .get(local_name)
                    .expect("Root renaming entry is present for every in-scope address");
                let Some(new_addr) = new_address_assignment.get(root_name) else {
                    anyhow::bail!(
                        "IPE: Address {root_name} (local name = {local_name}) not found in new address mapping -- this shouldn't happen",
                    );
                };
                *old_addr = *new_addr;
            }
        }
        Ok(())
    }
}
