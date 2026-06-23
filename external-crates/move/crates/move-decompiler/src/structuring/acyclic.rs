// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Acyclic-region structuring. Two structurers live here:
//!
//! 1. **NMG-proper (reaching conditions)**: [`structure_nmg`] computes reaching
//!    conditions over the region's acyclic projection, lays each node out in topo order
//!    guarded by its formula, then runs [`refine_initial_ast`]'s two-phase refinement
//!    (implication nesting + condition-based factoring) to recover nested control flow.
//!    This is the primary path; returns `None` only on `Variants` (switch nodes), which
//!    the dom-tree path then handles.
//!
//! 2. **Dominator-tree absorption**: [`structure_acyclic_region`] and friends. Builds
//!    IfElse/Switch from `ichildren` absorption + post-dom joins; hoists "orphan"
//!    dom-tree children as siblings. Used as the fallback path and by
//!    `loops::structure_loop` for body assembly when NMG can't handle the body's shape.

use crate::config;
use crate::structuring::{
    StructureContext,
    ast::{self as D, GotoSource},
    dom_tree,
    graph::Graph,
    predicates::{self, Formula},
};
use petgraph::Direction;
use petgraph::algo;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Structure an acyclic whole-function region. The region is the set of all `input` keys;
/// out-of-region edges can only be CFG sinks (return/abort). Returns `None` to fall back to
/// the dom-tree structurer when nothing folds.
pub fn structure_full_function(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    structured_blocks: &BTreeMap<NodeIndex, D::Structured>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
) -> Option<D::Structured> {
    let members: HashSet<NodeIndex> = input.keys().copied().collect();
    structure_inner(
        config,
        terms,
        structured_blocks,
        input,
        entry,
        &members,
        Mode::WholeFunction,
    )
}

/// Structure an acyclic sub-region. `members` are the nodes considered inside the region;
/// any CFG edge leaving that set becomes a `Jump(ReachingExit, ...)` for `insert_breaks` to
/// rewrite as `Break`/`Continue`. `entry` may itself be outside `members` (e.g. a loop head
/// passed by `loops::structure_loop` -- the head is bypassed at the region's entry but
/// back-edges to it from inside fire the out-of-region rule, becoming exit jumps).
pub fn structure_acyclic(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    structured_blocks: &BTreeMap<NodeIndex, D::Structured>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    members: &HashSet<NodeIndex>,
) -> Option<D::Structured> {
    structure_inner(
        config,
        terms,
        structured_blocks,
        input,
        entry,
        members,
        Mode::LoopBody,
    )
}

/// Closed dichotomy: whole-function or loop-body. Controls only whether out-of-region edges
/// emit `Jump(ReachingExit)` for downstream rewriting (loop-body) or bail (whole-function,
/// where they can only be CFG sinks).
#[derive(Clone, Copy)]
enum Mode {
    WholeFunction,
    LoopBody,
}

impl Mode {
    fn emit_exit_jumps(self) -> bool {
        matches!(self, Mode::LoopBody)
    }
}

fn structure_inner(
    _config: &config::Config,
    _terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    structured_blocks: &BTreeMap<NodeIndex, D::Structured>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    members: &HashSet<NodeIndex>,
    mode: Mode,
) -> Option<D::Structured> {
    structure_nmg(structured_blocks, input, entry, members, mode)
}

// =================================================================================================
// NMG-proper acyclic structurer (§IV-B steps 1+2)
// =================================================================================================
// Compute reaching conditions over the region's acyclic projection; lay out each node in
// topological order guarded by its formula. NMG's "refinement" steps (condition-based
// fusion, switch detection, reachability cascades) are handled by the refinement pipeline --
// they ARE refinements.
//
// Projection rules:
//   - Back-edges to `entry` from inside the region are redirected to a synthetic `Continue`
//     sink (loop-body mode only; whole-function regions have no back-edges by assumption).
//   - Edges to nodes outside `members` are redirected to per-target synthetic exit sinks.
//   - The region is then acyclic and `reaching_conditions` succeeds.
//
// Bails (`None`) on `Variants` (NMG's switch case needs `subject == variant_K` atoms which
// aren't modeled in the predicate algebra yet) or on `reaching_conditions` failure.

fn structure_nmg(
    structured_blocks: &BTreeMap<NodeIndex, D::Structured>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    members: &HashSet<NodeIndex>,
    mode: Mode,
) -> Option<D::Structured> {
    if input.values().any(|i| matches!(i, D::Input::Variants(..))) {
        return None;
    }
    let proj = build_acyclic_projection(input, entry, members);
    let reach = reaching_conditions(&proj.input, entry)?;
    let topo = topological_order(&proj.input)?;

    // Initial AST: `Seq[ if(c_r(n_1)) { n_1 }; ...; if(c_r(n_k)) { n_k } ]` per NMG §IV-B
    // step 1. Keep guards in their *factored* form (raw `And`/`Or` from the smart
    // constructors); calling `.simplify()` here distributes `And` over `Or` to DNF and
    // destroys the structure that lets the refinement step find compound factors like
    // a reaching condition shared as a top-level conjunct. We `.simplify()` only at
    // emission time. Drop `False` guards (dead code); `entry` is unconditional.
    let mut items: Vec<(Formula, D::Structured)> = Vec::with_capacity(topo.len());
    for n in topo {
        let guard = if n == entry {
            predicates::true_()
        } else {
            reach.get(&n).cloned().unwrap_or_else(predicates::true_)
        };
        if guard == predicates::false_() {
            continue;
        }
        let body = render_projection_node(n, &proj, structured_blocks, mode, entry);
        items.push((guard, body));
    }

    // NMG §IV-B step 2: condition-based refinement. Iteratively factor common top-level
    // conjuncts out of sibling guards (and fuse complementary pairs) until fixed point.
    items = refine_initial_ast(items);

    Some(emit_seq_from_items(items))
}

/// Iteratively apply NMG's refinement steps to a flat sequence of guarded items until
/// no more rewrites apply. Two phases per pass:
///
///   1. **Implication nesting**: when a later item's guard structurally implies an
///      earlier item's (via `has_factor`), nest the later item inside the earlier
///      one's body with a residual guard. This recovers Move's "definitely assigned"
///      structure - e.g. `__c27 = check; assert!(__c27)` lives inside the same
///      `if (__c24) { ... }` block so the read of `__c27` is on the path where it
///      was just written.
///
///   2. **Condition-based factoring**: factor out common literals / top-level
///      conjuncts across sibling guards. See [`try_refine_once`].
///
/// Order matters: implication nesting first keeps related items together, so the
/// subsequent factoring doesn't drag a pair apart by picking a higher-coverage but
/// scope-fracturing factor.
fn refine_initial_ast(mut items: Vec<(Formula, D::Structured)>) -> Vec<(Formula, D::Structured)> {
    loop {
        if let Some(new_items) = try_implication_nest(&items) {
            items = new_items;
            continue;
        }
        if let Some(new_items) = try_refine_once(&items) {
            items = new_items;
            continue;
        }
        return items;
    }
}

/// Find the earliest item `i` such that one or more later items `j > i` have guards
/// that structurally factor through `guard(i)` (via [`Formula::has_factor`]). Those
/// implied items get pulled inside `i`'s body with their residual guards.
///
/// Skips items whose guard is `True` (the entry item) as the outer - nesting all
/// implied items inside the entry would be vacuous.
fn try_implication_nest(
    items: &[(Formula, D::Structured)],
) -> Option<Vec<(Formula, D::Structured)>> {
    for i in 0..items.len() {
        let g_i = &items[i].0;
        if *g_i == predicates::true_() {
            continue;
        }
        let implied: Vec<usize> = items
            .iter()
            .enumerate()
            .skip(i + 1)
            .filter_map(|(j, (g, _))| g.has_factor(g_i).then_some(j))
            .collect();
        if implied.is_empty() {
            continue;
        }

        // Inner items: each gets its guard's `g_i` factor stripped, then we recursively
        // refine the inner sequence so nested implications resolve too.
        let inner: Vec<(Formula, D::Structured)> = implied
            .iter()
            .map(|&j| (items[j].0.without_factor(g_i), items[j].1.clone()))
            .collect();
        let inner_refined = refine_initial_ast(inner);
        let inner_seq = emit_seq_from_items(inner_refined);

        // Splice the original body and the new inner sequence into one Seq. Flatten
        // when either side is already a Seq so we don't pile up empty wrappers.
        let i_body = items[i].1.clone();
        let new_body = splice_into_seq(i_body, inner_seq);

        let implied_set: HashSet<usize> = implied.into_iter().collect();
        let mut new_items: Vec<(Formula, D::Structured)> = Vec::with_capacity(items.len());
        for (k, item) in items.iter().enumerate() {
            if k == i {
                new_items.push((g_i.clone(), new_body.clone()));
            } else if !implied_set.contains(&k) {
                new_items.push(item.clone());
            }
        }
        return Some(new_items);
    }
    None
}

/// Concatenate two `Structured` values into a flat `Seq`, splicing through any
/// top-level `Seq`s on either side.
fn splice_into_seq(a: D::Structured, b: D::Structured) -> D::Structured {
    use D::Structured as DS;
    let mut out: Vec<DS> = Vec::new();
    match a {
        DS::Seq(items) => out.extend(items),
        other => out.push(other),
    }
    match b {
        DS::Seq(items) => out.extend(items),
        other => out.push(other),
    }
    DS::Seq(out)
}

/// One iteration of NMG's condition-based refinement. Returns `Some(refined)` if a
/// factoring happened, `None` if no candidate produced a refinement.
///
/// Strategy: scan literal candidates (atom or negated atom) that can be factored out of
/// 2+ items' guards via [`Formula::has_factor`] (which sees through DNF disjunctions).
/// For each candidate `c`, partition items into `Vc` (have `c` as factor) and `V_neg_c`
/// (have `¬c` as factor). If `|Vc| + |V_neg_c| >= 2`, splice a
/// `CondIf(c, Seq(Vc with c stripped), Some(Seq(V_neg_c with ¬c stripped)))` at the
/// earliest affected position.
fn try_refine_once(items: &[(Formula, D::Structured)]) -> Option<Vec<(Formula, D::Structured)>> {
    let candidates = candidate_factors(items);
    for c in candidates {
        let neg_c = predicates::not(c.clone());
        let mut vc_indices: Vec<usize> = Vec::new();
        let mut vneg_indices: Vec<usize> = Vec::new();
        for (i, (g, _)) in items.iter().enumerate() {
            if g.has_factor(&c) {
                vc_indices.push(i);
            } else if g.has_factor(&neg_c) {
                vneg_indices.push(i);
            }
        }
        if vc_indices.len() + vneg_indices.len() < 2 {
            continue;
        }

        // Children keep their R = guard \ factor. Don't `.simplify()` here -- it would
        // distribute the residual to DNF and break the next refinement layer's ability to
        // find compound factors.
        let vc_items: Vec<(Formula, D::Structured)> = vc_indices
            .iter()
            .map(|&i| {
                let (g, body) = &items[i];
                (g.without_factor(&c), body.clone())
            })
            .collect();
        let vneg_items: Vec<(Formula, D::Structured)> = vneg_indices
            .iter()
            .map(|&i| {
                let (g, body) = &items[i];
                (g.without_factor(&neg_c), body.clone())
            })
            .collect();
        let conseq = emit_seq_from_items(refine_initial_ast(vc_items));
        let alt = if vneg_items.is_empty() {
            None
        } else {
            Some(emit_seq_from_items(refine_initial_ast(vneg_items)))
        };
        let compound = D::Structured::CondIf(c, Box::new(conseq), Box::new(alt));

        let earliest = vc_indices
            .iter()
            .chain(vneg_indices.iter())
            .min()
            .copied()
            .unwrap();
        let affected: HashSet<usize> = vc_indices.into_iter().chain(vneg_indices).collect();
        let mut new_items: Vec<(Formula, D::Structured)> = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            if i == earliest {
                new_items.push((predicates::true_(), compound.clone()));
            } else if !affected.contains(&i) {
                new_items.push(item.clone());
            }
        }
        return Some(new_items);
    }
    None
}

/// Collect factor candidates from `items` and score each by how many items it (or its
/// negation) factors out of.
///
/// Two sources of candidates so we get both atom-level factoring (inside DNF disjuncts)
/// and compound factoring (when an `Or` formula sits as a top-level conjunct alongside
/// atom factors):
///   - Every atom that appears anywhere in a guard - surfaces `__c38` even when guards
///     are `Or(And(...,__c38,...), And(...,__c38,...))`.
///   - Every top-level conjunct of each guard's `conjuncts()` - surfaces a compound
///     `Or` formula `g` when items have guards like `g`, `g && __c41`, `g && !__c41`.
///     Without this, the three items share `g` as a factor but no single atom is.
///
/// Order is fully deterministic: highest coverage first, then `Formula::Ord`.
fn candidate_factors(items: &[(Formula, D::Structured)]) -> Vec<Formula> {
    // `BTreeSet` (sorted) instead of `HashSet` so subsequent iteration order is fixed.
    let mut candidates: BTreeSet<Formula> = BTreeSet::new();
    for (g, _) in items {
        for s in g.atoms() {
            candidates.insert(predicates::atom(s));
        }
        for c in g.conjuncts() {
            candidates.insert(c);
        }
    }
    candidates.remove(&predicates::true_());
    candidates.remove(&predicates::false_());
    let mut scored: Vec<(Formula, usize)> = candidates
        .into_iter()
        .map(|c| {
            let neg = predicates::not(c.clone());
            let n = items
                .iter()
                .filter(|(g, _)| g.has_factor(&c) || g.has_factor(&neg))
                .count();
            (c, n)
        })
        .filter(|(_, n)| *n >= 2)
        .collect();
    scored.sort_by(|a, b| {
        // Higher count first; then `Formula::Ord` for total determinism.
        b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))
    });
    // Dedup polarity: if both `c` and `!c` survived as candidates, keep the un-negated.
    let mut seen: BTreeSet<Formula> = BTreeSet::new();
    let mut out: Vec<Formula> = Vec::new();
    for (c, _) in scored {
        let neg = predicates::not(c.clone());
        if seen.contains(&c) || seen.contains(&neg) {
            continue;
        }
        seen.insert(c.clone());
        out.push(c);
    }
    out
}

/// Emit a final `Structured` from a list of guarded items. `true` guards drop the
/// wrapper. Each remaining guard is `.simplify()`-ed at this point (and not earlier,
/// see [`structure_nmg`]) so the emitted form is minimal without sacrificing the
/// factor structure the refinement loop relied on.
fn emit_seq_from_items(items: Vec<(Formula, D::Structured)>) -> D::Structured {
    let mut out: Vec<D::Structured> = Vec::with_capacity(items.len());
    for (guard, body) in items {
        let g = guard.simplify();
        if g == predicates::true_() {
            out.push(body);
        } else if g != predicates::false_() {
            out.push(D::Structured::CondIf(g, Box::new(body), Box::new(None)));
        }
    }
    D::Structured::Seq(out)
}

/// The body of node `n` in the projection. Synthetic sinks emit exit-jumps (or empty for
/// whole-function); real nodes emit `Block(code)` (Code/Condition/Variants) or pull the
/// pre-structured form from `structured_blocks` (Reduced).
fn render_projection_node(
    n: NodeIndex,
    proj: &AcyclicProjection,
    structured_blocks: &BTreeMap<NodeIndex, D::Structured>,
    mode: Mode,
    entry: NodeIndex,
) -> D::Structured {
    if Some(n) == proj.back_edge_sink {
        if mode.emit_exit_jumps() {
            D::Structured::exit_jump(entry)
        } else {
            D::Structured::Seq(vec![])
        }
    } else if let Some(&target) = proj.exit_sinks.get(&n) {
        if mode.emit_exit_jumps() {
            D::Structured::exit_jump(target)
        } else {
            D::Structured::Seq(vec![])
        }
    } else {
        match proj.input.get(&n) {
            Some(D::Input::Code(_, code, _))
            | Some(D::Input::Condition(_, code, _, _))
            | Some(D::Input::Variants(_, code, _, _)) => D::Structured::Block(*code),
            Some(D::Input::Reduced(label, _)) => structured_blocks
                .get(label)
                .cloned()
                .unwrap_or_else(|| D::Structured::Seq(vec![])),
            None => D::Structured::Seq(vec![]),
        }
    }
}

/// Acyclic projection of an `input` map: original nodes with their edges to out-of-region
/// targets and to `entry` (back-edges) redirected to synthetic sinks, plus the sinks
/// themselves as terminal `Code(_, 0, None)` entries.
struct AcyclicProjection {
    input: BTreeMap<NodeIndex, D::Input>,
    /// Synthetic sink that absorbs back-edges to `entry`. `None` if no back-edges exist
    /// (whole-function mode).
    back_edge_sink: Option<NodeIndex>,
    /// Maps each synthetic exit sink to the original out-of-region target. The reverse
    /// map (target -> sink) is used during projection construction; we keep this direction
    /// because the rendering step needs target.
    exit_sinks: HashMap<NodeIndex, NodeIndex>,
}

fn build_acyclic_projection(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    members: &HashSet<NodeIndex>,
) -> AcyclicProjection {
    // 1. Discover unique exit targets and whether we need a back-edge sink.
    let in_projection = |n: NodeIndex| -> bool { members.contains(&n) || n == entry };
    let mut needs_back_edge_sink = false;
    let mut unique_exit_targets: Vec<NodeIndex> = Vec::new();
    let mut seen_targets: HashSet<NodeIndex> = HashSet::new();
    for (&node, inp) in input {
        if !in_projection(node) {
            continue;
        }
        for (_, v) in inp.edges() {
            if v == entry && members.contains(&node) {
                // Back-edge from inside.
                needs_back_edge_sink = true;
            } else if !in_projection(v) && seen_targets.insert(v) {
                unique_exit_targets.push(v);
            }
        }
    }

    // 2. Allocate synthetic sink ids past anything in `input`.
    let mut next_id = input.keys().map(|n| n.index() + 1).max().unwrap_or(0);
    let back_edge_sink = if needs_back_edge_sink {
        let id = NodeIndex::new(next_id);
        next_id += 1;
        Some(id)
    } else {
        None
    };
    // target -> sink (used to remap edges below).
    let mut target_to_sink: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    // sink -> target (kept on the projection for rendering).
    let mut exit_sinks: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    for target in unique_exit_targets {
        let id = NodeIndex::new(next_id);
        next_id += 1;
        target_to_sink.insert(target, id);
        exit_sinks.insert(id, target);
    }

    // 3. Build the projection: keep in-projection nodes, redirect their edges, add sinks.
    let remap = |v: NodeIndex, from_member: bool| -> NodeIndex {
        if let Some(sink) = back_edge_sink
            && v == entry
            && from_member
        {
            sink
        } else if let Some(&sink) = target_to_sink.get(&v) {
            sink
        } else {
            v
        }
    };
    let mut projection: BTreeMap<NodeIndex, D::Input> = BTreeMap::new();
    for (&node, inp) in input {
        if !in_projection(node) {
            continue;
        }
        let from_member = members.contains(&node);
        projection.insert(node, redirect_input(inp.clone(), |v| remap(v, from_member)));
    }
    if let Some(sink) = back_edge_sink {
        projection.insert(sink, D::Input::Code(sink, 0, None));
    }
    for &sink in exit_sinks.keys() {
        projection.insert(sink, D::Input::Code(sink, 0, None));
    }

    AcyclicProjection {
        input: projection,
        back_edge_sink,
        exit_sinks,
    }
}

/// Apply `f` to every edge target in `inp`, returning a fresh `Input` with the remapped
/// edges.
fn redirect_input(inp: D::Input, f: impl Fn(NodeIndex) -> NodeIndex) -> D::Input {
    match inp {
        D::Input::Condition(l, c, t, e) => D::Input::Condition(l, c, f(t), f(e)),
        D::Input::Variants(l, c, en, items) => D::Input::Variants(
            l,
            c,
            en,
            items.into_iter().map(|(v, t)| (v, f(t))).collect(),
        ),
        D::Input::Code(l, c, Some(n)) => D::Input::Code(l, c, Some(f(n))),
        D::Input::Code(l, c, None) => D::Input::Code(l, c, None),
        D::Input::Reduced(l, succs) => D::Input::Reduced(l, succs.into_iter().map(f).collect()),
    }
}

/// Topological order over the projection. Returns `None` if there's a cycle (shouldn't
/// happen for a valid projection; defensive).
fn topological_order(input: &BTreeMap<NodeIndex, D::Input>) -> Option<Vec<NodeIndex>> {
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

// Reaching conditions (No More Gotos, phase 1). For each node, the boolean formula over branch
// predicates under which control reaches it:
//
//     R(entry) = true
//     R(n)     = OR_{p -> n}  R(p) && cond(p -> n)
//
// Atoms are named via the `__c{N}` convention so locals reassigned across regions don't
// conflate.

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
        _ => predicates::true_(),
    }
}

/// Reaching conditions for every node of an acyclic region. `None` if the region has a cycle
/// or any `Variants` dispatch (not yet modeled).
pub fn reaching_conditions(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
) -> Option<BTreeMap<NodeIndex, Formula>> {
    if input.values().any(|i| matches!(i, D::Input::Variants(..))) {
        return None;
    }
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

// =================================================================================================
// Dominator-tree acyclic structurer
// =================================================================================================
// Builds an IfElse/Switch by absorbing each arm target that's an immediate dom-tree child of
// the conditional, emitting `Jump` for arms targeting the post-dominator or outside the dom
// subtree. Orphan ichildren (still in `structured_blocks` after arm processing) get appended
// as siblings. Loop-body RPO adjacency is handled in `loops::structure_loop`'s assembly.

/// Dom-tree dispatch for a single node. Latches go to `structure_latch_node`; everything
/// else to `structure_acyclic_region`. Used by the orchestrator's post-order pass and by
/// `loops::structure_loop`'s body assembly when reaching can't handle the body's shape.
pub(super) fn structure_acyclic_node(
    ctx: StructureContext<'_>,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    node: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_successor: Option<NodeIndex>,
) {
    let config = ctx.config;
    if graph.back_edges.contains_key(&node) {
        let result = structure_latch_node(config, graph, node, input[&node].clone());
        structured_blocks.insert(node, result);
    } else {
        let result = structure_acyclic_region(
            config,
            graph,
            structured_blocks,
            input,
            node,
            loop_successor,
        );
        structured_blocks.insert(node, result);
    }
}

/// A CFG node with no outgoing edges - i.e. terminated by `return`/`abort`.
fn is_cfg_sink(target: NodeIndex, cfg: &petgraph::graph::DiGraph<(), ()>) -> bool {
    cfg.neighbors_directed(target, Direction::Outgoing).count() == 0
}

/// A node is "singly entered" iff exactly one of its CFG predecessors lies outside its own
/// dom subtree. Predecessors inside the subtree are back-edges from a contained loop's
/// latch; they don't represent independent entry into the scope. The `target` itself is part
/// of the subtree so a self-loop's self-edge counts as a back-edge. This is the criterion
/// `arm_for` uses to decide whether an arm body owns its target outright (embed) or whether
/// the target is a shared join point that other paths also reach (sibling-hoist).
fn is_singly_entered(
    target: NodeIndex,
    cfg: &petgraph::graph::DiGraph<(), ()>,
    dom_tree: &dom_tree::DominatorTree,
) -> bool {
    let subtree: HashSet<NodeIndex> = dom_tree
        .get(target)
        .all_children()
        .chain(std::iter::once(target))
        .collect();
    cfg.neighbors_directed(target, Direction::Incoming)
        .filter(|pred| !subtree.contains(pred))
        .count()
        == 1
}

fn structure_acyclic_region(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    input: &mut BTreeMap<D::Label, D::Input>,
    start: NodeIndex,
    loop_successor: Option<NodeIndex>,
) -> D::Structured {
    // Code has no arms and no post-dominator role; delegate before the dom-tree work.
    if matches!(&input[&start], D::Input::Code(..)) {
        let code_node = input[&start].clone();
        return structure_code_node(config, graph, structured_blocks, start, code_node);
    }

    let dom_node = graph.dom_tree.get(start);
    let ichildren = dom_node.immediate_children().collect::<HashSet<_>>();

    if config.debug_print.structuring {
        println!("structuring acyclic region at node {start:#?}");
        println!("  blocks: {structured_blocks:#?}");
        println!("  immediate children: {ichildren:#?}");
        if let Some(s) = loop_successor {
            println!("  loop successor: {s:#?}");
        }
    }

    let enclosing_loop_exits: Option<HashSet<NodeIndex>> = graph.loop_exits.get(&start).cloned();

    /// Classify one Condition/Switch arm:
    ///   - `target == start`: back-edge to a loop head; emit `Jump(DegenerateJumpIf)`.
    ///   - `loop_successor == Some(target)`: loop-head arm exits the loop. Emit `Jump(LoopBreak)`
    ///     for `insert_breaks` to convert to `Break`.
    ///   - At a loop head, `target \in loop_exits /\ sink (abort/return)`: embed inline.
    ///     `structure_loop` only appends one loop successor after the `Loop` form, and
    ///     the orphan hoist runs only for non-loop calls; without this branch, a sink
    ///     that's an extra loop exit gets neither placement and its arm-Jump survives
    ///     as a goto. Embedding here keeps the abort inline so `recover_asserts` can fire.
    ///   - `target` is an exit of an enclosing loop (and we're *not* at the head of that
    ///     loop): emit `Jump(ArmOutsideSubtree)`. The outer `structure_loop` will append
    ///     `target` after its `Loop` form, and `insert_breaks` will rewrite this Jump to a
    ///     `Break`. We must not embed even if `target` is singly entered - that would bury
    ///     the loop exit inside the body.
    ///   - `target in ichildren` and `target` is singly-entered (the only CFG predecessor
    ///     outside its own dom subtree is the edge from `start`): embed the structured form
    ///     as the arm body. Back-edges from inside `target`'s subtree don't count - a loop
    ///     head is entered once from outside, even though its latch loops back in.
    ///   - Otherwise: emit `Jump(ArmOutsideSubtree)`. If `target` is a join point in our
    ///     scope, the owned-children hoist below places it as a sibling and elides this
    ///     Jump. If `target` is owned by an ancestor scope, the Jump survives for that
    ///     scope's hoist or `insert_breaks`.
    fn arm_for(
        graph: &Graph,
        structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
        ichildren: &HashSet<NodeIndex>,
        loop_successor: Option<NodeIndex>,
        loop_exits: Option<&HashSet<NodeIndex>>,
        start: NodeIndex,
        target: NodeIndex,
    ) -> (D::Structured, bool) {
        if target == start {
            (
                D::Structured::Jump(GotoSource::DegenerateJumpIf, target),
                false,
            )
        } else if Some(target) == loop_successor {
            (D::Structured::Jump(GotoSource::LoopBreak, target), false)
        } else if loop_successor.is_some()
            && loop_exits.is_some_and(|e| e.contains(&target))
            && is_cfg_sink(target, &graph.cfg)
            && ichildren.contains(&target)
            && structured_blocks.contains_key(&target)
        {
            (structured_blocks.remove(&target).unwrap(), false)
        } else if loop_exits.is_some_and(|e| e.contains(&target)) {
            (
                D::Structured::Jump(GotoSource::ArmOutsideSubtree, target),
                false,
            )
        } else if ichildren.contains(&target)
            && is_singly_entered(target, &graph.cfg, &graph.dom_tree)
        {
            (structured_blocks.remove(&target).unwrap(), true)
        } else {
            (
                D::Structured::Jump(GotoSource::ArmOutsideSubtree, target),
                false,
            )
        }
    }

    let mut absorbed_arms: Vec<NodeIndex> = Vec::new();
    let structured = match input[&start].clone() {
        D::Input::Condition(_lbl, code, conseq, alt) => {
            let (conseq_arm, conseq_absorbed) = arm_for(
                graph,
                structured_blocks,
                &ichildren,
                loop_successor,
                enclosing_loop_exits.as_ref(),
                start,
                conseq,
            );
            let (alt_arm, alt_absorbed) = arm_for(
                graph,
                structured_blocks,
                &ichildren,
                loop_successor,
                enclosing_loop_exits.as_ref(),
                start,
                alt,
            );

            if conseq_absorbed {
                absorbed_arms.push(conseq);
            }
            if alt_absorbed {
                absorbed_arms.push(alt);
            }
            if !absorbed_arms.is_empty() {
                graph.update_latch_branch_nodes(start, absorbed_arms.clone());
            }

            graph.mark_emitted(code);
            D::Structured::CondIf(
                predicates::cond_atom(code),
                Box::new(conseq_arm),
                Box::new(Some(alt_arm)),
            )
        }
        D::Input::Variants(_lbl, code, enum_, items) => {
            let latches = items
                .iter()
                .map(|(_v, item)| item)
                .filter(|item| Some(**item) != loop_successor)
                .cloned()
                .collect::<Vec<_>>();
            graph.update_latch_branch_nodes(start, latches);

            // Variant arms can share a target (e.g. several variants branching to the same
            // fall-through label). `arm_for`'s `ichildren`-embed path calls
            // `structured_blocks.remove(&target).unwrap()`, which would panic on the second
            // occurrence. With the in-degree-1 absorb predicate, a shared target also has
            // in-degree > 1, so `arm_for` naturally emits `Jump(AOS)` for each. We keep the
            // explicit count guard as a defensive belt-and-suspenders: even if some future
            // refactor weakens the predicate, the panic stays at bay.
            let mut counts: HashMap<NodeIndex, usize> = HashMap::new();
            for (_, item) in &items {
                *counts.entry(*item).or_insert(0) += 1;
            }
            let arms = items
                .into_iter()
                .map(|(v, item)| {
                    let body = if counts.get(&item).copied().unwrap_or(0) > 1 {
                        D::Structured::Jump(GotoSource::ArmOutsideSubtree, item)
                    } else {
                        let (arm, absorbed) = arm_for(
                            graph,
                            structured_blocks,
                            &ichildren,
                            loop_successor,
                            enclosing_loop_exits.as_ref(),
                            start,
                            item,
                        );
                        if absorbed {
                            absorbed_arms.push(item);
                        }
                        arm
                    };
                    (v, body)
                })
                .collect();
            graph.mark_emitted(code);
            // Maybe we could reconstruct matches from the arms? It would require a lot more -
            // and more painful - analysis.
            D::Structured::Switch(code, enum_, arms)
        }
        D::Input::Code(..) => unreachable!("Code shortcut at top of structure_acyclic_region"),
        D::Input::Reduced(label, _) => {
            // Already-structured abstract node arrived via the dom-tree path. Its structured
            // form lives in `structured_blocks`; emit it verbatim. The orphan hoist below
            // still runs normally for any dom-tree children we own (e.g. a post-loop block
            // that's a dom-child of `start`).
            structured_blocks
                .remove(&label)
                .expect("Reduced(label, _) must have structured_blocks[label]")
        }
    };

    // Hoist orphan dom-tree children. After arm processing, any `ichildren` of `start` that
    // weren't absorbed as arms and weren't the loop successor remain in `structured_blocks`.
    // They're "owned" by us - every CFG path to them goes through `start` - so they
    // semantically belong in our sequence. Append them as siblings; surviving tail `Jump`s
    // to them flow to `goto_to_break` for labeled-break rewriting.
    //
    // We always hoist (otherwise the orphan leaks - its idom is `start`, no ancestor scope
    // sees it as an ichild). We skip the hoist at loop heads: the loop's successor stays in
    // `structured_blocks` so `structure_loop` can append it after the `Loop` form, and
    // body-side ichildren are placed by the body-assembly logic. We also skip orphans that
    // are succ_nodes of an enclosing loop - `structure_loop` for that outer loop will append
    // them after its `Loop`, so we mustn't eat them at this inner level.
    let mut exp = vec![structured];
    if loop_successor.is_none() {
        let mut orphans: Vec<NodeIndex> = ichildren
            .iter()
            .copied()
            .filter(|c| structured_blocks.contains_key(c))
            .filter(|c| {
                enclosing_loop_exits
                    .as_ref()
                    .map(|exits| !exits.contains(c))
                    .unwrap_or(true)
            })
            .collect();
        orphans.sort_by_key(|n| n.index());
        hoist_orphans(graph, start, orphans, structured_blocks, &mut exp);
    }
    D::Structured::Seq(exp)
}

/// Place each orphan as a sibling of `seq`'s existing items in CFG-topo order over the
/// orphan-induced subgraph (`hoist_order`). Surviving tail `Jump`s flow to `goto_to_break`
/// downstream for labeled-break rewriting.
///
/// `orphans` should already be the filtered + sorted list (caller is closer to the source
/// data - `ichildren` + scope-specific exclusions like `Some(c) != next` in
/// `structure_code_node`).
fn hoist_orphans(
    graph: &Graph,
    start: NodeIndex,
    orphans: Vec<NodeIndex>,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    seq: &mut Vec<D::Structured>,
) {
    for orphan in hoist_order(graph, start, &orphans) {
        let body = structured_blocks.remove(&orphan).unwrap();
        seq.push(body);
    }
}

/// Order orphan ichildren of `start` by CFG-reachability: each orphan should appear in the
/// Seq after the orphan(s) whose subtrees branch to it. Topological sort over the subgraph
/// induced by `orphans`, breaking cycles by index. In practice the orphan set is tiny
/// (typically just one - the post-dom of the IfElse) and any ordering works.
fn hoist_order(graph: &Graph, _start: NodeIndex, orphans: &[NodeIndex]) -> Vec<NodeIndex> {
    let cfg = &graph.cfg;
    let set: HashSet<NodeIndex> = orphans.iter().copied().collect();
    // `BTreeMap` for deterministic initial iteration when seeding the ready queue.
    let mut in_deg: BTreeMap<NodeIndex, usize> = orphans.iter().map(|&n| (n, 0)).collect();
    for &n in orphans {
        for succ in cfg.neighbors(n) {
            if set.contains(&succ) && succ != n {
                *in_deg.entry(succ).or_insert(0) += 1;
            }
        }
    }
    let mut ready: Vec<NodeIndex> = in_deg
        .iter()
        .filter_map(|(&n, &d)| (d == 0).then_some(n))
        .collect();
    // Sort descending so `pop()` returns the *smallest* index first.
    ready.sort_by_key(|n| std::cmp::Reverse(n.index()));
    let mut out = Vec::new();
    while let Some(n) = ready.pop() {
        out.push(n);
        for succ in cfg.neighbors(n) {
            if let Some(d) = in_deg.get_mut(&succ) {
                *d -= 1;
                if *d == 0 {
                    ready.push(succ);
                    ready.sort_by_key(|n: &NodeIndex| std::cmp::Reverse(n.index()));
                }
            }
        }
    }
    // Anything left is on a cycle; append by index for determinism.
    let mut remaining: Vec<NodeIndex> = orphans
        .iter()
        .copied()
        .filter(|n| !out.contains(n))
        .collect();
    remaining.sort_by_key(|n| n.index());
    out.extend(remaining);
    out
}

fn structure_latch_node(
    config: &config::Config,
    graph: &mut Graph,
    node_ndx: NodeIndex,
    node: D::Input,
) -> D::Structured {
    if config.debug_print.structuring {
        println!("structuring latch node {node_ndx:#?}");
    }
    assert!(graph.back_edges.contains_key(&node_ndx));
    match node {
        D::Input::Condition(_, code, conseq, alt) => {
            graph.mark_emitted(code);
            D::Structured::JumpIf(GotoSource::LatchTest, code, conseq, alt)
        }
        D::Input::Code(_, code, next) => {
            graph.mark_emitted(code);
            D::Structured::Seq(vec![
                D::Structured::Block(code),
                D::Structured::Jump(GotoSource::LatchCode, next.unwrap()),
            ])
        }
        D::Input::Variants(_, _, _, _) => unreachable!(),
        D::Input::Reduced(_, _) => unreachable!("Reduced never appears as a latch node"),
    }
}

fn structure_code_node(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    node_ndx: NodeIndex,
    node: D::Input,
) -> D::Structured {
    if config.debug_print.structuring {
        println!("structuring code node: {node:#?}");
    }
    match node {
        D::Input::Code(_, code, Some(next)) if next == node_ndx => {
            graph.mark_emitted(code);
            D::Structured::Seq(vec![
                D::Structured::Block(code),
                D::Structured::Jump(GotoSource::SelfLoop, next),
            ])
        }
        D::Input::Code(_, code, next) => {
            // Fuse `next` only if it's our exclusive dom-tree child - i.e. it's in
            // `ichildren` and singly entered (no other path from outside its own subtree
            // reaches it). For Code nodes specifically, `ichildren.contains(&next)` already
            // implies single-entry (Code has only one CFG successor, so any other
            // predecessor of `next` would prevent `next in ichildren`); we spell the
            // `is_singly_entered` check explicitly so the principle reads identically to
            // `arm_for`'s: fold the target into our region iff control enters it
            // exclusively through this edge. The `enclosing_loop_exits` guard prevents
            // burying a loop-exit body inside an inner Code block; `structure_loop` will
            // append it after its `Loop`.
            let ichildren: HashSet<NodeIndex> =
                graph.dom_tree.get(node_ndx).immediate_children().collect();
            let enclosing_loop_exits: Option<HashSet<NodeIndex>> =
                graph.loop_exits.get(&node_ndx).cloned();
            graph.mark_emitted(code);
            let mut seq = vec![D::Structured::Block(code)];
            match next {
                Some(next)
                    if ichildren.contains(&next)
                        && structured_blocks.contains_key(&next)
                        && is_singly_entered(next, &graph.cfg, &graph.dom_tree)
                        && !enclosing_loop_exits
                            .as_ref()
                            .is_some_and(|e| e.contains(&next)) =>
                {
                    let successor = structured_blocks.remove(&next).unwrap();
                    graph.update_latch_nodes(node_ndx, next);
                    seq.push(successor);
                }
                Some(next) => {
                    // `next` is not exclusively ours - either it's reached from other
                    // paths or it's owned by an enclosing structure. Emit an explicit
                    // `Jump(CodeBranch)` so the owned-children hoist or `insert_breaks`
                    // can see and rewrite it. Without this, the branch lives only in the
                    // bytecode terminator and is invisible to elision.
                    seq.push(D::Structured::Jump(GotoSource::CodeBranch, next));
                }
                None => {}
            }

            // Owned-children hoist: same shape as `structure_acyclic_region`'s - place any
            // remaining `ichildren` as siblings. In practice a Code node's `ichildren` is
            // `{}` or `{next}`, so this loop is usually empty; we run it for symmetry and so
            // any future CFG with a Code node dominating more than its `next` still gets a
            // consistent placement.
            let mut orphans: Vec<NodeIndex> = ichildren
                .iter()
                .copied()
                .filter(|c| Some(*c) != next)
                .filter(|c| structured_blocks.contains_key(c))
                .filter(|c| {
                    enclosing_loop_exits
                        .as_ref()
                        .map(|exits| !exits.contains(c))
                        .unwrap_or(true)
                })
                .collect();
            orphans.sort_by_key(|n| n.index());
            hoist_orphans(graph, node_ndx, orphans, structured_blocks, &mut seq);
            let mut result = D::Structured::Seq(seq);
            super::flatten_sequence(&mut result);
            result
        }
        D::Input::Condition(..) | D::Input::Variants(..) | D::Input::Reduced(..) => {
            unreachable!()
        }
    }
}
