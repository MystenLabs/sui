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

use super::{LinterDiagnosticCategory, EXCESSIVE_PARAMS_DIAG_CODE, LINT_WARNING_PREFIX};

const MAX_PARAMETERS: usize = 10; // Recommended limit for parameters

const EXCESSIVE_PARAMS_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    EXCESSIVE_PARAMS_DIAG_CODE,
    "Function has too many parameters, which may hinder readability and maintainability.",
);

pub struct ExcessiveParametersCheck;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for ExcessiveParametersCheck {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        let num_params = fdef.signature.parameters.len();
        if num_params > MAX_PARAMETERS {
            report_excessive_parameters(
                self.env,
                &function_name.0.value.as_str(),
                num_params,
                fdef.body.loc,
            );
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

fn report_excessive_parameters(
    env: &mut CompilationEnv,
    func_name: &str,
    num_params: usize,
    loc: Loc,
) {
    let msg = format!(
        "Function '{}' has too many parameters ({}). Consider refactoring to improve readability.",
        func_name, num_params
    );
    let diag = diag!(EXCESSIVE_PARAMS_DIAG, (loc, msg));
    env.add_diag(diag);
}
