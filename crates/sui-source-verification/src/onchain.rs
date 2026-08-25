// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use sui_rpc_api::Client;
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;
use sui_types::object::Data;

use crate::error::Error;

/// The on-chain representation of a package: its modules (keyed by name), its original (runtime)
/// package id, and its linkage table reduced to `original id -> storage id`.
pub struct OnChainPackage {
    pub original_id: AccountAddress,
    pub modules: BTreeMap<String, CompiledModule>,
    pub linkage: BTreeMap<AccountAddress, AccountAddress>,
}

/// Fetch and decode the on-chain package at `on_chain_id`.
pub async fn fetch(client: &Client, on_chain_id: ObjectID) -> Result<OnChainPackage, Error> {
    let pkg = pkg_for_address(client, on_chain_id).await?;

    let mut modules = BTreeMap::new();
    for (name, bytes) in pkg.serialized_module_map() {
        let module = CompiledModule::deserialize_with_defaults(bytes).map_err(|_| {
            Error::OnChainModuleDeserialization {
                address: on_chain_id.into(),
                module: name.as_str().into(),
            }
        })?;
        modules.insert(name.as_str().to_string(), module);
    }

    if modules.is_empty() {
        return Err(Error::EmptyOnChainPackage(on_chain_id.into()));
    }

    let linkage = pkg
        .linkage_table()
        .iter()
        .map(|(original, info)| ((*original).into(), info.upgraded_id.into()))
        .collect();

    Ok(OnChainPackage {
        original_id: pkg.original_package_id().into(),
        modules,
        linkage,
    })
}

/// Fetch the object at `id`, requiring it to be a Move package.
pub(crate) async fn pkg_for_address(client: &Client, id: ObjectID) -> Result<MovePackage, Error> {
    let obj = client
        .clone()
        .get_object(id)
        .await
        .map_err(|e| Error::PackageReadFailure(e.to_string()))?;

    match &obj.data {
        Data::Package(pkg) => Ok(pkg.clone()),
        Data::Move(_) => Err(Error::ObjectFoundWhenPackageExpected(id)),
    }
}
