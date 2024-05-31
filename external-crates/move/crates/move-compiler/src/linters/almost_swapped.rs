// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements a linter check in Rust to detect unnecessary variable swap sequences in Move language code.
//! The linter identifies consecutive assignments that effectively swap two variables without using a temporary variable.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::ModuleIdent,
    naming::ast::Var_,
    parser::ast::FunctionName,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, LValue_, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use std::collections::VecDeque;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, SWAP_SEQUENCE_DIAG_CODE};

const SWAP_SEQUENCE_OVERFLOW_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    SWAP_SEQUENCE_DIAG_CODE,
    "Unnecessary swap sequence detected. Consider simplifying the code or using a temporary variable if swapping is intended.",
);

pub struct SwapSequence;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    last_assignment: VecDeque<(Var_, Var_)>,
}

impl TypingVisitorConstructor for SwapSequence {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            last_assignment: VecDeque::new(),
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        if let T::FunctionBody_::Defined((_, vec_item)) = &mut fdef.body.value {
            vec_item.iter().for_each(|sp!(_, seq_item)| {
                if let SequenceItem_::Seq(seq) = seq_item {
                    if let UnannotatedExp_::Assign(sp!(_, value_list), _, rhs) = &seq.exp.value {
                        if let Some(sp!(_, LValue_::Var { var, .. })) = value_list.get(0) {
                            if let UnannotatedExp_::Copy {
                                var: sp!(_, rhs_var),
                                ..
                            } = &rhs.exp.value
                            {
                                if let Some((prev_var1, prev_var2)) =
                                    self.last_assignment.pop_front()
                                {
                                    if prev_var1 == *rhs_var && prev_var2 == var.value {
                                        report_almost_swapped(self.env, seq.exp.loc);
                                    }
                                }
                                self.last_assignment
                                    .push_back((var.value.clone(), rhs_var.clone()));
                            }
                        }
                    } else {
                        self.last_assignment.clear();
                    }
                }
            });
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

fn report_almost_swapped(env: &mut CompilationEnv, loc: Loc) {
    let msg = format!(
        "Unnecessary swap sequence detected. Consider simplifying the code or using a temporary variable if swapping is intended.",
    );
    let diag = diag!(SWAP_SEQUENCE_OVERFLOW_DIAG, (loc, msg));
    env.add_diag(diag);
}
