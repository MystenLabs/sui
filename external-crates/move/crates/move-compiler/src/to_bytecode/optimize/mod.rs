// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod remove_fallthrough_jumps;
mod remove_nop_store;
mod remove_unused_locals;
mod remove_write_back;

use crate::{cfgir::ast::SyntaxInfo, parser::ast::FunctionName};
use move_ir_types::ast::{self as IR};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

/// Per-block syntactic context maintained in parallel with BytecodeBlocks.
/// `block_info[i][j]` is the syntactic context (macro expansion chain) for
/// instruction `j` in block `i`.
pub(crate) type BlockSyntaxInfo = Vec<Vec<Option<Arc<SyntaxInfo>>>>;

pub type Optimization = fn(
    &FunctionName,
    &BTreeSet<IR::BlockLabel_>,
    &mut Vec<(IR::Var, IR::Type)>,
    &mut IR::BytecodeBlocks,
    &mut BlockSyntaxInfo,
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
    block_info: &mut BlockSyntaxInfo,
) {
    debug_assert_alignment(blocks, block_info);
    let mut count = 0;
    for optimization in OPTIMIZATIONS.iter().cycle() {
        // if we have fully cycled through the list of optimizations without a change,
        // it is safe to stop
        if count >= OPTIMIZATIONS.len() {
            debug_assert_eq!(count, OPTIMIZATIONS.len());
            break;
        }

        // reset the count if something has changed
        if optimization(f, loop_heads, locals, blocks, block_info) {
            count = 0
        } else {
            count += 1
        }
        debug_assert_alignment(blocks, block_info);
    }
}

/// Verifies that the syntactic-context metadata lines up with bytecode
/// instructions. The metadata is stored in a parallel per-block structure
/// mirroring bytecode instruction storage. Optimizations mutate both
/// structures, so their shape must remain identical: one entry per
/// instruction in each block.
fn debug_assert_alignment(blocks: &IR::BytecodeBlocks, block_info: &BlockSyntaxInfo) {
    debug_assert!(
        blocks.len() == block_info.len(),
        "syntax info block count mismatch: bytecode has {} blocks, info has {} blocks",
        blocks.len(),
        block_info.len(),
    );
    for ((label, block), infos) in blocks.iter().zip(block_info.iter()) {
        debug_assert!(
            block.len() == infos.len(),
            "syntax info length mismatch in block {:?}",
            label,
        );
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
