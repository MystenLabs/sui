// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::shared::types::{DefiningTypeId, OriginalId, VersionId};

use indexmap::IndexMap;
use move_binary_format::CompiledModule;
use move_core_types::{language_storage::ModuleId, resolver::IntraPackageName};

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// A deserialized, internally-verified package.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Package {
    pub(crate) original_id: OriginalId,
    pub(crate) version_id: VersionId,
    pub(crate) modules: BTreeMap<ModuleId, Module>,
    pub(crate) type_origin_table: IndexMap<IntraPackageName, DefiningTypeId>,
    pub(crate) linkage_table: BTreeMap<OriginalId, VersionId>,
    pub(crate) version: u64,
}

/// A deserialized, internally-verified module.
#[derive(Debug, Clone)]
pub struct Module {
    // This field is intentionally crate-only, as we should not allow creation of verified Packages
    // without going through the verifier.
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

    pub fn original_id(&self) -> OriginalId {
        self.original_id
    }

    pub fn version_id(&self) -> VersionId {
        self.version_id
    }

    pub fn modules(&self) -> &BTreeMap<ModuleId, Module> {
        &self.modules
    }

    pub fn type_origin_table(&self) -> &IndexMap<IntraPackageName, DefiningTypeId> {
        &self.type_origin_table
    }

    pub fn linkage_table(&self) -> &BTreeMap<OriginalId, VersionId> {
        &self.linkage_table
    }

    pub fn version(&self) -> u64 {
        self.version
    }
}

impl Module {
    pub fn to_compiled_module(&self) -> CompiledModule {
        self.value.clone()
    }

    pub fn compiled_module(&self) -> &CompiledModule {
        &self.value
    }
}
