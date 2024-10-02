// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::parser::ast::FunctionName;
use move_ir_types::ast::{self as IR};
use std::collections::{BTreeSet, HashMap};

// Removes any "fall through jumps", i.e. this a is a jump directly to the next instruction.
// Iterates to find a fixpoint as it might create empty blocks which could create more jumps to
// clean up

#[allow(clippy::ptr_arg)]
pub fn optimize(
    _f: &FunctionName,
    loop_heads: &BTreeSet<IR::BlockLabel_>,
    _locals: &mut Vec<(IR::Var, IR::Type)>,
    blocks: &mut IR::BytecodeBlocks,
) -> bool {
    let fall_through_removed = remove_fall_through(loop_heads, blocks);
    let block_removed = remove_empty_blocks(blocks);
    fall_through_removed || block_removed
}

fn remove_fall_through(
    loop_heads: &BTreeSet<IR::BlockLabel_>,
    blocks: &mut IR::BytecodeBlocks,
) -> bool {
    use IR::Bytecode_ as B;
    let mut changed = false;
    for idx in 0..(blocks.len() - 1) {
        let next_block = &blocks[idx + 1].0.clone();
        let (lbl, block) = &mut blocks[idx];
        // Don't inline loop heads for the move-prover
        if loop_heads.contains(lbl) {
            continue;
        }

        let remove_last =
            matches!(&block.last().unwrap().value, B::Branch(lbl) if lbl == next_block);
        if remove_last {
            changed = true;
            block.pop();
        }
    }
    changed
}

fn remove_empty_blocks(blocks: &mut IR::BytecodeBlocks) -> bool {
    let mut label_map = HashMap::new();
    let mut cur_label = None;
    let mut removed = false;
    let old_blocks = std::mem::take(blocks);
    for (label, block) in old_blocks.into_iter().rev() {
        if block.is_empty() {
            removed = true;
        } else {
            cur_label = Some(label.clone());
            blocks.push((label.clone(), block))
        }
        label_map.insert(label, cur_label.clone().unwrap());
    }
    blocks.reverse();

    if removed {
        super::remap_labels(blocks, &label_map);
    }

    removed
}
