use std::collections::BTreeMap;

use crate::shared::types::{DefiningTypeId, OriginalId};

use move_binary_format::CompiledModule;
use move_core_types::{
    language_storage::ModuleId,
    resolver::{SerializedPackage, TypeOrigin},
};

#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub(crate) original_id: OriginalId,
    pub(crate) version_id: DefiningTypeId,
    pub(crate) modules: BTreeMap<ModuleId, CompiledModule>,
    #[allow(dead_code)]
    pub(crate) type_origin_table: Vec<TypeOrigin>,
    #[allow(dead_code)]
    pub(crate) linkage_table: BTreeMap<OriginalId, DefiningTypeId>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Package {
    pub fn new(
        original_id: OriginalId,
        modules: Vec<CompiledModule>,
        pkg: SerializedPackage,
    ) -> Self {
        Self {
            original_id,
            modules: modules.into_iter().map(|m| (m.self_id(), m)).collect(),
            version_id: pkg.storage_id,
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
