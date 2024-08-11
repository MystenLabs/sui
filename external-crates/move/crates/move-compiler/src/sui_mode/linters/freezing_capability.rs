// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Implements lint to warn against freezing capability-like types in Sui, identifying function calls that may incorrectly freeze such types.
//! The lint checks for specific freezing functions defined in constants and inspects their type arguments for capability-like type names.

use super::{LinterDiagnosticCategory, LinterDiagnosticCode, LINT_WARNING_PREFIX};
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    naming::ast::TypeName_,
    shared::{CompilationEnv, Identifier},
    sui_mode::linters::{FREEZE_FUN, PUBLIC_FREEZE_FUN, SUI_PKG_NAME, TRANSFER_MOD_NAME},
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::*;
use regex::Regex;

const FREEZE_CAPABILITY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::FreezingCapability as u8,
    "Freezing a capability-like type can lead to design issues.",
);

const FREEZE_FUNCTIONS: &[(&str, &str, &str)] = &[
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, PUBLIC_FREEZE_FUN),
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, FREEZE_FUN),
];

pub struct WarnFreezeCapability;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    capability_regex: Regex,
}

impl TypingVisitorConstructor for WarnFreezeCapability {
    type Context<'a> = Context<'a>;
    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            capability_regex: Regex::new(r"Cap(ability)?(\w*v?\d*)?$").unwrap(),
        }
    }
}

impl<'a> TypingVisitorContext for Context<'a> {
    fn visit_module_custom(
        &mut self,
        _ident: crate::expansion::ast::ModuleIdent,
        mdef: &mut T::ModuleDefinition,
    ) -> bool {
        // skips if true
        mdef.attributes.is_test_or_test_only()
    }

    fn visit_function_custom(
        &mut self,
        _module: crate::expansion::ast::ModuleIdent,
        _function_name: crate::parser::ast::FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        // skips if true
        fdef.attributes.is_test_or_test_only()
    }

    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let T::UnannotatedExp_::ModuleCall(fun) = &exp.exp.value {
            if self.is_freeze_function(fun) {
                self.check_type_arguments(fun, exp.exp.loc);
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

impl<'a> Context<'a> {
    fn is_freeze_function(&self, fun: &T::ModuleCall) -> bool {
        FREEZE_FUNCTIONS.iter().any(|(addr, module, fname)| {
            fun.module.value.is(*addr, *module) && &fun.name.value().as_str() == fname
        })
    }

    fn check_type_arguments(&mut self, fun: &T::ModuleCall, loc: Loc) {
        for sp!(_, type_arg) in &fun.type_arguments {
            if let Some(sp!(_, TypeName_::ModuleType(_, struct_name))) = type_arg.type_name() {
                if self.capability_regex.is_match(struct_name.value().as_str()) {
                    self.report_freeze_capability(loc);
                    break;
                }
            }
        }
    }

    fn report_freeze_capability(&mut self, loc: Loc) {
        let msg = "Freezing a capability-like type can lead to design issues.";
        let diag = diag!(FREEZE_CAPABILITY_DIAG, (loc, msg));
        self.env.add_diag(diag);
    }
}
