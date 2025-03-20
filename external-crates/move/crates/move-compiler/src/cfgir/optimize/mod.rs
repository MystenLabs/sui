// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod constant_fold;
mod eliminate_locals;
mod forwarding_jumps;
mod inline_blocks;
mod simplify_jumps;

use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;

use crate::{
    cfgir::cfg::MutForwardCFG,
    diagnostics::DiagnosticReporter,
    editions::FeatureGate,
    expansion::ast::Mutability,
    hlir::ast::*,
    parser::ast::ConstantName,
    shared::{unique_map::UniqueMap, CompilationEnv},
};

pub type Optimization = fn(
    &DiagnosticReporter,
    &FunctionSignature,
    &UniqueMap<Var, (Mutability, SingleType)>,
    &UniqueMap<ConstantName, Value>,
    &mut MutForwardCFG,
) -> bool;

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
pub fn optimize(
    env: &CompilationEnv,
    reporter: &DiagnosticReporter,
    package: Option<Symbol>,
    signature: &FunctionSignature,
    locals: &UniqueMap<Var, (Mutability, SingleType)>,
    constants: &UniqueMap<ConstantName, Value>,
    cfg: &mut MutForwardCFG,
) {
    let mut count = 0;
    let optimizations = if env.supports_feature(package, FeatureGate::Move2024Optimizations) {
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
        if optimization(reporter, signature, locals, constants, cfg) {
            count = 0
        } else {
            count += 1
        }
    }
}
