// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    HoistArmAssignments.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement
//
// Lift a trailing single-target assignment shared by every arm of an IfElse/Switch out into a
// single outer assignment whose RHS is the if-as-expression (or match-as-expression):
//
//     let l5;
//     if (cond) { ... ; l5 = e1; } else { ... ; l5 = e2; };
//
// becomes:
//
//     let l5;
//     l5 = if (cond) { ... ; e1 } else { ... ; e2 };
//
// A later pass can fuse the `let l5;` with the resulting `Assign` to recover the idiomatic
// `let l5 = if (cond) { ... } else { ... };`. Only fires when every arm ends in `Assign([X], _)`
// for the same single `X` — multi-target assignments, missing else, and disagreeing targets are
// left alone.

struct HoistArmAssignments;

impl Refine for HoistArmAssignments {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Some(target) = common_trailing_target(exp) else {
            return false;
        };
        for arm in arms_mut(exp) {
            strip_trailing_assign(arm);
        }
        exp.map_mut(|e| Exp::Assign(vec![target], Box::new(e)));
        true
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers

/// If `exp` is an `IfElse(_, _, Some)` or `Switch` whose arms all end with `Assign([X], _)` for
/// the same `X`, return that `X`.
fn common_trailing_target(exp: &Exp) -> Option<String> {
    let arms: Vec<&Exp> = match exp {
        Exp::IfElse(_, conseq, alt) => vec![conseq, (**alt).as_ref()?],
        Exp::Switch(_, _, cases) => cases.iter().map(|(_, a)| a).collect(),
        _ => return None,
    };
    let (first, rest) = arms.split_first()?;
    let target = trailing_assign_target(first)?;
    rest.iter()
        .all(|a| trailing_assign_target(a) == Some(target))
        .then(|| target.to_owned())
}

/// Mutable references to the arm bodies. Caller has already verified the shape via
/// `common_trailing_target`, so the IfElse alt is guaranteed present.
fn arms_mut(exp: &mut Exp) -> Vec<&mut Exp> {
    match exp {
        Exp::IfElse(_, conseq, alt) => {
            let alt = (**alt).as_mut().expect("checked by common_trailing_target");
            vec![conseq, alt]
        }
        Exp::Switch(_, _, cases) => cases.iter_mut().map(|(_, a)| a).collect(),
        _ => unreachable!("checked by common_trailing_target"),
    }
}

/// `Assign([X], _)` directly, or the trailing entry of a `Seq` that is — returns `X`.
fn trailing_assign_target(exp: &Exp) -> Option<&str> {
    match exp {
        Exp::Assign(names, _) if names.len() == 1 => Some(&names[0]),
        Exp::Seq(items) => items.last().and_then(trailing_assign_target),
        _ => None,
    }
}

/// Replace the trailing `Assign([_], rhs)` (possibly nested in a `Seq`) with just `rhs`, in
/// place, so the arm now evaluates to what the assignment's RHS was.
fn strip_trailing_assign(exp: &mut Exp) {
    match exp {
        Exp::Assign(_, _) => exp.map_mut(|e| match e {
            Exp::Assign(_, rhs) => *rhs,
            _ => unreachable!(),
        }),
        Exp::Seq(items) => {
            if let Some(last) = items.last_mut() {
                strip_trailing_assign(last);
            }
        }
        _ => {}
    }
}
