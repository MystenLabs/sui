// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    LoopToSeq.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement

struct LoopToSeq;

impl Refine for LoopToSeq {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;
        let Exp::Seq(seq) = &mut **body else {
            return false;
        };

        // Only fire when the trailing break exits *this* loop (label matches). If it has
        // another loop's label, we can't drop it — it'd change meaning.
        if matches!(seq.last(), Some(Exp::Break(l)) if *l == loop_label) {
            // If there is a continue, we cannot drop the break.
            if seq
                .iter()
                .any(|e| e.contains_continue() || e.contains_break())
            {
                return false;
            }
            // If the last expression is a break, we can just drop it.
            seq.pop();
            true
        } else {
            false
        }
    }
}
