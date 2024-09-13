// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::loader::ast::RuntimePackageId;
use move_binary_format::CompiledModule;
use move_core_types::language_storage::ModuleId;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct BinaryFormatPackage {
    pub(crate) runtime_id: RuntimePackageId,
    pub(crate) modules: BTreeMap<ModuleId, CompiledModule>,
}

impl BinaryFormatPackage {
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
