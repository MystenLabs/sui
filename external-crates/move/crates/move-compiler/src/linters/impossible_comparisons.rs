// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects contradictory comparisons within an `AND` context in Move code that logically cannot succeed,
//! such as `x < 5 AND x > 10`, and reports them as potential logical errors.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    naming::ast::Var_,
    parser::ast::BinOp_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagCategory, LINTER_DOUBLE_COMPARISON_DIAG_CODE, LINT_WARNING_PREFIX};

const DOUBLE_COMPARISON_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::Correctness as u8,
    LINTER_DOUBLE_COMPARISON_DIAG_CODE,
    "Detected a double comparison that can never succeed, indicating a possible logical error.",
);

pub struct ImpossibleDoubleComparison;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for ImpossibleDoubleComparison {
    type Context<'a> = Context<'a>;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        _program_info: &'a TypingProgramInfo,
        _program: &T::Program_,
    ) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, sp!(_, BinOp_::And), _, rhs) = &exp.exp.value {
            if let Some((var1, value1, op1)) = extract_comparison_details(lhs) {
                if let Some((var2, value2, op2)) = extract_comparison_details(rhs) {
                    if var1 == var2 && are_comparisons_contradictory(&op1, &op2, &value1, &value2) {
                        report_impossible_comparison(self.env, exp.exp.loc);
                    }
                }
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

fn extract_comparison_details(exp: &T::Exp) -> Option<(Var_, Value_, BinOp_)> {
    if let UnannotatedExp_::BinopExp(lhs, sp!(_, op), _, rhs) = &exp.exp.value {
        if matches!(op, BinOp_::Lt | BinOp_::Le | BinOp_::Gt | BinOp_::Ge) {
            if let (
                UnannotatedExp_::Copy {
                    var: sp!(_, var), ..
                },
                UnannotatedExp_::Value(sp!(_, value)),
            ) = (&lhs.exp.value, &rhs.exp.value)
            {
                return Some((*var, value.clone(), *op));
            }

            if let (
                UnannotatedExp_::Value(sp!(_, value)),
                UnannotatedExp_::Copy {
                    var: sp!(_, var), ..
                },
            ) = (&lhs.exp.value, &rhs.exp.value)
            {
                match op {
                    BinOp_::Lt => return Some((*var, value.clone(), BinOp_::Gt)),
                    BinOp_::Le => return Some((*var, value.clone(), BinOp_::Ge)),
                    BinOp_::Gt => return Some((*var, value.clone(), BinOp_::Lt)),
                    BinOp_::Ge => return Some((*var, value.clone(), BinOp_::Le)),
                    _ => (),
                }
            }
        }
    }
    None
}

fn are_comparisons_contradictory(
    op1: &BinOp_,
    op2: &BinOp_,
    value1: &Value_,
    value2: &Value_,
) -> bool {
    let val1 = extract_value(value1);
    let val2 = extract_value(value2);

    match (op1, op2) {
        // Handle exclusive contradictions
        (BinOp_::Lt | BinOp_::Le, BinOp_::Gt | BinOp_::Ge) if val1 < val2 => true,
        (BinOp_::Gt | BinOp_::Ge, BinOp_::Lt | BinOp_::Le) if val1 > val2 => true,
        (_, _) if val1 == val2 => false,
        _ => false,
    }
}

fn extract_value(value: &Value_) -> u128 {
    match value {
        Value_::U8(v) => *v as u128,
        Value_::U64(v) => *v as u128,
        Value_::U32(v) => *v as u128,
        Value_::U16(v) => *v as u128,
        Value_::U128(v) => *v,
        _ => unreachable!(),
    }
}

fn report_impossible_comparison(env: &mut CompilationEnv, loc: Loc) {
    let msg =
        "Detected a double comparison that can never succeed, indicating a possible logical error.";
    let diag = diag!(DOUBLE_COMPARISON_DIAG, (loc, msg));
    env.add_diag(diag);
}
