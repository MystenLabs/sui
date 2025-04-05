// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{dbg_println, validation::verification::ast as input};

pub mod ast;
pub mod optimizations;
pub mod translate;

pub fn to_optimized_form(input: input::Package) -> ast::Package {
    translate::package(input)
}

pub fn optimize(input: input::Package) -> ast::Package {
    let mut opt = translate::package(input);
    dbg_println!(flag: optimizer, "Blocks: {:#?}", opt);
    optimizations::dead_code_elim::package(&mut opt);
    dbg_println!(flag: optimizer, "Dead Code elim: {:#?}", opt);
    opt
}
