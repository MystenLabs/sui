// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Collapse `*(&e)` (and `*(&mut e)`) to just `e`.
//!
//! In our AST that's `Data { op: ReadRef, args: [Borrow(_, inner)] }`. The borrow puts a
//! reference to `inner` on the stack and the read immediately dereferences it; the round-trip
//! is semantically `copy inner` (sound: if the bytecode emitted this, the inner type has
//! `copy`, since otherwise `*&x` wouldn't have type-checked at the source). Stripping the
//! pair leaves the same value without the extra ref-then-deref noise.

use crate::{ast::Exp, refinement::Refine};
use move_stackless_bytecode_2::ast::DataOp;

pub fn refine(exp: &mut Exp) -> bool {
    SimplifyBorrowDeref.refine(exp)
}

struct SimplifyBorrowDeref;

impl Refine for SimplifyBorrowDeref {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Data {
            op: DataOp::ReadRef,
            args,
        } = exp
        else {
            return false;
        };
        if args.len() != 1 {
            return false;
        }
        if !matches!(&args[0], Exp::Borrow(_, _)) {
            return false;
        }
        // Pull the inner expression out of the borrow and replace the whole node.
        let Exp::Borrow(_, inner) = args.pop().unwrap() else {
            unreachable!("matched above")
        };
        *exp = *inner;
        true
    }
}
