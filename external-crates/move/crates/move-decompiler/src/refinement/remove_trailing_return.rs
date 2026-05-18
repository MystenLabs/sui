// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::ast::Exp;

// Elide a bare `return` (no operands) when it appears in tail position of the function
// body. Recurs through `Seq` (last element), `IfElse` (both arms), and `Switch` (all
// cases). Does NOT recur through loops/whiles — `return` there exits the function,
// not the loop, so it is not interchangeable with falling through.
pub fn refine(exp: &mut Exp) -> bool {
    elide_tail(exp)
}

fn elide_tail(exp: &mut Exp) -> bool {
    match exp {
        Exp::Return(es) if es.is_empty() => {
            *exp = Exp::Seq(vec![]);
            true
        }
        Exp::Seq(seq) => seq.last_mut().map(elide_tail).unwrap_or(false),
        Exp::IfElse(_, then_b, else_b) => {
            let r1 = elide_tail(then_b);
            let r2 = (**else_b).as_mut().map(elide_tail).unwrap_or(false);
            r1 || r2
        }
        Exp::Switch(_, _, cases) => {
            let mut changed = false;
            for (_, case) in cases.iter_mut() {
                changed |= elide_tail(case);
            }
            changed
        }
        _ => false,
    }
}
