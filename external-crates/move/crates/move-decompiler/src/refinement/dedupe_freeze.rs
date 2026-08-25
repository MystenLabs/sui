// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Strip redundant `freeze` wrappings:
//!
//! - `freeze(freeze(e))` -> `freeze(e)`. Move's `freeze` is `&mut T -> &T`; applying it
//!   twice has the same observable result as once (after the inner the type is already
//!   `&T`, and the outer would no-op).
//! - `freeze(&e)` -> `&e`. An immutable `Borrow(false, _)` is already `&T`, so wrapping
//!   it in `freeze` is a no-op. Note we *don't* strip `freeze(&mut e)` - that one
//!   genuinely downgrades `&mut e` to `&e`.
//!
//! Both shapes survive the structurer in cases where the bytecode emitted a `FreezeRef`
//! that the source didn't (or used to chain through reference-shape transformations).
//! Snapshot impact is modest, but the visible noise of `freeze(freeze(...))` and
//! `freeze(&l0)` is easy to read past at a glance.

use move_stackless_bytecode_2::ast::DataOp;

use crate::{ast::Exp, refinement::Refine};

pub fn refine(exp: &mut Exp) -> bool {
    DedupeFreeze.refine(exp)
}

struct DedupeFreeze;

impl Refine for DedupeFreeze {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Data {
            op: DataOp::FreezeRef,
            args,
        } = exp
        else {
            return false;
        };
        let [inner] = args.as_slice() else {
            return false;
        };
        match inner {
            // `freeze(freeze(e))` -> `freeze(e)`. After the inner the value is already `&T`,
            // so the outer is a no-op. Drop the outer, keep the inner `Data { FreezeRef }`.
            Exp::Data {
                op: DataOp::FreezeRef,
                ..
            } => {
                *exp = args.pop().expect("checked above");
                true
            }
            // `freeze(&e)` -> `&e`. The immutable borrow already produces `&T`, so the
            // outer freeze is a no-op. We do *not* match `Borrow(true, _)` - freezing a
            // `&mut T` is a real downgrade we must keep.
            Exp::Borrow(false, _) => {
                *exp = args.pop().expect("checked above");
                true
            }
            _ => false,
        }
    }
}

// No `.move` test exercises this: Move source has no syntactic `freeze`, and the structurer
// only emits single, load-bearing `FreezeRef`s (the `&mut T -> &T` at call args), never the
// nested / freeze-of-`&` shapes this pass targets. These unit tests build the AST directly so
// the refinement still has positive coverage.
#[cfg(test)]
mod tests {
    use super::*;

    fn var(n: &str) -> Exp {
        Exp::Variable(n.to_string())
    }

    fn freeze(e: Exp) -> Exp {
        Exp::Data {
            op: DataOp::FreezeRef,
            args: vec![e],
        }
    }

    #[test]
    fn collapses_nested_freeze() {
        // freeze(freeze(x)) -> freeze(x)
        let mut e = freeze(freeze(var("x")));
        assert!(refine(&mut e));
        let Exp::Data {
            op: DataOp::FreezeRef,
            args,
        } = &e
        else {
            panic!("expected a single freeze, got {e:?}");
        };
        assert!(matches!(args.as_slice(), [Exp::Variable(n)] if n == "x"));
    }

    #[test]
    fn collapses_freeze_of_immutable_borrow() {
        // freeze(&x) -> &x
        let mut e = freeze(Exp::Borrow(false, Box::new(var("x"))));
        assert!(refine(&mut e));
        assert!(matches!(&e, Exp::Borrow(false, _)));
    }

    #[test]
    fn keeps_freeze_of_mut_borrow() {
        // freeze(&mut x) is a real downgrade - leave it alone.
        let mut e = freeze(Exp::Borrow(true, Box::new(var("x"))));
        assert!(!refine(&mut e));
        assert!(matches!(
            &e,
            Exp::Data {
                op: DataOp::FreezeRef,
                ..
            }
        ));
    }
}
