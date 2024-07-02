// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Implementslint to warn against freezing capability-like types in Sui, identifying function calls that may incorrectly freeze such types.
//! The lint checks for specific freezing functions defined in constants and inspects their type arguments for capability-like type names.

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    naming::ast::{TypeName_, Type_},
    shared::{CompilationEnv, Identifier},
    sui_mode::linters::{FREEZE_FUN, PUBLIC_FREEZE_FUN, SUI_PKG_NAME, TRANSFER_MOD_NAME},
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::*;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, WARN_FREEZE_CAPABILITY_DIAG_CODE};

const FREEZE_WRAPPING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    WARN_FREEZE_CAPABILITY_DIAG_CODE,
    "Freezing a capability-like type can lead to design issues.",
);

const FREEZE_FUNCTIONS: &[(&str, &str, &str)] = &[
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, PUBLIC_FREEZE_FUN),
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, FREEZE_FUN),
];

pub struct WarnFreezeCapability;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for WarnFreezeCapability {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl<'a> TypingVisitorContext for Context<'a> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        if let E::ModuleCall(fun) = &exp.exp.value {
            if FREEZE_FUNCTIONS.iter().any(|(addr, module, fname)| {
                fun.module.value.is(*addr, *module) && &fun.name.value().as_str() == fname
            }) {
                fun.type_arguments.iter().for_each(|sp!(_, type_arg)| {
                    if let Type_::Apply(_, sp!(_, type_name), _) = type_arg {
                        if let TypeName_::ModuleType(_, struct_name) = &type_name {
                            if struct_name.0.value.as_str().ends_with("Cap")
                                || struct_name.0.value.as_str().ends_with("Capability")
                            {
                                report_freeze_capability(self.env, exp.exp.loc);
                            }
                        }
                    }
                });
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

fn report_freeze_capability(env: &mut CompilationEnv, loc: Loc) {
    let msg = format!("Freezing a capability-like type can lead to design issues.",);
    let diag = diag!(FREEZE_WRAPPING_DIAG, (loc, msg));
    env.add_diag(diag);
}
