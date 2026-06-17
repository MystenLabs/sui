// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Hoist a shared trailing `continue` out of both arms of an `IfElse`:
// `if (t) { e0; continue 'L; } else { e0'; continue 'L; }` => `if (t) { e0 } else { e0' }; continue 'L;`
//
// Applies anywhere; the relocated continue is `remove_trailing_continue`'s concern.
//
// Preconditions:
//   - Both arms end in `Continue` for the same label.

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{seq_or_singleton, strip_trailing_continue_into_seq, trailing_continue_label},
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    HoistDualContinue.refine(exp)
}

struct HoistDualContinue;

impl Refine for HoistDualContinue {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::IfElse(_, then_b, else_b) = exp else {
            return false;
        };
        let Some(else_b) = else_b.as_ref().as_ref() else {
            return false;
        };
        let Some(then_label) = trailing_continue_label(then_b) else {
            return false;
        };
        let Some(else_label) = trailing_continue_label(else_b) else {
            return false;
        };
        if then_label != else_label {
            return false;
        }
        let label = then_label;

        exp.map_mut(|e| {
            let Exp::IfElse(test, then_b, else_b) = e else {
                unreachable!()
            };
            let then_b = seq_or_singleton(strip_trailing_continue_into_seq(*then_b));
            let else_b = seq_or_singleton(strip_trailing_continue_into_seq(else_b.unwrap()));
            let new_if = Exp::IfElse(test, Box::new(then_b), Box::new(Some(else_b)));
            Exp::Seq(vec![new_if, Exp::Continue(label)])
        });
        true
    }
}
