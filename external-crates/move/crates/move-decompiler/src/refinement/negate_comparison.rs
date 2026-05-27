// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Push `!` through comparison operators so the negation lands on the operator itself.
//!
//! | input        | output |
//! |--------------|--------|
//! | `!(a == b)`  | `a != b` |
//! | `!(a != b)`  | `a == b` |
//! | `!(a < b)`   | `a >= b` |
//! | `!(a > b)`   | `a <= b` |
//! | `!(a <= b)`  | `a > b`  |
//! | `!(a >= b)`  | `a < b`  |
//!
//! Double-negation (`!!e` → `e`) is already handled by `utils::negate`; this pass only
//! covers the comparison case. The structurer often emits `!(cond)` shapes after inverting
//! a branch direction (e.g., `swap_continue_break`'s output), and these rewrites remove a
//! visible parenthesis and one logical step per guard.

use move_stackless_bytecode_2::ast::PrimitiveOp;

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    NegateComparison.refine(exp)
}

struct NegateComparison;

impl Refine for NegateComparison {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Primitive {
            op: PrimitiveOp::Not,
            args,
        } = exp
        else {
            return false;
        };
        if args.len() != 1 {
            return false;
        }
        let Exp::Primitive {
            op: inner_op,
            args: inner_args,
        } = &mut args[0]
        else {
            return false;
        };
        let Some(dual) = dual_op(inner_op) else {
            return false;
        };
        let new_args = std::mem::take(inner_args);
        *exp = Exp::Primitive {
            op: dual,
            args: new_args,
        };
        true
    }
}

fn dual_op(op: &PrimitiveOp) -> Option<PrimitiveOp> {
    match op {
        PrimitiveOp::Equal => Some(PrimitiveOp::NotEqual),
        PrimitiveOp::NotEqual => Some(PrimitiveOp::Equal),
        PrimitiveOp::LessThan => Some(PrimitiveOp::GreaterThanOrEqual),
        PrimitiveOp::GreaterThan => Some(PrimitiveOp::LessThanOrEqual),
        PrimitiveOp::LessThanOrEqual => Some(PrimitiveOp::GreaterThan),
        PrimitiveOp::GreaterThanOrEqual => Some(PrimitiveOp::LessThan),
        _ => None,
    }
}
