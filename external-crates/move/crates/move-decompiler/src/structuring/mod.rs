// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod acyclic;
pub(crate) mod ast;
pub(crate) mod dom_tree;
pub(crate) mod graph;
pub(crate) mod hoist_declarations;
pub(crate) mod loops;
pub(crate) mod predicates;
pub(crate) mod term_reconstruction;

use crate::{
    config::{self, print_heading},
    structuring::{
        ast::{self as D},
        graph::Graph,
    },
};

use petgraph::{graph::NodeIndex, visit::DfsPostOrder};

use std::collections::BTreeMap;

// ------------------------------------------------------------------------------------------------
// Structuring Algorithm
// ------------------------------------------------------------------------------------------------
// This algorithm is (loosely) based on No More Gotos (2015), with a number of modifications to
// make it Move-specific. Part of the change also includes leveraging what we know about Move
// compilation to avoid some of the more-complex structuring issues that arise in general
// decompilation.

/// Read-only context threaded through the structurer pipeline. Bundles the two references
/// that every internal step needs:
/// - `config` - for debug-print gating and structuring-policy toggles.
/// - `terms` - per-block lowered `Exp` content, consulted by `bodies_equivalent` in the
///   reaching diamond folder and by the lowering layer (`translate.rs`) downstream.
///
/// `Copy` so the context can be passed by value through the recursion without per-call
/// `&ctx` plumbing. The two contained refs are themselves `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct StructureContext<'a> {
    pub config: &'a config::Config,
    pub terms: &'a BTreeMap<D::Label, crate::ast::Exp>,
}

pub(crate) fn structure(
    config: &config::Config,
    mut input: BTreeMap<D::Label, D::Input>,
    entry_node: D::Label,
    terms: &BTreeMap<D::Label, crate::ast::Exp>,
) -> (D::Structured, Vec<u64>) {
    // Native functions have empty basic blocks - return early to avoid panicking in Graph::new
    if input.is_empty() {
        return (D::Structured::Seq(vec![]), vec![]);
    }

    let mut graph = Graph::new(config, &input, entry_node);
    // Capture node ids up front - by the time we report `unemitted` the map has been
    // partially drained (loop bodies, absorbed succs).
    let all_nodes: Vec<NodeIndex> = input.keys().copied().collect();
    let ctx = StructureContext { config, terms };

    let mut structured_blocks: BTreeMap<D::Label, D::Structured> = BTreeMap::new();

    if config.debug_print.structuring {
        let mut post_order = DfsPostOrder::new(&graph.cfg, entry_node);
        print_heading("post-order traversal");
        println!("cfg: {:#?}", graph.cfg);
        while let Some(node) = post_order.next(&graph.cfg) {
            print!("{:?}  ", node.index());
        }
        println!();
    }

    if config.debug_print.control_flow_graph
        && let Some(reach) = acyclic::reaching_conditions(&input, entry_node)
    {
        print_heading("reaching conditions");
        for (node, formula) in &reach {
            println!("R({}) = {formula}", node.index());
        }
    }

    // Pre-process loops innermost-first: each loop body is structured (recursively by
    // `structure_loop`, which calls NMG on the loop body), then `Input::Reduced(loop_head,
    // succs)` replaces the loop's nodes in `input`. After this pass, `input` contains the
    // function's acyclic skeleton with loops collapsed to opaque `Reduced` markers.
    //
    // `DfsPostOrder` visits children before parents, so nested loops are reduced before
    // their enclosing scope sees them. Non-loop nodes are left untouched - they stay in
    // `input` for the NMG top-level pass to walk.
    let mut loop_structured: BTreeMap<D::Label, D::Structured> = BTreeMap::new();
    let mut post_order = DfsPostOrder::new(&graph.cfg, entry_node);
    while let Some(node) = post_order.next(&graph.cfg) {
        if !graph.loop_heads.contains(&node) {
            continue;
        }
        if ctx.config.debug_print.structuring {
            println!("Structuring loop at node {node:#?}");
        }
        loops::structure_loop(ctx, &mut graph, &mut structured_blocks, node, &mut input);
        // Snapshot the wrapped `Loop` form. NMG reads from `loop_structured` to render
        // `Input::Reduced` markers regardless of any later consumption.
        if let Some(form) = structured_blocks.get(&node) {
            loop_structured.insert(node, form.clone());
        }
    }

    // NMG runs once on the function's residue (loops are all `Reduced` markers now).
    // Must succeed - if it returns `None`, that's a real bug to fix, not silently fall
    // back to goto-laden output.
    let mut body = acyclic::structure_full_function(
        config,
        terms,
        &loop_structured,
        &input,
        entry_node,
    )
    .expect("NMG-proper failed on top-level function residue after loop collapse");
    flatten_sequence(&mut body);
    for n in &all_nodes {
        graph.mark_emitted(n.index() as u64);
    }
    let unemitted = graph.unemitted_from(&all_nodes);
    (body, unemitted)
}

/// Final normalization pass over the structured output. Recursively drops empty `Seq`s,
/// splices non-empty nested `Seq`s into their parent, and collapses `Some(Seq([]))` alts
/// to `None`. Called once at the end of `structure()`; the per-scope code paths leave
/// stray empties from tail-elision and from `insert_breaks`'s in-loop-Jump replacement,
/// and this is the single place that normalizes them away.
pub(super) fn flatten_sequence(s: &mut D::Structured) {
    use D::Structured as DS;
    match s {
        DS::Seq(items) => {
            for item in items.iter_mut() {
                flatten_sequence(item);
            }
            let mut flat = Vec::with_capacity(items.len());
            for item in items.drain(..) {
                match item {
                    DS::Seq(inner) if inner.is_empty() => {}
                    DS::Seq(inner) => flat.extend(inner),
                    other => flat.push(other),
                }
            }
            *items = flat;
        }
        DS::CondIf(_, conseq, alt) => {
            flatten_sequence(conseq);
            if let Some(alt_inner) = alt.as_mut().as_mut() {
                flatten_sequence(alt_inner);
            }
            // `arm_for` always returns *some* body, so an alt that elided away ends up as
            // an empty Seq; later refinements/printers treat `CondIf(_, _, None)` as the
            // canonical "no-else" shape.
            if matches!(alt.as_ref().as_ref(), Some(DS::Seq(items)) if items.is_empty()) {
                **alt = None;
            }
        }
        DS::Switch(_, _, cases) => {
            for (_, body) in cases.iter_mut() {
                flatten_sequence(body);
            }
        }
        DS::Loop(_, body) => flatten_sequence(body),
        DS::SelectorMatch(_, arms) => {
            for (_, body) in arms.iter_mut() {
                flatten_sequence(body);
            }
        }
        DS::Block(_)
        | DS::Break(_)
        | DS::Continue(_)
        | DS::Jump(_, _)
        | DS::JumpIf(_, _, _, _)
        | DS::Let(_)
        | DS::Assign(_, _) => {}
    }
}
