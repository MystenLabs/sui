// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;
pub mod optimization;

use crate::{
    jit::execution::ast::Package, jit::optimization::optimize, natives::functions::NativeFunctions,
    shared::linkage_context::LinkageContext, validation::verification,
};
use move_binary_format::errors::PartialVMResult;

pub fn translate_package(
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    // TODO: Take the VM config to toggle optimizations
    let opt_package = optimize(loaded_package);
    execution::translate::package(natives, link_context, opt_package)
}
