// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    let r1 = LoopRemoveTrailingContinue.refine(exp);
    let r2 = WhileRemoveTrailingContinue.refine(exp);
    r1 || r2
}

// -------------------------------------------------------------------------------------------------
// Refinement

struct LoopRemoveTrailingContinue;

impl Refine for LoopRemoveTrailingContinue {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;

        match &mut **body {
            Exp::Seq(seq) if !seq.is_empty() => {
                // Only drop a trailing continue if it targets this loop (label matches).
                if matches!(seq.last(), Some(Exp::Continue(l)) if *l == loop_label) {
                    seq.pop();
                    true
                } else {
                    false
                }
            }
            Exp::Continue(l) if *l == loop_label => {
                **body = Exp::Seq(vec![]);
                true
            }
            _ => false,
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Refinement

struct WhileRemoveTrailingContinue;

impl Refine for WhileRemoveTrailingContinue {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::While(loop_label, _, body) = exp else {
            return false;
        };
        let loop_label = *loop_label;

        match &mut **body {
            Exp::Seq(seq) if !seq.is_empty() => {
                if matches!(seq.last(), Some(Exp::Continue(l)) if *l == loop_label) {
                    seq.pop();
                    true
                } else {
                    false
                }
            }
            Exp::Continue(l) if *l == loop_label => {
                **body = Exp::Seq(vec![]);
                true
            }
            _ => false,
        }
    }
}
