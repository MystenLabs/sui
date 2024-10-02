// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod remove_fallthrough_jumps;
mod remove_nop_store;
mod remove_unused_locals;
mod remove_write_back;

use crate::parser::ast::FunctionName;
use move_ir_types::ast::{self as IR};
use std::collections::{BTreeSet, HashMap};

pub type Optimization = fn(
    &FunctionName,
    &BTreeSet<IR::BlockLabel_>,
    &mut Vec<(IR::Var, IR::Type)>,
    &mut IR::BytecodeBlocks,
) -> bool;

const OPTIMIZATIONS: &[Optimization] = &[
    remove_fallthrough_jumps::optimize,
    remove_nop_store::optimize,
    remove_write_back::optimize,
    remove_unused_locals::optimize,
];

pub(crate) fn code(
    f: &FunctionName,
    loop_heads: &BTreeSet<IR::BlockLabel_>,
    locals: &mut Vec<(IR::Var, IR::Type)>,
    blocks: &mut IR::BytecodeBlocks,
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
        if optimization(f, loop_heads, locals, blocks) {
            count = 0
        } else {
            count += 1
        }
    }
}

fn remap_labels(blocks: &mut IR::BytecodeBlocks, map: &HashMap<IR::BlockLabel_, IR::BlockLabel_>) {
    use IR::Bytecode_ as B;
    for (_, block) in blocks {
        for instr in block {
            match &mut instr.value {
                B::Branch(lbl) | B::BrTrue(lbl) | B::BrFalse(lbl) => {
                    *lbl = map[lbl].clone();
                }
                _ => (),
            }
        }
    }
}
