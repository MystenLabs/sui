// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

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
// guarded sequence back into `&&`/`||`/`if` is a separate, semantics-preserving step.

use crate::structuring::{ast::Input, predicates};
use petgraph::graph::NodeIndex;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use predicates::Formula;

/// The predicate under which edge `p → n` is taken, given `p`'s input node.
fn edge_condition(pred_input: Option<&Input>, p: NodeIndex, n: NodeIndex) -> Formula {
    match pred_input {
        Some(Input::Condition(_, _, then, els)) => {
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
    input: &BTreeMap<NodeIndex, Input>,
    entry: NodeIndex,
) -> Option<BTreeMap<NodeIndex, Formula>> {
    if input.values().any(|i| matches!(i, Input::Variants(..))) {
        return None;
    }

    // Build adjacency from the input edges.
    let mut succs: BTreeMap<NodeIndex, Vec<NodeIndex>> = BTreeMap::new();
    let mut preds: BTreeMap<NodeIndex, Vec<NodeIndex>> = BTreeMap::new();
    let mut nodes: BTreeSet<NodeIndex> = input.keys().copied().collect();
    for inp in input.values() {
        for (u, v) in inp.edges() {
            succs.entry(u).or_default().push(v);
            preds.entry(v).or_default().push(u);
            nodes.insert(u);
            nodes.insert(v);
        }
    }

    // Kahn's topological sort; a remaining cycle means the region isn't loop-free.
    let mut indeg: BTreeMap<NodeIndex, usize> = nodes.iter().map(|&n| (n, 0)).collect();
    for (&v, ps) in &preds {
        *indeg.get_mut(&v).unwrap() = ps.len();
    }
    let mut queue: VecDeque<NodeIndex> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(&n, _)| n)
        .collect();
    let mut topo: Vec<NodeIndex> = Vec::with_capacity(nodes.len());
    while let Some(n) = queue.pop_front() {
        topo.push(n);
        for &s in succs.get(&n).into_iter().flatten() {
            let d = indeg.get_mut(&s).unwrap();
            *d -= 1;
            if *d == 0 {
                queue.push_back(s);
            }
        }
    }
    if topo.len() != nodes.len() {
        return None;
    }

    // Forward-propagate. In a DAG every predecessor precedes its successor in `topo`, so each
    // `reach[p]` is already populated when we reach `n`.
    let mut reach: BTreeMap<NodeIndex, Formula> = BTreeMap::new();
    reach.insert(entry, predicates::true_());
    for &n in &topo {
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
