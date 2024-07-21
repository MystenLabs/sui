// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements a lint to detect and warn about binary operations with equal operands in Move code.
//! Targets comparison, logical, bitwise, subtraction, and division operations where such usage might indicate errors or redundancies.
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

use super::{LinterDiagnosticCategory, EQUAL_OPERANDS_DIAG_CODE, LINT_WARNING_PREFIX};

const EQUAL_OPERANDS_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    EQUAL_OPERANDS_DIAG_CODE,
    "Equal operands detected in binary operation, which might indicate a logical error or redundancy.",
);

pub struct EqualOperandsCheck;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for EqualOperandsCheck {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, sp!(_, op), _, rhs) = &exp.exp.value {
            if lhs.exp.value == rhs.exp.value && is_relevant_op(op) {
                report_equal_operands(self.env, exp.exp.loc);
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

fn is_relevant_op(op: &BinOp_) -> bool {
    matches!(
        op,
        BinOp_::Eq
            | BinOp_::Neq
            | BinOp_::Gt
            | BinOp_::Ge
            | BinOp_::Lt
            | BinOp_::Le
            | BinOp_::And
            | BinOp_::Or
            | BinOp_::BitAnd
            | BinOp_::BitOr
            | BinOp_::Xor
            | BinOp_::Sub
            | BinOp_::Div
    )
}

fn report_equal_operands(env: &mut CompilationEnv, loc: Loc) {
    let msg = "Equal operands detected in binary operation, which might indicate a logical error or redundancy.";
    let diag = diag!(EQUAL_OPERANDS_DIAG, (loc, msg));
    env.add_diag(diag);
}
