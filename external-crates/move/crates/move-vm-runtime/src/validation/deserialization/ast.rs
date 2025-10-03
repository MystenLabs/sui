use crate::shared::types::{DefiningTypeId, OriginalId, VersionId};
use indexmap::IndexMap;
use move_binary_format::CompiledModule;
use move_core_types::{
    language_storage::ModuleId,
    resolver::{IntraPackageName, SerializedPackage},
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub(crate) original_id: OriginalId,
    pub(crate) version_id: VersionId,
    pub(crate) modules: BTreeMap<ModuleId, CompiledModule>,
    #[allow(dead_code)]
    pub(crate) type_origin_table: IndexMap<IntraPackageName, DefiningTypeId>,
    #[allow(dead_code)]
    pub(crate) linkage_table: BTreeMap<OriginalId, VersionId>,
    pub(crate) version: u64,
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
            version_id: pkg.version_id,
            type_origin_table: pkg.type_origin_table,
            linkage_table: pkg.linkage_table,
            version: pkg.version,
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
