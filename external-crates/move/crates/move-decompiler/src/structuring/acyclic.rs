// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Acyclic-region structuring (NMG §IV-B). [`structure_nmg`] computes reaching
//! conditions over the region's acyclic projection, lays each node out in topo order
//! guarded by its formula, then runs [`refine_initial_ast`]'s three-phase refinement
//! (implication nesting + condition-based factoring + abort-aware terminator elision)
//! to recover nested control flow.

use crate::config;
use crate::structuring::{
    ast::{self as D},
    predicates::{self, Formula},
};
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
    let proj = build_acyclic_projection(input, entry, members);
    let reach = reaching_conditions(&proj.input, entry)?;
    let topo = topological_order(&proj.input)?;

    // Codes whose projection node has no outgoing edges - abort/return blocks and
    // back-edge / out-of-region synthetic sinks. The elide step uses these to recognize
    // "this item terminates, so subsequent siblings only run when !item.guard".
    let sink_codes: HashSet<u64> = proj
        .input
        .iter()
        .filter_map(|(n, inp)| {
            if inp.edges().is_empty() {
                Some(n.index() as u64)
            } else {
                None
            }
        })
        .collect();

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
    items = refine_initial_ast(items, &sink_codes);

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
fn refine_initial_ast(
    mut items: Vec<(Formula, D::Structured)>,
    sink_codes: &HashSet<u64>,
) -> Vec<(Formula, D::Structured)> {
    loop {
        if let Some(new_items) = try_implication_nest(&items, sink_codes) {
            items = new_items;
            continue;
        }
        if let Some(new_items) = try_refine_once(&items, sink_codes) {
            items = new_items;
            continue;
        }
        // Run elide LAST so factoring has reduced guards to small per-item residuals
        // before we feed them to QM. The assumption set is bounded by the depth of any
        // terminating `if (G) { abort }` patterns inside each item's body - small in
        // practice once refinement has pulled common factors up to outer scopes.
        if try_elide_via_terminators(&mut items, sink_codes) {
            continue;
        }
        return items;
    }
}

/// Walk `items` left-to-right tracking assumptions harvested from each item's role as
/// `if (guard) { body }`. When an item's `body` always-terminates and its guard isn't
/// `True`, subsequent siblings only run when `!guard` - record that. Nested-Seq
/// early-exits inside item bodies are harvested too via `collect_terminator_assumptions`.
/// For each item, if accumulated assumptions imply its guard, set the guard to `True`
/// so `emit_seq_from_items` drops the `CondIf` wrapper.
fn try_elide_via_terminators(
    items: &mut Vec<(Formula, D::Structured)>,
    sink_codes: &HashSet<u64>,
) -> bool {
    let mut changed = false;
    let mut assumptions: Vec<Formula> = Vec::new();
    for (guard, body) in items.iter_mut() {
        if *guard != predicates::true_() && assumptions_imply(&assumptions, guard) {
            *guard = predicates::true_();
            changed = true;
        }
        // An item itself acts like `if (guard) { body }` at the surrounding scope; when
        // `body` always-terminates and `guard` isn't trivially `True`, subsequent items
        // only run when `!guard` - feed that into the accumulator.
        if *guard != predicates::true_() && always_terminates_structured(body, sink_codes) {
            assumptions.push(predicates::not(guard.clone()));
        }
        // Harvest nested-Seq early-exits from inside `body`. The body sits inside the
        // item's wrapper, so use `[guard]` as the outer guard_stack - any local
        // assumption gets lifted to `guard → local` for the surrounding sibling list.
        let body_stack: Vec<Formula> = if *guard == predicates::true_() {
            Vec::new()
        } else {
            vec![guard.clone()]
        };
        collect_terminator_assumptions(body, &body_stack, sink_codes, &mut assumptions);
    }
    changed
}

/// True iff the conjunction of `assumptions` implies `guard`. Cheap structural shortcut
/// (verbatim match) before falling back to QM. Atom-overlap prefilter discards
/// assumptions whose atoms are disjoint from `guard`'s - they can't contribute to the
/// implication and just inflate the QM input.
fn assumptions_imply(assumptions: &[Formula], guard: &Formula) -> bool {
    if *guard == predicates::true_() {
        return true;
    }
    if assumptions.is_empty() {
        return false;
    }
    if assumptions.iter().any(|a| a == guard) {
        return true;
    }
    let guard_atoms = guard.atoms();
    let mut conj: Vec<Formula> = assumptions
        .iter()
        .filter(|a| !a.atoms().is_disjoint(&guard_atoms))
        .cloned()
        .collect();
    if conj.is_empty() {
        return false;
    }
    conj.push(predicates::not(guard.clone()));
    predicates::and(conj).simplify() == predicates::false_()
}

/// True iff every path through `s` leaves the surrounding sibling sequence -
/// `Break`/`Continue`/`Jump`/`JumpIf`, `Block(code)` whose `code` is a CFG sink
/// (abort/return), a `Seq` whose last item terminates, or a `CondIf` whose both arms
/// terminate.
fn always_terminates_structured(s: &D::Structured, sink_codes: &HashSet<u64>) -> bool {
    use D::Structured as DS;
    match s {
        DS::Break(_) | DS::Continue(_) | DS::Jump(..) | DS::JumpIf(..) => true,
        DS::Block(code) => sink_codes.contains(code),
        DS::Seq(items) => items
            .last()
            .is_some_and(|x| always_terminates_structured(x, sink_codes)),
        DS::CondIf(_, then, alt) => {
            always_terminates_structured(then, sink_codes)
                && alt
                    .as_ref()
                    .as_ref()
                    .is_some_and(|a| always_terminates_structured(a, sink_codes))
        }
        // Loop/Switch/SelectorMatch may iterate or branch in ways we don't analyze; be
        // conservative and treat as non-terminating.
        _ => false,
    }
}

/// Walk `s` collecting assumptions implied by terminators encountered. `guard_stack`
/// is the conjunction of enclosing `CondIf` conds that must hold for `s` to run.
/// At a `Seq`, items execute left-to-right; a `CondIf(c, body, None)` whose body
/// always-terminates means subsequent items in the same `Seq` only run when `!c` -
/// recorded both locally (so later siblings within `s` see it) and externally (so the
/// outer scope inherits `guard_stack → !c`).
fn collect_terminator_assumptions(
    s: &D::Structured,
    guard_stack: &[Formula],
    sink_codes: &HashSet<u64>,
    out: &mut Vec<Formula>,
) {
    use D::Structured as DS;
    fn lift(local: Formula, gs: &[Formula]) -> Formula {
        if gs.is_empty() {
            return local;
        }
        let guard_conj = predicates::and(gs.to_vec());
        predicates::or(vec![predicates::not(guard_conj), local])
    }
    match s {
        DS::Seq(items) => {
            let mut local: Vec<Formula> = Vec::new();
            for item in items {
                let mut local_stack: Vec<Formula> = guard_stack.to_vec();
                local_stack.extend(local.iter().cloned());
                collect_terminator_assumptions(item, &local_stack, sink_codes, out);
                // Recognize three early-exit shapes inside this Seq:
                //   - `CondIf(c, term, None)`             -> assume !c for subsequent siblings.
                //   - `CondIf(c, term, Some(non_term))`   -> assume !c (we took non-term).
                //   - `CondIf(c, non_term, Some(term))`   -> assume  c (we took non-term).
                if let DS::CondIf(g, body, alt) = item {
                    let then_term = always_terminates_structured(body, sink_codes);
                    let alt_term = alt
                        .as_ref()
                        .as_ref()
                        .is_some_and(|a| always_terminates_structured(a, sink_codes));
                    match (then_term, alt_term, alt.as_ref().as_ref().is_some()) {
                        // then terminates, no alt: subsequent only run when !c.
                        (true, _, false) => local.push(predicates::not(g.clone())),
                        // then terminates, alt doesn't: we continued via alt, so !c.
                        (true, false, true) => local.push(predicates::not(g.clone())),
                        // alt terminates, then doesn't: we continued via then, so c.
                        (false, true, true) => local.push(g.clone()),
                        _ => {}
                    }
                }
            }
            for l in local {
                out.push(lift(l, guard_stack));
            }
        }
        DS::CondIf(g, then, alt) => {
            let mut then_stack: Vec<Formula> = guard_stack.to_vec();
            then_stack.push(g.clone());
            collect_terminator_assumptions(then, &then_stack, sink_codes, out);
            if let Some(a) = alt.as_ref().as_ref() {
                let mut else_stack: Vec<Formula> = guard_stack.to_vec();
                else_stack.push(predicates::not(g.clone()));
                collect_terminator_assumptions(a, &else_stack, sink_codes, out);
            }
        }
        // Loop bodies may run zero or many times - we can't carry assumptions through.
        DS::Loop(..) => {}
        _ => {}
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
    sink_codes: &HashSet<u64>,
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
        let inner_refined = refine_initial_ast(inner, sink_codes);
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
fn try_refine_once(
    items: &[(Formula, D::Structured)],
    sink_codes: &HashSet<u64>,
) -> Option<Vec<(Formula, D::Structured)>> {
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
        let conseq = emit_seq_from_items(refine_initial_ast(vc_items, sink_codes));
        let alt = if vneg_items.is_empty() {
            None
        } else {
            Some(emit_seq_from_items(refine_initial_ast(vneg_items, sink_codes)))
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

