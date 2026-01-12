// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::validation::verification::ast as input;

pub mod ast;
pub mod peephole;
pub mod translate;

pub fn to_optimized_form(input: input::Package) -> ast::Package {
    translate::package(input)
}

pub fn optimize(input: input::Package) -> ast::Package {
    // First translate to optimization AST
    let package = translate::package(input);
    // Then apply peephole optimizations (super-instruction fusion)
    peephole::optimize_package(package)
}
