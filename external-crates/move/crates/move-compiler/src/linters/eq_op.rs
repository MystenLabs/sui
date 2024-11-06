// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements a lint to detect and warn about binary operations with equal operands in Move code.
//! Targets comparison, logical, bitwise, subtraction, and division operations where such usage might indicate errors or redundancies.

use super::StyleCodes;
use crate::parser::ast::BinOp_;
use crate::{
    diag,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    EqualOperandsCheck,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, sp!(_, op), _, rhs) = &exp.exp.value {
            if should_check_operands(lhs, rhs, op) {
                let diag = diag!(
                    StyleCodes::EqualOperands.diag_info(),
                    (
                        exp.exp.loc,
                        "Equal operands detected in binary operation, which might indicate a logical error or redundancy."
                    )
                );
                self.add_diag(diag);
            }
        }
        false
    }
);

fn should_check_operands(lhs: &T::Exp, rhs: &T::Exp, op: &BinOp_) -> bool {
    lhs.exp.value == rhs.exp.value && is_relevant_op(op)
}

fn is_relevant_op(op: &BinOp_) -> bool {
    use BinOp_::*;
    matches!(
        op,
        Eq | Neq | Gt | Ge | Lt | Le | And | Or | BitAnd | BitOr | Xor | Sub | Div
    )
}
