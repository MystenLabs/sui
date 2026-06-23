// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Elide if-wrappers whose guards are tautologies given previous `assert!`s.
//
// NMG computes reaching conditions over the acyclic projection; a "post-assert join" block has a
// guard of the form `X || !G` where the previous sibling is `if (G) { ...; assert!(X) }`. The
// formula is mathematically correct (the OR-of-paths into the join), but at the source level
// it's vacuous: by Move semantics, reaching the join means either the assert wasn't entered
// (`!G`) or it was entered and passed (`X`), and either way the body of the wrapping `if` would
// run. Worse, the formula references `X = __cN` whose definite-assignment scope is just the
// arm where `__cN = test` lives - keeping the wrapper forces us to either manifest `__cN` as a
// let-bound outer local (broken) or rely on short-circuit gymnastics in `Or`'s operand order.
//
// This pass tracks the assertions encountered while walking a `Seq` left-to-right and converts
// each item's guard (when its shape is `Variable(__cN)` and boolean primitives over them) into
// a `Formula`. If the accumulated assumptions imply the guard via the existing predicate
// algebra (`and(assumptions, !guard).simplify() == false_`), the `IfElse` wrapper is replaced
// by its then-arm body. After elision the synthetic `__cN` typically becomes single-use and
// `collapse_let_usage` inlines it - the synthetic local disappears entirely.
//
// Conservative on shape and scope:
//   - Only fires for `IfElse(cond, body, None)` with no else - matches NMG's post-join shape.
//   - The cond must convert to a `Formula` via [`exp_to_formula`] (variables + `Not`/`And`/`Or`
//     only). Anything else - a comparison, a function call, a borrow - leaves the wrapper.
//   - Assumptions accumulate per `Seq` and don't escape `Loop`/`While` (the body may not run).

use crate::{
    ast::{Exp, ModuleRef},
    refinement::{Refine, utils::always_terminates},
    structuring::predicates::{self, Formula},
};
use move_stackless_bytecode_2::ast::PrimitiveOp;
use move_symbol_pool::Symbol;

pub fn refine(exp: &mut Exp) -> bool {
    ElidePostAssertGuards.refine(exp)
}

struct ElidePostAssertGuards;

impl Refine for ElidePostAssertGuards {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };
        elide_in_seq(items)
    }
}

// -------------------------------------------------------------------------------------------------
// Driver

fn elide_in_seq(items: &mut Vec<Exp>) -> bool {
    let mut changed = false;
    let mut assumptions: Vec<Formula> = Vec::new();
    let mut i = 0;
    while i < items.len() {
        // Try to elide an `IfElse(cond, body, None)` wrapper whose `cond` is implied by
        // accumulated assumptions.
        if let Exp::IfElse(cond, then_b, else_b) = &items[i]
            && else_b.as_ref().as_ref().is_none()
            && let Some(g) = exp_to_formula(cond)
            && assumptions_imply(&assumptions, &g)
        {
            let body = (**then_b).clone();
            items[i] = body;
            changed = true;
            // Don't advance i - the new item at i may itself be an elidable IfElse, or its
            // body may contribute more assumptions before we look at i+1.
            continue;
        }
        // Collect any post-assertions this item adds.
        collect_assumptions(&items[i], &Vec::new(), &mut assumptions);
        i += 1;
    }
    changed
}

// -------------------------------------------------------------------------------------------------
// Exp <-> Formula bridge

/// Convert `exp` to a `Formula` if it's the shape `Formula::to_exp` produces: `Variable` for
/// atoms, `Value` for boolean constants, `Primitive` for `Not`/`And`/`Or`. Anything else
/// (comparisons, function calls, borrows) returns `None`.
fn exp_to_formula(exp: &Exp) -> Option<Formula> {
    use move_core_types::runtime_value::MoveValue;
    match exp {
        Exp::Variable(n) => Some(predicates::atom(Symbol::from(n.as_str()))),
        Exp::Value(MoveValue::Bool(b)) => Some(if *b {
            predicates::true_()
        } else {
            predicates::false_()
        }),
        Exp::Primitive { op, args } => match (op, args.as_slice()) {
            (PrimitiveOp::Not, [inner]) => Some(predicates::not(exp_to_formula(inner)?)),
            (PrimitiveOp::And, [a, b]) => Some(predicates::and(vec![
                exp_to_formula(a)?,
                exp_to_formula(b)?,
            ])),
            (PrimitiveOp::Or, [a, b]) => Some(predicates::or(vec![
                exp_to_formula(a)?,
                exp_to_formula(b)?,
            ])),
            _ => None,
        },
        _ => None,
    }
}

/// True iff the conjunction of `assumptions` implies `guard`. Tries a cheap structural
/// shortcut first: if `guard` is `true_` or appears verbatim in `assumptions`, we're done
/// without invoking the QM simplifier. Otherwise fall back to
/// `simplify(and(assumptions, !guard)) == false_`.
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
    let mut conj: Vec<Formula> = assumptions.to_vec();
    conj.push(predicates::not(guard.clone()));
    predicates::and(conj).simplify() == predicates::false_()
}

// -------------------------------------------------------------------------------------------------
// Assumption accumulation

/// Walk `exp` and append assumptions derived from the asserts (and early-exit branches) it
/// executes to `out`. `guard_stack` is the conjunction of enclosing `IfElse` conds that must
/// hold for `exp` to run; an assert of `X` inside `if (G1) { if (G2) { ...; assert!(X); ... } }`
/// yields the assumption `(G1 ∧ G2) → X`.
///
/// "Early-exit" assumptions: an item `if (cond_t) { terminator }` within a `Seq` means later
/// items in the same `Seq` only run when `!cond_t`. Same idea as the `assert!` post-cond -
/// `assert!(X)` is just `if (!X) abort`, and `continue`/`break`/`return` kill the iteration
/// the same way `abort` kills the function. We collect `!cond_t` for the outer-scope post-cond.
fn collect_assumptions(exp: &Exp, guard_stack: &[Formula], out: &mut Vec<Formula>) {
    /// Wrap `local` with `guard_stack → local` for the outer scope.
    fn lift(local: Formula, guard_stack: &[Formula]) -> Formula {
        if guard_stack.is_empty() {
            return local;
        }
        let guard_conj = predicates::and(guard_stack.to_vec());
        predicates::or(vec![predicates::not(guard_conj), local])
    }
    match exp {
        Exp::Call((ModuleRef::Builtin, name), args)
            if name.as_str() == "assert!" && args.len() == 2 =>
        {
            if let Some(cond_f) = exp_to_formula(&args[0]) {
                out.push(lift(cond_f, guard_stack));
            }
        }
        Exp::Seq(items) => {
            // Walk left-to-right. Each `if (c_t) { always_terminates }` item makes
            // `!c_t` available to later items in this Seq AND to the outer scope.
            let mut local: Vec<Formula> = Vec::new();
            for item in items {
                // Inner walk: items see prior local assumptions (relative to this Seq)
                // additively appended to guard_stack - each local must hold for control
                // to have reached this item.
                let mut local_stack: Vec<Formula> = guard_stack.to_vec();
                local_stack.extend(local.iter().cloned());
                collect_assumptions(item, &local_stack, out);
                if let Exp::IfElse(cond, body, else_b) = item
                    && else_b.as_ref().as_ref().is_none()
                    && always_terminates(body)
                    && let Some(g) = exp_to_formula(cond)
                {
                    local.push(predicates::not(g));
                }
            }
            // Propagate the locals to the outer scope under the enclosing guard stack.
            for l in local {
                out.push(lift(l, guard_stack));
            }
        }
        Exp::IfElse(cond, then_b, else_b) => {
            if let Some(g) = exp_to_formula(cond) {
                let mut then_stack: Vec<Formula> = guard_stack.to_vec();
                then_stack.push(g.clone());
                collect_assumptions(then_b, &then_stack, out);
                if let Some(alt) = else_b.as_ref().as_ref() {
                    let mut else_stack: Vec<Formula> = guard_stack.to_vec();
                    else_stack.push(predicates::not(g));
                    collect_assumptions(alt, &else_stack, out);
                }
            }
            // If the cond doesn't translate to a Formula, an enclosing block may still find
            // an assert inside, but the guard is unknown - skip rather than under-constrain.
        }
        // Loop / While bodies may not execute, so post-conditions inside them don't survive.
        Exp::Loop(..) | Exp::While(..) => {}
        // Calls, primitives, etc. - no asserts to harvest.
        _ => {}
    }
}
