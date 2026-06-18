// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Reaching-condition acyclic structuring (No More Gotos)
// -------------------------------------------------------------------------------------------------
// For a loop-free function whose dom-tree structuring would emit a goto for a guarded forward
// skip, build the clean nested form directly. We recognize "skip diamonds" and fold each into a
// single compound `CondIf`, nesting the continuation in the `else`. The skip never becomes a
// goto: it's the fall-through after the `CondIf`'s then-branch.
//
// This is deliberately conservative: it returns `None` (handing back to the dom-tree structurer)
// on any shape it doesn't recognize, so it only ever *replaces* output that currently has a goto.
//
// -------------------------------------------------------------------------------------------------
// Vocabulary
// -------------------------------------------------------------------------------------------------
// Throughout this file, a **skip diamond** is the bytecode shape produced by abs-diff-style
// idioms like `abs_diff(a, b)`, `max(a, b)`, and Pyth-style threshold guards: a check whose two
// arms each set a flag (or compute the same value with swapped operands) and either skip to a
// shared far join or fall through to the next check in a chain. Diagram:
//
//     node ─then→ I1 ─{stale S1 → J,  K}
//          ─else→ I2 ─{stale S2 → J,  K}
//
// where `I1`/`I2` are inner condition blocks and `K` is the shared continuation both inner
// conditions can branch to.
//
// A **stale arm** is the `S1` (or `S2`) branch: the side whose computation the diamond fold
// will *discard*, keeping only the other arm's body for the recovered single `CondIf`. The
// reason it's "stale" is the abs-diff idiom: both arms compute the same Move-level result
// (one with `a - b`, the other with `b - a`), so keeping one and dropping the other is sound
// iff they're observationally equivalent. `bodies_equivalent` is the guard that enforces it.
//
// The **far join** `J` is the block both stale arms reach after their code chain. It's the
// post-diamond fall-through. The **continuation** `K` is the shared "next check" both inner
// conditions branch to when neither arm is taken; it becomes the `else` of the recovered
// `CondIf`.

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

/// Structure an acyclic region via reaching conditions:
///
/// - `config`   — global config (debug-print toggles).
/// - `terms`    — per-block lowered `Exp` content (consulted by `bodies_equivalent`).
/// - `input`    — the region's basic blocks; the caller trims it to the region's nodes.
/// - `entry`    — where to start emitting.
///
/// Edges leaving the region (to a node not in `input`) signal an unexpected escape — emission
/// relies on natural CFG sinks. Returns `None` to fall back to the dom-tree structurer when
/// the shape isn't a recognized skip-diamond chain (so clean regions are left byte-identical).
pub fn structure(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
) -> Option<D::Structured> {
    structure_inner(config, terms, input, entry, Mode::WholeFunction)
}

/// Same as [`structure`], but every edge leaving the region (either to the explicit `exit` or
/// to a node not in `input`) is emitted as `Structured::Jump(GotoSource::ReachingExit, …)`.
/// Used for loop-body regions where `insert_breaks` rewrites the Jumps to `Break`/`Continue`
/// downstream. Returns `None` when no diamond was folded so the dom-tree structurer keeps
/// owning shapes reaching doesn't improve on.
pub fn structure_with_exit_jumps(
    config: &config::Config,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    exit: NodeIndex,
) -> Option<D::Structured> {
    structure_inner(config, terms, input, entry, Mode::LoopBody { exit })
}

/// What kind of region we're structuring. Whole-function vs loop-body is a closed dichotomy —
/// either the caller is the top-level driver (no explicit exit, escapes are errors) or it's
/// `structure_loop` handing us a back-edge-rooted region (explicit exit, escapes lower to
/// `Jump(ReachingExit, …)` for `insert_breaks` to rewrite). Encoding it as one enum keeps the
/// pair of related decisions in lockstep so we can't construct an invalid "exit, no jumps" or
/// "jumps, no exit" combination.
#[derive(Clone, Copy)]
enum Mode {
    /// Whole-function (top-level driver). No explicit region exit; an escape from `input` is
    /// an unexpected control-flow leak and bails to the dom-tree structurer.
    WholeFunction,
    /// Loop-body region. `exit` is the back-edge target; edges to `exit` or to nodes outside
    /// `input` are emitted as `Structured::Jump(GotoSource::ReachingExit, target)`.
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
    // Process the region's entry without the `node == stop` short-circuit — for a loop-body
    // region where `entry == exit` (back-edge target), the entry IS the natural exit but we
    // still need to emit its content the first time through. Recursive descent (via
    // `structure_reachable_subregion`) does check `node == stop`, preventing back-edge cycles.
    let body = ctx.process_node(entry, mode.region_exit())?;
    // Only take over when we actually folded a skip — otherwise let the existing structurer
    // own the (already-clean) output so its snapshots don't churn.
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
    /// What kind of region we're structuring — whole-function or a loop-body. Encodes both
    /// the region exit (for `in_region`) and whether escapes lower to explicit `Jump`s.
    mode: Mode,
    /// Nodes currently being processed by `process_node`. Reaching only handles acyclic
    /// regions; any revisit means the region contains an inner cycle (typically a nested
    /// loop whose body is in our `input` snapshot), so we bail with `None` and the caller
    /// falls back to the dom-tree structurer.
    visiting: HashSet<NodeIndex>,
    /// Per-block term map (lowered `Exp` content), keyed by `NodeIndex` whose value matches
    /// the basic-block id. Consulted by `bodies_equivalent` in `recognize_skip_diamond` to guard
    /// the s1/s2 fold against non-uniform arms.
    terms: &'a BTreeMap<NodeIndex, crate::ast::Exp>,
}

impl Ctx<'_> {
    /// Whether escapes from this region lower to explicit `Jump`s (for `insert_breaks` to
    /// rewrite as `Continue`/`Break` downstream) or fall through to natural CFG sinks.
    fn emit_exit_jumps(&self) -> bool {
        self.mode.emit_exit_jumps()
    }

    /// The region's back-edge target (loop-body mode) or `None` (whole-function mode).
    fn region_exit(&self) -> Option<NodeIndex> {
        self.mode.region_exit()
    }
}

impl Ctx<'_> {
    /// Build the `Structured` form for the sub-flow rooted at `node`, walked forward up to
    /// (but not including) `stop`. Returns `Some(Seq([]))` when `node == stop` (we're at the
    /// region boundary; nothing to emit), `None` on failure (region cycle, Variants shape,
    /// arm escape without a shared join), otherwise `Some(Structured)`.
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
                        // Edge leaves the region. Loop-body callers emit an explicit `Jump`
                        // (rewritten downstream by `insert_breaks` to `Continue`/`Break`);
                        // whole-function callers fall through to a natural CFG sink, since
                        // `WholeFunction` mode means the region IS the function and any
                        // non-region edge can only be a sink (return/abort) — there's no
                        // outer scope to land in. The `debug_assert!` locks that invariant
                        // in: if it ever fires, we'd be silently swallowing a real control
                        // edge here, repeating the unsound-elision bug from earlier rounds.
                        if self.emit_exit_jumps() {
                            Some(seq(head, exit_jump(*next)))
                        } else {
                            debug_assert!(
                                self.input
                                    .get(next)
                                    .map(|i| i.edges().is_empty())
                                    .unwrap_or(true),
                                "WholeFunction acyclic structurer: dropping edge {node:?} -> {next:?} \
                                 where target is not a CFG sink — would silently swallow control flow",
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
                if let Some(diamond) = self.recognize_skip_diamond(node, then, els) {
                    self.folded_any = true;
                    // then: the stale block (sets the flag); else: the continuation, structured
                    // only up to the far join. The join itself is emitted *after* the `CondIf`,
                    // so both the stale fall-through and the fresh continuation reach it once —
                    // no goto, no duplication.
                    let then_body = diamond.stale_body;
                    let else_body = self.structure_reachable_subregion(
                        diamond.continue_at,
                        Some(diamond.far_join),
                    )?;
                    let cond_if = D::Structured::CondIf(
                        diamond.cond,
                        Box::new(then_body),
                        Box::new(non_empty(else_body)),
                    );
                    let rest = self.structure_reachable_subregion(diamond.far_join, stop)?;
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

    /// Recognize a skip diamond rooted at `node` (see "Vocabulary" at the top of this file).
    /// Returns `None` unless the shape matches; on match, returns the recovered guard plus the
    /// stale body, the shared continuation, and the far join.
    fn recognize_skip_diamond(
        &mut self,
        node: NodeIndex,
        then: NodeIndex,
        els: NodeIndex,
    ) -> Option<Diamond> {
        let (i1c, i1t, i1e) = self.as_condition(then)?;
        let (i2c, i2t, i2e) = self.as_condition(els)?;
        // Continuation = the node both inner conditions branch to; the *other* arm of each is
        // its stale block.
        let k = [i1t, i1e].into_iter().find(|x| *x == i2t || *x == i2e)?;
        if (i1t == k) == (i1e == k) {
            return None; // both or neither inner-1 arm is the continuation
        }
        let s1 = if i1t == k { i1e } else { i1t };
        let s2 = if i2t == k { i2e } else { i2t };
        let s1_then = s1 == i1t;
        let s2_then = s2 == i2t;
        // Follow each stale arm's code chain to its convergence point. Both must converge on
        // the same far join, or this isn't a diamond.
        let (s1_codes, j1) = self.code_chain_to(s1, k)?;
        let (s2_codes, j2) = self.code_chain_to(s2, k)?;
        if j1 != j2 {
            return None;
        }
        // Soundness: the fold keeps `s1_codes` and discards `s2_codes`. Sound iff the two are
        // observationally equivalent; `bodies_equivalent` is a structural-on-Exp guard (see
        // its doc-comment for what that covers and what it doesn't). Non-uniform shapes fall
        // back to the dom-tree path.
        if !bodies_equivalent(&s1_codes, &s2_codes, self.terms) {
            return None;
        }
        // cond = (node ∧ stale-arm(I1)) ∨ (¬node ∧ stale-arm(I2)), as atoms over block ids.
        // The smart constructors normalize (NNF + sort + dedup + absorption + complementation),
        // so the recovered guard lands canonical without a separate pass.
        let a_node = predicates::cond_atom(node.index() as u64);
        let cond = predicates::or(vec![
            predicates::and(vec![a_node.clone(), atom_pol(i1c, s1_then)]),
            predicates::and(vec![predicates::not(a_node), atom_pol(i2c, s2_then)]),
        ]);
        Some(Diamond {
            cond,
            stale_body: blocks_seq(&s1_codes),
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
    /// hit `stop` (the shared diamond continuation), leave the region, or run into a non-Code
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

struct Diamond {
    cond: predicates::Formula,
    stale_body: D::Structured,
    continue_at: NodeIndex,
    far_join: NodeIndex,
}

/// Structural-equivalence guard for the s1/s2 stale arms of a recognized diamond. Drops
/// empty / Goto-only padding blocks (CFG-anchor blocks the Move compiler sometimes emits
/// on one arm but not the other when the source-level shape requires it, even though they
/// have no observable effect) before comparing the surviving block bodies pairwise via
/// `exp_struct_eq`.
///
/// NB: this is purely structural equivalence on the *pre-refinement* lowered `Exp` shape.
/// It does NOT prove semantic equivalence. A semantic-prover-grade guard would need alias
/// analysis or symbolic execution and is out of scope. The structural check is sufficient
/// for the corpus today and conservatively rejects any shape we can't be sure about —
/// non-uniform diamonds fall back to the dom-tree path.
///
/// What passes:
///   - `{ flag = true }`  vs  `{ flag = true }` — identical assigns.
///   - `{ x = a - b }`  vs  `{ x = a - b }` — identical arithmetic.
///   - `{ flag = true; goto J }`  vs  `{ flag = true }` — one arm's Goto-only padding is
///     dropped before comparison.
///
/// What fails (correctly):
///   - `{ flag = true }`  vs  `{ flag = true; counter = counter + 1 }` — non-uniform; the
///     second arm has an extra observable effect. Falls back to dom-tree. See
///     `non_uniform_arms` fixture in `tests/move/staleness/sources/staleness.move`.
///   - `{ x = a + 0 }`  vs  `{ x = a }` — semantically equal but syntactically different;
///     this is the conservative-by-design failure mode.
///   - `{ x = a - b; y = c }`  vs  `{ y = c; x = a - b }` — reordered statements; the
///     pre-refinement comparison is positional.
fn bodies_equivalent(
    s1_codes: &[u64],
    s2_codes: &[u64],
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
) -> bool {
    use crate::ast::Exp;
    use crate::ast::UnstructuredNode;
    fn is_padding(exp: &Exp) -> bool {
        match exp {
            // An empty Seq (no statements) — the block has no observable effect.
            Exp::Seq(items) if items.is_empty() => true,
            // A single-statement Seq holding only `Unstructured([Goto(_)])` — the block
            // exists only to anchor a CFG edge, no observable effect.
            Exp::Seq(items) if items.len() == 1 => matches!(
                &items[0],
                Exp::Unstructured(nodes)
                    if nodes.len() == 1
                        && matches!(&nodes[0], UnstructuredNode::Goto(_))
            ),
            // A bare Unstructured Goto outside a Seq wrapper — same as above.
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

// -------------------------------------------------------------------------------------------------
// Post-dominators (immediate post-dominator = where a branch's arms rejoin)
// -------------------------------------------------------------------------------------------------
// `dom_tree::DominatorTree` carries the regular dominator tree; the dom-tree structurer in
// `structuring/mod.rs` walks it via `Graph::dom_tree`. We don't carry post-dominators globally
// — they're only consumed by this acyclic structurer, and the region we structure is always a
// strict subset of the function's blocks (a loop body region for `structure_with_exit_jumps`,
// or the whole function for `structure`). Building post-doms region-locally keeps the cost
// bounded by region size rather than total function size, and avoids carrying the synthetic-
// exit + dual-edge plumbing in `Graph` for a structurer that may not even run.

struct PostDom {
    doms: Dominators<NodeIndex>,
    /// Synthetic exit's petgraph index (the dominator-algorithm root).
    exit_internal: NodeIndex,
    /// Map CFG `NodeIndex` (input keys) to its index inside `doms`'s reversed graph. We
    /// intern only the region's nodes, so the graph is `O(|input|)` regardless of how
    /// sparsely the original CFG numbers them.
    to_internal: HashMap<NodeIndex, NodeIndex>,
    /// Inverse of `to_internal`. The synthetic exit's slot is `None`.
    from_internal: Vec<Option<NodeIndex>>,
}

impl PostDom {
    /// Build post-dominators over the region by running the dominator algorithm on the reversed
    /// region CFG, rooted at a synthetic exit that absorbs every edge leaving the region. An
    /// edge whose target is `region_exit` or not in `input` is treated as a sink for its source,
    /// so a branch that escapes the region post-dom'd at the synthetic exit. `None` if no node
    /// in the region reaches a sink (no escapes and no natural terminators — shouldn't happen
    /// for a real CFG region).
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

    /// The immediate post-dominator of `node`, or `None` when it is the synthetic exit (i.e. the
    /// branch's arms don't rejoin before the function returns).
    fn ipostdom(&self, node: NodeIndex) -> Option<NodeIndex> {
        let n_int = *self.to_internal.get(&node)?;
        match self.doms.immediate_dominator(n_int) {
            Some(ip) if ip != self.exit_internal => self.from_internal[ip.index()],
            _ => None,
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Reaching conditions (No More Gotos, phase 1)
// -------------------------------------------------------------------------------------------------
// For a loop-free region, the reaching condition of a node is the boolean formula over branch
// predicates under which control reaches that node:
//
//     R(entry) = true
//     R(n)     = ⋁_{p → n}  R(p) ∧ cond(p → n)
//
// where `cond(p → n)` is the predicate at `p`'s branch taken to reach `n` (the atom for the
// `then` edge, its negation for the `else` edge). Atoms are named by the convention
// [`predicates::cond_var_name`] (`__c{N}` for condition block N), so a local that's reassigned
// between regions yields a distinct atom per test and is never conflated.
//
// This is the pattern-independent half of No More Gotos: every node of an acyclic region gets a
// guard, so there's nothing left to "fail to structure" — no gotos are required. Folding the
// guarded sequence back into `&&`/`||`/`if` is a separate, semantics-preserving step (handled
// above by [`recognize_skip_diamond`] and the rest of the acyclic structurer).

/// The predicate under which edge `p → n` is taken, given `p`'s input node.
fn edge_condition(pred_input: Option<&D::Input>, p: NodeIndex, n: NodeIndex) -> Formula {
    match pred_input {
        Some(D::Input::Condition(_, _, then, els)) => {
            if n == *then {
                predicates::cond_atom(p.index() as u64)
            } else if n == *els {
                predicates::not(predicates::cond_atom(p.index() as u64))
            } else {
                // `n` is not an arm of `p` — the caller's edge set is inconsistent with the
                // condition's recorded arms. The adjacency build above only enumerates edges
                // produced by `Input::edges`, which for a `Condition` returns exactly
                // `(p, then)` and `(p, else)`, so reaching this arm means a Condition's arms
                // were rewritten after the topo build. In release we fall back to a conservative
                // `True` guard rather than panic — the resulting reaching set is over-broad but
                // sound enough to keep the dom-tree fallback honest.
                debug_assert!(
                    false,
                    "edge {p:?} -> {n:?} not in Condition's arms (then={then:?}, else={els:?})",
                );
                predicates::true_()
            }
        }
        // Unconditional fall-through, or a node we don't model: the edge is always taken.
        _ => predicates::true_(),
    }
}

/// Compute reaching conditions for every node of an acyclic region described by `input`, rooted
/// at `entry`. Returns `None` if the region contains a cycle (a back edge — not loop-free) or
/// any enum-`Variants` dispatch (not yet modeled).
pub fn reaching_conditions(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
) -> Option<BTreeMap<NodeIndex, Formula>> {
    if input.values().any(|i| matches!(i, D::Input::Variants(..))) {
        return None;
    }

    // Intern region nodes into a compact petgraph index space and feed `algo::toposort` —
    // the same `petgraph` reduction `PostDom::build` uses, just for the forward graph.
    // `toposort` returns `Err(Cycle)` if the region has a back edge; we propagate as `None`
    // so callers fall back to the dom-tree structurer.
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

    // Forward-propagate. In a DAG every predecessor precedes its successor in `topo`, so each
    // `reach[p]` is already populated when we reach `n`.
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
