// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::validation::verification::ast as input;
use move_binary_format::errors::PartialVMResult;

pub mod ast;
pub mod translate;

pub fn to_optimized_form(input: input::Package) -> PartialVMResult<ast::Package> {
    translate::package(input)
}

pub fn optimize(input: input::Package) -> PartialVMResult<ast::Package> {
    // There are currently no optimizations implemented.
    translate::package(input)
}
