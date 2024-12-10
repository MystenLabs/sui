// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::shared::types::{PackageStorageId, RuntimePackageId};

use move_binary_format::CompiledModule;
use move_core_types::{language_storage::ModuleId, resolver::TypeOrigin};

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// A deserialized, internally-verified package.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Package {
    pub(crate) runtime_id: RuntimePackageId,
    pub(crate) storage_id: PackageStorageId,
    pub(crate) modules: BTreeMap<ModuleId, Module>,
    pub(crate) type_origin_table: Vec<TypeOrigin>,
    pub(crate) linkage_table: BTreeMap<RuntimePackageId, PackageStorageId>,
}

/// A deserialized, internally-verified module.
#[derive(Debug, Clone)]
pub struct Module {
    // This field is intentionally private, as we should not allow creation of verified Packages without
    // going through the verifier.
    pub(crate) value: CompiledModule,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Package {
    pub fn into_modules(self) -> Vec<Module> {
        self.modules.into_values().collect()
    }

    pub fn as_modules(&self) -> impl IntoIterator<Item = &Module> {
        self.modules.values()
    }
}

impl Module {
    pub fn to_compiled_module(&self) -> CompiledModule {
        self.value.clone()
    }
}
