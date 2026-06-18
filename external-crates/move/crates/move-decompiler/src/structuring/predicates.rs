// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Boolean predicate algebra over named atoms.
//!
//! `Formula` is the system's single representation of recovered boolean
//! conditions. It's built by the dom-tree structurer (single-atom guards)
//! and the reaching-condition acyclic structurer (compound guards from the
//! diamond fold and the reaching-conditions analysis), consumed by the
//! `CondIf` lowering in `translate.rs` via [`Formula::to_exp`].
//!
//! All values returned by the smart constructors below are in a strong
//! normal form:
//!
//!   1. **NNF.** `Not` only ever wraps an `Atom`.
//!   2. **Flat.** No `And` inside `And`, no `Or` inside `Or`.
//!   3. **Sorted.** Children of `And` / `Or` are in canonical order.
//!   4. **Deduped.** No repeated children.
//!   5. **Identities / short-circuits.** `True && x = x`, `False && x = False`,
//!      symmetric for `Or`.
//!   6. **Complementation.** `A && !A -> False`, `A || !A -> True`.
//!   7. **Absorption.** `A || (A && X) -> A`, `A && (A || X) -> A`.
//!
//! Distribution (`(A&&X) || (A&&Y) -> A&&(X||Y)`) is intentionally NOT applied -
//! it can blow up depth and the formulas this layer produces today don't
//! need it.
//!
//! Atoms carry their variable name directly. The structurer convention
//! `__c{N}` (for condition block N's test value) is centralized via
//! [`cond_var_name`] / [`cond_block_from_name`] / [`cond_atom`]; nothing
//! else in the system encodes the mapping.

use crate::ast::Exp;
use move_stackless_bytecode_2::ast::PrimitiveOp;
use move_symbol_pool::Symbol;
use petgraph::graph::NodeIndex;

use std::collections::BTreeSet;

// -------------------------------------------------------------------------------------------------
// Type
// -------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Formula(FormulaTree);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum FormulaTree {
    // Discriminant order is load-bearing - it's the canonical sort order for the
    // normalizing constructors. `False` < `True` < `Atom` < `Not` < `And` < `Or` so
    // constant short-circuits collapse first, then literals, then compounds.
    False,
    True,
    Atom(Symbol),
    /// In NNF; `Not` only ever wraps an `Atom`.
    Not(Box<Formula>),
    And(Vec<Formula>),
    Or(Vec<Formula>),
}

// -------------------------------------------------------------------------------------------------
// Bridges: condition block ids <-> atom names
// -------------------------------------------------------------------------------------------------

/// Prefix for synthetic locals binding a condition block's test value. The leading `__`
/// keeps the name outside any user-writable identifier space; the `c` prefix is greppable
/// so a reader of decompiled source can tell it's a synthesized condition variable.
const COND_PREFIX: &str = "__c";

/// Synthetic local name for the test value of condition block `n`. Called at the lowering
/// site that emits `let __c{n} = <test>` and at every atom-construction site that names
/// the same block; the two agree by construction.
pub fn cond_var_name(n: NodeIndex) -> Symbol {
    Symbol::from(format!("{COND_PREFIX}{}", n.index()))
}

/// Inverse of [`cond_var_name`]. Returns `None` if `name` wasn't produced by the
/// convention.
pub fn cond_block_from_name(name: Symbol) -> Option<NodeIndex> {
    name.as_str()
        .strip_prefix(COND_PREFIX)?
        .parse::<usize>()
        .ok()
        .map(NodeIndex::new)
}

/// `Atom` over a condition block's id (`u64`-shaped, as the structurer's `Code` /
/// `Label` types).
pub fn cond_atom(code: u64) -> Formula {
    atom(cond_var_name(NodeIndex::new(code as usize)))
}

/// `cond_atom(code)` if `positive`, otherwise its negation. Used where a branch is matched
/// by either its `then` polarity or its `else` polarity depending on caller context.
pub fn cond_atom_polarized(code: u64, positive: bool) -> Formula {
    let a = cond_atom(code);
    if positive { a } else { not(a) }
}

// -------------------------------------------------------------------------------------------------
// Smart constructors (normalizing - see module-level comment for the invariants)
// -------------------------------------------------------------------------------------------------

pub fn atom(name: Symbol) -> Formula {
    Formula(FormulaTree::Atom(name))
}

pub fn true_() -> Formula {
    Formula(FormulaTree::True)
}

pub fn false_() -> Formula {
    Formula(FormulaTree::False)
}

pub fn not(f: Formula) -> Formula {
    match f.0 {
        FormulaTree::True => false_(),
        FormulaTree::False => true_(),
        FormulaTree::Atom(s) => Formula(FormulaTree::Not(Box::new(atom(s)))),
        FormulaTree::Not(inner) => *inner,
        // De Morgan: results pass back through `and`/`or` so the rest of the normal
        // form is re-established.
        FormulaTree::And(children) => or(children.into_iter().map(not).collect()),
        FormulaTree::Or(children) => and(children.into_iter().map(not).collect()),
    }
}

pub fn and(formulas: Vec<Formula>) -> Formula {
    let mut out: Vec<Formula> = Vec::new();
    for f in formulas {
        match f.0 {
            FormulaTree::True => continue,
            FormulaTree::False => return false_(),
            FormulaTree::And(inner) => out.extend(inner),
            other => out.push(Formula(other)),
        }
    }
    out.sort();
    out.dedup();
    if has_complementary_pair(&out) {
        return false_();
    }
    absorb_or_children(&mut out);
    match out.len() {
        0 => true_(),
        1 => out.into_iter().next().unwrap(),
        _ => Formula(FormulaTree::And(out)),
    }
}

pub fn or(formulas: Vec<Formula>) -> Formula {
    let mut out: Vec<Formula> = Vec::new();
    for f in formulas {
        match f.0 {
            FormulaTree::False => continue,
            FormulaTree::True => return true_(),
            FormulaTree::Or(inner) => out.extend(inner),
            other => out.push(Formula(other)),
        }
    }
    out.sort();
    out.dedup();
    if has_complementary_pair(&out) {
        return true_();
    }
    absorb_and_children(&mut out);
    match out.len() {
        0 => false_(),
        1 => out.into_iter().next().unwrap(),
        _ => Formula(FormulaTree::Or(out)),
    }
}

/// `xs` contains both `X` and `Not(X)` for some `X`. Inputs are NNF, so the only `Not`s
/// directly inside `xs` wrap atoms.
fn has_complementary_pair(xs: &[Formula]) -> bool {
    let mut negated_inners: BTreeSet<&Formula> = BTreeSet::new();
    for f in xs {
        if let FormulaTree::Not(inner) = &f.0 {
            negated_inners.insert(inner.as_ref());
        }
    }
    xs.iter().any(|f| negated_inners.contains(f))
}

/// Inside an outer `Or`: drop any `And`-child whose conjuncts include any other outer
/// disjunct. `A || (A && X) -> A`.
fn absorb_and_children(xs: &mut Vec<Formula>) {
    let drop: Vec<bool> = xs
        .iter()
        .enumerate()
        .map(|(i, f)| match &f.0 {
            FormulaTree::And(conjuncts) => conjuncts
                .iter()
                .any(|c| xs.iter().enumerate().any(|(j, x)| j != i && x == c)),
            _ => false,
        })
        .collect();
    let kept: Vec<Formula> = std::mem::take(xs)
        .into_iter()
        .zip(drop)
        .filter_map(|(f, drop)| if drop { None } else { Some(f) })
        .collect();
    *xs = kept;
}

/// Inside an outer `And`: drop any `Or`-child whose disjuncts include any other outer
/// conjunct. `A && (A || X) -> A`.
fn absorb_or_children(xs: &mut Vec<Formula>) {
    let drop: Vec<bool> = xs
        .iter()
        .enumerate()
        .map(|(i, f)| match &f.0 {
            FormulaTree::Or(disjuncts) => disjuncts
                .iter()
                .any(|d| xs.iter().enumerate().any(|(j, x)| j != i && x == d)),
            _ => false,
        })
        .collect();
    let kept: Vec<Formula> = std::mem::take(xs)
        .into_iter()
        .zip(drop)
        .filter_map(|(f, drop)| if drop { None } else { Some(f) })
        .collect();
    *xs = kept;
}

// -------------------------------------------------------------------------------------------------
// Queries
// -------------------------------------------------------------------------------------------------

impl Formula {
    /// Every distinct atom name referenced by the formula.
    pub fn atoms(&self) -> BTreeSet<Symbol> {
        fn go(f: &Formula, out: &mut BTreeSet<Symbol>) {
            match &f.0 {
                FormulaTree::True | FormulaTree::False => {}
                FormulaTree::Atom(s) => {
                    out.insert(*s);
                }
                FormulaTree::Not(inner) => go(inner, out),
                FormulaTree::And(fs) | FormulaTree::Or(fs) => fs.iter().for_each(|x| go(x, out)),
            }
        }
        let mut out = BTreeSet::new();
        go(self, &mut out);
        out
    }

    /// Every distinct condition-block id referenced by the formula, parsed via the
    /// [`cond_var_name`] convention. Atoms not matching the convention are skipped.
    pub fn cond_atoms(&self) -> BTreeSet<NodeIndex> {
        self.atoms()
            .into_iter()
            .filter_map(cond_block_from_name)
            .collect()
    }

    /// `Some(name)` iff `self` is a single atom (the dom-tree structurer's product).
    pub fn as_atom(&self) -> Option<Symbol> {
        match &self.0 {
            FormulaTree::Atom(s) => Some(*s),
            _ => None,
        }
    }

    /// `Some(n)` iff `self` is a single condition-block atom parseable via the
    /// [`cond_var_name`] convention.
    pub fn as_cond_atom(&self) -> Option<NodeIndex> {
        self.as_atom().and_then(cond_block_from_name)
    }

    /// Lower to an `Exp`. Atoms become `Variable(name)`; the surrounding `let __c{n}`
    /// bindings live in the structured form's setup (emitted by the caller - typically
    /// the `CondIf` handler in `translate.rs`, which hoists each contributing condition
    /// block's term as setup), so this method has no environment to thread.
    pub fn to_exp(&self) -> Exp {
        fn prim(op: PrimitiveOp, args: Vec<Exp>) -> Exp {
            Exp::Primitive { op, args }
        }
        match &self.0 {
            FormulaTree::True => Exp::Value(move_core_types::runtime_value::MoveValue::Bool(true)),
            FormulaTree::False => {
                Exp::Value(move_core_types::runtime_value::MoveValue::Bool(false))
            }
            FormulaTree::Atom(s) => Exp::Variable(s.as_str().to_string()),
            FormulaTree::Not(inner) => prim(PrimitiveOp::Not, vec![inner.to_exp()]),
            FormulaTree::And(fs) => fs
                .iter()
                .map(Formula::to_exp)
                .reduce(|a, b| prim(PrimitiveOp::And, vec![a, b]))
                .expect("non-empty And after normalization"),
            FormulaTree::Or(fs) => fs
                .iter()
                .map(Formula::to_exp)
                .reduce(|a, b| prim(PrimitiveOp::Or, vec![a, b]))
                .expect("non-empty Or after normalization"),
        }
    }
}

impl std::fmt::Display for Formula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            FormulaTree::True => write!(f, "true"),
            FormulaTree::False => write!(f, "false"),
            FormulaTree::Atom(s) => write!(f, "{s}"),
            FormulaTree::Not(inner) => write!(f, "!{inner}"),
            FormulaTree::And(fs) => {
                let parts: Vec<String> = fs.iter().map(|x| x.to_string()).collect();
                write!(f, "({})", parts.join(" & "))
            }
            FormulaTree::Or(fs) => {
                let parts: Vec<String> = fs.iter().map(|x| x.to_string()).collect();
                write!(f, "({})", parts.join(" | "))
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn a(name: &str) -> Formula {
        atom(Symbol::from(name))
    }

    #[test]
    fn double_negation() {
        let p = a("p");
        assert_eq!(not(not(p.clone())), p);
    }

    #[test]
    fn de_morgan_and() {
        let p = a("p");
        let q = a("q");
        let lhs = not(and(vec![p.clone(), q.clone()]));
        let rhs = or(vec![not(p), not(q)]);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn de_morgan_or() {
        let p = a("p");
        let q = a("q");
        let lhs = not(or(vec![p.clone(), q.clone()]));
        let rhs = and(vec![not(p), not(q)]);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn complementation_and() {
        let p = a("p");
        assert_eq!(and(vec![p.clone(), not(p)]), false_());
    }

    #[test]
    fn complementation_or() {
        let p = a("p");
        assert_eq!(or(vec![p.clone(), not(p)]), true_());
    }

    #[test]
    fn absorption_or() {
        let p = a("p");
        let q = a("q");
        assert_eq!(or(vec![p.clone(), and(vec![p.clone(), q])]), p);
    }

    #[test]
    fn absorption_and() {
        let p = a("p");
        let q = a("q");
        assert_eq!(and(vec![p.clone(), or(vec![p.clone(), q])]), p);
    }

    #[test]
    fn dedup() {
        let p = a("p");
        assert_eq!(and(vec![p.clone(), p.clone()]), p);
        assert_eq!(or(vec![p.clone(), p.clone()]), p);
    }

    #[test]
    fn commutative() {
        let p = a("p");
        let q = a("q");
        assert_eq!(
            and(vec![p.clone(), q.clone()]),
            and(vec![q.clone(), p.clone()])
        );
        assert_eq!(or(vec![p.clone(), q.clone()]), or(vec![q, p]));
    }

    #[test]
    fn flatten_and() {
        let p = a("p");
        let q = a("q");
        let r = a("r");
        let lhs = and(vec![and(vec![p.clone(), q.clone()]), r.clone()]);
        let rhs = and(vec![p, q, r]);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn identity_constants() {
        let p = a("p");
        assert_eq!(and(vec![p.clone(), true_()]), p);
        assert_eq!(and(vec![p.clone(), false_()]), false_());
        assert_eq!(or(vec![p.clone(), false_()]), p);
        assert_eq!(or(vec![p, true_()]), true_());
    }

    #[test]
    fn nnf_full() {
        let p = a("p");
        let q = a("q");
        let r = a("r");
        // !(p && (q || r)) = !p || (!q && !r)
        let lhs = not(and(vec![p.clone(), or(vec![q.clone(), r.clone()])]));
        let rhs = or(vec![not(p), and(vec![not(q), not(r)])]);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn cond_bridge_roundtrip() {
        let n = NodeIndex::new(42);
        let sym = cond_var_name(n);
        assert_eq!(cond_block_from_name(sym), Some(n));
    }

    #[test]
    fn cond_atoms_filters_non_convention() {
        let cond = and(vec![cond_atom(3), a("__staleness")]);
        let blocks = cond.cond_atoms();
        assert_eq!(blocks.len(), 1);
        assert!(blocks.contains(&NodeIndex::new(3)));
    }
}
