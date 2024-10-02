// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod runtime;

use crate::{
    cache::type_cache::TypeCache,
    jit::runtime::ast::Package,
    natives::functions::NativeFunctions,
    shared::{linkage_context::LinkageContext, types::PackageStorageId},
    validation::verification,
};

use move_binary_format::errors::PartialVMResult;

use parking_lot::RwLock;

pub fn translate_package(
    type_cache: &RwLock<TypeCache>,
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    storage_id: PackageStorageId,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    let runtime_id = loaded_package.runtime_id;
    let modules = loaded_package
        .into_modules()
        .into_iter()
        .map(|module| module.value)
        .collect();
    // FIXME: change this signature to be against a verified module, too.
    runtime::translate::package(
        type_cache,
        natives,
        link_context,
        storage_id,
        runtime_id,
        modules,
    )
}
