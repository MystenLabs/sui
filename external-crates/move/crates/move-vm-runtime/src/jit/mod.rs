// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;
pub mod optimization;

use crate::{
    cache::identifier_interner::IdentifierInterner,
    jit::{
        execution::ast::Package,
        optimization::{optimize, to_optimized_form},
    },
    natives::functions::NativeFunctions,
    validation::verification,
};
use move_binary_format::errors::PartialVMResult;
use move_vm_config::runtime::VMConfig;

pub fn translate_package(
    vm_config: &VMConfig,
    interner: &IdentifierInterner,
    natives: &NativeFunctions,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    let opt_package = if vm_config.optimize_bytecode {
        optimize(loaded_package)?
    } else {
        to_optimized_form(loaded_package)?
    };
    execution::translate::package(natives, interner, opt_package)
}
