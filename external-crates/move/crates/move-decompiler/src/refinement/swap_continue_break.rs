// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Swap the tail of a loop body shaped as `[…, IfElse(t, e0;continue, ∅), e1, break]`
// into `[…, IfElse(!t, e1;break, ∅), e0, continue]`. The flipped form puts the `break`
// inside the then-arm — reads more naturally as "exit when the test fails" — and lifts the
// `continue` to a trailing position so `remove_trailing_continue` can drop it. The continue
// is preserved (relocated, never deleted): elision is owned by the one canonical pass.
//
// Preconditions:
//   - The pattern sits at the tail of a `Loop` body (last three items of the body's `Seq`),
//     so dropping the `break` and the subsequent trailing `continue` is semantically
//     equivalent to the original loop iteration.
//   - The `IfElse` has no else-arm (or an empty one) — when both arms continue, that's
//     `hoist_dual_continue`'s territory; mixing the two would either oscillate or change
//     semantics.
//   - The inner `continue` and the trailing `break` target the immediate enclosing loop.
//     Label equality (`*l == loop_label`) handles both the labeled form and the
//     post-`strip_loop_labels` `None == None` form. The continue sits inside an `IfElse`
//     directly under the body — no nested loop between, so the label check is sound.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{
            else_is_empty_or_missing, ends_with_continue, negate, strip_trailing_continue_into_seq,
        },
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    SwapContinueBreak.refine(exp)
}

struct SwapContinueBreak;

impl Refine for SwapContinueBreak {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;
        let Exp::Seq(seq) = body.as_mut() else {
            return false;
        };
        if seq.len() < 3 {
            return false;
        }
        let last = seq.len() - 1;
        let break_idx = last;
        let if_idx = last - 2;

        // Trailing `Break` must target this loop.
        let Exp::Break(b) = &seq[break_idx] else {
            return false;
        };
        if *b != loop_label {
            return false;
        }
        // `IfElse` must have empty/missing else and a then-arm ending in `Continue(loop_label)`.
        let Exp::IfElse(_, then_b, else_b) = &seq[if_idx] else {
            return false;
        };
        if !else_is_empty_or_missing(else_b.as_ref().as_ref()) {
            return false;
        }
        if !ends_with_continue(then_b, loop_label) {
            return false;
        }

        // Rewrite. Pop the trailing `[e1, break]`; rebuild the `IfElse` with the negated
        // test and `[e1, break]` as the new then-arm; append the old then-arm's leading
        // items (everything before its trailing continue) and a relocated `continue`.
        let break_exp = seq.pop().unwrap();
        let e1 = seq.pop().unwrap();
        let Exp::IfElse(test, then_b, _) = std::mem::replace(&mut seq[if_idx], Exp::Seq(vec![]))
        else {
            unreachable!()
        };
        let mut test = test;
        negate(&mut test);
        let e0_seq = strip_trailing_continue_into_seq(*then_b);
        seq[if_idx] = Exp::IfElse(
            test,
            Box::new(Exp::Seq(vec![e1, break_exp])),
            Box::new(None),
        );
        seq.extend(e0_seq);
        seq.push(Exp::Continue(loop_label));
        true
    }
}
