// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::validation::verification::ast as input;
use move_binary_format::errors::PartialVMResult;

pub mod ast;
pub mod insert_charge;
pub mod translate;

pub fn to_optimized_form(input: input::Package) -> PartialVMResult<ast::Package> {
    let pkg = translate::package(input)?;
    optimize(pkg)
}

/// Entry point for optimization passes. Currently applies the Charge-insertion
/// pass which hoists per-instruction gas costs out of the interpreter loop.
pub fn optimize(pkg: ast::Package) -> PartialVMResult<ast::Package> {
    insert_charge::pass(pkg)
}
