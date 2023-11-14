// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod constant_fold;
mod eliminate_locals;
// mod forwarding_jumps;
mod inline_blocks;
mod simplify_jumps;

use crate::{
    cfgir::cfg::MutForwardCFG, hlir::ast::*, parser::ast::ConstantName,
    shared::unique_map::UniqueMap,
};

pub type Optimization = fn(
    &FunctionSignature,
    &UniqueMap<Var, SingleType>,
    &UniqueMap<ConstantName, Value>,
    &mut MutForwardCFG,
) -> bool;

const OPTIMIZATIONS: &[Optimization] = &[
    eliminate_locals::optimize,
    constant_fold::optimize,
    // forwarding_jumps::optimize,
    simplify_jumps::optimize,
    inline_blocks::optimize,
];

pub fn optimize(
    signature: &FunctionSignature,
    locals: &UniqueMap<Var, SingleType>,
    constants: &UniqueMap<ConstantName, Value>,
    cfg: &mut MutForwardCFG,
) {
    let mut count = 0;
    for optimization in OPTIMIZATIONS.iter().cycle() {
        // if we have fully cycled through the list of optimizations without a change,
        // it is safe to stop
        if count >= OPTIMIZATIONS.len() {
            debug_assert_eq!(count, OPTIMIZATIONS.len());
            break;
        }

        // reset the count if something has changed
        if optimization(signature, locals, constants, cfg) {
            count = 0
        } else {
            count += 1
        }
    }
}
