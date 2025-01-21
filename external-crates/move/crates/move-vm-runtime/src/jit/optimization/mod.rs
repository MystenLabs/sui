// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::dbg_println;

pub mod ast;
pub mod optimizations;
pub mod translate;

pub fn to_optimized_form(input: crate::validation::verification::ast::Package) -> ast::Package {
    translate::package(input)
}

pub fn optimize(input: crate::validation::verification::ast::Package) -> ast::Package {
    let mut opt = translate::package(input);
    dbg_println!(flag: optimizer, "Blocks: {:#?}", opt);
    optimizations::dead_code_elim::package(&mut opt);
    dbg_println!(flag: optimizer, "Dead Code elim: {:#?}", opt);
    optimizations::inline_immediate_constants::package(&mut opt);
    dbg_println!(flag: optimizer, "Immediate Constants Inlined: {:#?}", opt);
    opt
}
