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
        for i in 0..items.len() {
            if let Some(lift) = take_eligible(&mut items[i]) {
                let Lift {
                    cond,
                    terminator,
                    surviving,
                    target,
                    which,
                } = lift;
                let guard_cond = match which {
                    Which::Then => cond,
                    // Else-terminates: negate so the lifted guard reads as an early-exit on
                    // the original `false` branch.
                    Which::Else => {
                        let mut negated = *cond;
                        negate(&mut negated);
                        Box::new(negated)
                    }
                };
                items[i] = Exp::IfElse(guard_cond, Box::new(terminator), Box::new(None));
                items.insert(i + 1, Exp::LetBind(target, Box::new(surviving)));
                return true;
            }
        }
        false
    }
}

// ------------------------------------------------------------------------------------------------
// Eligibility + ownership transfer

enum Which {
    Then,
    Else,
}

struct Lift {
    cond: Box<Exp>,
    terminator: Exp,
    surviving: Exp,
    target: Vec<String>,
    which: Which,
}

/// Check `item`'s shape on the immutable side; if eligible, consume the `LetBind` and
/// return its pieces. Otherwise leave `item` untouched and return `None`.
fn take_eligible(item: &mut Exp) -> Option<Lift> {
    // Shape check (immutable).
    let Exp::LetBind(targets, rhs) = &*item else {
        return None;
    };
    if targets.len() != 1 {
        return None;
    }
    let Exp::IfElse(_, conseq, alt) = rhs.as_ref() else {
        return None;
    };
    let alt = alt.as_ref().as_ref()?;
    let which = match (always_terminates(conseq), always_terminates(alt)) {
        (true, false) => Which::Then,
        (false, true) => Which::Else,
        _ => return None,
    };

    // Commit: tear the `LetBind(_, IfElse(cond, conseq, Some(alt)))` shape apart. Every
    // `expect` below restates an invariant the immutable check above established.
    let Exp::LetBind(target, rhs) = std::mem::replace(item, Exp::Seq(vec![])) else {
        unreachable!("shape checked above")
    };
    let Exp::IfElse(cond, conseq, alt) = *rhs else {
        unreachable!("shape checked above")
    };
    let alt = (*alt).expect("shape checked above");

    let (terminator, surviving) = match which {
        Which::Then => (*conseq, alt),
        Which::Else => (alt, *conseq),
    };
    Some(Lift {
        cond,
        terminator,
        surviving,
        target,
        which,
    })
}
