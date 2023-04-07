// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod constant_fold;
mod eliminate_locals;
mod inline_blocks;
mod simplify_jumps;

use crate::{cfgir::cfg::BlockCFG, hlir::ast::*, shared::unique_map::UniqueMap};

pub type Optimization = fn(&FunctionSignature, &UniqueMap<Var, SingleType>, &mut BlockCFG) -> bool;

const OPTIMIZATIONS: &[Optimization] = &[
    eliminate_locals::optimize,
    constant_fold::optimize,
    simplify_jumps::optimize,
    inline_blocks::optimize,
];

pub fn optimize(
    signature: &FunctionSignature,
    locals: &UniqueMap<Var, SingleType>,
    cfg: &mut BlockCFG,
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
        if optimization(signature, locals, cfg) {
            count = 0
        } else {
            count += 1
        }
    }
}
