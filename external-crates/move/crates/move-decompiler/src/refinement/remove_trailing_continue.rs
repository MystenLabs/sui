// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Drop a `Continue(loop_label)` reachable from a loop body's tail. Walks the last item of
// a `Seq`, both arms of an `IfElse`, every case of a `Switch`/`Match`, and through `Block`
// wrappers — anywhere the implicit fall-through at the loop's true tail is itself
// iteration:
// `loop { …; if (t) { …; continue } else { …; continue } }` => same loop without the continues
//
// Preconditions:
//   - The `Continue`'s label matches the enclosing loop's (label equality).
//   - The `IfElse`-with-`then;continue`-and-`else=Break` shape is deferred to
//     `swap_continue_break_else`, which produces a guard form we prefer.
//   - We don't descend through nested `Loop`/`While`; their continues target themselves.

use crate::{
    ast::{Exp, Label},
    refinement::{
        Refine,
        utils::{peek, peek_mut},
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    let r1 = LoopRemoveTrailingContinue.refine(exp);
    let r2 = WhileRemoveTrailingContinue.refine(exp);
    r1 || r2
}

struct LoopRemoveTrailingContinue;

impl Refine for LoopRemoveTrailingContinue {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;
        elide_tail_continue(peek_mut(body), loop_label)
    }
}

struct WhileRemoveTrailingContinue;

impl Refine for WhileRemoveTrailingContinue {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::While(loop_label, _, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;
        elide_tail_continue(peek_mut(body), loop_label)
    }
}

/// Recursively elide `Continue(label)` from any tail position of `exp`. The recursion
/// descends through tail-position containers (`Seq` last item, both arms of `IfElse`, every
/// `Switch`/`Match` arm, transparent `Block` wrapper) and stops at nested `Loop`/`While`.
fn elide_tail_continue(exp: &mut Exp, label: Option<Label>) -> bool {
    match exp {
        Exp::Continue(l) if *l == label => {
            *exp = Exp::Seq(vec![]);
            true
        }
        Exp::Seq(seq) => seq
            .last_mut()
            .map(|last| elide_tail_continue(peek_mut(last), label))
            .unwrap_or(false),
        Exp::IfElse(_, then_b, else_b) => {
            // Defer the `then;continue / else=Break` shape to `swap_continue_break_else`.
            let then_has_cont = matches!(
                tail_continue_label(peek(then_b)),
                Some(l) if l == label
            );
            let else_is_break = matches!(
                else_b.as_ref().as_ref().map(peek),
                Some(Exp::Break(b)) if *b == label
            );
            if then_has_cont && else_is_break {
                return false;
            }
            let r1 = elide_tail_continue(peek_mut(then_b), label);
            let r2 = else_b
                .as_mut()
                .as_mut()
                .map(|e| elide_tail_continue(peek_mut(e), label))
                .unwrap_or(false);
            // Collapse an emptied else to `None` so the renderer doesn't carry an
            // `else { }` shell.
            if matches!(else_b.as_ref().as_ref(), Some(Exp::Seq(items)) if items.is_empty()) {
                **else_b = None;
            }
            r1 || r2
        }
        Exp::Switch(_, _, cases) => {
            let mut changed = false;
            for (_, body) in cases.iter_mut() {
                changed |= elide_tail_continue(peek_mut(body), label);
            }
            changed
        }
        Exp::Match(_, _, arms) => {
            let mut changed = false;
            for (_, _, body) in arms.iter_mut() {
                changed |= elide_tail_continue(peek_mut(body), label);
            }
            changed
        }
        Exp::Block(_, body) => elide_tail_continue(body, label),
        _ => false,
    }
}

/// Final-position `Continue` label, if any. Walks through `Seq.last` and `Block` only —
/// doesn't descend into `IfElse`/`Switch` (those have multiple arms; an "ends in continue"
/// answer at that level is per-arm).
fn tail_continue_label(exp: &Exp) -> Option<Option<Label>> {
    match exp {
        Exp::Continue(l) => Some(*l),
        Exp::Seq(items) => items.last().and_then(tail_continue_label),
        Exp::Block(_, body) => tail_continue_label(body),
        _ => None,
    }
}
