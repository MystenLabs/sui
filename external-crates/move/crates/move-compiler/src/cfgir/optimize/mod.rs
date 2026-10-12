// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod constant_fold;
mod context;
mod eliminate_locals;
mod forwarding_jumps;
mod inline_blocks;
mod simplify_jumps;

pub use constant_fold::report_always_erroring_operations;
pub use context::Context;

use move_proc_macros::growing_stack;

use crate::{cfgir::cfg::MutForwardCFG, editions::FeatureGate};

pub type Optimization = fn(&mut Context, &mut MutForwardCFG) -> bool;

const OPTIMIZATIONS: &[Optimization] = &[
    eliminate_locals::optimize,
    constant_fold::optimize,
    simplify_jumps::optimize,
    inline_blocks::optimize,
];

const MOVE_2024_OPTIMIZATIONS: &[Optimization] = &[
    eliminate_locals::optimize,
    constant_fold::optimize,
    forwarding_jumps::optimize,
    simplify_jumps::optimize,
    inline_blocks::optimize,
];

#[growing_stack]
pub fn optimize(context: &mut Context, cfg: &mut MutForwardCFG) {
    let mut count = 0;
    let optimizations = if context
        .env
        .supports_feature(context.package(), FeatureGate::Move2024Optimizations)
    {
        MOVE_2024_OPTIMIZATIONS
    } else {
        OPTIMIZATIONS
    };
    let opt_count = optimizations.len();
    for optimization in optimizations.iter().cycle() {
        // if we have fully cycled through the list of optimizations without a change,
        // it is safe to stop
        if count >= opt_count {
            debug_assert_eq!(count, opt_count);
            break;
        }

        // reset the count if something has changed
        if optimization(context, cfg) {
            count = 0
        } else {
            count += 1
        }
    }
}
