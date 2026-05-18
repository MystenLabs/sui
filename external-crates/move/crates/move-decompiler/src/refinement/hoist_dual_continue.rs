// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Rewrite `IfElse(t, e0;continue 'L, Some(e0';continue 'L))` — both arms continuing to the
// same loop — into `Seq([IfElse(t, e0, Some(e0')), continue 'L])`. The continue is moved
// outside the `IfElse` and lives in a trailing position where `remove_trailing_continue`
// can drop it (when at a loop tail).
//
// Applies anywhere, not just at a loop tail: hoisting a shared trailing continue out of
// matching arms is always a sound normalization. Whether the relocated continue ultimately
// disappears is the trailing-continue pass's concern.
//
// Locality: the two arms must continue to the *same* label. We don't constrain that label
// to be the immediate enclosing loop — the AST already encodes which loop each continue
// targets, and preserving the label keeps semantics intact wherever the rewritten
// expression sits in the tree.

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
