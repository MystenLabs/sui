// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects excessively nested blocks of code, warning when nesting exceeds a predefined threshold.
//! Aims to improve code readability and maintainability by encouraging simpler, flatter code structures.
//! Issues a single warning for each sequence of nested blocks that surpasses the limit, to avoid redundant alerts.
use crate::{
    diag,
    diagnostics::WarningFilters,
    linters::StyleCodes,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

const NESTING_THRESHOLD: usize = 3;
pub struct ExcessiveNesting;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    nesting_level: usize,
    warning_issued: bool,
}

impl TypingVisitorConstructor for ExcessiveNesting {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            nesting_level: 0,
            warning_issued: false,
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

    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        if let UnannotatedExp_::Block(_) = &exp.exp.value {
            self.nesting_level += 1;

            if self.nesting_level > NESTING_THRESHOLD && !self.warning_issued {
                self.env.add_diag(diag!(
                    StyleCodes::ExcessiveNesting.diag_info(),
                    (exp.exp.loc, "Detected excessive block nesting. Consider refactoring to simplify the code."),
                ));
                self.warning_issued = true;
            }
        } else if self.nesting_level <= NESTING_THRESHOLD {
            self.warning_issued = false;
        };
        false
    }
}
