// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! NMG §V-B multi-exit loop structuring primitives.
//!
//! `structure_loop` produces a single-exit `Loop` and an abstract `Input::Reduced` marker
//! whose outgoing edges carry the reaching condition of each exit from the loop body. The
//! outer scope's acyclic structurer treats the abstract node like any other multi-successor
//! node - `region::edge_condition` reads the per-edge formula, `reaching_conditions`
//! propagates it into the post-loop items list, and `recover_control_flow`'s existing
//! refinement passes build the `if`/`else` cascade from those reach conditions. No dedicated
//! cascade emitter, no `SelectorMatch` dispatch, no per-loop special case in the outer scope.
//!
//! This module holds the V-B-specific helpers that don't fit cleanly into `loops.rs`.

use crate::structuring::{
    ast::{self as D},
    predicates::{self, Formula},
    region::{self, SinkBehavior, SinkRendering},
};
use petgraph::graph::NodeIndex;
use std::collections::{BTreeMap, HashSet};

/// Per-exit reaching-condition formulas for a natural loop.
///
/// For loop head `loop_head` with body `loop_nodes`, projects the body into an acyclic
/// subgraph and computes `reaching_conditions` over it. Each exit sink in the projection
/// corresponds to a distinct exit target `v ∉ loop_nodes`; its reach condition is the
/// Boolean formula under which body-flow leaves the loop toward `v`.
///
/// Returned map: original exit target → its body-exit formula. The back-edge to
/// `loop_head` is intentionally excluded from the result (it's a Continue, not an exit).
///
/// Returns `None` iff `region::reaching_conditions` fails on the projection - which
/// shouldn't happen for a well-formed loop, since `build_acyclic_projection` guarantees
/// the result is acyclic. Callers can `.expect` unless they're constructing pathological
/// inputs.
pub(crate) fn compute_loop_exit_formulas(
    input: &BTreeMap<NodeIndex, D::Input>,
    loop_head: NodeIndex,
    loop_nodes: &HashSet<NodeIndex>,
) -> Option<BTreeMap<NodeIndex, Formula>> {
    let region_input: BTreeMap<NodeIndex, D::Input> = input
        .iter()
        .filter(|(k, _)| loop_nodes.contains(k))
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    let members: HashSet<NodeIndex> = loop_nodes
        .iter()
        .copied()
        .filter(|n| *n != loop_head)
        .collect();

    let projection =
        region::build_acyclic_projection(&region_input, loop_head, &members, SinkRendering::Loop);
    let reach = region::reaching_conditions(&projection.input, loop_head)?;

    let mut out: BTreeMap<NodeIndex, Formula> = BTreeMap::new();
    for (sink_id, behavior) in &projection.sinks {
        // Only real out-of-region sinks carry an exit target != loop_head. The back-edge
        // sink also renders as `Exit(loop_head)` under `SinkRendering::Loop`; skip it.
        let SinkBehavior::Exit(target) = behavior else {
            continue;
        };
        if *target == loop_head {
            continue;
        }
        let formula = reach
            .get(sink_id)
            .cloned()
            .unwrap_or_else(predicates::true_);
        out.insert(*target, formula);
    }
    Some(out)
}

/// Rewrite every `Structured::Jump(_, target)` with `target ∈ exit_set` to
/// `Structured::Break(loop_head)`. Recurs through every containing form
/// (`Seq`/`CondIf`/`Loop`/`Switch`/`SelectorMatch`).
///
/// Used by `structure_loop` in the V-B path: `insert_breaks` already turns the primary
/// exit into a `Break`; this pass extends that rewriting to *every* body-exit target
/// regardless of ownership. After V-B the loop is single-exit and the outer scope's
/// acyclic structuring picks up the per-exit distinction from the `Reduced` marker's
/// per-edge formulas rather than from a `SelectorMatch`.
///
/// `exit_set` is deliberately not a `&HashSet` - callers usually build a small `Vec` of
/// owned + unowned successors, and the `contains` check on a slice is fine at typical
/// loop-exit counts (≤ 4 in practice).
pub(crate) fn rewrite_owned_jumps_as_breaks(
    node: &mut D::Structured,
    loop_head: NodeIndex,
    exit_set: &[NodeIndex],
) {
    use D::Structured as S;
    match node {
        S::Jump(_, target) if exit_set.contains(target) => {
            *node = S::Break(loop_head);
        }
        S::Seq(items) => {
            for item in items.iter_mut() {
                rewrite_owned_jumps_as_breaks(item, loop_head, exit_set);
            }
        }
        S::CondIf(_, conseq, alt) => {
            rewrite_owned_jumps_as_breaks(conseq, loop_head, exit_set);
            if let Some(alt_inner) = alt.as_mut().as_mut() {
                rewrite_owned_jumps_as_breaks(alt_inner, loop_head, exit_set);
            }
        }
        S::Loop(_, body) => rewrite_owned_jumps_as_breaks(body, loop_head, exit_set),
        S::Switch(_, _, arms) => {
            for (_, body) in arms.iter_mut() {
                rewrite_owned_jumps_as_breaks(body, loop_head, exit_set);
            }
        }
        S::Jump(..) | S::Block(_) | S::Break(_) | S::Continue(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structuring::predicates;
    use move_symbol_pool::Symbol;
    use petgraph::graph::NodeIndex;
    use std::collections::{BTreeMap, HashSet};

    fn label(n: u32) -> NodeIndex {
        NodeIndex::new(n as usize)
    }

    /// Two-owned-succ loop:
    ///   0 (head) --cond--> 1 (body) -true--> 2 (back-edge to 0)
    ///                                  -false--> 3 (owned exit A)
    ///                              --false--> 4 (owned exit B)
    ///
    /// Under the paper-faithful V-B, the body's projection sees exits to 3 and 4 gated by
    /// distinct branch predicates from block 1 (the inner condition).
    #[test]
    fn two_exits_get_distinct_formulas() {
        let mut input: BTreeMap<NodeIndex, D::Input> = BTreeMap::new();
        // Head at 0: Condition, then=1, else=4 (unconditional exit to B).
        input.insert(
            label(0),
            D::Input::Condition(label(0), 0, label(1), label(4)),
        );
        // Body block 1: Condition, then=2 (back-edge to head), else=3 (exit A).
        input.insert(
            label(1),
            D::Input::Condition(label(1), 1, label(2), label(3)),
        );
        // Block 2: back-edge to head (Code with target 0).
        input.insert(label(2), D::Input::Code(label(2), 2, Some(label(0))));
        // Exit targets 3 and 4 are outside the loop; they don't appear in `input` here.

        let loop_nodes: HashSet<NodeIndex> = [label(0), label(1), label(2)].into_iter().collect();
        let formulas = compute_loop_exit_formulas(&input, label(0), &loop_nodes)
            .expect("reaching_conditions must succeed");
        assert_eq!(formulas.len(), 2, "expected formulas for both exits");
        let f_a = formulas.get(&label(3)).cloned().expect("exit A missing");
        let f_b = formulas.get(&label(4)).cloned().expect("exit B missing");
        assert_ne!(f_a, f_b, "distinct exits must get distinct formulas");
        // Neither formula should be `False` - both exits are actually reachable.
        assert_ne!(f_a, predicates::false_(), "exit A formula is False");
        assert_ne!(f_b, predicates::false_(), "exit B formula is False");
    }

    /// Single-exit loop:
    ///   0 (head) --cond--> 1 (back-edge to 0)
    ///                      --other--> 2 (exit)
    ///
    /// Returns exactly one formula, for the exit target.
    #[test]
    fn single_exit_yields_one_formula() {
        let mut input: BTreeMap<NodeIndex, D::Input> = BTreeMap::new();
        input.insert(
            label(0),
            D::Input::Condition(label(0), 0, label(1), label(2)),
        );
        input.insert(label(1), D::Input::Code(label(1), 1, Some(label(0))));

        let loop_nodes: HashSet<NodeIndex> = [label(0), label(1)].into_iter().collect();
        let formulas =
            compute_loop_exit_formulas(&input, label(0), &loop_nodes).expect("must succeed");
        assert_eq!(formulas.len(), 1);
        assert!(formulas.contains_key(&label(2)));
    }

    /// Loop with a `Variants` node in the body. The formulas for exits from each variant
    /// arm should reference the corresponding `__matchN_K` atoms.
    #[test]
    fn variants_body_produces_match_atom_formulas() {
        let mut input: BTreeMap<NodeIndex, D::Input> = BTreeMap::new();
        // Head at 0: Variants with two arms, one back-edges (loops), one exits.
        let variant_a: Symbol = Symbol::from("A");
        let variant_b: Symbol = Symbol::from("B");
        let enum_qid: (move_binary_format::normalized::ModuleId<Symbol>, Symbol) = (
            move_binary_format::normalized::ModuleId {
                address: move_core_types::account_address::AccountAddress::ZERO,
                name: Symbol::from("M"),
            },
            Symbol::from("E"),
        );
        input.insert(
            label(0),
            D::Input::Variants(
                label(0),
                0,
                enum_qid,
                vec![(variant_a, label(1)), (variant_b, label(2))],
            ),
        );
        // Block 1: back-edge to head.
        input.insert(label(1), D::Input::Code(label(1), 1, Some(label(0))));
        // Block 2: exit (target 3 is outside the loop).

        let loop_nodes: HashSet<NodeIndex> = [label(0), label(1)].into_iter().collect();
        let formulas =
            compute_loop_exit_formulas(&input, label(0), &loop_nodes).expect("must succeed");
        assert_eq!(formulas.len(), 1);
        let f = formulas.get(&label(2)).cloned().expect("exit missing");
        // The formula should reference variant B's match atom.
        let match_b = predicates::match_atom(0, variant_b.as_str());
        // The reach formula for the exit should include this atom (possibly wrapped in
        // conjunctions with True/entry conditions).
        assert!(
            f.atoms()
                .contains(&predicates::match_atom_name(0, variant_b.as_str())),
            "expected exit formula to reference match_atom for B; got: {f:?}"
        );
        // Sanity: shouldn't reference the LOOPING variant.
        assert!(
            !f.atoms()
                .contains(&predicates::match_atom_name(0, variant_a.as_str())),
            "exit formula shouldn't reference match_atom for A (the back-edge variant)"
        );
        // Silence unused if match_b isn't needed (kept above for readability).
        let _ = match_b;
    }

    // ---------------------------------------------------------------------------------------
    // rewrite_owned_jumps_as_breaks
    // ---------------------------------------------------------------------------------------

    #[test]
    fn rewrites_jumps_to_exit_targets() {
        // Seq[ Jump(3), Jump(4), Jump(5) ] with exit_set = [3, 4] rewrites to
        // Seq[ Break(loop_head), Break(loop_head), Jump(5) ].
        let mut body = D::Structured::Seq(vec![
            D::Structured::Jump(D::GotoSource::ReachingExit, label(3)),
            D::Structured::Jump(D::GotoSource::ReachingExit, label(4)),
            D::Structured::Jump(D::GotoSource::ReachingExit, label(5)),
        ]);
        let loop_head = label(0);
        let exits = vec![label(3), label(4)];
        rewrite_owned_jumps_as_breaks(&mut body, loop_head, &exits);
        let D::Structured::Seq(items) = &body else {
            panic!("expected Seq")
        };
        assert!(matches!(items[0], D::Structured::Break(n) if n == loop_head));
        assert!(matches!(items[1], D::Structured::Break(n) if n == loop_head));
        assert!(matches!(items[2], D::Structured::Jump(_, n) if n == label(5)));
    }

    #[test]
    fn recurs_into_condif_and_loop() {
        // CondIf(_, Jump(3), Some(Loop(_, Jump(4)))) with exit_set=[3,4] rewrites both.
        let mut body = D::Structured::CondIf(
            predicates::true_(),
            Box::new(D::Structured::Jump(D::GotoSource::ReachingExit, label(3))),
            Box::new(Some(D::Structured::Loop(
                label(9),
                Box::new(D::Structured::Jump(D::GotoSource::ReachingExit, label(4))),
            ))),
        );
        rewrite_owned_jumps_as_breaks(&mut body, label(0), &[label(3), label(4)]);
        let D::Structured::CondIf(_, then, alt) = &body else {
            panic!("expected CondIf")
        };
        assert!(matches!(**then, D::Structured::Break(n) if n == label(0)));
        let D::Structured::Loop(_, inner) = alt.as_ref().as_ref().unwrap() else {
            panic!("expected Loop in alt")
        };
        assert!(matches!(**inner, D::Structured::Break(n) if n == label(0)));
    }

    #[test]
    fn leaves_non_exit_jumps_alone() {
        let mut body = D::Structured::Jump(D::GotoSource::ReachingExit, label(99));
        rewrite_owned_jumps_as_breaks(&mut body, label(0), &[label(3), label(4)]);
        assert!(matches!(body, D::Structured::Jump(_, n) if n == label(99)));
    }
}
