// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Drop a trailing `continue` from one arm of an `IfElse` at loop tail; the implicit
// fall-through at the body's end is itself iteration:
// `if (t) { e; continue; } else { e' }` => `if (t) { e } else { e' }`
// (and symmetric for an else-arm continue).
//
// Preconditions:
//   - The `IfElse` is the last item of the loop's body `Seq`.
//   - Exactly one arm ends in `Continue(loop_label)`.
//   - When the other arm is exactly `Break(loop_label)`, defer to `swap_continue_break_else`.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{
            ends_with_continue, loop_body_seq_mut, seq_or_singleton,
            strip_trailing_continue_into_seq,
        },
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    HoistTailContinue.refine(exp)
}

struct HoistTailContinue;

impl Refine for HoistTailContinue {
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

        let then_has_cont = ends_with_continue(then_b, loop_label);
        let else_has_cont = else_b
            .as_ref()
            .as_ref()
            .is_some_and(|e| ends_with_continue(e, loop_label));

        // Dual-continue is `hoist_dual_continue`'s job.
        if then_has_cont && else_has_cont {
            return false;
        }
        if !then_has_cont && !else_has_cont {
            return false;
        }
        // The continue-then / break-else shape goes to `swap_continue_break_else`, which
        // produces a guard form we prefer there.
        if then_has_cont
            && matches!(else_b.as_ref().as_ref(), Some(Exp::Break(b)) if *b == loop_label)
        {
            return false;
        }

        let Exp::IfElse(test, then_b, else_b) =
            std::mem::replace(&mut seq[if_idx], Exp::Seq(vec![]))
        else {
            unreachable!()
        };
        let (new_then, new_else) = if then_has_cont {
            let then_b = seq_or_singleton(strip_trailing_continue_into_seq(*then_b));
            (Box::new(then_b), else_b)
        } else {
            // else_has_cont: peel the Option, strip, rewrap. Collapse an emptied else to
            // `None` so the rendered output doesn't carry a vestigial `else { }`.
            let stripped = seq_or_singleton(strip_trailing_continue_into_seq(else_b.unwrap()));
            let stripped_else = if matches!(&stripped, Exp::Seq(items) if items.is_empty()) {
                None
            } else {
                Some(stripped)
            };
            (then_b, Box::new(stripped_else))
        };
        seq[if_idx] = Exp::IfElse(test, new_then, new_else);
        seq.push(Exp::Continue(loop_label));
        true
    }
}
