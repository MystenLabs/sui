// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::ast::Exp;

mod flatten_seq;
mod introduce_while;
mod loop_to_seq;
mod remove_trailing_continue;

pub type Refinement = fn(&mut Exp) -> bool;

const REFINEMENTS: &[Refinement] = &[
    flatten_seq::refine,
    introduce_while::refine,
    loop_to_seq::refine,
    remove_trailing_continue::refine,
];

// -------------------------------------------------------------------------------------------------
// Public Interface

pub fn refine(exp: &mut Exp) {
    let mut changed = true;
    while changed {
        changed = false;
        for r in REFINEMENTS {
            changed |= r(exp);
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Refinmenet Traint

trait Refine {
    fn refine(&mut self, exp: &mut Exp) -> bool {
        macro_rules! or {
            ($e:expr, $($rest:expr),+) => { {
                    let e = $e;
                    let es = or!($($rest),+);
                    e || es
                }
            };
            ($e:expr) => {
                $e
            };
        }

        use Exp as E;
        if self.refine_custom(exp) {
            return true;
        }
        match exp {
            E::Loop(e) => self.refine(e),
            E::Seq(es) => self.refine_seq(es),
            E::While(e0, e1) => {
                or!(self.refine(e0), self.refine(e1))
            }
            E::IfElse(e0, e1, e2) => {
                or!(
                    self.refine(e0),
                    self.refine(e1),
                    (**e2).as_mut().map(|e| self.refine(e)).unwrap_or(false)
                )
            }
            E::Switch(e, es) => or!(self.refine(e), self.refine_seq(es)),
            E::Return(es) => self.refine_seq(es),
            E::Assign(_, e) => self.refine(e),
            E::LetBind(_, e) => self.refine(e),
            E::Call(_, es) => self.refine_seq(es),
            E::Abort(e) => self.refine(e),
            E::Primitive { op: _, args } => self.refine_seq(args),
            E::Data { op: _, args } => self.refine_seq(args),
            E::Borrow(_, e) => self.refine(e),

            E::Break => false,
            E::Continue => false,

            E::Value(_) => false,
            E::Variable(_) => false,
            E::Constant(_) => false,
        }
    }

    fn refine_seq(&mut self, exps: &mut Vec<Exp>) -> bool {
        let mut changed = false;
        for exp in exps.iter_mut() {
            changed |= self.refine(exp);
        }
        changed
    }

    fn refine_custom(&mut self, _exp: &mut Exp) -> bool;
}
