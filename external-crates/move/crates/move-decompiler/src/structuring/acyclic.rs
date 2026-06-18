// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Reaching-condition acyclic structuring (No More Gotos): walk the region forward, fold
// duplicate-arm branches via `fold_duplicate_arm_branches`, emit the recovered structure.
// Returns `None` on anything unrecognized so the dom-tree structurer owns it.

use crate::config;
use crate::structuring::{
    ast::{self as D, GotoSource},
    predicates::{self, Formula},
};
use petgraph::algo::{
    self,
    dominators::{self, Dominators},
};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Structure an acyclic whole-function region. Returns `None` to fall back to the dom-tree
/// structurer when nothing folds.
pub fn structure(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
) -> Option<D::Structured> {
    structure_inner(config, terms, input, entry, Mode::WholeFunction)
}

/// Same as [`structure`] but for loop-body regions: edges leaving the region become
/// `Jump(ReachingExit, ...)` for `insert_breaks` to rewrite as `Break`/`Continue`.
pub fn structure_with_exit_jumps(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    exit: NodeIndex,
) -> Option<D::Structured> {
    structure_inner(config, terms, input, entry, Mode::LoopBody { exit })
}

/// Closed dichotomy: whole-function or loop-body. Encoded as one enum so we can't construct
/// the invalid "exit, no jumps" or "jumps, no exit" combinations.
#[derive(Clone, Copy)]
enum Mode {
    WholeFunction,
    LoopBody { exit: NodeIndex },
}

impl Mode {
    fn region_exit(self) -> Option<NodeIndex> {
        match self {
            Mode::WholeFunction => None,
            Mode::LoopBody { exit } => Some(exit),
        }
    }

    fn emit_exit_jumps(self) -> bool {
        matches!(self, Mode::LoopBody { .. })
    }
}

fn structure_inner(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    mode: Mode,
) -> Option<D::Structured> {
    let pdom = PostDom::build(input, mode.region_exit())?;
    let mut ctx = Ctx {
        config,
        input,
        pdom,
        folded_any: false,
        mode,
        visiting: HashSet::new(),
        terms,
    };
    // Bypass the `node == stop` short-circuit on the region's entry: for a loop body where
    // `entry == exit`, we still need the entry's content emitted once. Recursive descent
    // through `structure_reachable_subregion` honors `stop` from there on.
    let body = ctx.process_node(entry, mode.region_exit())?;
    // Only take over when we actually folded; otherwise let the dom-tree structurer keep its
    // existing output.
    if !ctx.folded_any {
        return None;
    }
    Some(body)
}

struct Ctx<'a> {
    config: &'a config::Config,
    input: &'a BTreeMap<NodeIndex, D::Input>,
    pdom: PostDom,
    folded_any: bool,
    mode: Mode,
    /// Nodes currently being processed by `process_node`. A revisit means the region has an
    /// inner cycle (typically a nested loop in our `input` snapshot), so we bail.
    visiting: HashSet<NodeIndex>,
    /// Per-block lowered `Exp`, consulted by `bodies_equivalent`.
    terms: &'a BTreeMap<NodeIndex, crate::ast::Exp>,
}

impl Ctx<'_> {
    fn emit_exit_jumps(&self) -> bool {
        self.mode.emit_exit_jumps()
    }

    fn region_exit(&self) -> Option<NodeIndex> {
        self.mode.region_exit()
    }

    /// Build the `Structured` form rooted at `node`, walking forward up to (but not including)
    /// `stop`. Returns `Some(Seq([]))` when `node == stop`; `None` on failure (cycle, Variants,
    /// arm escape without a shared join).
    fn structure_reachable_subregion(
        &mut self,
        node: NodeIndex,
        stop: Option<NodeIndex>,
    ) -> Option<D::Structured> {
        if Some(node) == stop {
            return Some(D::Structured::Seq(vec![]));
        }
        self.process_node(node, stop)
    }

    fn process_node(&mut self, node: NodeIndex, stop: Option<NodeIndex>) -> Option<D::Structured> {
        if !self.visiting.insert(node) {
            if self.config.debug_print.structuring {
                println!("reaching: revisited node {node:?} (region cycle); bailing");
            }
            return None;
        }
        let result = self.process_node_inner(node, stop);
        self.visiting.remove(&node);
        result
    }

    fn process_node_inner(
        &mut self,
        node: NodeIndex,
        stop: Option<NodeIndex>,
    ) -> Option<D::Structured> {
        match self.input.get(&node)? {
            D::Input::Variants(..) => None,
            D::Input::Code(_, code, next) => {
                let head = D::Structured::Block(*code);
                match next {
                    None => Some(head),
                    Some(next) if !self.in_region(*next) => {
                        // Edge leaves the region. Loop-body: emit a `Jump` for `insert_breaks`
                        // to rewrite. Whole-function: the region IS the function, so a non-
                        // region edge can only be a CFG sink (return/abort); the assert locks
                        // that in, otherwise we'd silently swallow control flow.
                        if self.emit_exit_jumps() {
                            Some(seq(head, exit_jump(*next)))
                        } else {
                            debug_assert!(
                                self.input
                                    .get(next)
                                    .map(|i| i.edges().is_empty())
                                    .unwrap_or(true),
                                "WholeFunction: dropping non-sink edge {node:?} -> {next:?}",
                            );
                            Some(head)
                        }
                    }
                    Some(next) if Some(*next) == stop => Some(head),
                    Some(next) => {
                        let rest = self.structure_reachable_subregion(*next, stop)?;
                        Some(seq(head, rest))
                    }
                }
            }
            D::Input::Condition(_, code, then, els) => {
                let (code, then, els) = (*code, *then, *els);
                if let Some(fold) = self.fold_duplicate_arm_branches(node, then, els) {
                    self.folded_any = true;
                    // then = kept arm body, else = continuation K up to far join J. J itself
                    // is emitted after the CondIf so both arms reach it exactly once.
                    let then_body = fold.arm_body;
                    let else_body =
                        self.structure_reachable_subregion(fold.continue_at, Some(fold.far_join))?;
                    let cond_if = D::Structured::CondIf(
                        fold.cond,
                        Box::new(then_body),
                        Box::new(non_empty(else_body)),
                    );
                    let rest = self.structure_reachable_subregion(fold.far_join, stop)?;
                    Some(seq(cond_if, rest))
                } else {
                    // Genuine branch: structure both arms up to where they rejoin, then continue.
                    let join = self.pdom.ipostdom(node);
                    let then_s = self.structure_arm_target(then, join)?;
                    let els_s = self.structure_arm_target(els, join)?;
                    let if_s = D::Structured::CondIf(
                        predicates::cond_atom(code),
                        Box::new(then_s),
                        Box::new(non_empty(els_s)),
                    );
                    match join {
                        Some(j) => Some(seq(if_s, self.structure_reachable_subregion(j, stop)?)),
                        None => Some(if_s),
                    }
                }
            }
        }
    }

    /// Structure one branch's arm-target. In-region targets recur; out-of-region targets
    /// emit an exit Jump (loop-body mode) or bail (whole-function mode, where there's no
    /// outer scope to land in).
    fn structure_arm_target(
        &mut self,
        target: NodeIndex,
        join: Option<NodeIndex>,
    ) -> Option<D::Structured> {
        if self.in_region(target) {
            self.structure_reachable_subregion(target, join)
        } else if self.emit_exit_jumps() {
            Some(exit_jump(target))
        } else {
            None
        }
    }

    /// True iff `node` is part of the structured region: it lives in `self.input` AND is not
    /// the loop-body `exit` (back-edge target).
    fn in_region(&self, node: NodeIndex) -> bool {
        self.input.contains_key(&node) && Some(node) != self.region_exit()
    }

    /// Recognize and fold the "duplicate-arm branch" pattern: a binary outer condition whose
    /// two arms each run an inner condition, and both inner conditions fire the *same* action
    /// when they pass. The Pyth-style example, in source:
    ///
    ///     if (a > b) { if (a - b >= t) { x } } else { if (b - a >= t) { x } }
    ///
    /// The split-by-sign exists because `a - b` and `b - a` underflow under opposite outer
    /// guards; the action `x` is the same on both sides. Fold into one `if` whose recovered
    /// boolean preserves the outer guard so each inner check still short-circuits in the world
    /// its operand direction is safe in:
    ///
    ///     if (a > b && a - b >= t || !(a > b) && b - a >= t) { x }
    ///
    /// Not `if (I1 || I2)` -- that would evaluate the unsafe operand direction.
    ///
    /// CFG shape and names used below:
    ///
    ///      node ---then--> I1 --{A1 -> J,  K}
    ///           ---else--> I2 --{A2 -> J,  K}
    ///
    ///   node    outer condition
    ///   I1, I2  inner conditions, one per outer arm
    ///   A1, A2  the two duplicate arms (the blocks that fire when the inner check passes)
    ///   J       the far join where both A1 and A2 land
    ///   K       the shared "neither arm fires" continuation
    ///
    /// Returns `None` unless the shape matches and `bodies_equivalent(A1, A2)` -- the latter
    /// is the soundness guard for dropping A2's body and keeping only A1's.
    fn fold_duplicate_arm_branches(
        &mut self,
        node: NodeIndex,
        then: NodeIndex,
        els: NodeIndex,
    ) -> Option<DuplicateArmFold> {
        let (i1c, i1t, i1e) = self.as_condition(then)?;
        let (i2c, i2t, i2e) = self.as_condition(els)?;
        // K = the single node both inner conditions branch to; each inner condition's other
        // arm is the candidate duplicate-arm head (A1 / A2).
        let k = [i1t, i1e].into_iter().find(|x| *x == i2t || *x == i2e)?;
        if (i1t == k) == (i1e == k) {
            return None; // both or neither inner-1 arm is the continuation
        }
        let a1 = if i1t == k { i1e } else { i1t };
        let a2 = if i2t == k { i2e } else { i2t };
        // Track which polarity of each inner check fires the duplicate-arm side, so the
        // recovered boolean preserves the outer guard's gating.
        let a1_then = a1 == i1t;
        let a2_then = a2 == i2t;
        // Both A1, A2 chains must end at the same far join.
        let (a1_codes, j1) = self.code_chain_to(a1, k)?;
        let (a2_codes, j2) = self.code_chain_to(a2, k)?;
        if j1 != j2 {
            return None;
        }
        if !bodies_equivalent(&a1_codes, &a2_codes, self.terms) {
            return None;
        }
        // Recovered boolean: `(node && I1) || (!node && I2)`, with I1/I2 polarity-corrected
        // to the side that fires the duplicate arm. Smart constructors normalize the result.
        let a_node = predicates::cond_atom(node.index() as u64);
        let cond = predicates::or(vec![
            predicates::and(vec![a_node.clone(), atom_pol(i1c, a1_then)]),
            predicates::and(vec![predicates::not(a_node), atom_pol(i2c, a2_then)]),
        ]);
        Some(DuplicateArmFold {
            cond,
            arm_body: blocks_seq(&a1_codes),
            continue_at: k,
            far_join: j1,
        })
    }

    fn as_condition(&self, n: NodeIndex) -> Option<(u64, NodeIndex, NodeIndex)> {
        match self.input.get(&n)? {
            D::Input::Condition(_, code, t, e) => Some((*code, *t, *e)),
            _ => None,
        }
    }

    /// Follow a straight-line `Code` chain from `start`, collecting block ids, until we either
    /// hit `stop` (the shared continuation `K`), leave the region, or run into a non-Code
    /// node (a `Condition`, a sink, or the loop-head back-edge target). Returns the collected
    /// code-block ids and the stopping node. Returns `None` if `start` isn't a Code chain at
    /// all.
    fn code_chain_to(&self, start: NodeIndex, stop: NodeIndex) -> Option<(Vec<u64>, NodeIndex)> {
        let mut codes = Vec::new();
        let mut cur = start;
        loop {
            match self.input.get(&cur)? {
                D::Input::Code(_, code, Some(next)) => {
                    codes.push(*code);
                    if *next == stop || !self.in_region(*next) {
                        return Some((codes, *next));
                    }
                    match self.input.get(next)? {
                        D::Input::Code(_, _, Some(_)) => cur = *next,
                        _ => return Some((codes, *next)),
                    }
                }
                _ => return None,
            }
        }
    }
}

struct DuplicateArmFold {
    cond: predicates::Formula,
    arm_body: D::Structured,
    continue_at: NodeIndex,
    far_join: NodeIndex,
}

/// Structural-equivalence guard for the two arm bodies. Drops empty / Goto-only padding
/// (CFG-anchor blocks the compiler sometimes emits on one arm but not the other) before
/// comparing the rest pairwise via `exp_struct_eq`.
///
/// Purely structural on pre-refinement `Exp`. Conservative-by-design: `{ x = a + 0 }` vs
/// `{ x = a }` fails (semantic equality is out of scope), and reordered statements fail
/// (comparison is positional). See `non_uniform_arms` in
/// `tests/move/staleness/sources/staleness.move` for the deliberate-failure fixture.
fn bodies_equivalent(
    s1_codes: &[u64],
    s2_codes: &[u64],
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
) -> bool {
    use crate::ast::Exp;
    use crate::ast::UnstructuredNode;
    fn is_padding(exp: &Exp) -> bool {
        match exp {
            Exp::Seq(items) if items.is_empty() => true,
            Exp::Seq(items) if items.len() == 1 => matches!(
                &items[0],
                Exp::Unstructured(nodes)
                    if nodes.len() == 1
                        && matches!(&nodes[0], UnstructuredNode::Goto(_))
            ),
            Exp::Unstructured(nodes) => {
                nodes.len() == 1 && matches!(&nodes[0], UnstructuredNode::Goto(_))
            }
            _ => false,
        }
    }
    let body_of = |code: u64| -> Option<&Exp> { terms.get(&NodeIndex::new(code as usize)) };
    let s1: Vec<&Exp> = s1_codes
        .iter()
        .filter_map(|c| body_of(*c))
        .filter(|e| !is_padding(e))
        .collect();
    let s2: Vec<&Exp> = s2_codes
        .iter()
        .filter_map(|c| body_of(*c))
        .filter(|e| !is_padding(e))
        .collect();
    s1.len() == s2.len()
        && s1
            .iter()
            .zip(s2.iter())
            .all(|(a, b)| crate::ast::exp_eq::exp_struct_eq(a, b))
}

fn blocks_seq(codes: &[u64]) -> D::Structured {
    if codes.len() == 1 {
        D::Structured::Block(codes[0])
    } else {
        D::Structured::Seq(codes.iter().map(|c| D::Structured::Block(*c)).collect())
    }
}

fn exit_jump(target: NodeIndex) -> D::Structured {
    D::Structured::Jump(GotoSource::ReachingExit, target)
}

fn atom_pol(code: u64, positive: bool) -> predicates::Formula {
    let atom = predicates::cond_atom(code);
    if positive {
        atom
    } else {
        predicates::not(atom)
    }
}

fn seq(head: D::Structured, tail: D::Structured) -> D::Structured {
    let mut items = Vec::new();
    let mut push = |s: D::Structured| match s {
        D::Structured::Seq(v) => items.extend(v),
        other => items.push(other),
    };
    push(head);
    push(tail);
    if items.len() == 1 {
        items.pop().unwrap()
    } else {
        D::Structured::Seq(items)
    }
}

fn non_empty(s: D::Structured) -> Option<D::Structured> {
    match &s {
        D::Structured::Seq(v) if v.is_empty() => None,
        _ => Some(s),
    }
}

// Region-local post-dominators. We build them per-region (always a strict subset of the
// function's blocks) rather than carrying a global pdom tree, so the cost is bounded by region
// size and `Graph` doesn't have to plumb the synthetic-exit graph for a structurer that may
// not run.

struct PostDom {
    doms: Dominators<NodeIndex>,
    exit_internal: NodeIndex,
    to_internal: HashMap<NodeIndex, NodeIndex>,
    from_internal: Vec<Option<NodeIndex>>,
}

impl PostDom {
    /// Build post-dominators by running the dominator algorithm on the reversed region CFG,
    /// rooted at a synthetic exit that absorbs every escape (edge to `region_exit` or to a
    /// node not in `input`). `None` if no node reaches a sink.
    fn build(
        input: &BTreeMap<NodeIndex, D::Input>,
        region_exit: Option<NodeIndex>,
    ) -> Option<Self> {
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
        let in_region = |n: NodeIndex| input.contains_key(&n) && Some(n) != region_exit;
        for (n, inp) in input {
            let u_int = to_internal[n];
            let succs = inp.edges();
            if succs.is_empty() {
                rev.add_edge(exit_internal, u_int, ());
                has_sink = true;
                continue;
            }
            for (u, v) in succs {
                debug_assert_eq!(u, *n, "Input::edges always sources from the input's label");
                if in_region(v) {
                    rev.add_edge(to_internal[&v], u_int, ());
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

    /// Immediate post-dominator (where this branch's arms rejoin), or `None` if the arms
    /// don't rejoin before the function returns.
    fn ipostdom(&self, node: NodeIndex) -> Option<NodeIndex> {
        let n_int = *self.to_internal.get(&node)?;
        match self.doms.immediate_dominator(n_int) {
            Some(ip) if ip != self.exit_internal => self.from_internal[ip.index()],
            _ => None,
        }
    }
}

// Reaching conditions (No More Gotos, phase 1). For each node, the boolean formula over branch
// predicates under which control reaches it:
//
//     R(entry) = true
//     R(n)     = OR_{p -> n}  R(p) && cond(p -> n)
//
// Atoms are named via the `__c{N}` convention so locals reassigned across regions don't
// conflate. Folding the guarded sequence back into source-level booleans is the rest of the
// file (`Ctx::fold_duplicate_arm_branches` and friends).

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

    let mut graph: DiGraph<(), ()> = DiGraph::new();
    let mut to_internal: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(input.len());
    let mut from_internal: Vec<NodeIndex> = Vec::with_capacity(input.len());
    let mut nodes: BTreeSet<NodeIndex> = input.keys().copied().collect();
    for inp in input.values() {
        for (u, v) in inp.edges() {
            nodes.insert(u);
            nodes.insert(v);
        }
    }
    for n in &nodes {
        let idx = graph.add_node(());
        to_internal.insert(*n, idx);
        from_internal.push(*n);
    }
    let mut preds: BTreeMap<NodeIndex, Vec<NodeIndex>> = BTreeMap::new();
    for inp in input.values() {
        for (u, v) in inp.edges() {
            graph.add_edge(to_internal[&u], to_internal[&v], ());
            preds.entry(v).or_default().push(u);
        }
    }

    let topo = algo::toposort(&graph, None).ok()?;

    // Forward-propagate: in a DAG every predecessor precedes its successor in topo order.
    let mut reach: BTreeMap<NodeIndex, Formula> = BTreeMap::new();
    reach.insert(entry, predicates::true_());
    for internal in topo {
        let n = from_internal[internal.index()];
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
