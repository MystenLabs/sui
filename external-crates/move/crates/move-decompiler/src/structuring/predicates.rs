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

use std::collections::{BTreeSet, HashMap, HashSet};

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

/// Prefix for synthetic variant-test atoms. A `Variants(switch, code, _, [(v_1, t_1), ...])`
/// edge `switch -> t_k` fires when `subject == v_k`; we model that as an atom named
/// `__match{code}_{variant}` and treat distinct `k` values as mutually exclusive at emission
/// (not within the Formula algebra - the smart constructors don't model mutex).
const MATCH_PREFIX: &str = "__match";

/// The synthetic atom name for a Variants edge: `__match{code}_{variant}`. Centralized
/// here so the structurer and the switch-recovery passes agree on the convention.
pub fn match_atom_name(code: u64, variant: &str) -> Symbol {
    Symbol::from(format!("{MATCH_PREFIX}{code}_{variant}"))
}

/// `Atom` for the `code`'s edge taken on variant `variant`.
pub fn match_atom(code: u64, variant: &str) -> Formula {
    atom(match_atom_name(code, variant))
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
    // Distributive factoring: `(A || X) && (A || Y) -> A || (X && Y)`. Find disjuncts that
    // appear at the top level of every conjunct in `out`, strip them, and re-wrap as an Or
    // over (common, And(stripped)). Each recursion strictly shrinks the tree (we remove at
    // least one disjunct from each conjunct), so this terminates.
    if out.len() >= 2 {
        let common = common_disjuncts(&out);
        if !common.is_empty() {
            let stripped: Vec<Formula> = out
                .into_iter()
                .map(|f| strip_disjuncts(f, &common))
                .collect();
            let inner = and(stripped);
            let mut wrap: Vec<Formula> = common.into_iter().collect();
            wrap.push(inner);
            return or(wrap);
        }
    }
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
    // Distributive factoring: `(A && X) || (A && Y) -> A && (X || Y)`. Find conjuncts that
    // appear at the top level of every disjunct in `out`, strip them, and re-wrap as an And
    // over (common, Or(stripped)). Each recursion strictly shrinks the tree, so this
    // terminates.
    if out.len() >= 2 {
        let common = common_conjuncts(&out);
        if !common.is_empty() {
            let stripped: Vec<Formula> = out
                .into_iter()
                .map(|f| strip_conjuncts(f, &common))
                .collect();
            let inner = or(stripped);
            let mut wrap: Vec<Formula> = common.into_iter().collect();
            wrap.push(inner);
            return and(wrap);
        }
    }
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
    // `xs` is sorted+deduped before we get here, so any inner conjunct that equals an
    // outer disjunct must equal some `xs[j]` with `j != i` (a sibling). We can replace
    // the O(N^2 * K) double-scan with a single BTreeSet lookup per conjunct.
    let xs_set: BTreeSet<&Formula> = xs.iter().collect();
    let drop: Vec<bool> = xs
        .iter()
        .map(|f| match &f.0 {
            FormulaTree::And(conjuncts) => conjuncts.iter().any(|c| xs_set.contains(c)),
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
    let xs_set: BTreeSet<&Formula> = xs.iter().collect();
    let drop: Vec<bool> = xs
        .iter()
        .map(|f| match &f.0 {
            FormulaTree::Or(disjuncts) => disjuncts.iter().any(|d| xs_set.contains(d)),
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

/// View `f` as a set of top-level conjuncts: `And(fs)` -> its children, anything else -> `{f}`.
fn top_conjuncts(f: &Formula) -> BTreeSet<Formula> {
    match &f.0 {
        FormulaTree::And(fs) => fs.iter().cloned().collect(),
        _ => std::iter::once(f.clone()).collect(),
    }
}

/// View `f` as a set of top-level disjuncts: `Or(fs)` -> its children, anything else -> `{f}`.
fn top_disjuncts(f: &Formula) -> BTreeSet<Formula> {
    match &f.0 {
        FormulaTree::Or(fs) => fs.iter().cloned().collect(),
        _ => std::iter::once(f.clone()).collect(),
    }
}

/// Conjuncts that appear at the top level of every disjunct in `disjuncts`.
fn common_conjuncts(disjuncts: &[Formula]) -> BTreeSet<Formula> {
    let Some((first, rest)) = disjuncts.split_first() else {
        return BTreeSet::new();
    };
    let mut common = top_conjuncts(first);
    for d in rest {
        common = common.intersection(&top_conjuncts(d)).cloned().collect();
        if common.is_empty() {
            return BTreeSet::new();
        }
    }
    common
}

/// Disjuncts that appear at the top level of every conjunct in `conjuncts`.
fn common_disjuncts(conjuncts: &[Formula]) -> BTreeSet<Formula> {
    let Some((first, rest)) = conjuncts.split_first() else {
        return BTreeSet::new();
    };
    let mut common = top_disjuncts(first);
    for c in rest {
        common = common.intersection(&top_disjuncts(c)).cloned().collect();
        if common.is_empty() {
            return BTreeSet::new();
        }
    }
    common
}

/// Remove every member of `common` from `f`'s top-level conjuncts. An `And(fs)` becomes
/// `And(fs \ common)` via the smart constructor (which collapses to `True` when empty); a
/// bare formula that's in `common` becomes `True`.
fn strip_conjuncts(f: Formula, common: &BTreeSet<Formula>) -> Formula {
    match f.0 {
        FormulaTree::And(fs) => {
            let remaining: Vec<Formula> = fs.into_iter().filter(|x| !common.contains(x)).collect();
            and(remaining)
        }
        _ => {
            if common.contains(&f) {
                true_()
            } else {
                f
            }
        }
    }
}

/// Remove every member of `common` from `f`'s top-level disjuncts. Dual of
/// [`strip_conjuncts`]; a bare formula in `common` becomes `False`.
fn strip_disjuncts(f: Formula, common: &BTreeSet<Formula>) -> Formula {
    match f.0 {
        FormulaTree::Or(fs) => {
            let remaining: Vec<Formula> = fs.into_iter().filter(|x| !common.contains(x)).collect();
            or(remaining)
        }
        _ => {
            if common.contains(&f) {
                false_()
            } else {
                f
            }
        }
    }
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

    /// The top-level conjuncts of `self`. `And(fs)` returns clones of `fs`; anything else
    /// returns `[self]`. Used by NMG's condition-based refinement to find common factors
    /// across sibling guards.
    pub fn conjuncts(&self) -> Vec<Formula> {
        match &self.0 {
            FormulaTree::And(fs) => fs.clone(),
            _ => vec![self.clone()],
        }
    }

    /// True iff `self` is semantically equivalent to a non-empty disjunction of atoms
    /// drawn from `allowed`. Used by switch recovery to recognize "this guard is reached
    /// iff we took one of these variant arms"; generalizes to any disjunction-over-atoms
    /// test. We build `target = OR of (self's atoms ∩ allowed)` and check `self <-> target`
    /// via [`Formula::classify`] - if the XOR is a contradiction the two sides are
    /// equivalent.
    pub fn is_disjunction_of_atoms(&self, allowed: &HashSet<Symbol>) -> bool {
        let self_atoms = self.atoms();
        if self_atoms.is_empty() {
            return false;
        }
        let allowed_atoms: Vec<Formula> = self_atoms
            .iter()
            .filter(|a| allowed.contains(*a))
            .map(|a| atom(*a))
            .collect();
        if allowed_atoms.is_empty() {
            return false;
        }
        let target = or(allowed_atoms);
        let xor = or(vec![
            and(vec![self.clone(), not(target.clone())]),
            and(vec![not(self.clone()), target]),
        ]);
        matches!(xor.classify(), Some(false))
    }

    /// True iff `&&(assumptions)` implies `self`. Verbatim-match shortcut, then an
    /// atom-overlap prefilter to keep the BDD input small. Used by the terminator-implication
    /// pass to recognize when a sibling's guard is already forced by accumulated assumptions
    /// (so its `CondIf` wrapper can drop).
    pub fn implied_by(&self, assumptions: &[Formula]) -> bool {
        if *self == true_() {
            return true;
        }
        if assumptions.is_empty() {
            return false;
        }
        if assumptions.iter().any(|a| a == self) {
            return true;
        }
        let self_atoms = self.atoms();
        let mut conj: Vec<Formula> = assumptions
            .iter()
            .filter(|a| !a.atoms().is_disjoint(&self_atoms))
            .cloned()
            .collect();
        if conj.is_empty() {
            return false;
        }
        conj.push(not(self.clone()));
        matches!(and(conj).classify(), Some(false))
    }

    /// True iff `factor` can be pulled out of `self`. Mirrors how distribution works:
    ///   - `self == factor` -> trivially.
    ///   - `And(fs)` -> any conjunct (recursively) has the factor; the others survive.
    ///   - `Or(fs)` -> every disjunct (recursively) has the factor.
    ///   - otherwise -> false.
    ///
    /// The recursion is what makes nested-DNF guards factorable. A guard like
    /// `And(Or(And(X, c24, c27), And(X, !c24)), __c38)` should still have X as a
    /// factor because the inner `Or` always carries X.
    ///
    /// Note: this is structurally sufficient, not boolean-complete - cases like
    /// `(X || Y) && (X || !Y) => X` aren't caught (neither conjunct alone has X), but
    /// they don't arise in practice from reaching-condition guards.
    pub fn has_factor(&self, factor: &Formula) -> bool {
        if self == factor {
            return true;
        }
        match &self.0 {
            FormulaTree::And(fs) => fs.iter().any(|f| f.has_factor(factor)),
            FormulaTree::Or(fs) => fs.iter().all(|f| f.has_factor(factor)),
            _ => false,
        }
    }

    /// Strip `factor` wherever it appears as a factor inside `self`. Mirror of
    /// [`has_factor`]: recurses through `And`/`Or` so a nested `And(Or(.., X), Y)`
    /// loses its X while keeping the surrounding structure intact. Caller has
    /// verified `has_factor(factor)`.
    pub fn without_factor(&self, factor: &Formula) -> Formula {
        if self == factor {
            return true_();
        }
        match &self.0 {
            FormulaTree::And(fs) => and(fs.iter().map(|f| f.without_factor(factor)).collect()),
            FormulaTree::Or(fs) => or(fs.iter().map(|d| d.without_factor(factor)).collect()),
            _ => self.clone(),
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

// -------------------------------------------------------------------------------------------------
// BDD-based tautology/contradiction detection
// -------------------------------------------------------------------------------------------------
//
// A minimal Reduced Ordered Binary Decision Diagram (ROBDD). Each non-terminal node is a
// triple `(var, low, high)` where `low` is the BDD when `var=false` and `high` when
// `var=true`. Two terminal nodes (FALSE_ID, TRUE_ID) sit at the leaves. The structure is:
//
//   - Reduced: `mk(var, low, high)` returns the existing `NodeId` whenever
//     `(var, low, high)` has been seen, so structurally-equal sub-BDDs are shared. If
//     `low == high`, `var` is irrelevant and `mk` returns `low` directly.
//   - Ordered: a fixed variable ordering means any path from the root has strictly
//     increasing variable indices.
//
// Given these two properties, `f` is a tautology iff its root is `TRUE_ID`, a
// contradiction iff `FALSE_ID`. Everything else means the formula is contingent.
//
// The `apply` operation (And/Or/Not) is O(|f| * |g|) with memoization, where |f| is the
// number of unique BDD nodes - typically much smaller than 2^n for the formulas this
// layer produces (reach-condition disjunctions over CFG paths). Polynomial in BDD size,
// which makes this the single canonicalizer the rest of the file relies on for
// `Formula::classify`, `implied_by`, and `is_disjunction_of_atoms`.
mod bdd {
    use super::{Formula, FormulaTree};
    use move_symbol_pool::Symbol;
    use std::collections::HashMap;

    pub type NodeId = u32;
    pub const FALSE_ID: NodeId = 0;
    pub const TRUE_ID: NodeId = 1;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct Node {
        var: u32,
        low: NodeId,
        high: NodeId,
    }

    pub struct Bdd {
        nodes: Vec<Node>,
        unique: HashMap<Node, NodeId>,
        apply_cache: HashMap<(u8, NodeId, NodeId), NodeId>,
        not_cache: HashMap<NodeId, NodeId>,
    }

    const OP_AND: u8 = 0;
    const OP_OR: u8 = 1;

    impl Bdd {
        fn new() -> Self {
            let f = Node {
                var: u32::MAX,
                low: 0,
                high: 0,
            };
            let t = Node {
                var: u32::MAX,
                low: 1,
                high: 1,
            };
            Self {
                nodes: vec![f, t],
                unique: HashMap::new(),
                apply_cache: HashMap::new(),
                not_cache: HashMap::new(),
            }
        }

        fn mk(&mut self, var: u32, low: NodeId, high: NodeId) -> NodeId {
            if low == high {
                return low;
            }
            let key = Node { var, low, high };
            if let Some(&id) = self.unique.get(&key) {
                return id;
            }
            let id = self.nodes.len() as NodeId;
            self.nodes.push(key);
            self.unique.insert(key, id);
            id
        }

        fn apply(&mut self, op: u8, a: NodeId, b: NodeId) -> NodeId {
            // Terminal shortcuts.
            match op {
                OP_AND => {
                    if a == FALSE_ID || b == FALSE_ID {
                        return FALSE_ID;
                    }
                    if a == TRUE_ID {
                        return b;
                    }
                    if b == TRUE_ID {
                        return a;
                    }
                }
                OP_OR => {
                    if a == TRUE_ID || b == TRUE_ID {
                        return TRUE_ID;
                    }
                    if a == FALSE_ID {
                        return b;
                    }
                    if b == FALSE_ID {
                        return a;
                    }
                }
                _ => unreachable!(),
            }
            if a == b {
                return a;
            }
            // Canonical key (op is commutative).
            let (lo_arg, hi_arg) = if a < b { (a, b) } else { (b, a) };
            let key = (op, lo_arg, hi_arg);
            if let Some(&r) = self.apply_cache.get(&key) {
                return r;
            }
            let va = self.nodes[a as usize].var;
            let vb = self.nodes[b as usize].var;
            let v = va.min(vb);
            let (a_lo, a_hi) = if va == v {
                (self.nodes[a as usize].low, self.nodes[a as usize].high)
            } else {
                (a, a)
            };
            let (b_lo, b_hi) = if vb == v {
                (self.nodes[b as usize].low, self.nodes[b as usize].high)
            } else {
                (b, b)
            };
            let lo = self.apply(op, a_lo, b_lo);
            let hi = self.apply(op, a_hi, b_hi);
            let r = self.mk(v, lo, hi);
            self.apply_cache.insert(key, r);
            r
        }

        fn not(&mut self, a: NodeId) -> NodeId {
            if a == FALSE_ID {
                return TRUE_ID;
            }
            if a == TRUE_ID {
                return FALSE_ID;
            }
            if let Some(&r) = self.not_cache.get(&a) {
                return r;
            }
            let v = self.nodes[a as usize].var;
            let l = self.nodes[a as usize].low;
            let h = self.nodes[a as usize].high;
            let nl = self.not(l);
            let nh = self.not(h);
            let r = self.mk(v, nl, nh);
            self.not_cache.insert(a, r);
            r
        }

        fn build(&mut self, f: &Formula, var_of: &HashMap<Symbol, u32>) -> NodeId {
            match &f.0 {
                FormulaTree::True => TRUE_ID,
                FormulaTree::False => FALSE_ID,
                FormulaTree::Atom(s) => {
                    let v = var_of[s];
                    self.mk(v, FALSE_ID, TRUE_ID)
                }
                FormulaTree::Not(inner) => {
                    let id = self.build(inner, var_of);
                    self.not(id)
                }
                FormulaTree::And(fs) => {
                    let mut acc = TRUE_ID;
                    for f in fs {
                        let id = self.build(f, var_of);
                        acc = self.apply(OP_AND, acc, id);
                        if acc == FALSE_ID {
                            return FALSE_ID;
                        }
                    }
                    acc
                }
                FormulaTree::Or(fs) => {
                    let mut acc = FALSE_ID;
                    for f in fs {
                        let id = self.build(f, var_of);
                        acc = self.apply(OP_OR, acc, id);
                        if acc == TRUE_ID {
                            return TRUE_ID;
                        }
                    }
                    acc
                }
            }
        }
    }

    /// `Some(true)` if `f` is a tautology, `Some(false)` if a contradiction, `None` if
    /// neither. Variables are ordered by `Symbol::Ord`; the order affects BDD *size* but
    /// not correctness or the tautology answer.
    pub fn classify(f: &Formula) -> Option<bool> {
        let atoms = f.atoms();
        if atoms.is_empty() {
            return match &f.0 {
                FormulaTree::True => Some(true),
                FormulaTree::False => Some(false),
                _ => None,
            };
        }
        let var_of: HashMap<Symbol, u32> = atoms
            .into_iter()
            .enumerate()
            .map(|(i, s)| (s, i as u32))
            .collect();
        let mut bdd = Bdd::new();
        let root = bdd.build(f, &var_of);
        if root == TRUE_ID {
            Some(true)
        } else if root == FALSE_ID {
            Some(false)
        } else {
            None
        }
    }
}

impl Formula {
    /// `Some(true)` if `self` is a boolean tautology, `Some(false)` if a contradiction,
    /// `None` otherwise. Backed by a ROBDD (see the `bdd` module); cheap enough to call
    /// on every emitted guard at any atom count.
    pub fn classify(&self) -> Option<bool> {
        bdd::classify(self)
    }
}

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
