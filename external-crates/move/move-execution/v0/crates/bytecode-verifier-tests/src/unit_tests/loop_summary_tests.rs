// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{control_flow_graph::VMControlFlowGraph, file_format::Bytecode};
use move_bytecode_verifier::loop_summary::{LoopPartition, LoopSummary};

macro_rules! assert_node {
    ( $summary:ident, $node:expr ; $block:expr, $preds:expr, $descs:expr, $backs:expr ) => {
        let (s, n) = (&$summary, $node);
        assert_eq!(s.block(n), $block, "Block");

        let descs = $descs;
        for d in descs {
            assert!(s.is_descendant(n, *d), "{:?} -> {:?}", n, d)
        }

        assert_eq!(s.pred_edges(n), $preds, "Predecessor Edges");
        assert_eq!(s.back_edges(n), $backs, "Back Edges");
    };
}

#[test]
fn linear_summary() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ Nop,
            /*        */ Branch(2),
            /* B2, L1 */ Nop,
            /*        */ Branch(4),
            /* B4, L2 */ Ret,
        ]))
    };

    let n: Vec<_> = summary.preorder().collect();

    assert_eq!(n.len(), 3);

    assert_node!(
        summary, n[0];
        /* block */ 0,
        /* preds */ &[],
        /* descs */ &[n[1], n[2]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[1];
        /* block */ 2,
        /* preds */ &[n[0]],
        /* descs */ &[n[2]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[2];
        /* block */ 4,
        /* preds */ &[n[1]],
        /* descs */ &[],
        /* backs */ &[]
    );
}

#[test]
fn non_loop_back_branch_summary() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ Nop,
            /*        */ Branch(3),
            /* B2, L2 */ Ret,
            /* B3, L1 */ Branch(2),
        ]))
    };

    let n: Vec<_> = summary.preorder().collect();

    assert_eq!(n.len(), 3);

    assert_node!(
        summary, n[0];
        /* block */ 0,
        /* preds */ &[],
        /* descs */ &[n[1], n[2]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[1];
        /* block */ 3,
        /* preds */ &[n[0]],
        /* descs */ &[n[2]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[2];
        /* block */ 2,
        /* preds */ &[n[1]],
        /* descs */ &[],
        /* backs */ &[]
    );
}

#[test]
fn branching_summary() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ LdTrue,
            /*        */ BrTrue(3),
            /* B2, L2 */ Nop,
            /* B3, L1 */ Ret,
        ]))
    };

    let n: Vec<_> = summary.preorder().collect();

    assert_eq!(n.len(), 3);

    assert_node!(
        summary, n[0];
        /* block */ 0,
        /* preds */ &[],
        /* descs */ &[n[1], n[2]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[1];
        /* block */ 3,
        /* preds */ &[n[0], n[2]],
        /* descs */ &[],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[2];
        /* block */ 2,
        /* preds */ &[n[0]],
        /* descs */ &[],
        /* backs */ &[]
    );

    // Although L2 -> L1 is an edge in the CFG, it's not an edge in the DFST, so L2 is said to have
    // no descendants.
    assert!(!summary.is_descendant(n[2], n[1]));
}

#[test]
fn looping_summary() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ LdTrue,
            /*        */ BrTrue(4),
            /* B2, L2 */ Nop,
            /*        */ Branch(0),
            /* B4, L1 */ Ret,
        ]))
    };

    let n: Vec<_> = summary.preorder().collect();

    assert_eq!(n.len(), 3);

    assert_node!(
        summary, n[0];
        /* block */ 0,
        /* preds */ &[],
        /* descs */ &[n[1], n[2]],
        /* backs */ &[n[2]]
    );

    assert_node!(
        summary, n[1];
        /* block */ 4,
        /* preds */ &[n[0]],
        /* descs */ &[],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[2];
        /* block */ 2,
        /* preds */ &[n[0]],
        /* descs */ &[],
        /* backs */ &[]
    );
}

#[test]
fn branches_in_loops_summary() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ LdTrue,
            /*        */ BrTrue(3),
            /* B2, L3 */ Nop,
            /* B3, L1 */ LdFalse,
            /*        */ BrFalse(0),
            /* B5, L2 */ Ret,
        ]))
    };

    let n: Vec<_> = summary.preorder().collect();

    assert_eq!(n.len(), 4);

    assert_node!(
        summary, n[0];
        /* block */ 0,
        /* preds */ &[],
        /* descs */ &[n[1], n[2], n[3]],
        /* backs */ &[n[1]]
    );

    assert_node!(
        summary, n[1];
        /* block */ 3,
        /* preds */ &[n[0], n[3]],
        /* descs */ &[n[2]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[2];
        /* block */ 5,
        /* preds */ &[n[1]],
        /* descs */ &[],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[3];
        /* block */ 2,
        /* preds */ &[n[0]],
        /* descs */ &[],
        /* backs */ &[]
    );
}

#[test]
fn loops_in_branches_summary() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0,  L0 */ LdTrue,
            /*         */ BrTrue(8),
            /* B2,  L5   */ Nop,
            /* B3,  L6     */ LdFalse,
            /*             */ BrFalse(3),
            /* B5,  L7   */ LdTrue,
            /*           */ BrTrue(2),
            /* B7,  L8 */ Branch(13),
            /* B8,  L1   */ Nop,
            /* B9,  L2   */ LdTrue,
            /*           */ BrTrue(8),
            /* B11, L3   */ LdFalse,
            /*           */ BrFalse(9),
            /* B13, L4 */ Ret,
        ]))
    };

    let n: Vec<_> = summary.preorder().collect();

    assert_eq!(n.len(), 9);

    assert_node!(
        summary, n[0];
        /* block */ 0,
        /* preds */ &[],
        /* descs */ &[n[1], n[2], n[3], n[4], n[5], n[6], n[7], n[8]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[1];
        /* block */ 8,
        /* preds */ &[n[0]],
        /* descs */ &[n[2], n[3], n[4]],
        /* backs */ &[n[2]]
    );

    assert_node!(
        summary, n[2];
        /* block */ 9,
        /* preds */ &[n[1]],
        /* descs */ &[n[3], n[4]],
        /* backs */ &[n[3]]
    );

    assert_node!(
        summary, n[3];
        /* block */ 11,
        /* preds */ &[n[2]],
        /* descs */ &[n[4]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[4];
        /* block */ 13,
        /* preds */ &[n[3], n[8]],
        /* descs */ &[],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[5];
        /* block */ 2,
        /* preds */ &[n[0]],
        /* descs */ &[n[6], n[7], n[8]],
        /* backs */ &[n[7]]
    );

    assert_node!(
        summary, n[6];
        /* block */ 3,
        /* preds */ &[n[5]],
        /* descs */ &[n[7], n[8]],
        /* backs */ &[n[6]]
    );

    assert_node!(
        summary, n[7];
        /* block */ 5,
        /* preds */ &[n[6]],
        /* descs */ &[n[8]],
        /* backs */ &[]
    );

    assert_node!(
        summary, n[8];
        /* block */ 7,
        /* preds */ &[n[7]],
        /* descs */ &[],
        /* backs */ &[]
    );
}

#[test]
fn loop_collapsing() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ LdTrue,
            /*        */ BrTrue(4),
            /* B2, L2 */ Nop,
            /*        */ Branch(0),
            /* B4, L1 */ Ret,
        ]))
    };

    let mut partition = LoopPartition::new(&summary);
    let n: Vec<_> = summary.preorder().collect();

    for id in &n {
        assert_eq!(*id, partition.containing_loop(*id), "Self-parent {:?}", id);
    }

    assert_eq!(partition.collapse_loop(n[0], &[n[2]].into()), 1);
    assert_eq!(partition.containing_loop(n[0]), n[0]);
    assert_eq!(partition.containing_loop(n[1]), n[1]);
    assert_eq!(partition.containing_loop(n[2]), n[0]);
}

#[test]
fn nested_loop_collapsing() {
    let summary = {
        use Bytecode::*;
        LoopSummary::new(&VMControlFlowGraph::new(&[
            /* B0, L0 */ Nop,
            /* B1, L1   */ LdTrue,
            /*          */ BrTrue(1),
            /* B3, L2 */ LdFalse,
            /*        */ BrFalse(0),
            /* B5, L3 */ LdTrue,
            /*        */ BrTrue(0),
            /* B7, L4 */ Ret,
        ]))
    };

    let mut partition = LoopPartition::new(&summary);
    let n: Vec<_> = summary.preorder().collect();

    // Self-loop is a special case -- its depth should still be bumped.
    assert_eq!(partition.collapse_loop(n[1], &[].into()), 1);
    assert_eq!(partition.collapse_loop(n[0], &[n[1], n[2]].into()), 2);
    assert_eq!(partition.collapse_loop(n[0], &[n[0], n[3]].into()), 3);
}
