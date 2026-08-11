// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Canonicalize comparisons with a literal so the literal sits on the right.
//!
//! `Move`'s stack model emits the first source argument first, so when the bytecode reads
//! "compare literal-first against a value pushed second", the natural lowering produces
//! `Value op Variable`. Most source code is written with the value on the left and the
//! literal on the right (`x == 0`), so we swap and rewrite the op:
//!
//! | input        | output |
//! |--------------|--------|
//! | `c == x`     | `x == c` |
//! | `c != x`     | `x != c` |
//! | `c < x`      | `x > c`  |
//! | `c > x`      | `x < c`  |
//! | `c <= x`     | `x >= c` |
//! | `c >= x`     | `x <= c` |
//!
//! `c` is a `Value` literal or `Constant`; the RHS must *not* itself be a literal-like, so
//! we never flip `0 == 1` (no improvement) or oscillate against ourselves.

use move_stackless_bytecode_2::ast::PrimitiveOp;

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    SimplifyZeroCompare.refine(exp)
}

struct SimplifyZeroCompare;

impl Refine for SimplifyZeroCompare {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Primitive { op, args } = exp else {
            return false;
        };
        let [lhs, rhs] = args.as_slice() else {
            return false;
        };
        // We only flip when the LHS is a literal and the RHS is not. RHS-also-literal
        // (`0 == 1`) is no improvement and would oscillate; LHS-not-literal is already
        // canonical.
        if !is_literal_like(lhs) || is_literal_like(rhs) {
            return false;
        }
        let Some(swapped) = swap_op(op) else {
            return false;
        };
        args.swap(0, 1);
        *op = swapped;
        true
    }
}

fn swap_op(op: &PrimitiveOp) -> Option<PrimitiveOp> {
    match op {
        PrimitiveOp::Equal => Some(PrimitiveOp::Equal),
        PrimitiveOp::NotEqual => Some(PrimitiveOp::NotEqual),
        PrimitiveOp::LessThan => Some(PrimitiveOp::GreaterThan),
        PrimitiveOp::GreaterThan => Some(PrimitiveOp::LessThan),
        PrimitiveOp::LessThanOrEqual => Some(PrimitiveOp::GreaterThanOrEqual),
        PrimitiveOp::GreaterThanOrEqual => Some(PrimitiveOp::LessThanOrEqual),
        _ => None,
    }
}

fn is_literal_like(exp: &Exp) -> bool {
    matches!(exp, Exp::Value(_) | Exp::Constant(_))
}
