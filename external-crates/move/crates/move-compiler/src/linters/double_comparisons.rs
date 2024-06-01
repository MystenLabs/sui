// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and simplifies double comparisons in code, such as `x == 10 || x < 10` into `x <= 10`,
//! which can enhance code clarity and maintainability.

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    parser::ast::BinOp_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, DOUBLE_COMPARISON_DIAG_CODE, LINT_WARNING_PREFIX};

const DOUBLE_COMPARISON_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    DOUBLE_COMPARISON_DIAG_CODE,
    "Double comparison detected that could be simplified to a single range check.",
);

pub struct DoubleComparisonCheck;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for DoubleComparisonCheck {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, sp!(_, BinOp_::Or), _, rhs) = &exp.exp.value {
            match (&lhs.exp.value, &rhs.exp.value) {
                (
                    UnannotatedExp_::BinopExp(lhs_l, sp!(_, lhs_op), _, lhs_r),
                    UnannotatedExp_::BinopExp(rhs_l, sp!(_, rhs_op), _, rhs_r),
                ) if are_expressions_comparable(lhs_l, rhs_l, lhs_r, rhs_r) => {
                    match (lhs_op, rhs_op) {
                        (BinOp_::Eq, BinOp_::Lt) | (BinOp_::Lt, BinOp_::Eq) => {
                            report_redundant_comparison(self.env, "<=", exp.exp.loc);
                        }
                        (BinOp_::Eq, BinOp_::Gt) | (BinOp_::Gt, BinOp_::Eq) => {
                            report_redundant_comparison(self.env, ">=", exp.exp.loc);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

fn are_expressions_comparable(
    lhs_l: &T::Exp,
    rhs_l: &T::Exp,
    lhs_r: &T::Exp,
    rhs_r: &T::Exp,
) -> bool {
    lhs_l == rhs_l && lhs_r == rhs_r || lhs_l == rhs_r && lhs_r == rhs_l
}

fn report_redundant_comparison(env: &mut CompilationEnv, oper: &str, loc: Loc) {
    let msg = format!(
        "Consider simplifying this comparison to `{}` for better clarity.",
        oper
    );
    let diag = diag!(DOUBLE_COMPARISON_DIAG, (loc, msg));
    env.add_diag(diag);
}
