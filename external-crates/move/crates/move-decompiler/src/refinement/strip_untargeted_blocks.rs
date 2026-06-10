// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Strip `Exp::Block(id, body)` wrappers whose `id` no surviving goto or labeled break/continue
//! references. The structurer wraps every basic block in a `Block(N)` marker so a residual
//! `Goto(N)` always has something to point at; once those gotos are gone (either fall-through
//! elided by earlier passes or rewritten to labeled `break`s by `goto_to_break`), the markers
//! are pure noise and we drop them.
//!
//! Running as a refinement lets us strip both before and after the goto-rewriting refinements
//! naturally — each pipeline iteration recomputes targets and strips again, so newly orphaned
//! blocks get cleaned up the next time around.

use std::collections::HashSet;

use crate::ast::Exp;

pub fn refine(exp: &mut Exp) -> bool {
    let targets = crate::translate::collect_goto_targets(exp);
    let mut changed = false;
    strip(exp, &targets, &mut changed);
    changed
}

fn strip(exp: &mut Exp, targets: &HashSet<u64>, changed: &mut bool) {
    while let Exp::Block(id, body) = exp
        && !targets.contains(id)
    {
        let inner = std::mem::replace(body.as_mut(), Exp::Seq(vec![]));
        *exp = inner;
        *changed = true;
    }
    match exp {
        Exp::Block(_, body)
        | Exp::Loop(_, body)
        | Exp::Assign(_, body)
        | Exp::LetBind(_, body)
        | Exp::Abort(body)
        | Exp::Borrow(_, body)
        | Exp::Unpack(_, _, body)
        | Exp::UnpackVariant(_, _, _, body)
        | Exp::VecUnpack(_, body) => strip(body, targets, changed),
        Exp::While(_, c, b) => {
            strip(c, targets, changed);
            strip(b, targets, changed);
        }
        Exp::IfElse(c, t, alt) => {
            strip(c, targets, changed);
            strip(t, targets, changed);
            if let Some(a) = alt.as_mut().as_mut() {
                strip(a, targets, changed);
            }
        }
        Exp::Switch(c, _, arms) => {
            strip(c, targets, changed);
            for (_, e) in arms {
                strip(e, targets, changed);
            }
        }
        Exp::Match(c, _, arms) => {
            strip(c, targets, changed);
            for (_, _, e) in arms {
                strip(e, targets, changed);
            }
        }
        Exp::MatchLit(c, arms) => {
            strip(c, targets, changed);
            for (_, e) in arms {
                strip(e, targets, changed);
            }
        }
        Exp::Seq(es) | Exp::Return(es) | Exp::Call(_, es) => {
            for e in es {
                strip(e, targets, changed);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                strip(a, targets, changed);
            }
        }
        Exp::Unstructured(nodes) => {
            use crate::ast::UnstructuredNode;
            for n in nodes {
                if let UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) = n {
                    strip(body, targets, changed);
                }
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_) => {}
    }
}
