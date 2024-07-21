// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects comparisons where a variable is compared to 'true' or 'false' using
//! equality (==) or inequality (!=) operators and provides suggestions to simplify the comparisons.
//! Examples: if (x == true) can be simplified to if (x), if (x == false) can be simplified to if (!x)
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    parser::ast::BinOp_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, BOOL_COMPARISON_DIAG_CODE, LINT_WARNING_PREFIX};

const BOOL_COMPARISON_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    BOOL_COMPARISON_DIAG_CODE,
    "unnecessary boolean comparison to true or false",
);

pub struct BoolComparison;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for BoolComparison {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(e1, op, _, e2) = &exp.exp.value {
            if let Some(message) = match (&op.value, &e1.exp.value, &e2.exp.value) {
                (
                    BinOp_::Or | BinOp_::And,
                    UnannotatedExp_::Value(sp!(_, Value_::Bool(bool))),
                    _,
                )
                | (
                    BinOp_::Or | BinOp_::And,
                    _,
                    UnannotatedExp_::Value(sp!(_, Value_::Bool(bool))),
                ) => {
                    let always_value = match op.value {
                        BinOp_::Or => *bool,
                        BinOp_::And => !*bool,
                        _ => unreachable!(),
                    };
                    Some(format!(
                        "This expression always evaluates to {} regardless of the other operand.",
                        always_value
                    ))
                }
                (
                    BinOp_::Eq | BinOp_::Neq,
                    UnannotatedExp_::Value(sp!(_, Value_::Bool(bool))),
                    _,
                )
                | (
                    BinOp_::Eq | BinOp_::Neq,
                    _,
                    UnannotatedExp_::Value(sp!(_, Value_::Bool(bool))),
                ) if matches!(bool, true | false) => {
                    let simplification = match (op.value, bool) {
                        (BinOp_::Eq,true) | (BinOp_::Neq, false) => {
                            "Consider simplifying this expression to the variable or function itself."
                        }
                        (BinOp_::Eq, false) | (BinOp_::Neq, true) => {
                            "Consider simplifying this expression using logical negation '!'."
                        }
                        _ => unreachable!(),
                    };
                    Some(simplification.to_owned())
                }
                _ => None,
            } {
                add_bool_comparison_diag(self.env, exp.exp.loc, &message);
                return true;
            }
        };

        false
    }
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

fn add_bool_comparison_diag(env: &mut CompilationEnv, loc: Loc, message: &str) {
    let d = diag!(
        BOOL_COMPARISON_DIAG,
        (
            loc,
            format!("This boolean comparison is unnecessary. {}", message)
        )
    );
    env.add_diag(d);
}
