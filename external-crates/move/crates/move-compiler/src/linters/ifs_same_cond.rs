// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements a lint to detect consecutive `if` statements with identical conditions in Move code.
//! Uses a saved last if to track `if` conditions across nested structures, ensuring accurate detection in complex scenarios.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, CONSECUTIVE_IFS_DIAG_CODE, LINT_WARNING_PREFIX};

const CONSECUTIVE_IFS_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    CONSECUTIVE_IFS_DIAG_CODE,
    "Consecutive `if` statements with the same condition detected. Consider combining these statements to simplify the code.",
);

pub struct ConsecutiveIfs;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    last_if_condition: Option<UnannotatedExp_>,
}

impl TypingVisitorConstructor for ConsecutiveIfs {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            last_if_condition: None,
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::IfElse(cond, _, _) = &exp.exp.value {
            if let Some(last_cond) = &self.last_if_condition {
                if last_cond == &cond.exp.value {
                    report_consecutive_ifs(self.env, exp.exp.loc);
                }
            }
            self.last_if_condition = Some(cond.exp.value.clone());
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

fn report_consecutive_ifs(env: &mut CompilationEnv, loc: Loc) {
    let msg = "Consecutive `if` statements with the same condition detected. Consider combining these statements to simplify the code.";
    let diag = diag!(CONSECUTIVE_IFS_DIAG, (loc, msg));
    env.add_diag(diag);
}
