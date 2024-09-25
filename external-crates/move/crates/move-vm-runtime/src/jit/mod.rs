// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod runtime;

use crate::{
    cache::{linkage_context::LinkageContext, type_cache::TypeCache},
    jit::runtime::ast::Package,
    natives::functions::NativeFunctions,
    on_chain::ast::{DeserializedPackage, PackageStorageId},
};

use move_binary_format::errors::PartialVMResult;
use move_vm_types::data_store::DataStore;

use parking_lot::RwLock;

pub fn translate_package(
    type_cache: &RwLock<TypeCache>,
    natives: &NativeFunctions,
    data_store: &impl DataStore,
    link_context: &LinkageContext,
    package_key: PackageStorageId,
    loaded_package: DeserializedPackage,
) -> PartialVMResult<Package> {
    let runtime_id = loaded_package.runtime_id;
    let modules = loaded_package.into_modules();
    runtime::translate::package(
        type_cache,
        natives,
        data_store,
        link_context,
        package_key,
        runtime_id,
        modules,
    )
}
