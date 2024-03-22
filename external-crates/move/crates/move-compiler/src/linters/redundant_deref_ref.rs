// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements lint rule for Move IR code to detect redundant dereference of a reference.
//! It identifies patterns where a dereference (`*`) is immediately followed by a borrow (`&` or `&mut`).
//! The lint aims to simplify expressions by removing unnecessary dereference-borrow sequences.

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

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const REDUNDANT_DEREF_REF_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::RedundantDerefRef as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "",
);

pub struct RedundantDerefRef;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for RedundantDerefRef {
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
        if let UnannotatedExp_::Dereference(deref_exp) = &exp.exp.value {
            if let UnannotatedExp_::Borrow(_, _, _) = &deref_exp.exp.value {
                report_deref_ref(self.env, exp.exp.loc);
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

fn report_deref_ref(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
       REDUNDANT_DEREF_REF_DIAG,
        (loc, "Redundant dereference of a reference detected (`*&` or `*&mut`). Consider simplifying the expression.")
    );
    env.add_diag(diag);
}
