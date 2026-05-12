// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    FuseLet.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement
//
// `hoist_declarations` and `hoist_arm_assignments` together produce the shape
//
//     let X;
//     X = e;
//
// where `let X;` is a `Declare([X])` and `X = e;` is an immediately following `Assign([X], e)`
// in the same `Seq`. Fuse the two back together into a single `LetBind([X], e)` — i.e.,
// `let X = e;`. Only single-target Declare/Assign pairs are fused, and the targets must match
// exactly; anything else is left for a future refinement.

struct FuseLet;

impl Refine for FuseLet {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };
        let mut changed = false;
        let mut i = 0;
        while i + 1 < items.len() {
            if !is_fusable_pair(&items[i], &items[i + 1]) {
                i += 1;
                continue;
            }
            // Take the Assign out, lift its RHS, replace the Declare in place.
            let Exp::Assign(targets, rhs) = items.remove(i + 1) else {
                unreachable!()
            };
            items[i] = Exp::LetBind(targets, rhs);
            changed = true;
            i += 1;
        }
        changed
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers

fn is_fusable_pair(a: &Exp, b: &Exp) -> bool {
    matches!(
        (a, b),
        (Exp::Declare(decl), Exp::Assign(targets, _))
            if decl.len() == 1 && decl == targets
    )
}
