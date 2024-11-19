// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements a lint to detect and warn about binary operations with equal operands in Move code.
//! Targets comparison, logical, bitwise, subtraction, and division operations where such usage might indicate errors or redundancies.

use super::StyleCodes;
use crate::cfgir::visitor::{is_zero, same_value_exp};
use crate::parser::ast::BinOp_;
use crate::{cfgir::visitor::simple_visitor, diag, hlir::ast as H};

simple_visitor!(
    EqualOperands,
    fn visit_exp_custom(&mut self, exp: &H::Exp) -> bool {
        let H::UnannotatedExp_::BinopExp(lhs, sp!(_, op), rhs) = &exp.exp.value else {
            return false;
        };
        if let Some(resulting_value) = equal_operands(lhs, *op, rhs) {
            let msg = format!(
                "Equal operands detected in binary operation, \
                which always evaluates to {resulting_value}"
            );
            let lhs_msg = "This expression";
            let rhs_msg = "Evaluates to the same value as this expression";
            self.add_diag(diag!(
                StyleCodes::EqualOperands.diag_info(),
                (exp.exp.loc, msg),
                (lhs.exp.loc, lhs_msg),
                (rhs.exp.loc, rhs_msg)
            ));
        };
        false
    }
);

fn equal_operands(lhs: &H::Exp, op: BinOp_, rhs: &H::Exp) -> Option<&'static str> {
    let resulting_value = match op {
        BinOp_::Div | BinOp_::Mod if is_zero(rhs) => return None, // warning reported elsewhere
        BinOp_::Sub | BinOp_::Mod | BinOp_::Xor => "'0'",
        BinOp_::Div => "'1'",
        BinOp_::BitOr | BinOp_::BitAnd | BinOp_::And | BinOp_::Or => "the same value",
        BinOp_::Neq | BinOp_::Lt | BinOp_::Gt => "'false'",
        BinOp_::Eq | BinOp_::Le | BinOp_::Ge => "'true'",
        BinOp_::Add
        | BinOp_::Mul
        | BinOp_::Shl
        | BinOp_::Shr
        | BinOp_::Range
        | BinOp_::Implies
        | BinOp_::Iff => return None,
    };
    if same_value_exp(lhs, rhs) {
        Some(resulting_value)
    } else {
        None
    }
}
