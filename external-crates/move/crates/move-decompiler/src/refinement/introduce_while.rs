// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

use move_stackless_bytecode_2::ast::PrimitiveOp;

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
        let Exp::Loop(body) = exp else {
            return false;
        };

        let Exp::IfElse(_, conseq, alt) = &**body else {
            return false;
        };
        let Some(alt) = &**alt else {
            return false;
        };
        if !matches!(&**conseq, Exp::Break) && !matches!(alt, Exp::Break) {
            return false;
        }

        exp.map_mut(|e| {
            let Exp::Loop(body) = e else { unreachable!() };
            let Exp::IfElse(mut test, conseq, alt) = *body else {
                unreachable!()
            };
            let alt = alt.unwrap();
            match (&*conseq, &alt) {
                (Exp::Break, _) => {
                    negate(&mut test);
                    Exp::While(test, Box::new(alt))
                }
                (_, Exp::Break) => Exp::While(test, conseq),
                _ => unreachable!(),
            }
        });
        true
    }
}

// -------------------------------------------------------------------------------------------------
// Other Refinement

struct IntroduceWhile1;

impl Refine for IntroduceWhile1 {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Loop(loop_body) = exp else {
            return false;
        };

        match &mut **loop_body {
            Exp::Seq(seq) if !seq.is_empty() => {
                let Exp::IfElse(_, conseq, alt) = &seq[0] else {
                    return false;
                };
                let Exp::Break = conseq.as_ref() else {
                    return false;
                };
                let None = alt.as_ref() else {
                    return false;
                };
                let Exp::IfElse(mut test, _, _) = seq.remove(0) else {
                    return false;
                };
                negate(&mut test);
                *exp = Exp::While(test, Box::new(Exp::Seq(std::mem::take(seq))));
                true
            }
            _ => false,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Helpers

// Optimize the given expression by applying a series of local rewrites.
fn negate(exp: &mut Exp) {
    // TODO: simplify double negation, De Morgan, etc.
    use Exp as E;
    match exp {
        E::Primitive { op, args } if *op == PrimitiveOp::Not && args.len() == 1 => {
            *exp = args.pop().unwrap();
        }
        _ => {
            *exp = Exp::Primitive {
                op: PrimitiveOp::Not,
                args: vec![exp.clone()],
            };
        }
    }
}
