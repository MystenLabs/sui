// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and reports explicit self-assignments in code, such as `x = x;`, which are generally unnecessary
//! and could indicate potential errors or misunderstandings in the code logic.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::CompilationEnv,
    typing::{
        ast::{self as T, LValue_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, SELF_ASSIGNMENT_DIAG_CODE};

const SELF_ASSIGNMENT_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    SELF_ASSIGNMENT_DIAG_CODE,
    "Explicit self-assignment detected. This operation is usually unnecessary and could indicate a typo or logical error.",
);
pub struct SelfAssignmentCheck;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for SelfAssignmentCheck {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::Assign(value_list, _, assign_exp) = &exp.exp.value {
            if let Some(
                sp!(
                    _,
                    LValue_::Var {
                        var: sp!(_, lhs),
                        ..
                    }
                ),
            ) = value_list.value.get(0)
            {
                match &assign_exp.exp.value {
                    UnannotatedExp_::Copy {
                        var: sp!(_, rhs), ..
                    } => {
                        if lhs == rhs {
                            report_self_assignment(self.env, &lhs.name.as_str(), exp.exp.loc);
                        }
                    }
                    UnannotatedExp_::Move {
                        var: sp!(_, rhs), ..
                    } => {
                        if lhs == rhs {
                            report_self_assignment(self.env, &lhs.name.as_str(), exp.exp.loc);
                        }
                    }
                    _ => (),
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

fn report_self_assignment(env: &mut CompilationEnv, var_name: &str, loc: Loc) {
    let msg = format!(
            "Explicit self-assignment detected for variable '{}'. Consider removing it to clarify intent.",
            var_name
        );
    let diag = diag!(SELF_ASSIGNMENT_DIAG, (loc, msg));
    env.add_diag(diag);
}
