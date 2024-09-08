// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Checks for functions with too many parameters in Move code. Functions that exceed a certain number
//! of parameters are flagged to encourage better modularity and design practices.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::ModuleIdent,
    parser::ast::FunctionName,
    shared::CompilationEnv,
    typing::{
        ast::{self as T},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::StyleCodes;

const MAX_PARAMETERS: usize = 10; // Recommended limit for parameters

pub struct TooManyArguments;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for TooManyArguments {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }
    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        let num_params = fdef.signature.parameters.len();
        if num_params > MAX_PARAMETERS {
            let msg = format!(
                "Function '{}' has too many parameters ({}). Consider refactoring to improve readability.",
                function_name.0.value.as_str(), num_params
            );
            let diag = diag!(
                StyleCodes::TooManyArguments.diag_info(),
                (fdef.body.loc, msg)
            );
            self.env.add_diag(diag);
        }
        false
    }
}
