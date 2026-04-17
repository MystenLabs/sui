// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::validation::verification::ast as input;
use move_binary_format::errors::PartialVMResult;

pub mod ast;
#[cfg(any(feature = "bb_regalloc", test))]
pub mod bb_regalloc;
pub mod translate;

pub fn to_optimized_form(input: input::Package) -> PartialVMResult<ast::Package> {
    translate::package(input)
}

/// For future usage, this will be the entry point for optimizations. For now this is uncalled.
#[allow(dead_code)]
pub fn optimize(input: input::Package) -> PartialVMResult<ast::Package> {
    let pkg = translate::package(input)?;
    #[cfg(feature = "bb_regalloc")]
    let pkg = bb_regalloc::optimize_package(pkg);
    Ok(pkg)
}
