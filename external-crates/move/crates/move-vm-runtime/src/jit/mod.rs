// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;
pub mod optimization;

use crate::{
    jit::execution::ast::Package, natives::functions::NativeFunctions,
    shared::linkage_context::LinkageContext, validation::verification,
};
use move_binary_format::errors::PartialVMResult;

pub fn translate_package(
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    execution::translate::package(natives, link_context, loaded_package)
}
