use std::collections::BTreeMap;

use crate::shared::types::{PackageStorageId, RuntimePackageId};

use move_binary_format::CompiledModule;
use move_core_types::{
    language_storage::ModuleId,
    resolver::{SerializedPackage, TypeOrigin},
};

#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub(crate) runtime_id: RuntimePackageId,
    pub(crate) storage_id: PackageStorageId,
    pub(crate) modules: BTreeMap<ModuleId, CompiledModule>,
    pub(crate) type_origin_table: Vec<TypeOrigin>,
    pub(crate) linkage_table: BTreeMap<RuntimePackageId, PackageStorageId>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Package {
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

    #[allow(dead_code)]
    pub fn into_modules(self) -> Vec<CompiledModule> {
        self.modules.into_values().collect()
    }

    #[allow(dead_code)]
    pub fn as_modules(&self) -> impl IntoIterator<Item = &CompiledModule> {
        self.modules.values()
    }
}
