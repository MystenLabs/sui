// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    control_flow_graph::{BlockId, ControlFlowGraph, VMControlFlowGraph},
    file_format::Bytecode,
};

#[test]
fn traversal_no_loops() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(&[
            /* L0 */ LdTrue,
            /*    */ BrTrue(3),
            /* L2 */ Branch(3),
            /* L3 */ Ret,
        ])
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 3);
    assert_eq!(traversal(&cfg), vec![0, 2, 3]);
}

#[test]
fn traversal_loops() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(&[
            /* L0: Outer head     */ LdTrue,
            /*     Outer break    */ BrTrue(6),
            /* L2: Inner head     */ LdTrue,
            /*     Inner break    */ BrTrue(5),
            /* L4: Inner continue */ Branch(2),
            /*     Outer continue */ Branch(0),
            /* L6:                */ Ret,
        ])
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 5);
    assert_eq!(traversal(&cfg), vec![0, 2, 4, 5, 6]);
}

#[test]
fn traversal_non_loop_back_branch() {
    let cfg = {
        use Bytecode::*;
        VMControlFlowGraph::new(&[
            /* L0 */ Branch(2),
            /* L1 */ Ret,
            /* L2 */ Branch(1),
        ])
    };

    cfg.display();
    assert_eq!(cfg.num_blocks(), 3);
    assert_eq!(traversal(&cfg), vec![0, 2, 1]);
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
