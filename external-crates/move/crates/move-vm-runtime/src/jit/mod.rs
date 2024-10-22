// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;

use crate::{
    cache::type_cache::CrossVersionPackageCache, jit::execution::ast::Package,
    natives::functions::NativeFunctions, shared::linkage_context::LinkageContext,
    validation::verification,
};
use move_binary_format::errors::PartialVMResult;
use parking_lot::RwLock;
use std::sync::Arc;

pub fn translate_package(
    package_cache: Arc<RwLock<CrossVersionPackageCache>>,
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    execution::translate::package(package_cache, natives, link_context, loaded_package)
}
