// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Swap the tail of a loop body shaped as `[…, IfElse(t, e0;continue, ∅), e1]` into
// `[…, IfElse(t, e0, Some(e1)), continue]`. `e1` was the implicit else of the original
// `IfElse` (reached when `t` is false and the if falls through); promote it into the
// actual else-arm so the source structure makes the branching explicit, and lift the
// `continue` to a trailing position where `remove_trailing_continue` can drop it.
//
// Preconditions (analogous to `swap_continue_break`, just without the trailing `break`):
//   - Pattern at the tail of a `Loop` body (last two items). `e1` is the last item.
//   - The last item is *not* a `Break` — that's `swap_continue_break`'s shape; we don't
//     want both rules grabbing the same input.
//   - The `IfElse` has no else-arm (or an empty one) — `hoist_dual_continue` handles the
//     case where both arms continue.
//   - The inner `continue` targets the immediate enclosing loop, via label equality.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{
            else_is_empty_or_missing, ends_with_continue, seq_or_singleton,
            strip_trailing_continue_into_seq,
        },
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    SwapContinueFallthrough.refine(exp)
}

struct SwapContinueFallthrough;

impl Refine for SwapContinueFallthrough {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;
        let Exp::Seq(seq) = body.as_mut() else {
            return false;
        };
        if seq.len() < 2 {
            return false;
        }
        let last = seq.len() - 1;
        let if_idx = last - 1;

        // Cede the `[IfElse, e1, Break]` shape to `swap_continue_break`.
        if matches!(&seq[last], Exp::Break(_)) {
            return false;
        }
        let Exp::IfElse(_, then_b, else_b) = &seq[if_idx] else {
            return false;
        };
        if !else_is_empty_or_missing(else_b.as_ref().as_ref()) {
            return false;
        }
        if !ends_with_continue(then_b, loop_label) {
            return false;
        }

        // Rewrite. Pop `e1`; rebuild the `IfElse` with `e0` (then-arm minus trailing
        // continue) as the new then-arm and `e1` as the new else; append a `continue`.
        let e1 = seq.pop().unwrap();
        let Exp::IfElse(test, then_b, _) = std::mem::replace(&mut seq[if_idx], Exp::Seq(vec![]))
        else {
            unreachable!()
        };
        let e0 = seq_or_singleton(strip_trailing_continue_into_seq(*then_b));
        seq[if_idx] = Exp::IfElse(test, Box::new(e0), Box::new(Some(e1)));
        seq.push(Exp::Continue(loop_label));
        true
    }
}
