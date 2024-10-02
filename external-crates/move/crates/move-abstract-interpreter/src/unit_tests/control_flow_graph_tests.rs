// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;

use crate::control_flow_graph::{BlockId, ControlFlowGraph, VMControlFlowGraph};

use move_binary_format::file_format::{
    Bytecode, EnumDefinitionIndex, JumpTableInner, VariantJumpTable, VariantJumpTableIndex,
};

#[test]
fn traversal_no_loops() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(
            &[
                /* L0 */ LdTrue,
                /*    */ BrTrue(3),
                /* L2 */ Branch(3),
                /* L3 */ Ret,
            ],
            &[],
        )
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 3);
    assert_eq!(traversal(&cfg), vec![0, 2, 3]);
}

#[test]
fn traversal_no_loops_with_switch() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(
            &[
                /* L0 */ VariantSwitch(VariantJumpTableIndex::new(0)),
                /*    */ Nop,
                /*    */ Nop,
                /*    */ Nop,
                /*    */ Nop,
                /*    */ Nop,
                /*    */ BrTrue(8),
                /* L2 */ Branch(8),
                /* L3 */ Ret,
            ],
            &[VariantJumpTable {
                // Doesn't matter
                head_enum: EnumDefinitionIndex::new(0),
                jump_table: JumpTableInner::Full(vec![1, 8, 2, 4]),
            }],
        )
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 6);
    assert_eq!(dbg!(traversal(&cfg)), vec![0, 1, 2, 4, 7, 8]);
}

#[test]
fn traversal_loops() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(
            &[
                /* L0: Outer head     */ LdTrue,
                /*     Outer break    */ BrTrue(6),
                /* L2: Inner head     */ LdTrue,
                /*     Inner break    */ BrTrue(5),
                /* L4: Inner continue */ Branch(2),
                /*     Outer continue */ Branch(0),
                /* L6:                */ Ret,
            ],
            &[],
        )
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 5);
    assert_eq!(traversal(&cfg), vec![0, 2, 4, 5, 6]);
}

#[test]
fn traversal_loops_with_switch() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(
            &[
                /* L0: Outer head     */ LdTrue,
                /*     Outer break    */ BrTrue(4),
                /* L2: Inner head     */ VariantSwitch(VariantJumpTableIndex::new(0)),
                /*     Outer continue */ Branch(0),
                /* L6:                */ Ret,
            ],
            &[VariantJumpTable {
                // Doesn't matter
                head_enum: EnumDefinitionIndex::new(0),
                jump_table: JumpTableInner::Full(vec![
                    /* Inner break */ 3, /* Inner continue */ 2,
                ]),
            }],
        )
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 4);
    assert_eq!(traversal(&cfg), vec![0, 2, 3, 4]);
}

#[test]
fn traversal_non_loop_back_branch() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(
            &[
                /* L0 */ Branch(2),
                /* L1 */ Ret,
                /* L2 */ Branch(1),
            ],
            &[],
        )
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 3);
    assert_eq!(traversal(&cfg), vec![0, 2, 1]);
}

#[test]
fn traversal_non_loop_back_branch_variant_switch() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(
            &[
                /* L0 */ VariantSwitch(VariantJumpTableIndex::new(0)),
                /* L1 */ Ret,
                /* L2 */ Branch(1),
            ],
            &[VariantJumpTable {
                // Doesn't matter
                head_enum: EnumDefinitionIndex::new(0),
                jump_table: JumpTableInner::Full(vec![2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2]),
            }],
        )
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 3);
    assert_eq!(traversal(&cfg), vec![0, 2, 1]);
}

#[test]
fn out_of_order_blocks_variant_switch() {
    const PERMUTATION_BOUND: usize = 2000;

    let blocks = (0..=127)
        .map(|i| {
            (
                i,
                vec![
                    Bytecode::Pop,      // Pop the value from the variant switch
                    Bytecode::LdU16(i), // Ld the number so we can track what block this is canonically
                    Bytecode::Pop,      // Then pop it
                    Bytecode::Ret,      // Then ret
                ],
            )
        })
        .collect::<Vec<_>>();

    let block_len = blocks.last().unwrap().1.len() as u16;

    let (canonical_blocks, canonical_traversal) = {
        let jump_table =
            JumpTableInner::Full(blocks.iter().map(|(i, _)| 1 + *i * block_len).collect());
        let mut start_block = vec![Bytecode::VariantSwitch(VariantJumpTableIndex::new(0))];
        start_block.extend(blocks.clone().into_iter().flat_map(|(_, block)| block));

        let cfg = VMControlFlowGraph::new(
            &start_block,
            &[VariantJumpTable {
                // Doesn't matter
                head_enum: EnumDefinitionIndex::new(0),
                jump_table,
            }],
        );

        cfg.display();
        (cfg.num_blocks(), traversal(&cfg))
    };

    assert_eq!(canonical_blocks, 129);
    assert_eq!(canonical_traversal.len(), 129);

    for permutation in blocks.into_iter().permutations(128).take(PERMUTATION_BOUND) {
        // orig index => new_index
        // identity permutation == perm[i] == i;
        let mut perm = vec![];
        let mut blocks = vec![Bytecode::VariantSwitch(VariantJumpTableIndex::new(0))];
        for (index, mut block) in permutation.into_iter() {
            perm.push(index);
            blocks.append(&mut block);
        }

        let jump_table = JumpTableInner::Full(perm.iter().map(|i| 1 + *i * block_len).collect());

        let cfg = VMControlFlowGraph::new(
            &blocks,
            &[VariantJumpTable {
                // Doesn't matter
                head_enum: EnumDefinitionIndex::new(0),
                jump_table,
            }],
        );
        assert_eq!(
            cfg.num_blocks(),
            canonical_blocks,
            "num blocks differ: Permutation: {:?}",
            perm
        );
        assert_eq!(
            traversal(&cfg),
            canonical_traversal,
            "traversal differs: Permutation: {:?}",
            perm
        );
    }
}

/// Return a vector containing the `BlockId`s from `cfg` in the order suggested by successively
/// calling `ControlFlowGraph::next_block` starting from the entry block.
fn traversal(cfg: &dyn ControlFlowGraph) -> Vec<BlockId> {
    let mut order = Vec::with_capacity(cfg.num_blocks() as usize);
    let mut next = Some(cfg.entry_block_id());

    while let Some(block) = next {
        order.push(block);
        next = cfg.next_block(block);
    }

    order
}
