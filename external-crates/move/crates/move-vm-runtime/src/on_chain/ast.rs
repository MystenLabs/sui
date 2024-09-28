// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::ModuleId,
    resolver::{SerializedPackage, TypeOrigin},
};
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub type DefiningTypeId = AccountAddress;

/// On-chain storage ID for the package we are linking account (e.g., v0 and v1 will use different
/// Packge Storage IDs).
pub type PackageStorageId = AccountAddress;

/// Runtime ID: An ID used at runtime. This is consistent between versions (e.g., v0 and v1 will
/// use the same Runtime Package ID).
pub type RuntimePackageId = AccountAddress;

#[derive(Debug, Clone)]
pub(crate) struct DeserializedPackage {
    pub(crate) storage_id: PackageStorageId,
    pub(crate) runtime_id: RuntimePackageId,
    pub(crate) modules: BTreeMap<ModuleId, CompiledModule>,
    pub(crate) type_origin_table: Vec<TypeOrigin>,
    pub(crate) linkage_table: BTreeMap<RuntimePackageId, PackageStorageId>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl DeserializedPackage {
    pub fn new(
        runtime_id: RuntimePackageId,
        modules: Vec<CompiledModule>,
        pkg: SerializedPackage,
    ) -> Self {
        Self {
            runtime_id,
            modules: modules.into_iter().map(|m| (m.self_id(), m)).collect(),
            storage_id: pkg.storage_id,
            type_origin_table: pkg.type_origin_table,
            linkage_table: pkg.linkage_table,
        }
    }

    pub fn into_modules(self) -> Vec<CompiledModule> {
        self.modules.into_values().collect()
    }

    pub fn as_modules(&self) -> impl IntoIterator<Item = &CompiledModule> {
        self.modules.values()
    }
}
