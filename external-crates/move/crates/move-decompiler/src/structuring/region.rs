// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Acyclic-region toolkit: projection construction, reachability, topological
//! order, and reaching-condition computation.
//!
//! A "region" here is a subgraph identified by an `entry` node plus a `members` set,
//! taken out of a larger CFG-like input map. The structurer (see `acyclic.rs`) and
//! loop-body code (see `loops.rs`) both need to:
//!
//!   1. Project the region into an *acyclic* shape, redirecting back-edges to a
//!      synthetic continue-sink and out-of-region edges to per-target exit sinks.
//!   2. Walk the projection in topological order.
//!   3. Compute the boolean formula under which each node is reached
//!      (No More Gotos, phase 1).
//!
//! The data shapes for these steps are shared here; the structurer composes them.

use crate::structuring::{
    ast::{self as D},
    predicates::{self, Formula},
};
use petgraph::algo;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeMap, HashMap, HashSet};

// -------------------------------------------------------------------------------------------------
// Acyclic projection
// -------------------------------------------------------------------------------------------------

/// How a synthetic sink should render at AST-emission time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkBehavior {
    /// Emit `exit_jump(target)`. `insert_breaks` rewrites this to `Break`/`Continue`
    /// once the surrounding loop's owned succs are known.
    Exit(NodeIndex),
    /// Emit an empty `Seq`. Used for whole-function regions where back-edges and
    /// out-of-region edges are redundant duplicates of jumps already embedded inside
    /// `Reduced` markers' pre-built bodies, so the synthetic sink should not produce any
    /// runtime effect.
    Silent,
}

/// Whether the synthetic sinks created during projection rendering produce real exit jumps
/// or silently absorb their edges.
///
/// `Loop` is the cyclic-region case (loop bodies). Back-edges to `entry` become `Continue`
/// targets; edges leaving the region's interior become `Break` targets. Both go through
/// `Jump(ReachingExit, _)` and are rewritten by `insert_breaks`.
///
/// `Function` is the whole-function residue. After loop collapse the function-level CFG
/// has no real back-edges or out-of-region edges, but `Reduced` markers carry succ edges
/// that duplicate jumps already structured inside the marker's pre-built body. Silencing
/// the sink keeps those duplicates from leaking out as residual gotos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkRendering {
    Loop,
    Function,
}

impl SinkRendering {
    fn back_edge(self, entry: NodeIndex) -> SinkBehavior {
        match self {
            SinkRendering::Loop => SinkBehavior::Exit(entry),
            SinkRendering::Function => SinkBehavior::Silent,
        }
    }

    fn out_of_region(self, target: NodeIndex) -> SinkBehavior {
        match self {
            SinkRendering::Loop => SinkBehavior::Exit(target),
            SinkRendering::Function => SinkBehavior::Silent,
        }
    }
}

/// Acyclic projection of an `input` map: original nodes with their edges to out-of-region
/// targets and to `entry` (back-edges) redirected to synthetic sinks, plus the sinks
/// themselves (as terminal `Code(_, 0, None)` entries in `input`) and their rendering
/// decisions (in `sinks`).
pub struct AcyclicProjection {
    pub input: BTreeMap<NodeIndex, D::Input>,
    /// Per synthetic-sink rendering decision. Set at projection-build time according to
    /// the requested [`SinkRendering`]; the structurer's AST emitter consults this map
    /// when it encounters a sink node.
    pub sinks: HashMap<NodeIndex, SinkBehavior>,
}

impl AcyclicProjection {
    /// Render `n` if it's a synthetic sink, otherwise `None` (caller falls through to the
    /// regular Block/Reduced/etc. rendering).
    pub fn render_sink(&self, n: NodeIndex) -> Option<D::Structured> {
        Some(match self.sinks.get(&n)? {
            SinkBehavior::Exit(target) => D::Structured::exit_jump(*target),
            SinkBehavior::Silent => D::Structured::Seq(vec![]),
        })
    }
}

/// Build the acyclic projection. `members` defines the region's interior; `entry` may
/// itself be outside `members` (the loop-body case: head is excluded from members so
/// back-edges to it from inside fire the out-of-region rule). `rendering` decides how
/// synthetic sinks lower at AST-emission time.
///
/// Restricts to nodes reachable from `entry`. Unreachable orphan members (e.g. an
/// isolated self-loop the post-order DFS never visits) would otherwise carry their own
/// cycles into the projection and trip `topological_order`. `structure()`'s
/// `unemitted_from` reports them separately.
pub fn build_acyclic_projection(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    members: &HashSet<NodeIndex>,
    rendering: SinkRendering,
) -> AcyclicProjection {
    // 1. Discover unique exit targets and whether we need a back-edge sink. An edge from
    // an in-projection node back to `entry` is a back-edge - this includes single-block
    // self-loops where the entry is its own predecessor.
    let reachable = reachable_from(input, entry, members);
    let in_projection = |n: NodeIndex| -> bool { reachable.contains(&n) };
    let mut needs_back_edge_sink = false;
    let mut unique_exit_targets: Vec<NodeIndex> = Vec::new();
    let mut seen_targets: HashSet<NodeIndex> = HashSet::new();
    for (&node, inp) in input {
        if !in_projection(node) {
            continue;
        }
        for (_, v) in inp.edges() {
            if v == entry {
                needs_back_edge_sink = true;
            } else if !in_projection(v) && seen_targets.insert(v) {
                unique_exit_targets.push(v);
            }
        }
    }

    // 2. Allocate synthetic sink ids past anything in `input` and record each one's
    // rendering decision.
    let mut next_id = input.keys().map(|n| n.index() + 1).max().unwrap_or(0);
    let mut sinks: HashMap<NodeIndex, SinkBehavior> = HashMap::new();
    let back_edge_sink = if needs_back_edge_sink {
        let id = NodeIndex::new(next_id);
        next_id += 1;
        sinks.insert(id, rendering.back_edge(entry));
        Some(id)
    } else {
        None
    };
    // target -> sink remap (used to redirect edges below).
    let mut target_to_sink: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    for target in unique_exit_targets {
        let id = NodeIndex::new(next_id);
        next_id += 1;
        target_to_sink.insert(target, id);
        sinks.insert(id, rendering.out_of_region(target));
    }

    // 3. Build the projection: keep in-projection nodes, redirect their edges, add sinks.
    // We build a single `remap` table once (`back_edge_sink` covers entry, the per-target
    // sinks cover out-of-region targets) and use it via `redirect_input`.
    let mut remap: HashMap<NodeIndex, NodeIndex> = target_to_sink;
    if let Some(sink) = back_edge_sink {
        remap.insert(entry, sink);
    }
    let mut projection: BTreeMap<NodeIndex, D::Input> = BTreeMap::new();
    for (&node, inp) in input {
        if !in_projection(node) {
            continue;
        }
        projection.insert(node, redirect_input(inp.clone(), &remap));
    }
    for &sink in sinks.keys() {
        projection.insert(sink, D::Input::Code(sink, 0, None));
    }

    AcyclicProjection {
        input: projection,
        sinks,
    }
}

/// Set of nodes reachable from `entry` over `input`'s edges, restricted to nodes in
/// `members`. `entry` itself is always included if it appears in `input` (a region's
/// entry need not be in `members` - see the loop-body case).
pub fn reachable_from(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    members: &HashSet<NodeIndex>,
) -> HashSet<NodeIndex> {
    let mut visited: HashSet<NodeIndex> = HashSet::new();
    let mut stack: Vec<NodeIndex> = vec![entry];
    while let Some(n) = stack.pop() {
        if !visited.insert(n) {
            continue;
        }
        let Some(inp) = input.get(&n) else { continue };
        for (_, v) in inp.edges() {
            if (members.contains(&v) || v == entry) && !visited.contains(&v) {
                stack.push(v);
            }
        }
    }
    visited
}

/// Apply `remap` to every edge target in `inp`, returning a fresh `Input` with the
/// remapped edges. Targets absent from `remap` pass through unchanged.
pub fn redirect_input(inp: D::Input, remap: &HashMap<NodeIndex, NodeIndex>) -> D::Input {
    let map = |v: NodeIndex| remap.get(&v).copied().unwrap_or(v);
    match inp {
        D::Input::Condition(l, c, t, e) => D::Input::Condition(l, c, map(t), map(e)),
        D::Input::Variants(l, c, en, items) => D::Input::Variants(
            l,
            c,
            en,
            items.into_iter().map(|(v, t)| (v, map(t))).collect(),
        ),
        D::Input::Code(l, c, Some(n)) => D::Input::Code(l, c, Some(map(n))),
        D::Input::Code(l, c, None) => D::Input::Code(l, c, None),
        D::Input::Reduced(l, succs) => {
            D::Input::Reduced(l, succs.into_iter().map(|(t, f)| (map(t), f)).collect())
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Topological order
// -------------------------------------------------------------------------------------------------

/// Topological order over the projection. Returns `None` if there's a cycle (shouldn't
/// happen for a valid projection; defensive).
pub fn topological_order(input: &BTreeMap<NodeIndex, D::Input>) -> Option<Vec<NodeIndex>> {
    let mut g: DiGraph<NodeIndex, ()> = DiGraph::new();
    let mut to_internal: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(input.len());
    for &n in input.keys() {
        let idx = g.add_node(n);
        to_internal.insert(n, idx);
    }
    for inp in input.values() {
        for (u, v) in inp.edges() {
            if let (Some(&ui), Some(&vi)) = (to_internal.get(&u), to_internal.get(&v)) {
                g.add_edge(ui, vi, ());
            }
        }
    }
    let topo = algo::toposort(&g, None).ok()?;
    Some(topo.into_iter().map(|i| g[i]).collect())
}

// -------------------------------------------------------------------------------------------------
// Reaching conditions (No More Gotos, phase 1)
// -------------------------------------------------------------------------------------------------
//
// For each node, the boolean formula over branch predicates under which control reaches
// it:
//
//     R(entry) = true
//     R(n)     = OR_{p -> n}  R(p) && cond(p -> n)
//
// Atoms are named via the `__c{N}` convention so locals reassigned across regions don't
// conflate.

// Forward region graph used by `reaching_conditions`. Built once and walked by the
// topological order; the synthetic exit absorbs any out-of-region edges so the graph is
// always acyclic and has a single sink.
struct RegionGraph {
    graph: DiGraph<(), ()>,
    /// Inverse of the per-node mapping (graph index -> original `NodeIndex`). The
    /// synthetic exit slot is `None`; skipped during reaching computation.
    from_internal: Vec<Option<NodeIndex>>,
}

impl RegionGraph {
    fn build(input: &BTreeMap<NodeIndex, D::Input>, members: &HashSet<NodeIndex>) -> Option<Self> {
        if input.is_empty() {
            return None;
        }
        let mut graph: DiGraph<(), ()> = DiGraph::new();
        let mut to_internal: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(input.len());
        let mut from_internal: Vec<Option<NodeIndex>> = Vec::with_capacity(input.len() + 1);
        for n in input.keys() {
            let idx = graph.add_node(());
            to_internal.insert(*n, idx);
            from_internal.push(Some(*n));
        }
        let exit_internal = graph.add_node(());
        from_internal.push(None);
        let mut has_sink = false;
        for (n, inp) in input {
            let u_int = to_internal[n];
            let succs = inp.edges();
            if succs.is_empty() {
                graph.add_edge(u_int, exit_internal, ());
                has_sink = true;
                continue;
            }
            for (u, v) in succs {
                debug_assert_eq!(u, *n, "Input::edges always sources from the input's label");
                if members.contains(&v) && to_internal.contains_key(&v) {
                    graph.add_edge(u_int, to_internal[&v], ());
                } else {
                    graph.add_edge(u_int, exit_internal, ());
                    has_sink = true;
                }
            }
        }
        if !has_sink {
            return None;
        }
        Some(RegionGraph {
            graph,
            from_internal,
        })
    }
}

/// The predicate under which edge `p -> n` is taken, given `p`'s input node.
fn edge_condition(pred_input: Option<&D::Input>, p: NodeIndex, n: NodeIndex) -> Formula {
    match pred_input {
        Some(D::Input::Condition(_, _, then, els)) => {
            if n == *then {
                predicates::cond_atom(p.index() as u64)
            } else if n == *els {
                predicates::not(predicates::cond_atom(p.index() as u64))
            } else {
                // `Input::edges` for a Condition returns exactly `(p, then)` and `(p, else)`,
                // so reaching this arm means a Condition's arms were rewritten after the topo
                // build. Conservative `True` guard rather than panic in release.
                debug_assert!(
                    false,
                    "edge {p:?} -> {n:?} not in Condition's arms (then={then:?}, els={els:?})",
                );
                predicates::true_()
            }
        }
        Some(D::Input::Variants(_, code, _, items)) => {
            // The edge fires when `subject == variant_K` for whichever variant K points at `n`.
            // Multiple variants can share a target; OR their match atoms together.
            let matching: Vec<Formula> = items
                .iter()
                .filter(|(_, target)| *target == n)
                .map(|(variant, _)| predicates::match_atom(*code, variant.as_str()))
                .collect();
            if matching.is_empty() {
                debug_assert!(
                    false,
                    "edge {p:?} -> {n:?} not in Variants's arms (items={items:?})",
                );
                predicates::true_()
            } else {
                predicates::or(matching)
            }
        }
        Some(D::Input::Reduced(_, succs)) => {
            // NMG §V-B: the abstract loop node's outgoing edges carry the reaching condition
            // of the exit path from the loop body's projection. Reading it here is what lets
            // the outer scope's `reaching_conditions` propagate the per-exit distinction into
            // the post-loop items list without a bespoke cascade emitter. `True` fallback for
            // markers installed before Phase 4 wires real formulas through.
            succs
                .iter()
                .find(|(target, _)| *target == n)
                .map(|(_, f)| f.clone())
                .unwrap_or_else(predicates::true_)
        }
        _ => predicates::true_(),
    }
}

/// Reaching conditions for every node of an acyclic region. `None` if the region has a cycle.
pub fn reaching_conditions(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
) -> Option<BTreeMap<NodeIndex, Formula>> {
    let members: HashSet<NodeIndex> = input.keys().copied().collect();
    let rgraph = RegionGraph::build(input, &members)?;

    // Toposort the forward region graph. `Err(Cycle)` -> the region has a back edge and we
    // can't compute reaching conditions; the synthetic exit slot in `from_internal` is `None`
    // and gets skipped below.
    let topo = algo::toposort(&rgraph.graph, None).ok()?;

    let mut preds: BTreeMap<NodeIndex, Vec<NodeIndex>> = BTreeMap::new();
    for inp in input.values() {
        for (u, v) in inp.edges() {
            preds.entry(v).or_default().push(u);
        }
    }

    let mut reach: BTreeMap<NodeIndex, Formula> = BTreeMap::new();
    reach.insert(entry, predicates::true_());
    for internal in topo {
        let Some(n) = rgraph.from_internal[internal.index()] else {
            continue; // synthetic exit
        };
        if n == entry {
            continue;
        }
        let mut terms = Vec::new();
        for &p in preds.get(&n).into_iter().flatten() {
            let rp = reach.get(&p).cloned().unwrap_or_else(predicates::false_);
            terms.push(predicates::and(vec![
                rp,
                edge_condition(input.get(&p), p, n),
            ]));
        }
        reach.insert(n, predicates::or(terms));
    }
    Some(reach)
}
