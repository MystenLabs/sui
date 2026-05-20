// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Promote a sibling fallthrough into the empty else-arm of an `IfElse` at loop tail:
// `if (t) { e0; continue; } e1` => `if (t) { e0 } else { e1 }; continue;`
//
// Preconditions:
//   - Pattern sits at the tail of the loop's body `Seq` (last two items).
//   - The last item is not a `Break` (that's `swap_continue_break`'s shape).
//   - The `IfElse` has no else-arm (or an empty one).
//   - The inner `continue` targets the immediate enclosing loop.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{
            else_is_empty_or_missing, ends_with_continue, loop_body_seq_mut, seq_or_singleton,
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
        let Some((loop_label, seq)) = loop_body_seq_mut(exp) else {
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
