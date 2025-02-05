// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;
pub mod optimization;

use crate::{
    jit::execution::ast::Package,
    jit::optimization::{optimize, to_optimized_form},
    natives::functions::NativeFunctions,
    shared::linkage_context::LinkageContext,
    validation::verification,
};
use move_binary_format::errors::PartialVMResult;
use move_vm_config::runtime::VMConfig;

pub fn translate_package(
    vm_config: &VMConfig,
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    let opt_package = if vm_config.optimize_bytecode {
        optimize(loaded_package)
    } else {
        to_optimized_form(loaded_package)
    };
    execution::translate::package(natives, link_context, opt_package)
}
