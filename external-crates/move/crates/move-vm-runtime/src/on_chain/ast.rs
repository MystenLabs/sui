// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_core_types::{account_address::AccountAddress, language_storage::ModuleId};
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
    pub(crate) runtime_id: RuntimePackageId,
    pub(crate) modules: BTreeMap<ModuleId, CompiledModule>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl DeserializedPackage {
    pub fn new(runtime_id: RuntimePackageId, modules: Vec<CompiledModule>) -> Self {
        Self {
            runtime_id,
            modules: modules.into_iter().map(|m| (m.self_id(), m)).collect(),
        }
    }
    pub fn into_modules(self) -> Vec<CompiledModule> {
        self.modules.into_values().collect()
    }

    pub fn as_modules(&self) -> impl IntoIterator<Item = &CompiledModule> {
        self.modules.values()
    }
}
