// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Reaching-condition acyclic structuring (No More Gotos)
// -------------------------------------------------------------------------------------------------
// For a loop-free function whose dom-tree structuring would emit a goto for a guarded forward
// skip, build the clean nested form directly. We recognize "skip diamonds" — an abs_diff-style
// check whose arms set a flag and either skip to a far join or fall through to the next check —
// and fold each into one compound condition (`CondIf`), nesting the continuation in the `else`.
// The skip never becomes a goto: it's the fall-through after the `CondIf`'s then-branch.
//
// This is deliberately conservative: it returns `None` (handing back to the dom-tree structurer)
// on any shape it doesn't recognize, so it only ever *replaces* output that currently has a goto.

use crate::structuring::{
    ast::{self as D, GotoSource},
    bdd::Bdd,
    reaching,
};
use petgraph::algo::dominators::{self, Dominators};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeMap, HashSet};

/// Structure an acyclic region via reaching conditions. `input` is the region's basic blocks
/// (the caller trims it to the region's nodes), `entry` is where to start emitting, and `exit`
/// — when `Some` — is where to stop: emission proceeds only up to but not including the exit
/// node, treating it as the region's natural rejoin point. Returns `None` to fall back to the
/// dom-tree structurer when the shape isn't a recognized skip-diamond chain (so clean regions
/// are left byte-identical).
pub fn structure(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    exit: Option<NodeIndex>,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
) -> Option<D::Structured> {
    structure_inner(input, entry, exit, /*emit_exit_jumps*/ false, terms)
}

/// Same as [`structure`], but every edge leaving the region (either to the explicit `exit` or
/// to a node not in `input`) is emitted as `Structured::Jump(GotoSource::ReachingExit, …)`.
/// Used for loop-body regions where `insert_breaks` rewrites the Jumps to `Break`/`Continue`
/// downstream. Like `structure`, returns `None` when no diamond was folded so the dom-tree
/// structurer keeps owning shapes reaching doesn't improve on (clean nested ifs in a loop
/// body, etc., where reaching's more-nested form would just churn snapshots).
pub fn structure_with_exit_jumps(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    exit: NodeIndex,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
) -> Option<D::Structured> {
    structure_inner(
        input,
        entry,
        Some(exit),
        /*emit_exit_jumps*/ true,
        terms,
    )
}

fn structure_inner(
    input: &BTreeMap<NodeIndex, D::Input>,
    entry: NodeIndex,
    exit: Option<NodeIndex>,
    emit_exit_jumps: bool,
    terms: &BTreeMap<NodeIndex, crate::ast::Exp>,
) -> Option<D::Structured> {
    let pdom = PostDom::build(input, exit)?;
    let mut ctx = Ctx {
        input,
        pdom,
        bdd: Bdd::new(),
        folded_any: false,
        region_exit: exit,
        emit_exit_jumps,
        visiting: HashSet::new(),
        terms,
    };
    // Process the region's entry without the `node == stop` short-circuit — for a loop-body
    // region where `entry == exit` (back-edge target), the entry IS the natural exit but we
    // still need to emit its content the first time through. Recursive descent (via `go`)
    // does check `node == stop`, preventing back-edge cycles.
    let body = ctx.go_body(entry, exit)?;
    // Only take over when we actually folded a skip — otherwise let the existing structurer
    // own the (already-clean) output so its snapshots don't churn.
    if !ctx.folded_any {
        return None;
    }
    Some(body)
}

struct Ctx<'a> {
    input: &'a BTreeMap<NodeIndex, D::Input>,
    pdom: PostDom,
    bdd: Bdd,
    folded_any: bool,
    /// The region's explicit exit node, when one is set (loop-body case). Edges to this node
    /// are treated as leaving the region just like edges to nodes outside `input`.
    region_exit: Option<NodeIndex>,
    /// When true (loop-body mode), edges leaving the region are emitted as explicit
    /// `Structured::Jump(GotoSource::ReachingExit, target)`. When false (whole-function), the
    /// caller relies on natural CFG sinks; an unexpected escape returns `None`.
    emit_exit_jumps: bool,
    /// Nodes currently being processed by `go_body`. Reaching only handles acyclic regions;
    /// any revisit means the region contains an inner cycle (typically a nested loop whose
    /// body is in our `input` snapshot), so we bail with `None` and the caller falls back to
    /// the dom-tree structurer.
    visiting: HashSet<NodeIndex>,
    /// Per-block term map (lowered `Exp` content), keyed by `NodeIndex` whose value matches
    /// the basic-block id. Consulted by `bodies_equivalent` in `recognize_diamond` to guard
    /// the s1/s2 fold against non-uniform arms.
    terms: &'a BTreeMap<NodeIndex, crate::ast::Exp>,
}

impl Ctx<'_> {
    /// Structure flow from `node` up to (but not including) `stop` (`None` = no boundary).
    /// `stop` is consulted only for recursive descent — the initial call from `structure`
    /// uses `go_body` directly so it can process the region's entry even when `entry == stop`.
    fn go(&mut self, node: NodeIndex, stop: Option<NodeIndex>) -> Option<D::Structured> {
        if Some(node) == stop {
            return Some(D::Structured::Seq(vec![]));
        }
        self.go_body(node, stop)
    }

    /// Process `node`'s content (Code chain or Condition), recursing via `go` for sub-flow
    /// so each recursive visit honors `stop`. Called directly for the region's entry to skip
    /// the entry-equals-stop short-circuit (loop-body region case).
    fn go_body(&mut self, node: NodeIndex, stop: Option<NodeIndex>) -> Option<D::Structured> {
        if !self.visiting.insert(node) {
            return None;
        }
        let result = self.go_body_inner(node, stop);
        self.visiting.remove(&node);
        result
    }

    fn go_body_inner(&mut self, node: NodeIndex, stop: Option<NodeIndex>) -> Option<D::Structured> {
        match self.input.get(&node)? {
            D::Input::Variants(..) => None,
            D::Input::Code(_, code, next) => {
                let head = D::Structured::Block(*code);
                match next {
                    None => Some(head),
                    Some(next) if !self.in_region(*next) => {
                        // Edge leaves the region (back edge in a loop-body region, or a
                        // straight-line exit). For loop-body callers we emit an explicit
                        // `Jump` so `insert_breaks` can rewrite it to `Continue`/`Break`;
                        // whole-function callers fall through to a natural sink and don't
                        // expect escapes here.
                        if self.emit_exit_jumps {
                            Some(seq(head, exit_jump(*next)))
                        } else {
                            Some(head)
                        }
                    }
                    Some(next) if Some(*next) == stop => Some(head),
                    Some(next) => {
                        let rest = self.go(*next, stop)?;
                        Some(seq(head, rest))
                    }
                }
            }
            D::Input::Condition(_, code, then, els) => {
                let (code, then, els) = (*code, *then, *els);
                if let Some(diamond) = self.recognize_diamond(node, then, els) {
                    self.folded_any = true;
                    // then: the stale block (sets the flag); else: the continuation, structured
                    // only up to the far join. The join itself is emitted *after* the `CondIf`,
                    // so both the stale fall-through and the fresh continuation reach it once —
                    // no goto, no duplication.
                    let then_body = diamond.stale_body;
                    let else_body = self.go(diamond.continue_at, Some(diamond.far_join))?;
                    let cond_if = D::Structured::CondIf(
                        diamond.cond,
                        Box::new(then_body),
                        Box::new(non_empty(else_body)),
                    );
                    let rest = self.go(diamond.far_join, stop)?;
                    Some(seq(cond_if, rest))
                } else {
                    // Genuine branch: structure both arms up to where they rejoin, then continue.
                    let join = self.pdom.ipostdom(node);
                    let then_s = self.arm(then, join)?;
                    let els_s = self.arm(els, join)?;
                    let if_s = D::Structured::CondIf(
                        reaching::Formula::Atom(NodeIndex::new(code as usize)),
                        Box::new(then_s),
                        Box::new(non_empty(els_s)),
                    );
                    match join {
                        Some(j) => Some(seq(if_s, self.go(j, stop)?)),
                        None => Some(if_s),
                    }
                }
            }
        }
    }

    /// Structure one Condition arm rooted at `target`. If `target` is already a region exit
    /// (e.g. the arm is a direct break-target jump), emit the appropriate `Jump`/empty form
    /// directly so we don't try to recur into a node that isn't in our `input`.
    fn arm(&mut self, target: NodeIndex, join: Option<NodeIndex>) -> Option<D::Structured> {
        if self.in_region(target) {
            self.go(target, join)
        } else if self.emit_exit_jumps {
            Some(exit_jump(target))
        } else {
            // Whole-function mode: an arm that escapes the region without going through the
            // shared `join` would orphan a control-flow edge. Bail and let the dom-tree path
            // own the shape.
            None
        }
    }

    /// True iff `node` is part of the structured region: it lives in `self.input` AND is not
    /// the explicit `region_exit` (back-edge target in a loop-body region).
    fn in_region(&self, node: NodeIndex) -> bool {
        self.input.contains_key(&node) && Some(node) != self.region_exit
    }

    /// Recognize a balanced abs_diff-style skip diamond rooted at `node`:
    ///
    /// ```text
    ///   node ─then→ I1 ─{stale S1 → J,  K}
    ///        ─else→ I2 ─{stale S2 → J,  K}
    /// ```
    ///
    /// where `I1`/`I2` are conditions, `S1`/`S2` are code chains that converge on a common far
    /// join `J`, and both inner conditions' other arm is the same continuation `K`. The
    /// recovered condition is the reaching condition of the stale blocks. Returns `None` unless
    /// the shape matches.
    ///
    /// Soundness: the fold keeps only `S1`'s body for both branches of the recovered condition,
    /// which is correct iff `S1` and `S2` are observationally equivalent. Checked via
    /// `bodies_equivalent`: compare the per-block `Exp` content (from the `terms` map plumbed
    /// in from `translate.rs`) after stripping empty/jump-only padding blocks the Move
    /// compiler sometimes emits asymmetrically. Non-uniform shapes — e.g., one arm sets a
    /// flag, the other ALSO bumps a counter — fall back to the dom-tree path (their dom-tree
    /// emission preserves the difference; see the `non_uniform_arms` regression fixture in
    /// `tests/move/staleness/sources/staleness.move`). The check is structural-on-Exp, not
    /// semantic — equivalent but syntactically-different shapes (e.g., `x + 0` vs `x`) would
    /// be rejected; a semantic-prover-grade guard would need alias analysis or symbolic
    /// execution and is out of scope.
    fn recognize_diamond(
        &mut self,
        node: NodeIndex,
        then: NodeIndex,
        els: NodeIndex,
    ) -> Option<Diamond> {
        let (i1c, i1t, i1e) = self.as_condition(then)?;
        let (i2c, i2t, i2e) = self.as_condition(els)?;
        // The continuation is the single node both inner conditions branch to; each inner
        // condition's *other* arm is its stale block. (Continuation may be a Code block — the
        // chain's last check — or the next check; we don't care which.)
        let k = [i1t, i1e].into_iter().find(|x| *x == i2t || *x == i2e)?;
        if (i1t == k) == (i1e == k) {
            return None; // both or neither inner-1 arm is the continuation
        }
        let s1 = if i1t == k { i1e } else { i1t };
        let s2 = if i2t == k { i2e } else { i2t };
        let s1_then = s1 == i1t;
        let s2_then = s2 == i2t;
        // Each stale arm is a straight-line code chain that converges on a common far join. We
        // follow each to its end rather than peeking a single hop, and stop at `k` so we don't
        // sweep the shared continuation block into the stale body (which would emit it twice
        // — once inside `stale_body`, once as part of the post-diamond `rest`). We keep the
        // first chain's body for the fold; see "Soundness" above for why we accept the second
        // silently today and what's needed to guard it for real.
        let (s1_codes, j1) = self.stale_chain(s1, k)?;
        let (s2_codes, j2) = self.stale_chain(s2, k)?;
        if j1 != j2 {
            return None;
        }
        // Body-equivalence guard. The fold keeps only `s1_codes` and discards `s2_codes`,
        // sound iff the two stale chains are observationally equivalent. Compare them
        // structurally on the lowered `Exp` content (via the `terms` map), modulo
        // empty/jump-only padding blocks the Move compiler sometimes emits on one arm but
        // not the other. Non-uniform arms (e.g. one arm sets a flag, the other ALSO bumps
        // a counter) fail this check; we return `None` and the dom-tree path takes over.
        if !bodies_equivalent(&s1_codes, &s2_codes, self.terms) {
            return None;
        }
        // cond = (node ∧ stale-arm(I1)) ∨ (¬node ∧ stale-arm(I2)), as atoms over block ids,
        // canonicalized through the BDD so the lowered guard comes out clean.
        let a_node = reaching::Formula::Atom(node);
        let cond = reaching::or(vec![
            reaching::and(vec![a_node.clone(), atom_pol(i1c, s1_then)]),
            reaching::and(vec![reaching::not(a_node), atom_pol(i2c, s2_then)]),
        ]);
        let id = self.bdd.build(&cond);
        Some(Diamond {
            cond: self.bdd.to_formula(id),
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

    /// Follow a stale arm's straight-line code chain, collecting block ids, until either:
    /// (1) the next node is `k` — the diamond's shared continuation, which is emitted as the
    ///     `else` arm and the post-diamond fall-through, NOT as part of the stale body; or
    /// (2) the next node is not a fall-through code block (a condition, the function's
    ///     `if (flag)` join, or the loop-head back-edge target).
    /// In either case that next node is the common far join. `None` if `s` isn't a code chain.
    fn stale_chain(&self, s: NodeIndex, k: NodeIndex) -> Option<(Vec<u64>, NodeIndex)> {
        let mut codes = Vec::new();
        let mut cur = s;
        loop {
            match self.input.get(&cur)? {
                D::Input::Code(_, code, Some(next)) => {
                    codes.push(*code);
                    if *next == k {
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
    cond: reaching::Formula,
    stale_body: D::Structured,
    continue_at: NodeIndex,
    far_join: NodeIndex,
}

/// Structural-equivalence guard for the s1/s2 stale arms of a recognized diamond. Drops
/// empty / jump-only "padding" blocks (the Move compiler sometimes pads one arm with an
/// extra empty block for alignment) before comparing the surviving block bodies pairwise
/// via `exp_struct_eq`.
///
/// NB: this is purely structural equivalence on the *pre-refinement* lowered `Exp` shape.
/// It does NOT prove semantic equivalence — e.g., `flag = false` and `let _ = (); flag =
/// false` would compare structurally unequal but are semantically equivalent. A
/// semantic-prover-grade guard would need alias analysis or symbolic execution and is out
/// of scope. The structural check is sufficient for the corpus today and conservatively
/// rejects any shape we can't be sure about — non-uniform diamonds fall back to the
/// dom-tree path.
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
    let mut s1: Vec<&Exp> = s1_codes
        .iter()
        .filter_map(|c| body_of(*c))
        .filter(|e| !is_padding(e))
        .collect();
    let mut s2: Vec<&Exp> = s2_codes
        .iter()
        .filter_map(|c| body_of(*c))
        .filter(|e| !is_padding(e))
        .collect();
    if s1.len() != s2.len() {
        return false;
    }
    while let (Some(a), Some(b)) = (s1.pop(), s2.pop()) {
        if !crate::exp_eq::exp_struct_eq(a, b) {
            return false;
        }
    }
    true
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

fn atom_pol(code: u64, positive: bool) -> reaching::Formula {
    let atom = reaching::Formula::Atom(NodeIndex::new(code as usize));
    if positive { atom } else { reaching::not(atom) }
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

struct PostDom {
    doms: Dominators<NodeIndex>,
    exit: NodeIndex,
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
        let max_input = input.keys().map(|n| n.index()).max()?;
        let max_idx = match region_exit {
            Some(e) => max_input.max(e.index()),
            None => max_input,
        };
        let exit = NodeIndex::new(max_idx + 1);
        let mut rev: DiGraph<(), ()> = DiGraph::new();
        while rev.node_count() <= exit.index() {
            rev.add_node(());
        }
        let mut has_sink = false;
        let in_region = |n: NodeIndex| input.contains_key(&n) && Some(n) != region_exit;
        for (n, inp) in input {
            let succs = inp.edges();
            if succs.is_empty() {
                rev.add_edge(exit, *n, ());
                has_sink = true;
                continue;
            }
            for (u, v) in succs {
                if in_region(v) {
                    rev.add_edge(v, u, ());
                } else {
                    rev.add_edge(exit, u, ());
                    has_sink = true;
                }
            }
        }
        if !has_sink {
            return None;
        }
        Some(PostDom {
            doms: dominators::simple_fast(&rev, exit),
            exit,
        })
    }

    /// The immediate post-dominator of `node`, or `None` when it is the synthetic exit (i.e. the
    /// branch's arms don't rejoin before the function returns).
    fn ipostdom(&self, node: NodeIndex) -> Option<NodeIndex> {
        match self.doms.immediate_dominator(node) {
            Some(ip) if ip != self.exit => Some(ip),
            _ => None,
        }
    }
}
