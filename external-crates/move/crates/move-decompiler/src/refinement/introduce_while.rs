// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{negate, peek, peek_mut, unwrap_block},
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    let r1 = IntroduceWhile0.refine(exp);
    let r2 = IntroduceWhile1.refine(exp);
    r1 || r2
}

// -------------------------------------------------------------------------------------------------
// Refinement

struct IntroduceWhile0;

impl Refine for IntroduceWhile0 {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, body) = exp else {
            return false;
        };

        let Exp::IfElse(_, conseq, alt) = peek(body) else {
            return false;
        };
        let Some(alt) = &**alt else {
            return false;
        };
        // Only fire when the Break(label) matches this loop's label (so the break really exits
        // *this* Loop and not a labeled outer one).
        let target = *loop_label;
        if !is_break_to(peek(conseq), target) && !is_break_to(peek(alt), target) {
            return false;
        }

        exp.map_mut(|e| {
            let Exp::Loop(loop_label, body) = e else {
                unreachable!()
            };
            let Exp::IfElse(mut test, conseq, alt) = unwrap_block(*body) else {
                unreachable!()
            };
            let alt = alt.unwrap();
            if is_break_to(peek(&conseq), loop_label) {
                negate(&mut test);
                Exp::While(loop_label, test, Box::new(alt))
            } else {
                Exp::While(loop_label, test, conseq)
            }
        });
        true
    }
}

fn is_break_to(exp: &Exp, loop_label: Option<crate::ast::Label>) -> bool {
    matches!(exp, Exp::Break(l) if *l == loop_label)
}

// -------------------------------------------------------------------------------------------------
// Other Refinement

struct IntroduceWhile1;

impl Refine for IntroduceWhile1 {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_label, loop_body) = exp else {
            return false;
        };
        let loop_label = *loop_label;

        match peek_mut(loop_body) {
            Exp::Seq(seq) if !seq.is_empty() => {
                let Exp::IfElse(_, conseq, alt) = peek(&seq[0]) else {
                    return false;
                };
                if !is_break_to(peek(conseq), loop_label) {
                    return false;
                }
                let None = alt.as_ref() else {
                    return false;
                };
                let Exp::IfElse(mut test, _, _) = unwrap_block(seq.remove(0)) else {
                    return false;
                };
                negate(&mut test);
                *exp = Exp::While(loop_label, test, Box::new(Exp::Seq(std::mem::take(seq))));
                true
            }
            _ => false,
        }
    }
}
