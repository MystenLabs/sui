// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    FlattenSeq.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement

struct FlattenSeq;

impl Refine for FlattenSeq {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(seq) = exp else {
            return false;
        };

        if seq.len() == 1 {
            *exp = seq.pop().unwrap();
            return true;
        }

        if !seq.iter().any(|e| matches!(e, Exp::Seq(_))) {
            return false;
        }

        let mut out_seq = vec![];

        for entry in std::mem::take(seq) {
            match entry {
                Exp::Seq(nested) => {
                    out_seq.extend(nested);
                }
                e => out_seq.push(e),
            }
        }

        std::mem::swap(seq, &mut out_seq);
        true
    }
}
