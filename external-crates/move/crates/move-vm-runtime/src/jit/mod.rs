// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;

use crate::{
    cache::type_cache::CrossVersionPackageCache,
    jit::execution::ast::Package,
    natives::functions::NativeFunctions,
    shared::{linkage_context::LinkageContext, types::PackageStorageId},
    validation::verification,
};
use move_binary_format::errors::PartialVMResult;
use parking_lot::RwLock;
use std::sync::Arc;

pub fn translate_package(
    package_cache: Arc<RwLock<CrossVersionPackageCache>>,
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
    execution::translate::package(
        package_cache,
        natives,
        link_context,
        storage_id,
        runtime_id,
        modules,
    )
}
