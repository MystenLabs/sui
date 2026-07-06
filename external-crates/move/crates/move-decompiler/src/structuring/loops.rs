// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cyclic-region structuring (NMG §IV-C multi-exit loops via §V-B).
//!
//! Per loop head identified by `Graph::loop_heads`, run `structure_loop`. It:
//!   1. Computes per-exit reach formulas from the body projection (`compute_loop_exit_formulas`).
//!   2. Structures the body via `acyclic::structure_region` under `SinkRendering::Loop`, so
//!      every exit path lands as a `Jump(ReachingExit, exit_target)`.
//!   3. Rewrites those jumps: `insert_breaks` handles the back-edge → Continue and the
//!      primary exit → Break; `vb::rewrite_owned_jumps_as_breaks` extends the treatment to
//!      every remaining exit target. After both passes the loop body is single-exit.
//!   4. Wraps as `Loop(loop_head, body)`.
//!   5. Installs `Input::Reduced(loop_head, [(target, formula)])` in the outer input map so
//!      the enclosing acyclic structuring picks up the per-exit distinction via
//!      `region::edge_condition` and produces the post-loop cascade naturally.
//!
//! No dispatch table, no `SelectorMatch`, no `__dispatch_<N>` selectors - the paper's §V-B
//! algorithm decorates the abstract loop node with condition-carrying edges and delegates
//! the surrounding structure to the acyclic recursion.

use crate::structuring::{
    StructureContext, acyclic,
    ast::{self as D},
    graph::Graph,
    predicates::Formula,
    region::SinkRendering,
    vb,
};

use petgraph::graph::NodeIndex;

use std::collections::{BTreeMap, HashSet};

pub(super) fn structure_loop(
    ctx: StructureContext<'_>,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    loop_head: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
    unstructured: &mut HashSet<u64>,
) {
    let config = ctx.config;
    if config.debug_print.structuring {
        println!("structuring loop at node {loop_head:#?}");
    }
    let (loop_nodes, succ_nodes) = graph.find_loop_nodes(loop_head);

    // NMG §V-B step 1: per-exit reach formulas from the loop-body projection.
    let exit_formulas: BTreeMap<NodeIndex, Formula> =
        vb::compute_loop_exit_formulas(input, loop_head, &loop_nodes).unwrap_or_default();

    if config.debug_print.structuring {
        println!("  loop nodes: {loop_nodes:#?}");
        println!("  successor nodes: {succ_nodes:#?}");
        for (target, f) in &exit_formulas {
            println!(
                "  exit formula for {} -> {}: {}",
                loop_head.index(),
                target.index(),
                f
            );
        }
    }

    // NMG §V-B step 2: structure the body from the projection. Inner sub-loops are already
    // `Input::Reduced` markers (dom-tree post-order DFS visited them first). Back-edges to
    // `loop_head` fire the out-of-region rule, emitting exit-jumps that `insert_breaks`
    // rewrites to `Continue`.
    let region_input: BTreeMap<D::Label, D::Input> = input
        .iter()
        .filter(|(k, _)| loop_nodes.contains(k))
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    let members: HashSet<NodeIndex> = loop_nodes
        .iter()
        .copied()
        .filter(|n| *n != loop_head)
        .collect();
    let body = acyclic::structure_region(
        structured_blocks,
        &region_input,
        loop_head,
        &members,
        SinkRendering::Loop,
        unstructured,
    )
    .expect("NMG failed on loop body");

    // NMG §V-B step 3: single-exit loop body. `insert_breaks` handles back-edges (→
    // Continue) and the smallest exit (→ Break, primary). `rewrite_owned_jumps_as_breaks`
    // extends the Break treatment to every remaining exit target. After both passes the
    // body has one exit shape and the abstract loop node's edges (installed below) carry
    // the per-exit distinction via their reach formulas.
    let primary_break: Option<NodeIndex> = succ_nodes.iter().copied().min();
    let mut body_with_breaks = insert_breaks(&loop_nodes, loop_head, primary_break, body);
    let exit_targets: Vec<NodeIndex> = succ_nodes.iter().copied().collect();
    vb::rewrite_owned_jumps_as_breaks(&mut body_with_breaks, loop_head, &exit_targets);

    // NMG §V-B step 4: wrap.
    graph.update_loop_info(loop_head);
    let loop_expr = D::Structured::Loop(loop_head, Box::new(body_with_breaks));
    structured_blocks.insert(loop_head, loop_expr);

    // NMG §V-B step 5: install the abstract node with per-edge formulas. Outer
    // `region::edge_condition` reads them; `reaching_conditions` propagates them into the
    // outer scope's items list; `recover_control_flow` factors them into `if`/`else`.
    install_reduced_marker(input, loop_head, &loop_nodes, &succ_nodes, &exit_formulas);
}

/// Replace the loop's body in `input` with `Reduced(loop_head, [(target, formula)])` so
/// outer scopes treat the loop as a single node whose outgoing edges carry the reach
/// conditions of each body exit.
///
/// `exit_formulas` covers every exit target the body actually reaches (from
/// `compute_loop_exit_formulas`); `succ_nodes` defines which succs to emit and provides a
/// deterministic ordering. Anything in `succ_nodes` without a formula gets `True` as a
/// conservative default.
fn install_reduced_marker(
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_head: NodeIndex,
    loop_nodes: &HashSet<NodeIndex>,
    succ_nodes: &HashSet<NodeIndex>,
    exit_formulas: &BTreeMap<NodeIndex, Formula>,
) {
    for n in loop_nodes {
        if *n != loop_head {
            input.remove(n);
        }
    }
    let live_succs: Vec<NodeIndex> = succ_nodes
        .iter()
        .copied()
        .filter(|s| !loop_nodes.contains(s))
        .collect();
    // Single-exit loops don't need a per-edge formula: if we exited, we took the one exit,
    // and reach conditions for the post-loop scope should carry no per-loop guard. Using
    // the computed formula here would produce a spurious `if (<exit_cond>) { post_loop }`
    // wrapper in the outer scope even though the exit is unconditional given we exited.
    // Multi-exit loops keep the real formulas so `edge_condition` distinguishes exits.
    let use_true_for_all = live_succs.len() <= 1;
    let mut succs: Vec<(NodeIndex, Formula)> = live_succs
        .into_iter()
        .map(|s| {
            let f = if use_true_for_all {
                crate::structuring::predicates::true_()
            } else {
                exit_formulas
                    .get(&s)
                    .cloned()
                    .unwrap_or_else(crate::structuring::predicates::true_)
            };
            (s, f)
        })
        .collect();
    succs.sort_by_key(|(t, _)| t.index());
    input.insert(loop_head, D::Input::Reduced(loop_head, succs));
}

pub(super) fn insert_breaks(
    loop_nodes: &HashSet<NodeIndex>,
    loop_head: NodeIndex,
    loop_successor: Option<NodeIndex>,
    node: D::Structured,
) -> D::Structured {
    use D::Structured as DS;

    // Classification of a Jump/JumpIf target relative to *this* loop only. Anything that targets
    // an enclosing loop will be a raw `DS::Jump(target)` at this layer and gets reclassified
    // when the next-outer `structure_loop` runs `insert_breaks` and recurs into our `Loop`
    // body - `Latch` is the fall-through that keeps the jump intact for that later pass.
    enum LatchKind {
        Continue,
        Break,
        InLoop,
        Latch,
    }

    fn find_latch_kind(
        loop_nodes: &HashSet<NodeIndex>,
        loop_head: NodeIndex,
        loop_successor: Option<NodeIndex>,
        node_ndx: NodeIndex,
    ) -> LatchKind {
        if node_ndx == loop_head {
            LatchKind::Continue
        } else if Some(node_ndx) == loop_successor {
            LatchKind::Break
        } else if loop_nodes.contains(&node_ndx) {
            LatchKind::InLoop
        } else {
            LatchKind::Latch
        }
    }

    match node {
        DS::Block(_) => node,
        DS::Seq(nodes) => DS::Seq(
            nodes
                .into_iter()
                .map(|node| insert_breaks(loop_nodes, loop_head, loop_successor, node))
                .collect::<Vec<_>>(),
        ),
        // Already-labeled Break/Continue (emitted by a nested loop's earlier insert_breaks)
        // target some inner loop, not this one - pass through unchanged.
        DS::Break(_) | DS::Continue(_) => node,
        DS::CondIf(cond, conseq, alt) => DS::CondIf(
            cond,
            Box::new(insert_breaks(
                loop_nodes,
                loop_head,
                loop_successor,
                *conseq,
            )),
            Box::new(alt.map(|alt| insert_breaks(loop_nodes, loop_head, loop_successor, alt))),
        ),
        DS::Jump(src, next) => match find_latch_kind(loop_nodes, loop_head, loop_successor, next) {
            LatchKind::Continue => DS::Continue(loop_head),
            LatchKind::Break => DS::Break(loop_head),
            // TODO check if jump target is the next node
            LatchKind::InLoop => D::Structured::Seq(vec![]),
            // Targets neither this loop nor anything dominated by it; leave the raw Jump for
            // an enclosing loop's pass (or for `generate_output` to lower to Unstructured).
            // Preserve the original creation tag: the same Jump escaping outward.
            LatchKind::Latch => DS::Jump(src, next),
        },
        DS::Loop(label, structured) => DS::Loop(
            label,
            Box::new(insert_breaks(
                loop_nodes,
                loop_head,
                loop_successor,
                *structured,
            )),
        ),
        DS::Switch(code, enum_, structureds) => {
            let result = structureds
                .into_iter()
                .map(|(v, structured)| {
                    (
                        v,
                        insert_breaks(loop_nodes, loop_head, loop_successor, structured),
                    )
                })
                .collect::<Vec<_>>();
            DS::Switch(code, enum_, result)
        }
    }
}
