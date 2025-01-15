// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
pub mod translate;

pub fn to_optimized_form(input: crate::validation::verification::ast::Package) -> ast::Package {
    translate::package(input)
}
