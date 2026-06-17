// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{always_terminates, negate},
    },
};

// Three local cleanups on `IfElse`:
//
// 1. If the then-arm `always_terminates` (abort/return/break/continue, or a Seq/IfElse
//    that recursively does), the else-arm is unreachable as fall-through from inside the
//    if — it only runs when the test was false. Hoist it out as a sibling after the if:
//      `if (t) { terminator } else alt` → `if (t) { terminator }; alt`.
//
// 2. Symmetric: if the else-arm always_terminates and the then-arm is non-empty, negate
//    the test, swap the arms, and hoist the (now-conseq's old-then) body out:
//      `if (t) { rest } else { terminator }` → `if (!t) { terminator }; rest`.
//    The negation keeps the rewrite in the early-exit idiom (`if (!t) abort; rest`)
//    rather than the equivalent `if (t) { rest }; (else-terminator-unreachable)` shape.
//
// 3. An empty else carries no information — drop it: `if (t) c else {}` → `if (t) c`.
//
// Precedence between rules 1 and 2 when both could fire (both arms always_terminate):
// prefer the rule that keeps an `Abort` in the conditional. When `else` is an `Abort` and
// `then` isn't, rule 2 wins — `recover_asserts` can then fold the result into
// `assert!(cond, code)`. Otherwise rule 1 wins (which also keeps abort-in-then where the
// then-arm is the abort). For both-non-abort terminators we just go with rule 1.
//
// Run before `recover_asserts` so the rewritten empty-arm shape with a single Abort
// terminator can fold to `assert!(cond, code)`.

pub fn refine(exp: &mut Exp) -> bool {
    SimplifyIf.refine(exp)
}

struct SimplifyIf;

impl Refine for SimplifyIf {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::IfElse(cond, then_b, else_b) = exp else {
            return false;
        };

        // Abort-preference for the rules-1-and-2 tiebreak: when else is a singleton Abort
        // and then isn't, prefer rule 2 so the abort stays in the conditional.
        let abort_in_else = else_b.as_ref().as_ref().is_some_and(is_singleton_abort);
        let abort_in_then = is_singleton_abort(then_b);
        let prefer_rule_2 = abort_in_else && !abort_in_then;

        // Rule 1 (skipped when abort-preference says rule 2 wins).
        if !prefer_rule_2
            && always_terminates(then_b)
            && let Some(alt) = else_b.as_ref().as_ref()
            && !is_empty(alt)
        {
            let if_only = Exp::IfElse(
                Box::new(cond.as_ref().clone()),
                Box::new(then_b.as_ref().clone()),
                Box::new(None),
            );
            *exp = with_rest(if_only, alt.clone());
            return true;
        }

        // Rule 2.
        if let Some(alt) = else_b.as_ref().as_ref()
            && always_terminates(alt)
            && !is_empty(then_b)
        {
            let mut neg = cond.as_ref().clone();
            negate(&mut neg);
            let if_only = Exp::IfElse(Box::new(neg), Box::new(alt.clone()), Box::new(None));
            *exp = with_rest(if_only, then_b.as_ref().clone());
            return true;
        }

        // Rule 3.
        if let Some(alt) = else_b.as_ref().as_ref()
            && is_empty(alt)
        {
            **else_b = None;
            return true;
        }

        false
    }
}

// ------------------------------------------------------------------------------------------------
// Helpers

/// `Abort(_)` directly, or wrapped in a singleton `Seq`. Used by the abort-preference
/// tiebreak between rules 1 and 2.
fn is_singleton_abort(exp: &Exp) -> bool {
    match exp {
        Exp::Abort(_) => true,
        Exp::Seq(items) if items.len() == 1 => is_singleton_abort(&items[0]),
        _ => false,
    }
}

/// Build `Seq[if_exp, ...rest]`, flattening if `rest` is already a `Seq` so we don't nest
/// pointlessly. An empty `rest` collapses to just `if_exp` — but the rules above only call
/// here with `!is_empty(rest)`, so this branch exists for safety.
fn with_rest(if_exp: Exp, rest: Exp) -> Exp {
    if is_empty(&rest) {
        return if_exp;
    }
    let mut items = Vec::with_capacity(2);
    items.push(if_exp);
    match rest {
        Exp::Seq(rs) => items.extend(rs),
        other => items.push(other),
    }
    Exp::Seq(items)
}

fn is_empty(exp: &Exp) -> bool {
    matches!(exp, Exp::Seq(items) if items.is_empty())
}
