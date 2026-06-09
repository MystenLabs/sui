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
// `then` edge, its negation for the `else` edge). Atoms are keyed by the *condition node*
// (a `NodeIndex`), not by the predicate's surface text, so a local that is reassigned between
// regions (e.g. pyth's `l19`) yields distinct atoms per test and is never conflated.
//
// This is the pattern-independent half of No More Gotos: every node of an acyclic region gets
// a guard, so there is nothing left to "fail to structure" — no gotos are required. Folding the
// guarded sequence back into `&&`/`||`/`if` is a separate, semantics-preserving step.

use crate::structuring::ast::Input;
use petgraph::graph::NodeIndex;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A boolean formula over branch-condition atoms. `Atom(n)` denotes "the test at condition
/// node `n` took its `then` edge".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Formula {
    True,
    False,
    Atom(NodeIndex),
    Not(Box<Formula>),
    And(Vec<Formula>),
    Or(Vec<Formula>),
}

impl Formula {
    /// Every distinct atom (condition-block id) referenced by the formula.
    pub fn atoms(&self) -> BTreeSet<NodeIndex> {
        fn go(f: &Formula, out: &mut BTreeSet<NodeIndex>) {
            match f {
                Formula::True | Formula::False => {}
                Formula::Atom(n) => {
                    out.insert(*n);
                }
                Formula::Not(inner) => go(inner, out),
                Formula::And(fs) | Formula::Or(fs) => fs.iter().for_each(|x| go(x, out)),
            }
        }
        let mut out = BTreeSet::new();
        go(self, &mut out);
        out
    }
}

impl std::fmt::Display for Formula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Formula::True => write!(f, "true"),
            Formula::False => write!(f, "false"),
            Formula::Atom(n) => write!(f, "{}", n.index()),
            Formula::Not(inner) => write!(f, "!{inner}"),
            Formula::And(fs) => {
                let parts: Vec<String> = fs.iter().map(|x| x.to_string()).collect();
                write!(f, "({})", parts.join(" & "))
            }
            Formula::Or(fs) => {
                let parts: Vec<String> = fs.iter().map(|x| x.to_string()).collect();
                write!(f, "({})", parts.join(" | "))
            }
        }
    }
}

/// `Formula::Atom` over a Move-bytecode block id. Wraps the `code as usize` → `NodeIndex`
/// conversion so callers don't open-code it (and so a tagged-label refactor has a single
/// site to update). Used at every site that builds a single-block guard — the dom-tree
/// structurer's `CondIf`, the reaching diamond folder's atomic predicates.
pub fn cond_atom(code: u64) -> Formula {
    Formula::Atom(NodeIndex::new(code as usize))
}

/// Conjunction with constructor hygiene only (flatten nested `And`, drop `True`, short-circuit
/// on `False`). This is *not* the simplifier — no absorption or complementarity reasoning; that
/// is the boolean-recovery refinement's job.
pub fn and(formulas: Vec<Formula>) -> Formula {
    let mut out = Vec::new();
    for f in formulas {
        match f {
            Formula::True => {}
            Formula::False => return Formula::False,
            Formula::And(inner) => out.extend(inner),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Formula::True,
        1 => out.pop().unwrap(),
        _ => Formula::And(out),
    }
}

/// Disjunction with constructor hygiene only (flatten nested `Or`, drop `False`, short-circuit
/// on `True`). See [`and`] on what this deliberately does *not* do.
pub fn or(formulas: Vec<Formula>) -> Formula {
    let mut out = Vec::new();
    for f in formulas {
        match f {
            Formula::False => {}
            Formula::True => return Formula::True,
            Formula::Or(inner) => out.extend(inner),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Formula::False,
        1 => out.pop().unwrap(),
        _ => Formula::Or(out),
    }
}

/// Negation, collapsing constants and double negation.
pub fn not(formula: Formula) -> Formula {
    match formula {
        Formula::True => Formula::False,
        Formula::False => Formula::True,
        Formula::Not(inner) => *inner,
        other => Formula::Not(Box::new(other)),
    }
}

/// The predicate under which edge `p → n` is taken, given `p`'s input node.
fn edge_condition(pred_input: Option<&Input>, p: NodeIndex, n: NodeIndex) -> Formula {
    match pred_input {
        Some(Input::Condition(_, _, then, els)) => {
            if n == *then {
                Formula::Atom(p)
            } else if n == *els {
                not(Formula::Atom(p))
            } else {
                // `n` is not an arm of `p` — the caller's edge set is inconsistent with the
                // condition's recorded arms. The adjacency build above only enumerates edges
                // produced by `Input::edges`, which for a `Condition` returns exactly
                // `(p, then)` and `(p, else)`, so reaching this arm means a Condition's
                // arms were rewritten after the topo build. In release we fall back to a
                // conservative `True` guard rather than panic — the resulting reaching set
                // is over-broad but sound enough to keep the dom-tree fallback honest.
                debug_assert!(
                    false,
                    "edge {p:?} -> {n:?} not in Condition's arms (then={then:?}, else={els:?})",
                );
                Formula::True
            }
        }
        // Unconditional fall-through, or a node we don't model: the edge is always taken.
        _ => Formula::True,
    }
}

/// Compute reaching conditions for every node of an acyclic region described by `input`,
/// rooted at `entry`. Returns `None` if the region contains a cycle (a back edge — not
/// loop-free) or any enum-`Variants` dispatch (not yet modeled).
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
    reach.insert(entry, Formula::True);
    for &n in &topo {
        if n == entry {
            continue;
        }
        let mut terms = Vec::new();
        for &p in preds.get(&n).into_iter().flatten() {
            let rp = reach.get(&p).cloned().unwrap_or(Formula::False);
            terms.push(and(vec![rp, edge_condition(input.get(&p), p, n)]));
        }
        reach.insert(n, or(terms));
    }
    Some(reach)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structuring::ast::Input as In;

    fn n(i: u32) -> NodeIndex {
        i.into()
    }

    fn atom(i: u32) -> Formula {
        Formula::Atom(n(i))
    }

    // Mirrors tests/structuring/guarded_chain_nested.stt: two chained pyth-style diamonds
    // (abs_diff + threshold) converging on a far join (20). Verifies the engine reproduces the
    // by-hand reaching-condition table.
    fn guarded_chain_nested() -> BTreeMap<NodeIndex, In> {
        let entries = vec![
            In::Condition(n(0), 0, n(1), n(2)),
            In::Condition(n(1), 1, n(3), n(4)),
            In::Condition(n(2), 2, n(5), n(4)),
            In::Code(n(3), 3, Some(n(20))),
            In::Code(n(5), 5, Some(n(20))),
            In::Condition(n(4), 4, n(6), n(7)),
            In::Condition(n(6), 6, n(8), n(10)),
            In::Condition(n(7), 7, n(9), n(10)),
            In::Code(n(8), 8, Some(n(20))),
            In::Code(n(9), 9, Some(n(20))),
            In::Code(n(10), 10, Some(n(20))),
            In::Code(n(20), 20, None),
        ];
        entries.into_iter().map(|e| (e.label(), e)).collect()
    }

    #[test]
    fn reaching_conditions_match_by_hand_table() {
        let input = guarded_chain_nested();
        let reach = reaching_conditions(&input, n(0)).expect("region is acyclic");

        // R(3) = a0 ∧ a1   (feed-a, then-side, stale)
        assert_eq!(reach[&n(3)], and(vec![atom(0), atom(1)]));
        // R(5) = ¬a0 ∧ a2  (feed-a, else-side, stale)
        assert_eq!(reach[&n(5)], and(vec![not(atom(0)), atom(2)]));
        // R(4) = (a0 ∧ ¬a1) ∨ (¬a0 ∧ ¬a2)  == "not stale on a"
        assert_eq!(
            reach[&n(4)],
            or(vec![
                and(vec![atom(0), not(atom(1))]),
                and(vec![not(atom(0)), not(atom(2))]),
            ])
        );
        // R(8) = R(4) ∧ a4 ∧ a6  (feed-b reached only when feed-a fresh)
        assert_eq!(
            reach[&n(8)],
            and(vec![reach[&n(4)].clone(), atom(4), atom(6)])
        );
        // R(9) = R(4) ∧ ¬a4 ∧ a7
        assert_eq!(
            reach[&n(9)],
            and(vec![reach[&n(4)].clone(), not(atom(4)), atom(7)])
        );
    }

    #[test]
    fn bails_on_cycle() {
        // A back edge 1 → 0 makes the region non-loop-free.
        let entries = vec![
            In::Condition(n(0), 0, n(1), n(2)),
            In::Code(n(1), 1, Some(n(0))),
            In::Code(n(2), 2, None),
        ];
        let input: BTreeMap<NodeIndex, In> = entries.into_iter().map(|e| (e.label(), e)).collect();
        assert!(reaching_conditions(&input, n(0)).is_none());
    }
}
