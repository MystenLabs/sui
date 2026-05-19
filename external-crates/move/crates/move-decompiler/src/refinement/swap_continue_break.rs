// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Swap the tail of a loop body:
// `if (t) { e0; continue; } e1; break;` => `if (!t) { e1; break; } e0; continue;`
// `if (t) { e0; continue; } break;`     => `if (!t) { break; } e0; continue;`
//
// The optional `e1` between the `IfElse` and the trailing `Break` is hoisted into the new
// then-arm when present; otherwise the new then-arm is just the `Break` itself. In both
// cases the relocated `continue` at the tail is left for `remove_trailing_continue` to strip.
//
// Preconditions:
//   - The pattern sits at the tail of the loop's body `Seq`.
//   - The inner `continue` and the trailing `break` target the immediate enclosing loop.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{
            else_is_empty_or_missing, ends_with_continue, loop_body_seq_mut, negate,
            strip_trailing_continue_into_seq,
        },
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    SwapContinueBreak.refine(exp)
}

struct SwapContinueBreak;

impl Refine for SwapContinueBreak {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Some((loop_label, seq)) = loop_body_seq_mut(exp) else {
            return false;
        };
        if seq.len() < 2 {
            return false;
        }
        let break_idx = seq.len() - 1;

        // Trailing `Break` must target this loop.
        let Exp::Break(b) = &seq[break_idx] else {
            return false;
        };
        if *b != loop_label {
            return false;
        }

        // Locate the IfElse: directly before the break (no e1) or one position earlier
        // (e1 sits between). Without e1, `if_idx == break_idx - 1`; with e1, `break_idx - 2`.
        let (if_idx, has_e1) = if matches!(&seq[break_idx - 1], Exp::IfElse(_, _, _)) {
            (break_idx - 1, false)
        } else if break_idx >= 2 && matches!(&seq[break_idx - 2], Exp::IfElse(_, _, _)) {
            (break_idx - 2, true)
        } else {
            return false;
        };

        // `IfElse` must have empty/missing else and a then-arm ending in `Continue(loop_label)`.
        let Exp::IfElse(_, then_b, else_b) = &seq[if_idx] else {
            unreachable!("if_idx selected by matches!()")
        };
        if !else_is_empty_or_missing(else_b.as_ref().as_ref()) {
            return false;
        }
        if !ends_with_continue(then_b, loop_label) {
            return false;
        }

        // Rewrite. Pop trailing items down to (but not including) the IfElse; rebuild the
        // IfElse with the negated test and the popped tail (`[e1, break]` or just `break`)
        // as the new then-arm; append the old then-arm's leading items and a relocated
        // `continue`.
        let break_exp = seq.pop().unwrap();
        let e1_opt = if has_e1 {
            Some(seq.pop().unwrap())
        } else {
            None
        };
        let Exp::IfElse(test, then_b, _) = std::mem::replace(&mut seq[if_idx], Exp::Seq(vec![]))
        else {
            unreachable!()
        };
        let mut test = test;
        negate(&mut test);
        let e0_seq = strip_trailing_continue_into_seq(*then_b);
        let new_then = match e1_opt {
            Some(e1) => Exp::Seq(vec![e1, break_exp]),
            None => break_exp,
        };
        seq[if_idx] = Exp::IfElse(test, Box::new(new_then), Box::new(None));
        seq.extend(e0_seq);
        seq.push(Exp::Continue(loop_label));
        true
    }
}
