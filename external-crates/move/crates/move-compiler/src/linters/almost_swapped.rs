// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Implements a linter check in Rust to detect unnecessary variable swap sequences in Move language code.
//! The linter identifies consecutive assignments that effectively swap two variables without using a temporary variable.
use crate::{
    diag,
    diagnostics::WarningFilters,
    expansion::ast::ModuleIdent,
    linters::StyleCodes,
    naming::ast::Var_,
    parser::ast::FunctionName,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, LValue_, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use std::collections::VecDeque;

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
        fdef: &T::Function,
    ) -> bool {
        if let T::FunctionBody_::Defined((_, vec_item)) = &fdef.body.value {
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
                                        self.env.add_diag(diag!(
                                            StyleCodes::AlmostSwapped.diag_info(),
                                            (seq.exp.loc, "Remove unnecessary swap sequence")
                                        ));
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
