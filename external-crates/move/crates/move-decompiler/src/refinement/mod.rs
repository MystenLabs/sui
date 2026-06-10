// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::ast::Exp;

mod bool_if_simplify;
mod collapse_let_usage;
mod collect_uses;
mod dedupe_freeze;
mod flatten_seq;
mod fuse_let;
mod hoist_arm_assignments;
mod hoist_dual_continue;
mod hoist_tail_continue;
mod inline_immutable_alias;
mod introduce_while;
mod lift_terminating_arm;
mod liveness;
mod loop_to_seq;
mod negate_comparison;
mod reconstruct_match;
mod recover_asserts;
mod recover_flag;
mod remove_trailing_continue;
mod remove_trailing_return;
mod simplify_borrow_deref;
mod simplify_if;
mod simplify_zero_compare;
mod strip_loop_labels;
mod swap_continue_break;
mod swap_continue_break_else;
mod swap_continue_fallthrough;
mod utils;

pub use collect_uses::collect_uses;
pub use liveness::collect_local_names;

pub type Refinement = fn(&mut Exp) -> bool;

const REFINEMENTS: &[Refinement] = &[
    flatten_seq::refine,
    fuse_let::refine,
    hoist_arm_assignments::refine,
    lift_terminating_arm::refine,
    hoist_dual_continue::refine,
    hoist_tail_continue::refine,
    introduce_while::refine,
    loop_to_seq::refine,
    reconstruct_match::refine,
    remove_trailing_continue::refine,
    remove_trailing_return::refine,
    simplify_borrow_deref::refine,
    dedupe_freeze::refine,
    simplify_zero_compare::refine,
    negate_comparison::refine,
    simplify_if::refine,
    bool_if_simplify::refine,
    recover_flag::refine,
    recover_asserts::refine,
    strip_loop_labels::refine,
    swap_continue_break::refine,
    swap_continue_break_else::refine,
    swap_continue_fallthrough::refine,
    inline_immutable_alias::refine,
    collapse_let_usage::refine,
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
            E::Loop(_, e) => self.refine(e),
            E::Seq(es) => self.refine_seq(es),
            E::While(_, e0, e1) => {
                or!(self.refine(e0), self.refine(e1))
            }
            E::IfElse(e0, e1, e2) => {
                or!(
                    self.refine(e0),
                    self.refine(e1),
                    (**e2).as_mut().map(|e| self.refine(e)).unwrap_or(false)
                )
            }
            E::Switch(e, _, es) => {
                let mut changed = self.refine(e);
                for (_, e) in es.iter_mut() {
                    changed |= self.refine(e);
                }
                changed
            }
            E::Match(e, _, es) => {
                let mut changed = self.refine(e);
                for (_, _, e) in es.iter_mut() {
                    changed |= self.refine(e);
                }
                changed
            }
            E::MatchLit(e, arms) => {
                let mut changed = self.refine(e);
                for (_, body) in arms.iter_mut() {
                    changed |= self.refine(body);
                }
                changed
            }
            E::Return(es) => self.refine_seq(es),
            E::Assign(_, e) => self.refine(e),
            E::LetBind(_, e) => self.refine(e),
            E::Declare(_) => false,
            E::Call(_, es) => self.refine_seq(es),
            E::Abort(e) => self.refine(e),
            E::Primitive { op: _, args } => self.refine_seq(args),
            E::Data { op: _, args } => self.refine_seq(args),
            E::Borrow(_, e) => self.refine(e),
            E::Break(_) => false,
            E::Continue(_) => false,
            E::Value(_) => false,
            E::Variable(_) => false,
            E::Constant(_) => false,
            E::VecUnpack(_, e) => self.refine(e),
            E::Unpack(_, _, e) => self.refine(e),
            E::UnpackVariant(_, _, _, e) => self.refine(e),
            E::Block(_, body) => self.refine(body),
            E::Unstructured(nodes) => {
                use crate::ast::UnstructuredNode;
                let mut changed = false;
                for node in nodes.iter_mut() {
                    match node {
                        UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                            changed |= self.refine(body);
                        }
                        UnstructuredNode::Goto(_) => {}
                    }
                }
                changed
            }
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
