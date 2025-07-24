// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast as Out,
    structuring::{
        ast::{self as D, Label},
        graph::Graph,
    },
};

use move_stackless_bytecode_2::stackless::ast as S;
use move_symbol_pool::Symbol;

use petgraph::{graph::NodeIndex, visit::DfsPostOrder};

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Module
// -------------------------------------------------------------------------------------------------

pub(crate) fn module(module: S::Module) -> Out::Module {
    let S::Module { name, functions } = module;

    let functions = functions
        .into_iter()
        .map(|(name, fun)| (name, function(fun)))
        .collect();

    Out::Module { name, functions }
}

// -------------------------------------------------------------------------------------------------
// Function
// -------------------------------------------------------------------------------------------------

fn function(fun: S::Function) -> Out::Function {
    let (name, terms, input, entry) = make_input(fun);
    let structured = crate::structuring::structure(input, entry);
    let code = generate_output(terms, structured);

    Out::Function { name, code }
}

fn make_input(
    fun: S::Function,
) -> (
    Symbol,
    BTreeMap<D::Label, D::Block>,
    BTreeMap<D::Label, D::Input>,
    D::Label,
) {
    let S::Function {
        name,
        entry_label,
        basic_blocks,
    } = fun;

    todo!()
}

fn generate_output(
    terms: BTreeMap<D::Label, D::Block>,
    structured: D::Structured,
) -> crate::ast::Exp {
    todo!()
}
