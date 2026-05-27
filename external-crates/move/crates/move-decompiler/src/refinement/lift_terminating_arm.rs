// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Hoist a terminating arm out of `let X = if (t) { ... } else { ... };`.
//!
//! When exactly one arm of the `IfElse` always terminates control flow (returns, aborts,
//! breaks, or continues), the surrounding `LetBind` is split into two statements:
//!
//! ```text
//!   let X = if (t) { ...; return e } else { rhs };
//!     ⇒  if (t) { ...; return e }; let X = rhs;
//!
//!   let X = if (t) { rhs } else { ...; abort };
//!     ⇒  if (!t) { ...; abort }; let X = rhs;
//! ```
//!
//! The lifted `if` becomes an unconditional early-exit guard, and the assignment to `X` no
//! longer hides inside a conditional. Downstream passes (`hoist_arm_assignments`,
//! `inline_*`) can then operate on the now-visible `let X = rhs` shape.
//!
//! Soundness: items before the `LetBind` in the surrounding `Seq` run unconditionally (same
//! as before); the lifted `if` either terminates control flow (matching the original arm's
//! exit) or falls through to the `let X = rhs`, which evaluates the surviving arm and
//! continues — matching the original's behavior in that branch. Both-arm-terminate cases
//! are skipped: the assignment to `X` is unreachable, and other refinements handle that.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{always_terminates, negate},
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    LiftTerminatingArm.refine(exp)
}

struct LiftTerminatingArm;

impl Refine for LiftTerminatingArm {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };

        let mut i = 0;
        while i < items.len() {
            if let Some(plan) = analyze(&items[i]) {
                let (cond, terminator, surviving, target) = take(&mut items[i]);
                let if_exp = match plan {
                    Which::Then => Exp::IfElse(cond, Box::new(terminator), Box::new(None)),
                    Which::Else => {
                        let mut negated = *cond;
                        negate(&mut negated);
                        Exp::IfElse(Box::new(negated), Box::new(terminator), Box::new(None))
                    }
                };
                let let_exp = Exp::LetBind(target, Box::new(surviving));
                items[i] = if_exp;
                items.insert(i + 1, let_exp);
                return true;
            }
            i += 1;
        }
        false
    }
}

// ------------------------------------------------------------------------------------------------
// Analysis

enum Which {
    Then,
    Else,
}

fn analyze(item: &Exp) -> Option<Which> {
    let Exp::LetBind(targets, rhs) = item else {
        return None;
    };
    if targets.len() != 1 {
        return None;
    }
    let Exp::IfElse(_, conseq, alt) = rhs.as_ref() else {
        return None;
    };
    let alt = alt.as_ref().as_ref()?;
    match (always_terminates(conseq), always_terminates(alt)) {
        (true, false) => Some(Which::Then),
        (false, true) => Some(Which::Else),
        _ => None,
    }
}

/// Consume `item` (asserted to be `LetBind([X], IfElse(t, conseq, Some(alt)))`) and return
/// `(t, terminating_arm, surviving_arm, target_names)`. The caller's `Which` selects which
/// arm is the terminator.
fn take(item: &mut Exp) -> (Box<Exp>, Exp, Exp, Vec<String>) {
    let Exp::LetBind(targets, rhs) = std::mem::replace(item, Exp::Seq(vec![])) else {
        unreachable!("analyze accepted this item")
    };
    let Exp::IfElse(cond, conseq, alt) = *rhs else {
        unreachable!("analyze accepted this item")
    };
    let alt = match *alt {
        Some(a) => a,
        None => unreachable!("analyze accepted this item"),
    };
    // Caller decides which arm is the terminator; we hand back both, then it picks.
    let (terminator, surviving) = if always_terminates(&conseq) {
        (*conseq, alt)
    } else {
        (alt, *conseq)
    };
    (cond, terminator, surviving, targets)
}
