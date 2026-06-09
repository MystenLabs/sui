// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Reduced Ordered BDD over branch-condition atoms (No More Gotos, phase 2 — the solver)
// -------------------------------------------------------------------------------------------------
// A reaching condition is a boolean function over branch-predicate atoms. We canonicalize it as
// a Reduced Ordered Binary Decision Diagram, which buys two things the DNF minimizer couldn't:
//
//  * Canonicity — equivalence, tautology, and contradiction are *pointer equality* on `BddId`.
//    "Is R(44) ≡ ¬stale_a?" is `build(R(44)) == build(¬stale_a)`.
//  * Factored read-back — a node `(v, low, high)` *is* `if (v) { high } else { low }`, and a
//    subterm shared across the function (e.g. `¬stale_a`) is one shared node, not copied onto
//    every product. Reading a node back is the nested `if`/`&&`/`||` we want; the gluing that
//    `stale_a ∨ (¬stale_a ∧ stale_b)` introduced never happens because the BDD reduces it away.
//
// Variable order matters for read-back shape: the topmost variable is the one whose if-then-else
// wraps the rest. The default constructor falls back to ascending `NodeIndex`. The reaching
// structurer prefers reverse-postorder rank instead (constructed via `with_order`) — that's the
// order the structurer walks the CFG, so the lowered guard nests in roughly source order rather
// than whatever NodeIndex assignment the bytecode happened to pick. Atoms are tiny per region,
// so the worst-case-exponential BDD size is a non-issue regardless.

use crate::structuring::reaching::{Formula, and, not, or};
use petgraph::graph::NodeIndex;
use std::collections::HashMap;

/// Index into the BDD node arena. `0` and `1` are the terminals.
pub type BddId = usize;

#[derive(Clone, Copy)]
struct Node {
    var: NodeIndex,
    /// Subgraph taken when `var` is false.
    low: BddId,
    /// Subgraph taken when `var` is true.
    high: BddId,
}

/// A BDD arena. All ids returned by one arena are only comparable within that arena.
pub struct Bdd {
    nodes: Vec<Node>,
    unique: HashMap<(NodeIndex, BddId, BddId), BddId>,
    not_memo: HashMap<BddId, BddId>,
    and_memo: HashMap<(BddId, BddId), BddId>,
    or_memo: HashMap<(BddId, BddId), BddId>,
    /// Per-atom rank (smallest = topmost in the diagram). Populated via `with_order` so the
    /// structurer can ask for read-back in reverse-postorder; empty for `new()` which falls
    /// back to ascending `NodeIndex`.
    var_order: HashMap<NodeIndex, u32>,
}

impl Default for Bdd {
    fn default() -> Self {
        Self::new()
    }
}

impl Bdd {
    pub const FALSE: BddId = 0;
    pub const TRUE: BddId = 1;

    pub fn new() -> Self {
        Self::with_order(HashMap::new())
    }

    /// Build an arena with an explicit variable order. `var_order[atom] = rank` (smaller =
    /// topmost). Atoms not in the map fall back to their `NodeIndex` for ordering purposes —
    /// callers who care about read-back shape should pass a rank for every atom they'll build.
    pub fn with_order(var_order: HashMap<NodeIndex, u32>) -> Self {
        // Placeholders so ids 0/1 are the terminals; their fields are never read because every
        // operation special-cases terminals before touching `var`/`low`/`high`.
        let placeholder = Node {
            var: NodeIndex::new(0),
            low: 0,
            high: 0,
        };
        Bdd {
            nodes: vec![placeholder, placeholder],
            unique: HashMap::new(),
            not_memo: HashMap::new(),
            and_memo: HashMap::new(),
            or_memo: HashMap::new(),
            var_order,
        }
    }

    /// Rank used for variable ordering. Caller-supplied rank takes precedence; otherwise we
    /// fall back to `NodeIndex` raw value so the default `new()` case orders by program order.
    fn rank(&self, v: NodeIndex) -> u32 {
        self.var_order
            .get(&v)
            .copied()
            .unwrap_or_else(|| v.index() as u32)
    }

    /// Reduced node constructor: collapse `low == high` (no decision to make) and hash-cons the
    /// rest, which together keep the diagram reduced and ordered.
    fn mk(&mut self, var: NodeIndex, low: BddId, high: BddId) -> BddId {
        if low == high {
            return low;
        }
        if let Some(&id) = self.unique.get(&(var, low, high)) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(Node { var, low, high });
        self.unique.insert((var, low, high), id);
        id
    }

    /// Build the canonical BDD for a [`Formula`].
    pub fn build(&mut self, formula: &Formula) -> BddId {
        match formula {
            Formula::True => Self::TRUE,
            Formula::False => Self::FALSE,
            Formula::Atom(n) => self.mk(*n, Self::FALSE, Self::TRUE),
            Formula::Not(inner) => {
                let f = self.build(inner);
                self.not(f)
            }
            Formula::And(fs) => {
                let mut acc = Self::TRUE;
                for f in fs {
                    let g = self.build(f);
                    acc = self.and(acc, g);
                }
                acc
            }
            Formula::Or(fs) => {
                let mut acc = Self::FALSE;
                for f in fs {
                    let g = self.build(f);
                    acc = self.or(acc, g);
                }
                acc
            }
        }
    }

    /// `id` is one of the two constant leaves (`FALSE` / `TRUE`). Use to short-circuit before
    /// pulling the node out of `nodes` (terminals' placeholder fields are not safe to read).
    fn is_terminal(id: BddId) -> bool {
        id == Self::FALSE || id == Self::TRUE
    }

    pub fn not(&mut self, f: BddId) -> BddId {
        if f == Self::FALSE {
            return Self::TRUE;
        }
        if f == Self::TRUE {
            return Self::FALSE;
        }
        if let Some(&r) = self.not_memo.get(&f) {
            return r;
        }
        let Node { var, low, high } = self.nodes[f];
        let low = self.not(low);
        let high = self.not(high);
        let r = self.mk(var, low, high);
        self.not_memo.insert(f, r);
        r
    }

    pub fn and(&mut self, f: BddId, g: BddId) -> BddId {
        if f == Self::FALSE || g == Self::FALSE {
            return Self::FALSE;
        }
        if f == Self::TRUE {
            return g;
        }
        if g == Self::TRUE {
            return f;
        }
        if f == g {
            return f;
        }
        debug_assert!(!Self::is_terminal(f) && !Self::is_terminal(g));
        let key = if f <= g { (f, g) } else { (g, f) };
        if let Some(&r) = self.and_memo.get(&key) {
            return r;
        }
        let (v, fl, fh, gl, gh) = self.cofactors(f, g);
        let low = self.and(fl, gl);
        let high = self.and(fh, gh);
        let r = self.mk(v, low, high);
        self.and_memo.insert(key, r);
        r
    }

    fn or(&mut self, f: BddId, g: BddId) -> BddId {
        if f == Self::TRUE || g == Self::TRUE {
            return Self::TRUE;
        }
        if f == Self::FALSE {
            return g;
        }
        if g == Self::FALSE {
            return f;
        }
        if f == g {
            return f;
        }
        debug_assert!(!Self::is_terminal(f) && !Self::is_terminal(g));
        let key = if f <= g { (f, g) } else { (g, f) };
        if let Some(&r) = self.or_memo.get(&key) {
            return r;
        }
        let (v, fl, fh, gl, gh) = self.cofactors(f, g);
        let low = self.or(fl, gl);
        let high = self.or(fh, gh);
        let r = self.mk(v, low, high);
        self.or_memo.insert(key, r);
        r
    }

    /// The topmost (smallest-rank) variable of two internal nodes, and the four cofactors of
    /// `f` and `g` at it. A node not mentioning `v` is its own cofactor on both branches.
    fn cofactors(&self, f: BddId, g: BddId) -> (NodeIndex, BddId, BddId, BddId, BddId) {
        let nf = self.nodes[f];
        let ng = self.nodes[g];
        let v = if self.rank(nf.var) <= self.rank(ng.var) {
            nf.var
        } else {
            ng.var
        };
        let (fl, fh) = if nf.var == v {
            (nf.low, nf.high)
        } else {
            (f, f)
        };
        let (gl, gh) = if ng.var == v {
            (ng.low, ng.high)
        } else {
            (g, g)
        };
        (v, fl, fh, gl, gh)
    }

    /// Read a BDD back to a factored [`Formula`]. A node `(v, low, high)` is `if v then high
    /// else low`; the constant-leaf cases collapse to `Atom`, `¬Atom`, `v ∧ …`, `v ∨ …` so the
    /// common short-circuit shapes come out directly.
    pub fn to_formula(&self, id: BddId) -> Formula {
        if id == Self::FALSE {
            return Formula::False;
        }
        if id == Self::TRUE {
            return Formula::True;
        }
        let Node { var, low, high } = self.nodes[id];
        let atom = Formula::Atom(var);
        let hi = self.to_formula(high);
        let lo = self.to_formula(low);
        match (&hi, &lo) {
            (Formula::True, Formula::False) => atom,
            (Formula::False, Formula::True) => not(atom),
            (Formula::True, _) => or(vec![atom, lo]),
            (_, Formula::False) => and(vec![atom, hi]),
            (Formula::False, _) => and(vec![not(atom), lo]),
            (_, Formula::True) => or(vec![not(atom), hi]),
            _ => or(vec![and(vec![atom.clone(), hi]), and(vec![not(atom), lo])]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structuring::ast::Input as In;
    use crate::structuring::reaching::reaching_conditions;
    use std::collections::BTreeMap;

    fn n(i: u32) -> NodeIndex {
        i.into()
    }
    fn atom(i: u32) -> Formula {
        Formula::Atom(n(i))
    }

    // Same shape as tests/structuring/guarded_chain_nested.stt.
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
    fn canonicalizes_glued_factor() {
        // stale_a ∨ (¬stale_a ∧ stale_b)  ≡  stale_a ∨ stale_b. The redundant `¬stale_a` the DNF
        // minimizer left glued onto stale_b becomes pointer equality here — the BDD never glues
        // it. (Disjoint atom sets so the two are genuinely equal.)
        let stale_a = or(vec![
            and(vec![atom(0), atom(1)]),
            and(vec![not(atom(0)), atom(2)]),
        ]);
        let stale_b = or(vec![
            and(vec![atom(4), atom(5)]),
            and(vec![not(atom(4)), atom(6)]),
        ]);
        let glued = or(vec![
            stale_a.clone(),
            and(vec![not(stale_a.clone()), stale_b.clone()]),
        ]);
        let clean = or(vec![stale_a, stale_b]);
        let mut b = Bdd::new();
        assert_eq!(b.build(&glued), b.build(&clean));
    }

    #[test]
    fn combining_and_constants() {
        let mut b = Bdd::new();
        // (a ∧ b) ∨ (a ∧ ¬b) = a
        let f = or(vec![
            and(vec![atom(0), atom(1)]),
            and(vec![atom(0), not(atom(1))]),
        ]);
        assert_eq!(b.build(&f), b.build(&atom(0)));
        assert_eq!(b.build(&or(vec![atom(0), not(atom(0))])), Bdd::TRUE);
        assert_eq!(b.build(&and(vec![atom(0), not(atom(0))])), Bdd::FALSE);
    }

    #[test]
    fn pyth_join_is_tautology_and_fresh_is_complement() {
        let input = guarded_chain_nested();
        let reach = reaching_conditions(&input, n(0)).expect("acyclic");
        let mut b = Bdd::new();
        // Every path reaches the join (20).
        assert_eq!(b.build(&reach[&n(20)]), Bdd::TRUE);
        // R(4) ("not stale on feed a") is the exact complement of stale_a = R(3) ∨ R(5).
        let stale_a = or(vec![reach[&n(3)].clone(), reach[&n(5)].clone()]);
        let r4 = b.build(&reach[&n(4)]);
        let neg_stale = b.build(&not(stale_a));
        assert_eq!(r4, neg_stale);
    }

    #[test]
    fn readback_round_trips() {
        let f = or(vec![
            and(vec![atom(0), atom(1)]),
            and(vec![not(atom(0)), atom(2)]),
        ]);
        let mut b = Bdd::new();
        let id = b.build(&f);
        let back = b.to_formula(id);
        // The recovered formula builds to the same canonical BDD.
        assert_eq!(b.build(&back), id);
    }
}
