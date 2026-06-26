// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod acyclic;
pub(crate) mod ast;
pub(crate) mod dom_tree;
pub(crate) mod graph;
pub(crate) mod hoist_declarations;
pub(crate) mod loops;
pub(crate) mod predicates;
pub(crate) mod region;
pub(crate) mod term_reconstruction;

use crate::{
    config::{self, print_heading},
    structuring::{
        ast::{self as D},
        graph::Graph,
        region::SinkRendering,
    },
};

use petgraph::{
    algo::dominators::{self, Dominators},
    graph::{DiGraph, NodeIndex},
    visit::DfsPostOrder,
};

use std::collections::{BTreeMap, HashMap, HashSet};

/// Read-only context threaded through the structurer pipeline. Currently just `config`
/// (debug-print gating + structuring-policy toggles). `Copy` so it threads by value.
#[derive(Clone, Copy)]
pub(crate) struct StructureContext<'a> {
    pub config: &'a config::Config,
}

pub(crate) fn structure(
    config: &config::Config,
    mut input: BTreeMap<D::Label, D::Input>,
    entry_node: D::Label,
) -> (D::Structured, Vec<u64>) {
    // Native functions have empty basic blocks - return early to avoid panicking in Graph::new
    if input.is_empty() {
        return (D::Structured::Seq(vec![]), vec![]);
    }

    let mut graph = Graph::new(config, &input, entry_node);
    let ctx = StructureContext { config };

    let mut structured_blocks: BTreeMap<D::Label, D::Structured> = BTreeMap::new();
    // Codes that haven't been emitted as a `Block(code)` yet. Every code-bearing input
    // (Code/Condition/Variants) starts here; `to_structured_ast` removes one each time it
    // commits a `Block(code)`. After structuring runs, anything still present is a node
    // the structurer silently dropped - the rendered function carries a `// unstructured
    // blocks: ...` notice listing them.
    let mut unstructured: HashSet<u64> = input
        .values()
        .filter_map(|inp| match inp {
            D::Input::Code(_, code, _)
            | D::Input::Condition(_, code, _, _)
            | D::Input::Variants(_, code, _, _) => Some(*code),
            D::Input::Reduced(_, _) => None,
        })
        .collect();

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
        && let Some(reach) = region::reaching_conditions(&input, entry_node)
    {
        print_heading("reaching conditions");
        for (node, formula) in &reach {
            println!("R({}) = {formula}", node.index());
        }
    }

    // Region-by-region structuring (NMG IV-B + IV-C). One pass over the dominator tree
    // in post-order: children before parents, so by the time we look at any node every
    // region nested inside it has already been collapsed to an `Input::Reduced` marker.
    // At each node we dispatch:
    //
    //   - Loop head -> NMG IV-C: `structure_loop` wraps the body in a `Loop` form,
    //     removes the body's members from `input`, and installs `Reduced(head, exits)`.
    //   - Otherwise -> NMG IV-B SESE slice: if the node's immediate post-dominator
    //     exists and the slice between them is single-entry, structure the slice in
    //     isolation and install `Reduced(node, [post_dom])` (single-exit). Non-SESE
    //     slices fall through to be folded into a parent slice's projection.
    //
    // Both branches write the region's structured AST into `region_bodies`; the
    // lookup map serves the next region up as it projects its own subgraph.
    //
    // Post-dom is computed once over the original CFG. For non-loop nodes the post-dom
    // doesn't change after loops collapse - loops are "transparent" to dominator-style
    // analyses, so a node's post-dom in the residue matches its post-dom in the original
    // graph. Loop heads themselves are processed as loops (not as SESE), so their
    // possibly-meaningless post-dom is never used.
    let post_dom = PostDom::build(&input);
    let mut region_bodies: BTreeMap<NodeIndex, D::Structured> = BTreeMap::new();
    for node in dom_tree_post_order(&graph, entry_node) {
        if !input.contains_key(&node) {
            continue;
        }
        if graph.loop_heads.contains(&node) {
            if ctx.config.debug_print.structuring {
                println!("Structuring loop at node {node:#?}");
            }
            if ctx.config.debug_print.regions {
                println!("[region] loop entry={}", node.index());
            }
            loops::structure_loop(
                ctx,
                &mut graph,
                &mut structured_blocks,
                node,
                &mut input,
                &mut unstructured,
            );
            if let Some(form) = structured_blocks.get(&node) {
                region_bodies.insert(node, form.clone());
            }
            continue;
        }
        // Skip SESE on nodes that sit inside an unprocessed loop body. `structure_loop`
        // needs the original body topology to build the loop's wrapped form, and the
        // dom-tree post-order visits body nodes before the loop head, so pre-collapsing
        // a body node would strip pieces the loop's own structuring still needs.
        if graph.loop_exits.contains_key(&node) {
            continue;
        }
        let Some(post_dom) = post_dom.as_ref() else {
            continue;
        };
        let Some(post_dom_node) = post_dom.ipostdom(node) else {
            continue;
        };
        let members = collect_slice(&input, node, post_dom_node);
        if members.len() < 2 {
            continue;
        }
        if !is_sese_slice(&input, &members, node) {
            continue;
        }
        let Some(body) = acyclic::structure_region(
            &region_bodies,
            &input,
            node,
            &members,
            SinkRendering::Function,
            &mut unstructured,
        ) else {
            continue;
        };
        if ctx.config.debug_print.regions {
            println!(
                "[region] fold entry={} pdom={} size={}",
                node.index(),
                post_dom_node.index(),
                members.len()
            );
        }
        region_bodies.insert(node, body);
        for m in &members {
            if *m != node {
                input.remove(m);
            }
        }
        input.insert(node, D::Input::Reduced(node, vec![post_dom_node]));
    }

    let residue_members: HashSet<NodeIndex> = input.keys().copied().collect();
    let mut body = acyclic::structure_region(
        &region_bodies,
        &input,
        entry_node,
        &residue_members,
        SinkRendering::Function,
        &mut unstructured,
    )
    .expect("structuring failed on top-level function residue after loop collapse");
    flatten_sequence(&mut body);
    let mut leftover: Vec<u64> = unstructured.into_iter().collect();
    leftover.sort_unstable();
    (body, leftover)
}

/// Post-order traversal of the function's dominator tree, rooted at `entry`. Children
/// before parents - the recursive structurer needs deeper slices folded before outer
/// slices look at them.
fn dom_tree_post_order(graph: &Graph, entry: NodeIndex) -> Vec<NodeIndex> {
    fn visit(graph: &Graph, node: NodeIndex, out: &mut Vec<NodeIndex>) {
        for child in graph.dom_tree.get(node).immediate_children() {
            visit(graph, child, out);
        }
        out.push(node);
    }
    let mut out = Vec::new();
    visit(graph, entry, &mut out);
    out
}

/// Nodes on every path from `entry` to (but not including) `exit`, restricted to members
/// of `input`. We BFS forward from `entry` over the *current* `input` edges (loops are
/// already Reduced markers by the time this runs, so reading `graph.cfg` would walk
/// through stale loop-body edges), stopping at `exit` and at any node not in `input`.
/// The result includes `entry` itself.
fn collect_slice(
    input: &BTreeMap<D::Label, D::Input>,
    entry: NodeIndex,
    exit: NodeIndex,
) -> HashSet<NodeIndex> {
    let mut slice: HashSet<NodeIndex> = HashSet::new();
    let mut stack: Vec<NodeIndex> = vec![entry];
    while let Some(n) = stack.pop() {
        if n == exit || !input.contains_key(&n) || !slice.insert(n) {
            continue;
        }
        let Some(inp) = input.get(&n) else { continue };
        for (_, target) in inp.edges() {
            if target != exit && input.contains_key(&target) {
                stack.push(target);
            }
        }
    }
    slice
}

/// A slice is structurable as an acyclic region iff
///   1. every CFG predecessor of every non-`entry` member is also a member
///      (single-entry: no external edge into the slice's interior), and
///   2. no member's outgoing edge targets `entry` (acyclic: no back-edge to the entry).
///
/// (1) is what makes folding the slice into a single `Reduced` marker semantics-preserving
/// for the parent region. (2) keeps us from accidentally folding a loop body whose head
/// `structure_loop` hasn't processed yet - the back-edge would be silently dropped by
/// `SinkRendering::Function` and the loop semantics would vanish. (We still rely on
/// `find_loop_heads_and_back_edges` to identify natural loops; the back-edge check here
/// is the "don't poach a loop body" guard for the unified walk.)
fn is_sese_slice(
    input: &BTreeMap<D::Label, D::Input>,
    members: &HashSet<NodeIndex>,
    entry: NodeIndex,
) -> bool {
    for member in members {
        if *member == entry {
            continue;
        }
        let Some(inp) = input.get(member) else {
            continue;
        };
        for (_, target) in inp.edges() {
            if target == entry {
                return false;
            }
        }
    }
    for src in input.values() {
        let from_label = src.label();
        if members.contains(&from_label) {
            continue;
        }
        for (_, target) in src.edges() {
            if target != entry && members.contains(&target) {
                return false;
            }
        }
    }
    true
}

/// Post-dominators over the residue. Built once per `structure()` call by running the
/// dominator algorithm on a reversed graph that includes a synthetic exit node absorbing
/// every "leaving" edge (edges to nodes outside `input`, plus every natural sink). The
/// resulting `ipostdom(n)` is the immediate join point a branch's arms converge on;
/// `None` means there is no join inside the residue (the branch's arms separately reach
/// function sinks).
struct PostDom {
    doms: Dominators<NodeIndex>,
    exit_internal: NodeIndex,
    to_internal: HashMap<NodeIndex, NodeIndex>,
    from_internal: Vec<Option<NodeIndex>>,
}

impl PostDom {
    fn build(input: &BTreeMap<D::Label, D::Input>) -> Option<Self> {
        if input.is_empty() {
            return None;
        }
        let mut rev: DiGraph<(), ()> = DiGraph::new();
        let mut to_internal: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(input.len());
        let mut from_internal: Vec<Option<NodeIndex>> = Vec::with_capacity(input.len() + 1);
        for n in input.keys() {
            let idx = rev.add_node(());
            to_internal.insert(*n, idx);
            from_internal.push(Some(*n));
        }
        let exit_internal = rev.add_node(());
        from_internal.push(None);
        let mut has_sink = false;
        for (n, inp) in input {
            let u_int = to_internal[n];
            let succs = inp.edges();
            if succs.is_empty() {
                rev.add_edge(exit_internal, u_int, ());
                has_sink = true;
                continue;
            }
            for (_u, v) in succs {
                if let Some(&v_int) = to_internal.get(&v) {
                    rev.add_edge(v_int, u_int, ());
                } else {
                    rev.add_edge(exit_internal, u_int, ());
                    has_sink = true;
                }
            }
        }
        if !has_sink {
            return None;
        }
        Some(PostDom {
            doms: dominators::simple_fast(&rev, exit_internal),
            exit_internal,
            to_internal,
            from_internal,
        })
    }

    fn ipostdom(&self, node: NodeIndex) -> Option<NodeIndex> {
        let n_int = *self.to_internal.get(&node)?;
        match self.doms.immediate_dominator(n_int) {
            Some(ip) if ip != self.exit_internal => self.from_internal[ip.index()],
            _ => None,
        }
    }
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
        | DS::Let(_)
        | DS::AssignTag(_, _) => {}
    }
}
