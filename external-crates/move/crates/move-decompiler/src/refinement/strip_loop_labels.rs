// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast::{Exp, Label},
    refinement::Refine,
};

pub fn refine(exp: &mut Exp) -> bool {
    StripLoopLabels.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement
//
// Structuring emits every loop and every break/continue with its target loop's label. Most of
// those labels aren't needed in the output — bare `break;` / `continue;` already means
// "innermost enclosing loop", so the label is only required when a labeled use lives inside a
// nested loop (where the unlabeled form would target the inner loop instead).
//
// For each labeled `Loop`/`While`, we ask: does the body contain a `Break(Some(L))` /
// `Continue(Some(L))` strictly inside a nested loop? If yes → keep the label. If no → strip:
// the loop loses its label AND every matching break/continue at this scope demotes to
// unlabeled, in lockstep. (Doing one without the other would be a bug.)

struct StripLoopLabels;

impl Refine for StripLoopLabels {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let (slot, body) = match exp {
            Exp::Loop(slot, body) | Exp::While(slot, _, body) => (slot, body),
            _ => return false,
        };
        let Some(target) = *slot else { return false };
        if used_inside_nested_loop(body, target) {
            return false;
        }
        Demote { target }.refine(body);
        *slot = None;
        true
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers

/// True iff `exp` contains a `Break(Some(target))` / `Continue(Some(target))` that sits strictly
/// inside a nested loop. Uses at the surrounding scope are fine — an unlabeled break/continue
/// there already targets the enclosing labeled loop. Uses inside a nested loop need the label
/// because an unlabeled break/continue there targets the nested loop instead.
fn used_inside_nested_loop(exp: &Exp, target: Label) -> bool {
    fn go(exp: &Exp, target: Label, in_nested: bool) -> bool {
        match exp {
            Exp::Break(Some(l)) | Exp::Continue(Some(l)) => in_nested && *l == target,
            // Anything inside a nested loop (including its `while` condition) is "in nested".
            Exp::Loop(_, body) => go(body, target, true),
            Exp::While(_, cond, body) => go(cond, target, true) || go(body, target, true),
            Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
                items.iter().any(|i| go(i, target, in_nested))
            }
            Exp::IfElse(cond, conseq, alt) => {
                go(cond, target, in_nested)
                    || go(conseq, target, in_nested)
                    || alt
                        .as_ref()
                        .as_ref()
                        .is_some_and(|a| go(a, target, in_nested))
            }
            Exp::Switch(cond, _, arms) => {
                go(cond, target, in_nested) || arms.iter().any(|(_, a)| go(a, target, in_nested))
            }
            Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
                args.iter().any(|a| go(a, target, in_nested))
            }
            Exp::Assign(_, e)
            | Exp::LetBind(_, e)
            | Exp::Abort(e)
            | Exp::Borrow(_, e)
            | Exp::Unpack(_, _, e)
            | Exp::UnpackVariant(_, _, _, e)
            | Exp::VecUnpack(_, e) => go(e, target, in_nested),
            Exp::Break(None)
            | Exp::Continue(None)
            | Exp::Value(_)
            | Exp::Variable(_)
            | Exp::Constant(_)
            | Exp::Unstructured(_) => false,
        }
    }
    go(exp, target, false)
}

/// Refine helper that demotes every matching break/continue at the current scope to unlabeled
/// form, leaving nested loops untouched (their internal uses of `target` must keep the label —
/// an unlabeled break/continue at that depth would target the nested loop, not us).
struct Demote {
    target: Label,
}

impl Refine for Demote {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        match exp {
            Exp::Break(l) | Exp::Continue(l) if *l == Some(self.target) => {
                *l = None;
                // Safe to short-circuit: Break/Continue have no children.
                true
            }
            // Short-circuit at nested loops so the framework doesn't descend into them.
            Exp::Loop(_, _) | Exp::While(_, _, _) => true,
            _ => false,
        }
    }
}
