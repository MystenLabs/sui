// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;
pub mod optimization;

use crate::{
    cache::{identifier_interner::IdentifierInterner, move_cache::Package as CachedPackage},
    jit::{execution::ast::Package, optimization::to_optimized_form},
    natives::functions::NativeFunctions,
    shared::types::OriginalId,
    validation::verification,
};
use move_binary_format::errors::PartialVMResult;
use move_vm_config::runtime::VMConfig;

use std::{collections::BTreeMap, sync::Arc};

/// Translate a package into its JIT'd execution form.
///
/// `system_packages` is the candidate set of pinned packages this translation may direct-call
/// into. The caller is responsible for filtering by the user pkg's linkage so we only
/// direct-resolve into a system pkg the user explicitly links at the pinned version.
pub fn translate_package(
    _vm_config: &VMConfig,
    interner: &IdentifierInterner,
    natives: &NativeFunctions,
    system_packages: &BTreeMap<OriginalId, Arc<CachedPackage>>,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    let opt_package = to_optimized_form(loaded_package)?;
    execution::translate::package(natives, interner, system_packages, opt_package)
}
