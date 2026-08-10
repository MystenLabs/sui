// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod remove_fallthrough_jumps;
mod remove_nop_store;
mod remove_unused_locals;
mod remove_write_back;

use crate::{parser::ast::FunctionName, shared::macro_frames::ExpansionColor};
use move_ir_types::ast::{self as IR};
use std::collections::{BTreeSet, HashMap};

/// Per-block color data maintained in parallel with BytecodeBlocks.
/// `block_colors[i][j]` is the macro expansion color for instruction `j`
/// in block `i`.
pub(crate) type BlockColors = Vec<Vec<ExpansionColor>>;

pub type Optimization = fn(
    &FunctionName,
    &BTreeSet<IR::BlockLabel_>,
    &mut Vec<(IR::Var, IR::Type)>,
    &mut IR::BytecodeBlocks,
    &mut BlockColors,
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
    colors: &mut BlockColors,
) {
    debug_assert_color_alignment(blocks, colors);
    let mut count = 0;
    for optimization in OPTIMIZATIONS.iter().cycle() {
        // if we have fully cycled through the list of optimizations without a change,
        // it is safe to stop
        if count >= OPTIMIZATIONS.len() {
            debug_assert_eq!(count, OPTIMIZATIONS.len());
            break;
        }

        // reset the count if something has changed
        debug_assert_color_alignment(blocks, colors);
        if optimization(f, loop_heads, locals, blocks, colors) {
            count = 0
        } else {
            count += 1
        }
        debug_assert_color_alignment(blocks, colors);
    }
}

/// Verifies that macro-color metadata lines up with bytecode instructions.
/// Macro colors are stored in a parallel per-block structure mirroring bytecode
/// instruction storage. Optimizations mutate both structures, so their shape
/// must remain identical: one color entry per instruction in each block.
fn debug_assert_color_alignment(blocks: &IR::BytecodeBlocks, colors: &BlockColors) {
    debug_assert!(
        blocks.len() == colors.len(),
        "color metadata block count mismatch: bytecode has {} blocks, colors has {} blocks",
        blocks.len(),
        colors.len(),
    );
    for ((label, block), block_colors) in blocks.iter().zip(colors.iter()) {
        debug_assert!(
            block.len() == block_colors.len(),
            "color metadata length mismatch in block {:?}",
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
