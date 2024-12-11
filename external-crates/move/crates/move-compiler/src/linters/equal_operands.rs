// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects operands that are equal in a binary operation which results in a constant value.
//! Unlike the warning generated during constant folding, this works over non-constant expressions.

use crate::{
    cfgir::visitor::{same_value_exp, simple_visitor},
    diag,
    hlir::ast as H,
    linters::StyleCodes,
    parser::ast::BinOp_,
};

simple_visitor!(
    EqualOperands,
    fn visit_exp_custom(&mut self, e: &H::Exp) -> bool {
        let H::UnannotatedExp_::BinopExp(lhs, op, rhs) = &e.exp.value else {
            return false;
        };

        if same_value_exp(lhs, rhs) {
            let resulting_value = match &op.value {
                // warning reported elsewhere
                BinOp_::Div | BinOp_::Mod if rhs.as_value().is_some_and(|v| v.value.is_zero()) => {
                    return false
                }
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
                | BinOp_::Iff => return false,
            };
            let msg = format!(
                "Always equal operands detected in binary operation, \
                    which will evaluate to {resulting_value}"
            );
            let lhs_msg = "This expression";
            let rhs_msg = "Will always evaluate to the same value as this expression";
            self.reporter.add_diag(diag!(
                StyleCodes::EqualOperands.diag_info(),
                (e.exp.loc, msg),
                (lhs.exp.loc, lhs_msg),
                (rhs.exp.loc, rhs_msg)
            ));
        };
        false
    }
);
