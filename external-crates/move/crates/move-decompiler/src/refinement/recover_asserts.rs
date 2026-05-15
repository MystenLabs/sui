// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ast::{Exp, ModuleRef},
    refinement::Refine,
};
use move_stackless_bytecode_2::ast::PrimitiveOp;
use move_symbol_pool::Symbol;

// Recognize the `assert!` idiom in structured output. The post-dom abort handling makes the
// bytecode-to-source form fall out as `if (cond) {} else { abort code }` (and occasionally
// the negated `if (!cond) { abort code }` with no else). Rewrite either into a single
// `assert!(cond, code)` builtin call.
//
// Conditions are negated only when the abort sits in the then-arm; the empty-then case is the
// common one and keeps `cond` exactly as it appears in the source.

pub fn refine(exp: &mut Exp) -> bool {
    RecoverAsserts.refine(exp)
}

struct RecoverAsserts;

impl Refine for RecoverAsserts {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::IfElse(cond, then_b, else_b) = exp else {
            return false;
        };

        // `if (cond) { } else { abort code }` — the natural shape from a `!cond` test that
        // branches to abort on failure. Pull `cond` and `code` verbatim.
        if is_empty(then_b)
            && let Some(else_inner) = else_b.as_ref().as_ref()
            && let Exp::Abort(code) = else_inner
        {
            *exp = assert_call(cond.as_ref().clone(), (**code).clone());
            return true;
        }

        // `if (cond) { abort code }` (or empty-else equivalent) — the negation of the above.
        // Wrap `cond` in `!` (or strip an existing `!`) so the recovered assertion reads in
        // the same direction.
        let else_is_empty_or_missing = else_b.as_ref().as_ref().is_none_or(is_empty);
        if else_is_empty_or_missing && let Exp::Abort(code) = then_b.as_ref() {
            let mut negated = cond.as_ref().clone();
            negate(&mut negated);
            *exp = assert_call(negated, (**code).clone());
            return true;
        }

        false
    }
}

// ------------------------------------------------------------------------------------------------
// Helpers

fn assert_call(cond: Exp, code: Exp) -> Exp {
    Exp::Call(
        (ModuleRef::Builtin, Symbol::from("assert!")),
        vec![cond, code],
    )
}

fn is_empty(exp: &Exp) -> bool {
    matches!(exp, Exp::Seq(items) if items.is_empty())
}

/// Local copy of `introduce_while::negate` — see there for the full intent. Strips a single
/// outer `!` if present, otherwise wraps in `!`.
fn negate(exp: &mut Exp) {
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
