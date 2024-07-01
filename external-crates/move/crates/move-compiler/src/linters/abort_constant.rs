// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Lint to encourage the use of named constants with 'abort' and 'assert' for enhanced code readability.
//! Detects cases where numeric literals are used directly and issues a warning.
//! Provides the `is_named_constant` helper function to determine if an expression represents a named constant.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::CompilationEnv,
    typing::{
        ast::{self as T, BuiltinFunction_, ExpListItem, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

use super::{LinterDiagnosticCategory, ABORT_CONSTANT_DIAG_CODE, LINT_WARNING_PREFIX};

const ABORT_CONSTANT_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    ABORT_CONSTANT_DIAG_CODE,
    "Prefer using named constants with 'abort' and 'assert' for clarity",
);

pub struct AssertAbortNamedConstants;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for AssertAbortNamedConstants {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        match &exp.exp.value {
            UnannotatedExp_::Abort(abort_exp) => {
                check_and_report(self.env, abort_exp);
            }
            UnannotatedExp_::Builtin(assert, assert_exp) => {
                let BuiltinFunction_::Assert(_) = assert.value else {
                    return false;
                };
                check_and_report(self.env, assert_exp);
            }
            _ => {}
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
fn check_and_report(env: &mut CompilationEnv, arg_exp: &Box<T::Exp>) {
    if !is_named_constant(&arg_exp.exp.value) {
        let diag = diag!(
            ABORT_CONSTANT_DIAG,
            (arg_exp.exp.loc, "Prefer using a named constant.")
        );
        env.add_diag(diag);
    }
}

fn is_named_constant(exp: &UnannotatedExp_) -> bool {
    match exp {
        UnannotatedExp_::Constant(_, _) => true,
        UnannotatedExp_::ExpList(exp_list) => {
            if let Some(ExpListItem::Single(exp, _)) = exp_list.get(1) {
                let UnannotatedExp_::Constant(_, _) = &exp.exp.value else {
                    return false;
                };
                return true;
            }
            false
        }
        _ => false,
    }
}
