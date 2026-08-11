// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Swap a continue-then / break-else at loop tail into a break guard followed by the work:
// `if (t) { e; continue; } else { break; }` => `if (!t) { break; } e`
//
// Both arms of the original definitively exit the if, so relocating `e` outside is sound;
// the implicit fall-through at the loop's true tail replaces the original `continue`.
//
// Preconditions:
//   - The `IfElse` is the last item of the loop's body `Seq`.
//   - The else-arm is exactly `Break(loop_label)`.
//   - The then-arm ends in `Continue(loop_label)`.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{ends_with_continue, loop_body_seq_mut, negate, strip_trailing_continue_into_seq},
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    SwapContinueBreakElse.refine(exp)
}

struct SwapContinueBreakElse;

impl Refine for SwapContinueBreakElse {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Some((loop_label, seq)) = loop_body_seq_mut(exp) else {
            return false;
        };
        if seq.is_empty() {
            return false;
        }
        let if_idx = seq.len() - 1;
        let Exp::IfElse(_, then_b, else_b) = &seq[if_idx] else {
            return false;
        };
        // The else-arm must be exactly `Break(loop_label)`.
        let Some(Exp::Break(b)) = else_b.as_ref().as_ref() else {
            return false;
        };
        if *b != loop_label {
            return false;
        }
        // The then-arm must end in `Continue(loop_label)`.
        if !ends_with_continue(then_b, loop_label) {
            return false;
        }

        // Rewrite. Replace the `IfElse` in place with the inverted guard `if (!t) { break }`,
        // then append the original then-arm's prefix (`e`) as siblings.
        let Exp::IfElse(test, then_b, _) = std::mem::replace(&mut seq[if_idx], Exp::Seq(vec![]))
        else {
            unreachable!()
        };
        let mut test = test;
        negate(&mut test);
        let e_items = strip_trailing_continue_into_seq(*then_b);
        seq[if_idx] = Exp::IfElse(test, Box::new(Exp::Break(loop_label)), Box::new(None));
        seq.extend(e_items);
        true
    }
}
