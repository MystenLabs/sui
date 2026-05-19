// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Helpers shared between refinements.

use crate::ast::Exp;
use move_stackless_bytecode_2::ast::PrimitiveOp;

/// Look through any `Exp::Block` wrappers to reach the inner expression. Used by refinements
/// whose pattern matching cares about the underlying form, not block delimiters. `Block`
/// carries a block ID for goto cross-referencing; refinements that aren't tracking block
/// boundaries (most of them) want the inner shape.
pub(super) fn peek(exp: &Exp) -> &Exp {
    match exp {
        Exp::Block(_, body) => peek(body),
        _ => exp,
    }
}

pub(super) fn peek_mut(exp: &mut Exp) -> &mut Exp {
    match exp {
        Exp::Block(_, body) => peek_mut(body),
        _ => exp,
    }
}

/// Owned counterpart to `peek`: consume any outer `Block` wrappers and return the inner
/// expression. Used when a refinement needs to destructure (move out of) the value, dropping
/// the block ID (typically because the surrounding control flow is being rewritten).
pub(super) fn unwrap_block(exp: Exp) -> Exp {
    match exp {
        Exp::Block(_, body) => unwrap_block(*body),
        e => e,
    }
}

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
