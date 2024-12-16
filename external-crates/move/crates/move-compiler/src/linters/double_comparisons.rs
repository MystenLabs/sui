// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and simplifies double comparisons in code, such as:
//! - `x == 10 || x < 10` into `x <= 10`
//! - `x == 10 || x > 10` into `x >= 10`
//! - `x < 10 || x > 20` into `x not in [10..20]`
//! - `x <= 10 || x >= 20` into `x not in (10..20)`
//! These simplifications enhance code clarity and maintainability.

use crate::{
    cfgir::visitor::simple_visitor,
    diag,
    diagnostics::DiagnosticReporter,
    hlir::ast::{self as H, UnannotatedExp_, Value_},
    linters::StyleCodes,
    parser::ast::BinOp_,
};
use move_ir_types::location::Loc;

simple_visitor!(
    DoubleComparisonCheck,
    fn visit_exp_custom(&mut self, exp: &H::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, sp!(_, BinOp_::Or), rhs) = &exp.exp.value {
            match (&lhs.exp.value, &rhs.exp.value) {
                (
                    UnannotatedExp_::BinopExp(lhs_l, sp!(_, lhs_op), lhs_r),
                    UnannotatedExp_::BinopExp(rhs_l, sp!(_, rhs_op), rhs_r),
                ) if are_expressions_comparable(lhs_l, rhs_l, lhs_r, rhs_r) => {
                    match (lhs_op, rhs_op) {
                        // Case 1: x == 10 || x < 10  ->  x <= 10
                        (BinOp_::Eq, BinOp_::Lt) | (BinOp_::Lt, BinOp_::Eq) => {
                            report_redundant_comparison(
                                &mut self.reporter,
                                "<=",
                                exp.exp.loc,
                                "single range check",
                            );
                        }
                        // Case 2: x == 10 || x > 10  ->  x >= 10
                        (BinOp_::Eq, BinOp_::Gt) | (BinOp_::Gt, BinOp_::Eq) => {
                            report_redundant_comparison(
                                &mut self.reporter,
                                ">=",
                                exp.exp.loc,
                                "single range check",
                            );
                        }
                        // Case 3: x < 10 || x > 20  ->  x not in [10..20]
                        // (BinOp_::Lt, BinOp_::Gt) | (BinOp_::Gt, BinOp_::Lt) => {
                        //     if let (Some(val1), Some(val2)) =
                        //         (extract_constant(lhs_r), extract_constant(rhs_r))
                        //     {
                        //         if val1 < val2 {
                        //             report_redundant_comparison(
                        //                 &mut self.reporter,
                        //                 "not in",
                        //                 exp.exp.loc,
                        //                 "range exclusion check",
                        //             );
                        //         }
                        //     }
                        // }
                        // // Case 4: x <= 10 || x >= 20  ->  x not in (10..20)
                        // (BinOp_::Le, BinOp_::Ge) | (BinOp_::Ge, BinOp_::Le) => {
                        //     if let (Some(val1), Some(val2)) =
                        //         (extract_constant(lhs_r), extract_constant(rhs_r))
                        //     {
                        //         if val1 < val2 {
                        //             report_redundant_comparison(
                        //                 &mut self.reporter,
                        //                 "not in",
                        //                 exp.exp.loc,
                        //                 "exclusive range check",
                        //             );
                        //         }
                        //     }
                        // }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        false
    }
);

fn report_redundant_comparison(
    reporter: &mut DiagnosticReporter,
    oper: &str,
    loc: Loc,
    suggestion_type: &str,
) {
    let msg = format!(
        "Consider simplifying this comparison using `{}` operator for a clearer {}.",
        oper, suggestion_type
    );
    let diag = diag!(StyleCodes::DoubleComparison.diag_info(), (loc, msg));
    reporter.add_diag(diag);
}

fn are_expressions_comparable(
    lhs_l: &H::Exp,
    rhs_l: &H::Exp,
    lhs_r: &H::Exp,
    rhs_r: &H::Exp,
) -> bool {
    lhs_l == rhs_l && lhs_r == rhs_r || lhs_l == rhs_r && lhs_r == rhs_l
}

/// Attempts to extract a constant value from an expression
fn extract_constant(exp: &H::Exp) -> Option<i64> {
    match &exp.exp.value {
        UnannotatedExp_::Value(sp!(_, value)) => match value {
            Value_::U8(val) => Some(*val as i64),
            Value_::U16(val) => Some(*val as i64),
            Value_::U32(val) => Some(*val as i64),
            Value_::U64(val) => Some(*val as i64),
            Value_::U128(val) => Some(*val as i64),
            Value_::U256(val) => Some(val.unchecked_as_u64() as i64),
            _ => None,
        },
        _ => None,
    }
}
