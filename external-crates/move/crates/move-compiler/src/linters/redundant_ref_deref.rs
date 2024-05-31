// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Implements lint rule for Move code to detect redundant ref/deref patterns.
// It identifies and reports unnecessary temporary borrow followed by a deref and a local borrow.
// Aims to improve code efficiency by suggesting direct usage of expressions without redundant operations.

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, REDUNDANT_REF_DEREF_DIAG_CODE};
use crate::parser::ast::Field;
use crate::typing::ast::Exp;
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

const REDUNDANT_REF_DEREF_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    REDUNDANT_REF_DEREF_DIAG_CODE,
    "",
);

pub struct RedundantRefDerefVisitor;

#[derive(Debug, Clone)]
struct StoredMutation {
    exp: Box<Exp>,
    field: Field,
}

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    stored_mutations: Vec<StoredMutation>,
}

impl TypingVisitorConstructor for RedundantRefDerefVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            stored_mutations: Vec::new(),
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }
    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_exp_custom(&mut self, exp: &mut Exp) -> bool {
        self.track_modifications(exp);
        self.check_redundant_ref_deref(exp);
        false
    }
}

impl Context<'_> {
    fn check_redundant_ref_deref(&mut self, exp: &Exp) {
        if let UnannotatedExp_::TempBorrow(false, borrow_exp) = &exp.exp.value {
            if let UnannotatedExp_::Dereference(deref_exp) = &borrow_exp.exp.value {
                // Only flag &*& pattern, not &mut *&
                self.check_borrow_pattern(exp, deref_exp);
            }
        }
    }

    fn check_borrow_pattern(&mut self, exp: &Exp, deref_exp: &Box<Exp>) {
        match &deref_exp.exp.value {
            UnannotatedExp_::BorrowLocal(false, _) | UnannotatedExp_::TempBorrow(false, _) => {
                self.add_redundant_ref_deref_diagnostic(exp.exp.loc);
            }
            UnannotatedExp_::Borrow(false, borrow_exp, field) => {
                if self.is_stored_mutation(borrow_exp, field) {
                    self.add_redundant_ref_deref_diagnostic(exp.exp.loc);
                }
            }
            _ => {}
        }
    }

    fn is_stored_mutation(&self, borrow_exp: &Box<Exp>, field: &Field) -> bool {
        self.stored_mutations
            .iter()
            .any(|stored| Self::compare_exp(&stored.exp, borrow_exp) && &stored.field == field)
    }

    fn add_redundant_ref_deref_diagnostic(&mut self, loc: Loc) {
        let diag = diag!(
            REDUNDANT_REF_DEREF_DIAG,
            (
                loc,
                "Redundant borrow-dereference detected. Consider removing the borrow-dereference operation and using the expression directly."
            )
        );
        self.env.add_diag(diag);
    }

    fn track_modifications(&mut self, exp: &T::Exp) {
        if let UnannotatedExp_::Mutate(lhs, _) = &exp.exp.value {
            self.mark_modified(lhs);
        }
    }

    fn mark_modified(&mut self, exp: &T::Exp) {
        if let UnannotatedExp_::Borrow(_, inner_exp, field) = &exp.exp.value {
            self.stored_mutations.push(StoredMutation {
                exp: inner_exp.clone(),
                field: field.clone(),
            });
        }
    }

    fn compare_exp(exp1: &Box<Exp>, exp2: &Box<Exp>) -> bool {
        match (&exp1.exp.value, &exp2.exp.value) {
            (
                UnannotatedExp_::Borrow(_, inner1, field1),
                UnannotatedExp_::Borrow(_, inner2, field2),
            ) => {
                field1 == field2
                    && (Self::compare_unannotated_exp(inner1, inner2)
                        || Self::compare_exp(&inner1, &inner2))
            }
            _ => Self::compare_unannotated_exp(exp1, exp2),
        }
    }

    fn compare_unannotated_exp(exp1: &Box<Exp>, exp2: &Box<Exp>) -> bool {
        std::mem::discriminant(&exp1.exp.value) == std::mem::discriminant(&exp2.exp.value)
            && exp1.ty == exp2.ty
    }
}
