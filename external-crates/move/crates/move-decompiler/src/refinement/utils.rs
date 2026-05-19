// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Helpers shared between refinements.

use crate::ast::Exp;
use move_stackless_bytecode_2::ast::PrimitiveOp;

/// Negate a boolean expression. Strips a single outer `!` if present, otherwise wraps in `!`.
//
// TODO: simplify double negation, De Morgan, etc.
pub(super) fn negate(exp: &mut Exp) {
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
